# Stage only synthetic, previously verified v0.1 evidence for native v0.2 tests.
# This copies files and records hashes; it neither launches apps nor runs commands
# from a project. Existing source evidence and earlier test directories survive.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$V01Project,
    [string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$sourceDirectory = [IO.Path]::GetFullPath($V01Project)
$sourceManifest = Join-Path $sourceDirectory 'project.json'
if (-not (Test-Path -LiteralPath $sourceManifest -PathType Leaf)) { throw 'Choose a complete saved v0.1 project directory.' }
$project = Get-Content -LiteralPath $sourceManifest -Raw | ConvertFrom-Json
if ($project.schema_version -ne '0.1.0' -or @($project.runs).Count -lt 2) { throw 'The compatibility fixture requires a v0.1 project with completed A/B runs.' }
if (@($project.runs | Where-Object alternative_id -EQ 'A').Count -eq 0 -or @($project.runs | Where-Object alternative_id -EQ 'B').Count -eq 0) { throw 'The fixture needs both A and B.' }
foreach ($run in $project.runs) {
    $parsedId = [guid]::Empty
    if (-not [guid]::TryParseExact($run.id, 'D', [ref]$parsedId) -or $run.path -ne "runs/$($run.id)") { throw 'Fixture run paths must use contained runs/<UUID> references.' }
    $provenancePath = Join-Path $sourceDirectory "$($run.path)/provenance.json"
    $provenance = Get-Content -LiteralPath $provenancePath -Raw | ConvertFrom-Json
    if ($provenance.application_version -ne '0.1.0') { throw 'Use original v0.1 application evidence for compatibility testing.' }
}
$sourceEntries = @(Get-ChildItem -LiteralPath $sourceDirectory -Force -Recurse)
if (@($sourceEntries | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count) { throw 'Do not stage linked/reparse-point project content.' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $env:LOCALAPPDATA ('GreenLens/native-verification/v0.2-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')) }
$destination = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $destination) { throw 'Native verification destination must be new.' }
if ($destination.StartsWith($sourceDirectory.TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'The test destination must not be inside source evidence.' }
$fixtureDirectory = Join-Path $destination 'v0.1 source copy 測試'
New-Item -ItemType Directory -Path $destination | Out-Null
$before = @($sourceEntries | Where-Object { -not $_.PSIsContainer } | ForEach-Object {
    @{file=[IO.Path]::GetRelativePath($sourceDirectory,$_.FullName).Replace('\','/'); sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(); bytes=$_.Length}
})
Copy-Item -LiteralPath $sourceDirectory -Destination $fixtureDirectory -Recurse
foreach ($entry in $before) {
    $copiedFile = Join-Path $fixtureDirectory $entry.file
    $sourceFile = Join-Path $sourceDirectory $entry.file
    foreach ($file in @($sourceFile,$copiedFile)) {
        if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw "Staging changed evidence: $($entry.file)" }
    }
}
$publicRecord = @{schema_version='greenlens.native-staging.v0.2'; staged_utc=[DateTime]::UtcNow.ToString('o'); fixture_application_version='0.1.0'; run_count=@($project.runs).Count; all_copied_files_identical=$true; source_files_unchanged=$true; files=$before}
$publicRecord | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $destination 'v0.1-fixture-hashes.json') -Encoding utf8
@{source_project=$sourceDirectory; fixture_project=$fixtureDirectory; evidence_directory=$destination} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination 'local-paths.json') -Encoding utf8
Write-Output "Native compatibility fixture staged without changing original evidence: $destination"
