param([ValidateSet('benchmark.mjs', 'benchmark-settlement.mjs', 'benchmark-kpi.mjs')][string]$Workload = 'benchmark.mjs', [int]$Seconds = 600)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'rust-env.ps1')
. (Join-Path $PSScriptRoot 'configure-dev.ps1')
& (Join-Path $PSScriptRoot 'start-local-db.ps1')
Push-Location $projectRoot
try {
    $portCheck = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 18000)
    try { $portCheck.Start() } finally { $portCheck.Stop() }
    cargo build --manifest-path backend/Cargo.toml --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
    $benchDatabase = 'polyntu_bench_' + [guid]::NewGuid().ToString('N')
    & ./.tools/postgres/pgsql/bin/createdb.exe -h 127.0.0.1 -p 55432 -U polyntu $benchDatabase
    if ($LASTEXITCODE -ne 0) { throw 'Isolated database creation failed' }
    $env:DATABASE_URL = 'postgres://polyntu@127.0.0.1:55432/' + $benchDatabase
    $env:POLYNTU_DEMO_MODE = 'false'
    $env:POLYNTU_BIND = '127.0.0.1:18000'
    $env:BENCH_URL = 'http://127.0.0.1:18000'
    $env:BENCH_SECONDS = $Seconds.ToString()
    $runId = [guid]::NewGuid().ToString('N')
    $executable = Join-Path $projectRoot ".local/bench-$runId.exe"
    Copy-Item -LiteralPath ./backend/target/release/polyntu.exe -Destination $executable
    $benchProcess = Start-Process -FilePath $executable -WindowStyle Hidden -PassThru -RedirectStandardOutput ".local/bench-$runId.stdout.log" -RedirectStandardError ".local/bench-$runId.stderr.log"
    try {
        @{ database=$benchDatabase; process_id=$benchProcess.Id; workload=$Workload } | ConvertTo-Json | Set-Content ".local/bench-$runId.json"
        $ready = $false
        for ($attempt=0; $attempt -lt 20; $attempt++) {
            if ($benchProcess.HasExited) { throw 'Benchmark server exited; inspect its .local logs' }
            try { Invoke-RestMethod "$env:BENCH_URL/health" | Out-Null; $ready=$true; break } catch { Start-Sleep -Milliseconds 500 }
        }
        if (-not $ready) { throw 'Benchmark server did not become ready' }
        node (Join-Path $PSScriptRoot $Workload)
        if ($LASTEXITCODE -ne 0) { throw 'Benchmark failed; inspect the report and server logs' }
    } finally {
        if (-not $benchProcess.HasExited) { Stop-Process -InputObject $benchProcess; $benchProcess.WaitForExit(10000) | Out-Null }
    }
    Write-Output "Retained isolated database $benchDatabase for inspection; results are under .local/"
} finally { Pop-Location }
