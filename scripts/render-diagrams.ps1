param([string]$Diagram = 'docs\diagrams\use-case.puml')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$toolsDir = Join-Path $projectRoot '.tools'
$jar = Join-Path $toolsDir 'plantuml.jar'
$source = Join-Path $projectRoot $Diagram
if (-not (Test-Path $source)) { throw "Diagram source not found: $Diagram" }
if (-not (Get-Command java -ErrorAction SilentlyContinue)) {
    throw 'Java is required to render diagrams. Install a JDK, then run this script again.'
}
if (-not (Test-Path $jar)) {
    # Windows PowerShell 5.1 defaults to older TLS; GitHub requires 1.2 or newer.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
    $release = Invoke-RestMethod 'https://api.github.com/repos/plantuml/plantuml/releases/latest' -Headers @{ 'User-Agent' = 'polyntu-docs' }
    $asset = $release.assets | Where-Object { $_.name -match '^plantuml-\d+\.\d+\.\d+\.jar$' } | Select-Object -First 1
    if (-not $asset) { throw 'No PlantUML jar found in the latest release' }
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $jar
    Write-Output "Downloaded $($asset.name) into .tools\plantuml.jar"
}
& java -jar $jar -tpng -charset UTF-8 $source
if ($LASTEXITCODE -ne 0) { throw 'PlantUML rendering failed' }
Write-Output "Rendered $([IO.Path]::ChangeExtension($Diagram, '.png'))"