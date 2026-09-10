param([int]$Port = 55432)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$pgBin = Join-Path $projectRoot '.tools\postgres\pgsql\bin'
$pgData = Join-Path $projectRoot '.local\postgres-data'
$pgLog = Join-Path $projectRoot '.local\postgres.log'
if (-not (Test-Path (Join-Path $pgBin 'initdb.exe'))) { throw 'Portable PostgreSQL is missing. See docs/development.md or use Docker Compose.' }
New-Item -ItemType Directory -Force -Path (Join-Path $projectRoot '.local') | Out-Null
if (-not (Test-Path (Join-Path $pgData 'PG_VERSION'))) {
    & (Join-Path $pgBin 'initdb.exe') -D $pgData -U polyntu -A trust --no-locale --encoding=UTF8
    if ($LASTEXITCODE -ne 0) { throw 'initdb failed' }
}
& (Join-Path $pgBin 'pg_ctl.exe') -D $pgData status 2>$null
if ($LASTEXITCODE -ne 0) {
    & (Join-Path $pgBin 'pg_ctl.exe') -D $pgData -l $pgLog -o "-h 127.0.0.1 -p $Port" -w start
    if ($LASTEXITCODE -ne 0) { throw 'PostgreSQL failed to start' }
}
$exists = & (Join-Path $pgBin 'psql.exe') -h 127.0.0.1 -p $Port -U polyntu -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='polyntu'"
if ($exists -ne '1') {
    & (Join-Path $pgBin 'createdb.exe') -h 127.0.0.1 -p $Port -U polyntu polyntu
    if ($LASTEXITCODE -ne 0) { throw 'Database creation failed' }
}
Write-Output "DATABASE_URL=postgres://polyntu@127.0.0.1:$Port/polyntu"
