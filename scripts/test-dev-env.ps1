#Requires -Version 7.5
[CmdletBinding()]
param([string]$OutputDirectory, [switch]$Worker)
$ErrorActionPreference = 'Stop'
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $env:LOCALAPPDATA ('GreenLens/verification/dev-env-' + [guid]::NewGuid().ToString('N')) }
if (-not $Worker) {
    if (Test-Path -LiteralPath $OutputDirectory) { throw 'Development-environment test destination must be new.' }
    New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
    # Isolate the real Visual Studio initializer from the invoking shell.
    & (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File $PSCommandPath -OutputDirectory $OutputDirectory -Worker
    if ($LASTEXITCODE -ne 0) { throw 'Fresh-shell Unicode development-environment regression failed.' }
    return
}

Remove-Item Env:VSCMD_VER -ErrorAction SilentlyContinue
$expectedEvidence = Join-Path $OutputDirectory 'ci-native 空間'
$expectedValue = 'Unicode 測試 = 42 ! %PATH% & example'
$unicodePath = Join-Path $OutputDirectory 'tool path 測試'
New-Item -ItemType Directory -Path $unicodePath | Out-Null
$env:CI_EVIDENCE = $expectedEvidence
$env:GREENLENS_DEV_ENV_PROBE = $expectedValue
$env:PATH = "$unicodePath;$env:PATH"
$checks = [Collections.Generic.List[string]]::new()

# Synthetic initializers exercise failure handling only; the actual installed
# Microsoft initializer is used for all successful environment assertions below.
$failedInitializer = Join-Path $OutputDirectory 'failed initializer.bat'
"@echo off`r`nset GREENLENS_DEV_ENV_PROBE=corrupted`r`nexit /b 17`r`n" | Set-Content -LiteralPath $failedInitializer -Encoding ascii
$wrongTargetInitializer = Join-Path $OutputDirectory 'wrong target.bat'
"@echo off`r`nset GREENLENS_DEV_ENV_PROBE=corrupted`r`nset VSCMD_VER=synthetic`r`nset VSCMD_ARG_TGT_ARCH=arm64`r`nexit /b 0`r`n" | Set-Content -LiteralPath $wrongTargetInitializer -Encoding ascii
foreach ($failure in @(
    @{path=(Join-Path $OutputDirectory 'missing initializer.bat'); message='initializer is missing'},
    @{path=$failedInitializer; message='exit 17'},
    @{path=$wrongTargetInitializer; message='required native x64 environment'}
)) {
    $caught = $null
    try { & (Join-Path $PSScriptRoot 'import-msvc-env.ps1') -VsDevCmdPath $failure.path }
    catch { $caught = $_.Exception.Message }
    if (-not $caught -or -not $caught.Contains($failure.message)) { throw 'Initializer failure was not rejected with the expected diagnosis.' }
    if ($env:GREENLENS_DEV_ENV_PROBE -cne $expectedValue -or $env:CI_EVIDENCE -cne $expectedEvidence) { throw 'A failed initializer modified the caller environment.' }
    $checks.Add('Rejects ' + $failure.message + ' without importing partial environment')
}

. (Join-Path $PSScriptRoot 'dev-env.ps1')
if ($env:CI_EVIDENCE -cne $expectedEvidence -or $env:GREENLENS_DEV_ENV_PROBE -cne $expectedValue) { throw 'Visual Studio initialization corrupted a caller environment value.' }
$checks.Add('Unicode and shell metacharacters in caller values are preserved')
if (-not (($env:PATH -split ';') -ccontains $unicodePath)) { throw 'Unicode PATH entry was lost or corrupted.' }
$checks.Add('Existing Unicode PATH entry survives native compiler initialization')
if ($env:VSCMD_ARG_TGT_ARCH -ne 'x64' -or -not (Get-Command cl.exe -ErrorAction SilentlyContinue) -or -not (Get-Command link.exe -ErrorAction SilentlyContinue)) { throw 'Native x64 compiler/linker were not configured.' }
$checks.Add('Native x64 compiler and linker remain available')
$probeDirectory = Join-Path $env:CI_EVIDENCE 'source identity 測試'
New-Item -ItemType Directory -Path $probeDirectory -Force | Out-Null
$probeFile = Join-Path $probeDirectory 'probe.txt'
[IO.File]::WriteAllText($probeFile, $expectedValue, [Text.UTF8Encoding]::new($false))
if ([IO.File]::ReadAllText($probeFile) -cne $expectedValue) { throw 'Unicode path file round-trip failed.' }
$checks.Add('The hosted failure path now supports exact file round-trip')
& (Join-Path $PSHOME 'pwsh.exe') -NoProfile -Command 'if ($env:GREENLENS_DEV_ENV_PROBE -cne ''Unicode 測試 = 42 ! %PATH% & example'') { exit 1 }'
if ($LASTEXITCODE -ne 0) { throw 'A child process did not inherit the exact Unicode environment.' }
$checks.Add('Child processes inherit unmodified Unicode values')
. (Join-Path $PSScriptRoot 'dev-env.ps1')
if ($env:CI_EVIDENCE -cne $expectedEvidence -or $env:GREENLENS_DEV_ENV_PROBE -cne $expectedValue -or -not (($env:PATH -split ';') -ccontains $unicodePath)) { throw 'Reusing the development shell corrupted its caller environment.' }
$checks.Add('Reusing the initialized shell preserves the same values')
$report = [ordered]@{schema_version='greenlens.dev-environment-checks.v1';passed=$true;checks=$checks;visual_studio_version=$env:VSCMD_VER;target_architecture=$env:VSCMD_ARG_TGT_ARCH;compiled_application=$false}
$report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'dev-env-checks.json') -Encoding utf8NoBOM
$report | ConvertTo-Json -Depth 5
