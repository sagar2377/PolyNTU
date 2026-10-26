# Release local run: same pipeline as run-dev.ps1 with a locked, optimised backend build.
# Also starts the NTU bus provider so the bus adapter can answer from the campus
# Omnibus feed; without it the adapter falls back to the deterministic simulator.
# Set POLYNTU_LIVE_MODE=true to refuse that fallback (see run-live.ps1).
param([switch]$NoProvider)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'rust-env.ps1')
. (Join-Path $PSScriptRoot 'configure-dev.ps1')
& (Join-Path $PSScriptRoot 'start-local-db.ps1')
if (-not $env:POLYNTU_NTUBUS_PROVIDER) { $env:POLYNTU_NTUBUS_PROVIDER = 'http://127.0.0.1:8090/resolve' }
$provider = $null
try {
    if (-not $NoProvider -and $env:POLYNTU_NTUBUS_PROVIDER) {
        $providerDir = Join-Path $projectRoot 'provider\ntubus'
        $provider = Start-Process -FilePath 'python' -ArgumentList 'service.py' -WorkingDirectory $providerDir -PassThru
        $ready = $false
        for ($i = 0; $i -lt 30; $i++) {
            try { $null = Invoke-RestMethod -Uri 'http://127.0.0.1:8090/' -TimeoutSec 2; $ready = $true; break } catch { Start-Sleep -Seconds 1 }
        }
        if ($ready) {
            $status = Invoke-RestMethod -Uri 'http://127.0.0.1:8090/' -TimeoutSec 2
            Write-Output "NTU bus provider ready: $($status.events) arrival events across $($status.routes.PSObject.Properties.Count) routes"
        } else {
            Write-Warning 'The NTU bus provider did not become ready; the adapter will fall back to simulated evidence.'
        }
    }
    Push-Location (Join-Path $projectRoot 'frontend')
    try { npm.cmd run build; if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' } } finally { Pop-Location }
    Push-Location (Join-Path $projectRoot 'backend')
    try { cargo run --locked --release; if ($LASTEXITCODE -ne 0) { throw 'Backend failed' } } finally { Pop-Location }
} finally {
    if ($provider -and -not $provider.HasExited) { Stop-Process -Id $provider.Id -Force }
}
