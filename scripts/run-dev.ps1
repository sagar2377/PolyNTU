$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'rust-env.ps1')
. (Join-Path $PSScriptRoot 'configure-dev.ps1')
& (Join-Path $PSScriptRoot 'start-local-db.ps1')
Push-Location (Join-Path $projectRoot 'frontend')
try { npm.cmd run build; if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' } } finally { Pop-Location }
Push-Location (Join-Path $projectRoot 'backend')
try { cargo run --locked --release; if ($LASTEXITCODE -ne 0) { throw 'Backend failed' } } finally { Pop-Location }
