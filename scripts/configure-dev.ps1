$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$secretFile = Join-Path $projectRoot '.local\dev-secrets.json'
if (-not (Test-Path $secretFile)) {
    New-Item -ItemType Directory -Force -Path (Split-Path $secretFile -Parent) | Out-Null
    function New-DevSecret {
        $bytes = New-Object byte[] 32
        $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
        try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
        [Convert]::ToBase64String($bytes)
    }
    @{ quote = (New-DevSecret); admin = (New-DevSecret) } | ConvertTo-Json | Set-Content -LiteralPath $secretFile -Encoding utf8
}
$secrets = Get-Content -LiteralPath $secretFile -Raw | ConvertFrom-Json
$env:POLYNTU_QUOTE_SECRET = $secrets.quote
$env:POLYNTU_ADMIN_TOKEN = $secrets.admin
$env:DATABASE_URL = 'postgres://polyntu@127.0.0.1:55432/polyntu'
$env:POLYNTU_DEMO_MODE = 'true'
$env:POLYNTU_BIND = '127.0.0.1:8000'
$env:POLYNTU_FRONTEND_DIR = Join-Path $projectRoot 'frontend\dist'
