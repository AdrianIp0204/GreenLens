# Required, entirely local provider/portable-storage verification. No network or
# Radiance process is started. Missing reviewed input or CLI is a hard failure.
[CmdletBinding()]
param([string]$Cli, [string]$OutputDirectory, [switch]$FunctionsOnly)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
function Assert-ReviewedWeatherBinding($Binding, $Dataset) {
    foreach ($field in @('original_sha256','normalized_sha256','time_axis_sha256')) {
        $expected = $Dataset.$field
        $actual = $Binding.$field
        if ($expected -isnot [string] -or $expected -cnotmatch '^[0-9a-f]{64}$') { throw "Reviewed weather inventory has no valid $field." }
        if ($actual -isnot [string] -or $actual -cne $expected) { throw "Required weather $field differs from the reviewed dependency inventory." }
    }
    if ($Binding.interpretation -cne $Dataset.interpretation) { throw 'Required weather interpretation differs from the reviewed dependency inventory.' }
}
# Only the explicit script regression uses this entry point. It produces no
# acceptance summary and does not count as a provider verification run.
if ($FunctionsOnly) { return }
if (-not $Cli) {
    if (-not $env:CARGO_TARGET_DIR) { throw 'Supply -Cli or dot-source scripts/dev-env.ps1 first.' }
    $Cli = Join-Path $env:CARGO_TARGET_DIR 'debug/greenlens.exe'
}
if (-not (Test-Path -LiteralPath $Cli -PathType Leaf)) { throw 'Required CLI unavailable; build greenlens-cli or supply -Cli. Verification was not executed.' }
$Cli = [IO.Path]::GetFullPath($Cli)
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$source = Join-Path $repository 'examples/weather/pvgis-hong-kong-tmy-2005-2023.epw'
$sourceRecord = Join-Path $repository 'examples/weather/pvgis-hong-kong.source.json'
$inventory = Get-Content -Raw -LiteralPath (Join-Path $repository 'dependencies/weather.json') | ConvertFrom-Json
$expectedHash = $inventory.datasets[0].original_sha256
if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw 'Required reviewed real EPW is unavailable; verification is failed, not skipped.' }
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $source).Hash.ToLowerInvariant() -ne $expectedHash) { throw 'Reviewed source digest differs from the dependency inventory.' }
if (-not (Test-Path -LiteralPath $sourceRecord -PathType Leaf)) { throw 'Required weather source/rights record unavailable.' }
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $env:LOCALAPPDATA ("GreenLens/verification/Weather 空間 " + [guid]::NewGuid().ToString())
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a fresh verification output directory; existing work is never replaced.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
function Assert-OwnedPath([string]$Path) {
    $absolute = [IO.Path]::GetFullPath($Path)
    if (-not $absolute.StartsWith($OutputDirectory + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Verification path escaped its owned output directory.' }
    return $absolute
}
function Invoke-Weather {
    param([string[]]$Arguments, [string]$Name, [switch]$MustFail, [string]$ExpectedError)
    $stdoutFile = Assert-OwnedPath (Join-Path $OutputDirectory "$Name.stdout.json")
    $stderrFile = Assert-OwnedPath (Join-Path $OutputDirectory "$Name.stderr.txt")
    $stdout = & $Cli @Arguments 2> $stderrFile
    $exit = $LASTEXITCODE
    $text = $stdout -join "`n"
    [IO.File]::WriteAllText($stdoutFile, $text, [Text.UTF8Encoding]::new($false))
    if ($MustFail) {
        if ($exit -eq 0) { throw "Expected $Name to fail, but it succeeded." }
        if ($ExpectedError -and (Get-Content -Raw -LiteralPath $stderrFile) -notmatch $ExpectedError) { throw "$Name did not report the expected actionable error." }
        return
    }
    if ($exit -ne 0) { throw "$Name failed with exit $exit. Inspect retained stderr." }
    return ($text | ConvertFrom-Json)
}
function File-Map([string]$Directory) {
    $result = @{}
    Get-ChildItem -LiteralPath $Directory -File | ForEach-Object {
        $result[$_.Name] = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
    }
    return $result
}
function Assert-SameFiles($Left, $Right) {
    if ($Left.Count -ne $Right.Count) { throw 'Relocated weather file count differs.' }
    foreach ($name in $Left.Keys) { if ($Left[$name] -ne $Right[$name]) { throw "Relocated weather bytes differ: $name" } }
}
$inputCopy = Assert-OwnedPath (Join-Path $OutputDirectory 'Imported source 空間.epw')
$asset = Assert-OwnedPath (Join-Path $OutputDirectory 'Original weather asset')
$relocated = Assert-OwnedPath (Join-Path $OutputDirectory 'Relocated weather 空間')
$hiddenInput = Assert-OwnedPath (Join-Path $OutputDirectory 'unavailable-original-input.epw')
$hiddenAsset = Assert-OwnedPath (Join-Path $OutputDirectory 'unavailable-original-asset')
Copy-Item -LiteralPath $source -Destination $inputCopy
$inspection = Invoke-Weather -Arguments @('weather','inspect',$inputCopy,'--source-profile','pvgis-tmy') -Name 'inspect'
Assert-ReviewedWeatherBinding $inspection.binding $inventory.datasets[0]
if ($inspection.summary.intervals -ne 8760 -or $inspection.binding.original_sha256 -ne $expectedHash) { throw 'Inspection did not retain the reviewed full-year source identity.' }
$imported = Invoke-Weather -Arguments @('weather','import',$inputCopy,$asset,'--source-profile','pvgis-tmy','--source-record',$sourceRecord) -Name 'import'
Assert-ReviewedWeatherBinding $imported.binding $inventory.datasets[0]
if ($inspection.binding.normalized_sha256 -ne $imported.binding.normalized_sha256) { throw 'Import differs from inspected normalization.' }
$before = File-Map $asset
Copy-Item -LiteralPath $asset -Destination $relocated -Recurse
Assert-SameFiles $before (File-Map $relocated)
# Only this verification's owned copy and asset are moved. The checked-in source,
# engine caches and all other running calculations remain untouched.
Move-Item -LiteralPath $inputCopy -Destination $hiddenInput
Move-Item -LiteralPath $asset -Destination $hiddenAsset
try {
    if ((Test-Path -LiteralPath $inputCopy) -or (Test-Path -LiteralPath $asset)) { throw 'Original import paths remained available.' }
    $reopened = Invoke-Weather -Arguments @('weather','verify',$relocated) -Name 'relocated-verify'
    Assert-ReviewedWeatherBinding $reopened.binding $inventory.datasets[0]
    if (($reopened | ConvertTo-Json -Depth 50 -Compress) -ne ($imported | ConvertTo-Json -Depth 50 -Compress)) { throw 'Relocated inspection/identity differs.' }
    Assert-SameFiles $before (File-Map $relocated)
    $assetRecord = Get-Content -Raw -LiteralPath (Join-Path $relocated 'asset.json') | ConvertFrom-Json
    if ($assetRecord.source.rights_note -notmatch 'Copernicus' -or $assetRecord.source.citation -notmatch 'PVGIS') { throw 'Portable weather source attribution was not retained.' }
    Invoke-Weather -Arguments @('weather','inspect',$inputCopy,'--source-profile','pvgis-tmy') -Name 'missing-original-rejected' -MustFail -ExpectedError 'Read local weather file'
} finally {
    if (-not (Test-Path -LiteralPath $inputCopy)) { Move-Item -LiteralPath $hiddenInput -Destination $inputCopy }
    if (-not (Test-Path -LiteralPath $asset)) { Move-Item -LiteralPath $hiddenAsset -Destination $asset }
}
$corrupt = Assert-OwnedPath (Join-Path $OutputDirectory 'Corrupt weather copy')
Copy-Item -LiteralPath $relocated -Destination $corrupt -Recurse
[IO.File]::AppendAllText((Join-Path $corrupt 'original.epw'), 'tampered', [Text.UTF8Encoding]::new($false))
Invoke-Weather -Arguments @('weather','verify',$corrupt) -Name 'checksum-corruption-rejected' -MustFail -ExpectedError 'checksum|digest|integrity'
$unsupported = Assert-OwnedPath (Join-Path $OutputDirectory 'Unsupported offset.epw')
$modified = [IO.File]::ReadAllText($source).Replace('Irradiance Time Offset (h):-0.5','Irradiance Time Offset (h):-0.25')
[IO.File]::WriteAllText($unsupported, $modified, [Text.UTF8Encoding]::new($false))
Invoke-Weather -Arguments @('weather','inspect',$unsupported,'--source-profile','pvgis-tmy') -Name 'unsupported-offset-rejected' -MustFail -ExpectedError 'irradiance_offset'
$duplicate = Assert-OwnedPath (Join-Path $OutputDirectory 'Conflicting duplicate offset.epw')
$modified = [IO.File]::ReadAllText($source).Replace('Irradiance Time Offset (h):-0.5','Irradiance Time Offset (h):-0.5, Irradiance Time Offset (h):0.5')
[IO.File]::WriteAllText($duplicate, $modified, [Text.UTF8Encoding]::new($false))
Invoke-Weather -Arguments @('weather','inspect',$duplicate,'--source-profile','pvgis-tmy') -Name 'duplicate-offset-rejected' -MustFail -ExpectedError 'Duplicate irradiation offset'
@{
    schema_version = 'greenlens.weather-cli-verification.v0.3'
    verified_utc = [DateTime]::UtcNow.ToString('o')
    platform = 'native Windows'
    passed = $true
    cli_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Cli).Hash.ToLowerInvariant()
    source_sha256 = $expectedHash
    intervals = 8760
    binding = $reopened.binding
    portable_files_sha256 = $before
    checks = @('required reviewed full-year EPW', 'original/normalized/time-axis hashes and interpretation match reviewed manifest at inspect/import/reopen', 'explicit profile inspection', 'portable import with attribution', 'byte-identical relocation', 'reopen with original import paths unavailable', 'no engine/network service required', 'missing original rejected', 'checksum corruption rejected', 'unsupported offset rejected', 'same-line conflicting offsets rejected')
    simulation_acceptance = $false
} | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8
Write-Output "Weather provider verification passed. Evidence: $OutputDirectory"
