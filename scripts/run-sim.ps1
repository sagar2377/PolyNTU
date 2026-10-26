# Simulated demonstration instance (port 8000), the default demo.
# Bus brackets answer from the deterministic simulator, and the virtual clock
# can be advanced from the interface. For the external-connector demonstration
# run scripts/run-live-demo.ps1 alongside it on port 8001; the two use separate
# databases because a clock advance makes every live bracket unanswerable.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'rust-env.ps1')
. (Join-Path $PSScriptRoot 'configure-dev.ps1')
& (Join-Path $PSScriptRoot 'start-local-db.ps1')
$env:POLYNTU_BIND = '127.0.0.1:8000'
$env:POLYNTU_NTUBUS_PROVIDER = ''
Remove-Item Env:POLYNTU_LIVE_MODE -ErrorAction SilentlyContinue
Push-Location (Join-Path $projectRoot 'backend')
try { cargo run --locked --release; if ($LASTEXITCODE -ne 0) { throw 'Backend failed' } } finally { Pop-Location }
