[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if (-not $env:CARGO_TARGET_DIR) { throw 'Dot-source scripts/dev-env.ps1 before packaging.' }
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $env:LOCALAPPDATA ("GreenLens/releases/0.1.0/" + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')) }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Artifact destination must be new; previous evidence is preserved.' }
$cli = Join-Path $env:CARGO_TARGET_DIR 'release/greenlens.exe'
$installers = @(Get-ChildItem -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'release/bundle/nsis') -Filter '*-setup.exe')
if (-not (Test-Path -LiteralPath $cli) -or $installers.Count -eq 0) { throw 'Build the release CLI and NSIS package first.' }
$inventory = Get-Content -LiteralPath (Join-Path $sourceRoot 'dependencies/resolved-inventory.json') -Raw | ConvertFrom-Json
if ($inventory.scope -like 'PARTIAL*' -or @($inventory.review_gaps).Count -ne 0) { throw 'Complete the strict dependency review before distributing artifacts.' }
foreach ($lock in @('Cargo.lock','apps/desktop/package-lock.json')) {
    $actual = (Get-FileHash -LiteralPath (Join-Path $sourceRoot $lock) -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($inventory.lockfiles.$lock -ne $actual) { throw "Dependency inventory is stale for $lock" }
}
# The inventory is copied as evidence; the strict generator must be run as a
# separate required release check after all lockfiles have settled.
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$cliDirectory = Join-Path $OutputDirectory 'GreenLens-CLI-0.1.0-windows-x64'
New-Item -ItemType Directory -Path $cliDirectory | Out-Null
Copy-Item -LiteralPath $cli -Destination (Join-Path $cliDirectory 'greenlens.exe')
foreach ($file in @('LICENSE','NOTICE')) { Copy-Item -LiteralPath (Join-Path $sourceRoot $file) -Destination (Join-Path $cliDirectory $file) }
Copy-Item -LiteralPath (Join-Path $sourceRoot 'third-party') -Destination (Join-Path $cliDirectory 'third-party') -Recurse
New-Item -ItemType Directory -Path (Join-Path $cliDirectory 'dependencies'),(Join-Path $cliDirectory 'docs') | Out-Null
Copy-Item -LiteralPath (Join-Path $sourceRoot 'dependencies/resolved-inventory.json') -Destination (Join-Path $cliDirectory 'dependencies/resolved-inventory.json')
Copy-Item -LiteralPath (Join-Path $sourceRoot 'dependencies/radiance.json') -Destination (Join-Path $cliDirectory 'dependencies/radiance.json')
Copy-Item -LiteralPath (Join-Path $sourceRoot 'dependencies/license-supplements.json') -Destination (Join-Path $cliDirectory 'dependencies/license-supplements.json')
foreach ($file in @('installation.md','replay.md','model-contract.md','scientific-validation.md','dependencies.md','windows-runtime.md','windows-installer.md')) { Copy-Item -LiteralPath (Join-Path $sourceRoot "docs/$file") -Destination (Join-Path $cliDirectory "docs/$file") }
Copy-Item -LiteralPath (Join-Path $sourceRoot 'examples') -Destination (Join-Path $cliDirectory 'examples') -Recurse
Compress-Archive -LiteralPath $cliDirectory -DestinationPath (Join-Path $OutputDirectory 'GreenLens-CLI-0.1.0-windows-x64.zip')
foreach ($installer in $installers) { Copy-Item -LiteralPath $installer.FullName -Destination (Join-Path $OutputDirectory $installer.Name) }
$hashes = Get-ChildItem -LiteralPath $OutputDirectory -File | ForEach-Object { @{file=$_.Name; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()} }
@{schema_version='0.1.0'; generated_utc=[DateTime]::UtcNow.ToString('o'); signed=$false; artifacts=@($hashes)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'checksums.json') -Encoding utf8
Write-Output "Unsigned artifacts with retained notices: $OutputDirectory"
