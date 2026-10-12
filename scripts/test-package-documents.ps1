#Requires -Version 7.2
# Documentation-only regression: no compilation, engine, installer or CLI execution.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/package-artifacts.ps1" -FunctionsOnly
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$fixtureRoot = Join-Path $env:LOCALAPPDATA ('GreenLens/verification/package documents ' + [Guid]::NewGuid().ToString('N') + ' 空間')
$destination = Join-Path $fixtureRoot 'package'
# Pure byte boundaries include the generated index; no large payload is needed.
$boundaryFiles = [long[]]@(2MB,2MB,2MB,2MB,2MB,(2MB - 10))
if ((Assert-PackageDocumentSizes $boundaryFiles 10) -ne 12MB) { throw 'The exact documentation total boundary was not accepted.' }
foreach ($invalid in @(
    @{files=$boundaryFiles;index=11},
    @{files=[long[]]@((2MB + 1));index=0},
    @{files=[long[]]@(0);index=(2MB + 1)},
    @{files=[long[]]@(-1);index=0}
)) {
    $rejected = $false
    try { Assert-PackageDocumentSizes $invalid.files $invalid.index | Out-Null } catch { $rejected = $true }
    if (!$rejected) { throw 'An invalid documentation byte boundary was accepted.' }
}
Copy-PackageDocuments -Source $sourceRoot -Destination $destination
$paths = @(Get-PackageDocumentPaths)
foreach ($required in @(
    'docs/v0.3-reference-protocol.md','docs/v0.3-engine-investigation.md','docs/v0.3-engine-investigation.json',
    'scripts/annual-validation-plan.json','dependencies/weather.json','docs/v0.3-paired-adapter-integration.md',
    'docs/evidence/v0.3-engine/rust-primary-stream-bridge.json','docs/evidence/v0.3-engine/hammersley-candidate-transport.json',
    'schemas/primary-streams-v1.schema.json','docs/v0.3-qualification-continuation.md',
    'schemas/qualification-v1.schema.json','crates/greenlens-runner/src/qualification-registry.json',
    'docs/evidence/v0.3-ci/final-main-572f0b7.json',
    'scripts/annual-reference-prerequisite/README.md','scripts/annual-reference-prerequisite/proposal.json',
    'scripts/annual-reference-prerequisite/decision.md','scripts/annual-reference-prerequisite/runtime-pins.json',
    'scripts/annual-reference-prerequisite/guard-review.json','scripts/annual-reference-prerequisite/prepare-check.json',
    'scripts/annual-reference-prerequisite/portable-test-check.json',
    'docs/evidence/v0.3-native/candidate08-build-audit.json','docs/evidence/v0.3-native/candidate08-native-qualification.json',
    'docs/evidence/v0.3-native/candidate08-hourly-qualification.jpg','docs/evidence/v0.3-native/candidate08-comparison-qualification.jpg'
)) {
    if ($required -notin $paths) { throw "Required reference document omitted: $required" }
}
foreach ($path in $paths) {
    if ((Get-FileHash -LiteralPath (Join-Path $sourceRoot $path)).Hash -cne (Get-FileHash -LiteralPath (Join-Path $destination $path)).Hash) {
        throw "Copied documentation bytes changed: $path"
    }
}
if (@(Get-ChildItem -LiteralPath $destination -Recurse -File | Where-Object Extension -in '.exe','.dll','.zip','.oct','.mtx').Count) {
    throw 'Documentation selection unexpectedly includes a binary or large evidence container.'
}
$linkCount = Assert-PackageDocumentLinks $destination
# Deleting one known file within this fresh private fixture must fail link closure.
$protocol = Join-Path $destination 'docs/v0.3-reference-protocol.md'
Remove-Item -LiteralPath $protocol
$rejected = $false
try { Assert-PackageDocumentLinks $destination | Out-Null } catch { $rejected = $true }
if (!$rejected) { throw 'A missing linked reference protocol was silently accepted.' }
Copy-Item -LiteralPath (Join-Path $sourceRoot 'docs/v0.3-reference-protocol.md') -Destination $protocol
# An escaping link must fail even when its target exists outside the package.
$guide = Join-Path $destination 'docs/v0.3-usage.md'
$original = [IO.File]::ReadAllBytes($guide)
[IO.File]::WriteAllText((Join-Path $fixtureRoot 'outside-package.md'), 'Original synthetic packaging fixture.')
[IO.File]::AppendAllText($guide, "`n[Escaping fixture](../../outside-package.md)`n")
$rejected = $false
try { Assert-PackageDocumentLinks $destination | Out-Null } catch { $rejected = $true }
[IO.File]::WriteAllBytes($guide, $original)
if (!$rejected) { throw 'A path outside the documentation package was silently accepted.' }
[IO.File]::AppendAllText($guide, "`n[Absolute fixture](<$guide>)`n")
$rejected = $false
try { Assert-PackageDocumentLinks $destination | Out-Null } catch { $rejected = $true }
[IO.File]::WriteAllBytes($guide, $original)
if (!$rejected) { throw 'An absolute machine path was silently accepted.' }
$null = Assert-PackageDocumentLinks $destination
# Exercise the actual pre-copy boundary with only fresh private fixture files.
$oversizedTarget = Join-Path $fixtureRoot 'rejected-per-file'
$originalGuide = [IO.File]::ReadAllBytes($guide)
try {
    $stream = [IO.File]::Open($guide, [IO.FileMode]::Open, [IO.FileAccess]::Write)
    try { $stream.SetLength(2MB + 1) } finally { $stream.Dispose() }
    $rejected = $false
    try { Copy-PackageDocuments $destination $oversizedTarget } catch { $rejected = $true }
    if (!$rejected -or (Test-Path -LiteralPath $oversizedTarget)) { throw 'Oversized file was not rejected before destination creation.' }
} finally { [IO.File]::WriteAllBytes($guide, $originalGuide) }
$totalTarget = Join-Path $fixtureRoot 'rejected-total'
$images = @($paths | Where-Object { $_.EndsWith('.png') } | Select-Object -First 3)
$originalImages = @{}
try {
    foreach ($relative in $images) {
        $file = Join-Path $destination $relative
        $originalImages[$relative] = [IO.File]::ReadAllBytes($file)
        $stream = [IO.File]::Open($file, [IO.FileMode]::Open, [IO.FileAccess]::Write)
        try { $stream.SetLength(2MB) } finally { $stream.Dispose() }
    }
    $fixtureBytes = ($paths | ForEach-Object { (Get-Item -LiteralPath (Join-Path $destination $_)).Length } | Measure-Object -Sum).Sum
    if ($fixtureBytes -le 12MB) { throw 'The synthetic total-size fixture did not exceed its declared limit.' }
    $rejected = $false
    try { Copy-PackageDocuments $destination $totalTarget } catch { $rejected = $true }
    if (!$rejected -or (Test-Path -LiteralPath $totalTarget)) { throw 'Oversized total was not rejected before destination creation.' }
} finally {
    foreach ($relative in $originalImages.Keys) { [IO.File]::WriteAllBytes((Join-Path $destination $relative), $originalImages[$relative]) }
}
foreach ($path in $paths) {
    if ((Get-FileHash -LiteralPath (Join-Path $sourceRoot $path)).Hash -cne (Get-FileHash -LiteralPath (Join-Path $destination $path)).Hash) { throw "Restored fixture bytes changed: $path" }
}
$bytes = (Get-ChildItem -LiteralPath $destination -Recurse -File | Measure-Object Length -Sum).Sum
[ordered]@{schema_version='greenlens.package-document-check.v1';passed=$true;source_files=$paths.Count;inline_local_links=$linkCount;retained_bytes=$bytes;limits=@{document_file_bytes=2MB;documentation_total_bytes=12MB;includes_generated_index=$true};exact_total_boundary_accepted=$true;total_plus_one_rejected=$true;file_plus_one_rejected=$true;generated_index_plus_one_rejected=$true;rejected_before_destination_creation=$true;missing_link_rejected=$true;outside_path_rejected=$true;absolute_path_rejected=$true;runtime_or_engine_executed=$false} |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination 'document-check.json') -Encoding utf8
Write-Host "Documentation checks passed: $($paths.Count) exact-byte files, $linkCount local links, $bytes bytes. Retained $destination"
