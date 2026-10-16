#Requires -Version 7.2
# Documentation-only regression: no compilation, engine, installer or CLI execution.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/package-artifacts.ps1" -FunctionsOnly
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$fixtureRoot = Join-Path $env:LOCALAPPDATA ('GreenLens/verification/package documents ' + [Guid]::NewGuid().ToString('N') + ' 空間')
$destination = Join-Path $fixtureRoot 'package'
# Pure byte boundaries include the generated index; no large payload is needed.
$boundaryFiles = [long[]]@(2MB,2MB,2MB,2MB,2MB,2MB,2MB,(2MB - 10))
if ((Assert-PackageDocumentSizes $boundaryFiles 10) -ne 16MB) { throw 'The exact documentation total boundary was not accepted.' }
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
# The installer has a separate resource map; CLI documentation coverage alone
# cannot demonstrate that its required notices/registry will be bundled.
$tauriRoot = Join-Path $sourceRoot 'apps/desktop/src-tauri'
$tauriResources = (Get-Content -LiteralPath (Join-Path $tauriRoot 'tauri.conf.json') -Raw | ConvertFrom-Json -AsHashtable).bundle.resources
$requiredResources = [ordered]@{
    '../../../dependencies/energyplus.json'='notices/energyplus.json'
    '../../../dependencies/typst.json'='notices/typst.json'
    '../../../dependencies/energy-qualification.json'='notices/energy-qualification.json'
    '../../../docs/v0.4-energy-method.md'='notices/v0.4-energy-method.md'
    '../../../docs/v0.4-report-method.md'='notices/v0.4-report-method.md'
    '../../../reports/energy/notices/'='notices/report-renderer/'
}
foreach ($resource in $requiredResources.Keys) {
    if (!$tauriResources.ContainsKey($resource) -or $tauriResources[$resource] -cne $requiredResources[$resource]) {
        throw "Required installer resource omitted or remapped: $resource"
    }
    if (!(Test-Path -LiteralPath (Join-Path $tauriRoot $resource))) { throw "Installer resource source missing: $resource" }
}
foreach ($notice in @('Typst-LICENSE.txt','Typst-NOTICE.txt','Typst-assets-NOTICE.txt','Typst-target-NOTICES.txt','NotoSansCJK-OFL.txt')) {
    if ("reports/energy/notices/$notice" -notin $paths -or !(Test-Path -LiteralPath (Join-Path $sourceRoot "reports/energy/notices/$notice") -PathType Leaf)) {
        throw "Required renderer notice missing from CLI or installer source: $notice"
    }
}
foreach ($required in @(
    'docs/v0.6-spec.md','docs/v0.6-implementation-plan.md','docs/v0.6-acceptance.md',
    'docs/v0.6-delivery.md','docs/v0.6-reference-contracts.md','docs/v0.6-intake-licenses.md',
    'docs/evidence/v0.6/opening-baseline.json','docs/evidence/v0.6/frontend-development-checkpoint.json',
    'docs/evidence/v0.6/opening-development-checkpoint.json',
    'docs/evidence/v0.6/candidate-opening-d19.json','docs/evidence/v0.6/cold-reports-d19.json',
    'docs/evidence/v0.6/workspace-interaction-repair.json',
    'docs/evidence/v0.6/ci-aa7-success.json','docs/evidence/v0.6/cold-reports-aa7.json',
        'docs/evidence/v0.6/native-aa7-checkpoint.json',
    'docs/evidence/v0.6/report-visual-coverage-d19.json','docs/evidence/v0.6/reference-resource-d19.json',
    'docs/evidence/v0.6/replay-d19.json','docs/evidence/v0.6/ci-d19-and-93a58e9.json',
    'schemas/reference-model-v0.6.schema.json','schemas/architectural-scope-v0.6.schema.json',
    'scripts/reference-synthetic-fixture.mjs','scripts/profile-project-opening.ps1',
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
    'docs/evidence/v0.3-engine/reference-prerequisite-rejection-checkpoint.json',
    'docs/evidence/v0.3-engine/reference-prerequisite-terminal.json',
    'tests/fixtures/reference-depth/README.md','tests/fixtures/reference-depth/manifest.json',
    'tests/fixtures/reference-depth/shared/weather-asset.json',
    'tests/fixtures/reference-depth/A4116/baseline.rgb.gz','tests/fixtures/reference-depth/A4116/depth.rgb.gz',
    'tests/fixtures/reference-depth/B8512/baseline.rgb.gz','tests/fixtures/reference-depth/B8512/depth.rgb.gz',
    'docs/evidence/v0.3-native/candidate08-build-audit.json','docs/evidence/v0.3-native/candidate08-native-qualification.json',
    'docs/evidence/v0.3-native/candidate08-hourly-qualification.jpg','docs/evidence/v0.3-native/candidate08-comparison-qualification.jpg',
    'docs/v0.4-energy-spec.md','docs/v0.4-energy-usage.md','docs/v0.4-report-method.md','examples/energy/revision1-inputs.json',
    'schemas/thermal-model-v0.4.schema.json','schemas/energy-report-presentation-v0.4.schema.json',
    'dependencies/energyplus.json','dependencies/typst.json','dependencies/energy-qualification.json',
    'reports/energy/notices/Typst-target-NOTICES.txt','reports/energy/notices/NotoSansCJK-OFL.txt',
    'docs/evidence/v0.4-energy/production-timestep.json','docs/evidence/v0.4-energy/report-typst-target-inventory.json'
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
    if ($fixtureBytes -le 16MB) { throw 'The synthetic total-size fixture did not exceed its declared limit.' }
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
[ordered]@{schema_version='greenlens.package-document-check.v2';passed=$true;source_files=$paths.Count;inline_local_links=$linkCount;retained_bytes=$bytes;limits=@{document_file_bytes=2MB;documentation_total_bytes=16MB;includes_generated_index=$true};exact_total_boundary_accepted=$true;total_plus_one_rejected=$true;file_plus_one_rejected=$true;generated_index_plus_one_rejected=$true;rejected_before_destination_creation=$true;missing_link_rejected=$true;outside_path_rejected=$true;absolute_path_rejected=$true;runtime_or_engine_executed=$false} |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination 'document-check.json') -Encoding utf8
Write-Host "Documentation checks passed: $($paths.Count) exact-byte files, $linkCount local links, $bytes bytes. Retained $destination"
