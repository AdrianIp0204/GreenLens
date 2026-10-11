[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if (-not $env:CARGO_TARGET_DIR) { throw 'Dot-source scripts/dev-env.ps1 before packaging.' }
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$applicationVersion = (Get-Content -LiteralPath (Join-Path $sourceRoot 'apps/desktop/package.json') -Raw | ConvertFrom-Json).version
if ($applicationVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Expected an explicit stable application version.' }
$tauriVersion = (Get-Content -LiteralPath (Join-Path $sourceRoot 'apps/desktop/src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
if ($tauriVersion -ne $applicationVersion) { throw 'Tauri and frontend application versions differ.' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $env:LOCALAPPDATA ("GreenLens/releases/$applicationVersion/" + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')) }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Artifact destination must be new; previous evidence is preserved.' }
$cli = Join-Path $env:CARGO_TARGET_DIR 'release/greenlens.exe'
$installers = @(Get-ChildItem -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'release/bundle/nsis') -Filter "GreenLens_${applicationVersion}_x64-setup.exe")
if (-not (Test-Path -LiteralPath $cli) -or $installers.Count -eq 0) { throw 'Build the release CLI and NSIS package first.' }
$cliVersion = & $cli --version
if ($LASTEXITCODE -ne 0 -or $cliVersion -notmatch "^greenlens-cli $([regex]::Escape($applicationVersion))$") { throw 'The CLI executable does not match the application release version.' }
$inventory = Get-Content -LiteralPath (Join-Path $sourceRoot 'dependencies/resolved-inventory.json') -Raw | ConvertFrom-Json
if ($inventory.scope -like 'PARTIAL*' -or @($inventory.review_gaps).Count -ne 0) { throw 'Complete the strict dependency review before distributing artifacts.' }
foreach ($lock in @('Cargo.lock','apps/desktop/package-lock.json')) {
    $actual = (Get-FileHash -LiteralPath (Join-Path $sourceRoot $lock) -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($inventory.lockfiles.$lock -ne $actual) { throw "Dependency inventory is stale for $lock" }
}
# The inventory is copied as evidence; the strict generator must be run as a
# separate required release check after all lockfiles have settled.
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$cliDirectory = Join-Path $OutputDirectory "GreenLens-CLI-$applicationVersion-windows-x64"
New-Item -ItemType Directory -Path $cliDirectory | Out-Null
Copy-Item -LiteralPath $cli -Destination (Join-Path $cliDirectory 'greenlens.exe')
foreach ($file in @('LICENSE','NOTICE')) { Copy-Item -LiteralPath (Join-Path $sourceRoot $file) -Destination (Join-Path $cliDirectory $file) }
Copy-Item -LiteralPath (Join-Path $sourceRoot 'third-party') -Destination (Join-Path $cliDirectory 'third-party') -Recurse
New-Item -ItemType Directory -Path (Join-Path $cliDirectory 'dependencies'),(Join-Path $cliDirectory 'docs') | Out-Null
Copy-Item -LiteralPath (Join-Path $sourceRoot 'dependencies/resolved-inventory.json') -Destination (Join-Path $cliDirectory 'dependencies/resolved-inventory.json')
Copy-Item -LiteralPath (Join-Path $sourceRoot 'dependencies/radiance.json') -Destination (Join-Path $cliDirectory 'dependencies/radiance.json')
Copy-Item -LiteralPath (Join-Path $sourceRoot 'dependencies/weather.json') -Destination (Join-Path $cliDirectory 'dependencies/weather.json')
Copy-Item -LiteralPath (Join-Path $sourceRoot 'dependencies/license-supplements.json') -Destination (Join-Path $cliDirectory 'dependencies/license-supplements.json')
foreach ($file in @('installation.md','replay.md','model-contract.md','scientific-validation.md','dependencies.md','windows-runtime.md','windows-installer.md','workspace-guide.md','rhino-interaction-matrix.md','v0.2-native-verification.md','v0.3-spec.md','v0.3-implementation-plan.md','v0.3-scientific-method.md','v0.3-weather-sources.md','v0.3-contracts.md','v0.3-usage.md','progress.md')) { Copy-Item -LiteralPath (Join-Path $sourceRoot "docs/$file") -Destination (Join-Path $cliDirectory "docs/$file") }
Copy-Item -LiteralPath (Join-Path $sourceRoot 'examples') -Destination (Join-Path $cliDirectory 'examples') -Recurse
Compress-Archive -LiteralPath $cliDirectory -DestinationPath (Join-Path $OutputDirectory "GreenLens-CLI-$applicationVersion-windows-x64.zip")
foreach ($installer in $installers) { Copy-Item -LiteralPath $installer.FullName -Destination (Join-Path $OutputDirectory $installer.Name) }
$hashes = Get-ChildItem -LiteralPath $OutputDirectory -File | ForEach-Object { @{file=$_.Name; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()} }
@{schema_version='0.1.0'; application_version=$applicationVersion; generated_utc=[DateTime]::UtcNow.ToString('o'); signed=$false; artifacts=@($hashes)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'checksums.json') -Encoding utf8
Write-Output "Unsigned artifacts with retained notices: $OutputDirectory"
