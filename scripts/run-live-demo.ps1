# Live demonstration instance (port 8001) for the external-connector part of
# the demo: bus brackets resolve only from the campus Omnibus feed through
# provider/ntubus and void at their deadline if it cannot answer.
#
# It runs beside scripts/run-sim.ps1 on a SEPARATE database. The demo clock
# cannot be shared: advancing it puts every new bracket in the provider's
# wall-clock future, where the feed can never answer. Live mode refuses clock
# advances outright, so this instance is left on the real clock.
#
# Start the provider first: python service.py from provider/ntubus/.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'rust-env.ps1')
. (Join-Path $PSScriptRoot 'configure-dev.ps1')
& (Join-Path $PSScriptRoot 'start-local-db.ps1')
# A live demonstration needs its own database: the demo mode and the clock
# offset are fixed when a database is initialized.
$psql = Join-Path $projectRoot '.tools\postgres\pgsql\bin\psql.exe'
$exists = & $psql 'postgres://polyntu@127.0.0.1:55432/postgres' -tAc "SELECT 1 FROM pg_database WHERE datname='polyntu_live_demo'"
if ($exists -ne '1') {
    & (Join-Path $projectRoot '.tools\postgres\pgsql\bin\createdb.exe') -h 127.0.0.1 -p 55432 -U polyntu polyntu_live_demo
    if ($LASTEXITCODE -ne 0) { throw 'Live demo database creation failed' }
}
$env:DATABASE_URL = 'postgres://polyntu@127.0.0.1:55432/polyntu_live_demo'
$env:POLYNTU_BIND = '127.0.0.1:8001'
$env:POLYNTU_LIVE_MODE = 'true'
$env:POLYNTU_NTUBUS_PROVIDER = 'http://127.0.0.1:8090/resolve'
Push-Location (Join-Path $projectRoot 'backend')
try { cargo run --locked --release; if ($LASTEXITCODE -ne 0) { throw 'Backend failed' } } finally { Pop-Location }
