# Optional Windows helper for an installed but unregistered MSVC build toolchain.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
if (Test-Path (Join-Path $projectRoot '.tools\cargo')) {
    $env:CARGO_HOME = Join-Path $projectRoot '.tools\cargo'
}
if ($IsWindows -or $env:OS -eq 'Windows_NT') {
    $msvcRoot = Get-ChildItem 'C:\Program Files (x86)\Microsoft Visual Studio\*\*\VC\Tools\MSVC\*' -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
    $sdkRoot = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\Lib\*' -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
    if ($msvcRoot -and $sdkRoot) {
        $msvcBin = Join-Path $msvcRoot.FullName 'bin\Hostx64\x64'
        $env:PATH = $msvcBin + ';' + $env:PATH
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = Join-Path $msvcBin 'link.exe'
        $env:LIB = (@((Join-Path $msvcRoot.FullName 'lib\x64'), (Join-Path $sdkRoot.FullName 'ucrt\x64'), (Join-Path $sdkRoot.FullName 'um\x64')) -join ';')
        $sdkInclude = Join-Path (Split-Path (Split-Path $sdkRoot.FullName -Parent) -Parent) ('Include\' + $sdkRoot.Name)
        $env:INCLUDE = (@((Join-Path $msvcRoot.FullName 'include'), (Join-Path $sdkInclude 'ucrt'), (Join-Path $sdkInclude 'shared'), (Join-Path $sdkInclude 'um')) -join ';')
    }
}
