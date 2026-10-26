# Live demonstration run: the same pipeline as run-prod.ps1, plus the NTU bus
# provider and live evidence mode, so bus brackets resolve from the campus
# Omnibus feed instead of the deterministic simulator.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'rust-env.ps1')
. (Join-Path $PSScriptRoot 'configure-dev.ps1')
& (Join-Path $PSScriptRoot 'start-local-db.ps1')
$env:POLYNTU_LIVE_MODE = 'true'
if (-not $env:POLYNTU_NTUBUS_PROVIDER) { $env:POLYNTU_NTUBUS_PROVIDER = 'http://127.0.0.1:8090/resolve' }
$provider = $null
try {
    $providerDir = Join-Path $projectRoot 'provider\ntubus'
    $provider = Start-Process -FilePath 'python' -ArgumentList 'service.py' -WorkingDirectory $providerDir -PassThru
    # The adapter needs the provider listening before the first bracket closes.
    $ready = $false
    for ($i = 0; $i -lt 30; $i++) {
        try { $null = Invoke-RestMethod -Uri 'http://127.0.0.1:8090/' -TimeoutSec 2; $ready = $true; break } catch { Start-Sleep -Seconds 1 }
    }
    if (-not $ready) { Write-Warning 'The NTU bus provider did not become ready; live brackets will void at their evidence deadline.' }
    Push-Location (Join-Path $projectRoot 'frontend')
    try { npm.cmd run build; if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' } } finally { Pop-Location }
    Push-Location (Join-Path $projectRoot 'backend')
    try { cargo run --locked --release; if ($LASTEXITCODE -ne 0) { throw 'Backend failed' } } finally { Pop-Location }
} finally {
    if ($provider -and -not $provider.HasExited) { Stop-Process -Id $provider.Id -Force }
}
