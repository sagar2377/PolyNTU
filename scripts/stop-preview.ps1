# Stop only the hidden local preview recorded by this workspace.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$metadata = Join-Path $projectRoot '.local/demo-process.json'
if (-not (Test-Path -LiteralPath $metadata)) { Write-Output 'No recorded preview'; return }
$preview = Get-Content -LiteralPath $metadata -Raw | ConvertFrom-Json
$expectedRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot '.local')) + [IO.Path]::DirectorySeparatorChar
$executable = [IO.Path]::GetFullPath($preview.executable)
if (-not $executable.StartsWith($expectedRoot, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($executable) -notlike 'polyntu-demo*.exe') { throw 'Preview metadata does not identify a workspace demo executable' }
$previewProcess = Get-Process -Id $preview.process_id -ErrorAction SilentlyContinue
if ($previewProcess -and $previewProcess.Path -eq $executable) {
    Stop-Process -InputObject $previewProcess
    $previewProcess.WaitForExit(10000) | Out-Null
    Write-Output 'Local preview stopped; PostgreSQL and its data remain available'
} else { Write-Output 'Recorded preview is already stopped' }
