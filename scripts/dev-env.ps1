# Dot-source this script in PowerShell: . ./scripts/dev-env.ps1
# Changes only this process and child processes; never user/system settings.
[CmdletBinding()]
param([switch]$SkipVisualStudio)
$ErrorActionPreference = 'Stop'

if (-not $env:LOCALAPPDATA) { throw 'Native Windows LOCALAPPDATA is required.' }
$greenLensRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$greenLensBytes = [Text.Encoding]::UTF8.GetBytes($greenLensRoot.ToLowerInvariant())
$greenLensSha = [Security.Cryptography.SHA256]::Create()
try {
    $greenLensWorkspaceId = ([BitConverter]::ToString($greenLensSha.ComputeHash($greenLensBytes))).Replace('-', '').Substring(0, 16).ToLowerInvariant()
} finally { $greenLensSha.Dispose() }
$env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA "GreenLens\build\$greenLensWorkspaceId"
$env:npm_config_cache = Join-Path $env:LOCALAPPDATA 'GreenLens\npm-cache'
New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR, $env:npm_config_cache | Out-Null
$greenLensCargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$greenLensCargoBin = Join-Path $greenLensCargoHome 'bin'
if (Test-Path -LiteralPath $greenLensCargoBin) { $env:PATH = "$greenLensCargoBin;$env:PATH" }

if (-not $SkipVisualStudio -and -not $env:VSCMD_VER) {
    $greenLensVsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $greenLensVsWhere)) { throw 'Visual Studio C++ Build Tools are required; see docs/toolchain.md.' }
    $greenLensVsPath = & $greenLensVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $greenLensVsPath) { throw 'Install Visual Studio Desktop development with C++ and a Windows SDK.' }
    $greenLensDevCmd = Join-Path $greenLensVsPath 'Common7\Tools\VsDevCmd.bat'
    & (Join-Path $PSScriptRoot 'import-msvc-env.ps1') -VsDevCmdPath $greenLensDevCmd
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Rust is missing; follow docs/toolchain.md to install the pinned native toolchain.' }
# Build identity is queried by the Rust build script for every Cargo invocation.
# Do not cache it in this reusable shell. Explicit CI assertions use the distinct
# GREENLENS_EXPECT_SOURCE_COMMIT / GREENLENS_EXPECT_SOURCE_DIRTY variables.
Remove-Item Env:GREENLENS_SOURCE_COMMIT, Env:GREENLENS_SOURCE_DIRTY -ErrorAction SilentlyContinue
Write-Host 'GreenLens native development environment ready (process-local caches and tool paths).'
