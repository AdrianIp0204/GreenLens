[CmdletBinding()]
param([string]$OutputDirectory, [switch]$FunctionsOnly)
$ErrorActionPreference = 'Stop'

function Get-PackageDocumentPaths {
    # Explicit compact evidence only: no directory-wide evidence/archive copies.
    @(
        'docs/installation.md','docs/replay.md','docs/persistence.md','docs/model-contract.md',
        'docs/scientific-validation.md','docs/dependencies.md','docs/windows-runtime.md',
        'docs/windows-installer.md','docs/workspace-guide.md','docs/rhino-interaction-matrix.md',
        'docs/v0.2-native-verification.md','docs/images/v0.2/README.md','docs/v0.3-spec.md',
        'docs/v0.3-implementation-plan.md','docs/v0.3-scientific-method.md',
        'docs/v0.3-weather-sources.md','docs/v0.3-contracts.md','docs/v0.3-usage.md','docs/v0.3-status.md',
        'docs/v0.3-native-verification.md','docs/v0.3-ci-package-validation.md','docs/progress.md',
        'docs/v0.3-reference-protocol.md','docs/v0.3-engine-investigation.md',
        'docs/v0.3-paired-adapter-integration.md','docs/v0.3-paired-hammersley-investigation.md',
        'docs/v0.3-stream-supervision.md','docs/v0.3-paired-process-supervision.md',
        'docs/v0.3-aperture-primary-quadrature.md',
        'docs/evidence/v0.3-engine/rust-primary-stream-bridge.json',
        'docs/evidence/v0.3-engine/hammersley-primary-stream.json',
        'docs/evidence/v0.3-engine/hammersley-coupling.json',
        'docs/evidence/v0.3-engine/hammersley-candidate-transport.json',
        'docs/evidence/v0.3-engine/hammersley-higher-scheduling-cancellation.json',
        'docs/evidence/v0.3-engine/aperture-primary-quadrature.json',
        'docs/evidence/v0.3-engine/aperture-primary-inputs/A-model.json',
        'docs/evidence/v0.3-engine/aperture-primary-inputs/B-model.json',
        'docs/evidence/v0.3-engine/aperture-primary-inputs/sensors.pts',
        'docs/evidence/v0.3-engine/paired-process-supervision.json',
        'scripts/annual-paired-grid.cs','scripts/annual-paired-native-process.cs',
        'scripts/annual-paired-process.ps1',
        'docs/v0.3-engine-investigation.json','scripts/annual-validation-plan.json',
        'scripts/annual-high-ad-uniform.ps1','scripts/annual-paired-primary.ps1',
        'scripts/annual-residual-audit.py',
        'docs/evidence/v0.3-native/candidate04-artifacts.json',
        'docs/evidence/v0.3-native/candidate04-historical-inspection.json',
        'docs/evidence/v0.3-native/candidate04-conflict-recovery.json',
        'docs/evidence/v0.3-native/candidate04-mixed-replay.json',
        'docs/evidence/v0.3-native/candidate06-loading-regression.json',
        'docs/evidence/v0.3-native/candidate07-build-audit.json',
        'docs/evidence/v0.3-native/candidate07-package-review.json',
        'docs/evidence/v0.3-native/candidate07-native-regression.json',
        'docs/evidence/v0.3-native/candidate07-bounded-manifest-rejected.png',
        'docs/evidence/v0.3-native/candidate07-restored-manifest-save.png',
        'docs/evidence/v0.3-native/candidate07-three-run-exported.png',
        'docs/evidence/v0.3-native/candidate07-layout-observation.json',
        'docs/evidence/v0.3-native/candidate07-maximized-diagnostics.png',
        'docs/evidence/v0.3-native/candidate07-compact-table-diagnostics.png',
        'docs/evidence/v0.3-engine/higher-MF2-MF4-A-all12-summary.json',
        'docs/evidence/v0.3-engine/higher-MF2-B-all12-summary.json',
        'docs/evidence/v0.3-engine/MF8-A-all12-and-bins-summary.json',
        'docs/evidence/v0.3-engine/final24-baseline-qualification.json',
        'docs/evidence/v0.3-engine/reference-A-depth-components.json',
        'docs/evidence/v0.3-engine/reference-A-depth-prefix-diagnostic.json',
        'docs/evidence/v0.3-engine/final-six-reference-sensitivities.json',
        'docs/evidence/v0.3-engine/reference-B-depth-components.json',
        'docs/evidence/v0.3-engine/reference-B-depth-prefix-diagnostic.json',
        'docs/evidence/v0.3-engine/reference-prefix-identity-regression.json',
        'docs/evidence/v0.3-engine/historical-explicit-hybrid-summary.json',
        'docs/evidence/v0.3-engine/fixed-phase-control-summary.json',
        'docs/evidence/v0.3-engine/explicit-source-control-summary.json',
        'docs/evidence/v0.3-engine/sky-time-block-four-case-summary.json',
        'docs/evidence/v0.3-engine/overlap-horizon-controls.json',
        'docs/evidence/v0.3-engine/horizon-moment-planar-controls.json',
        'docs/evidence/v0.3-engine/canonical128-moment-room-controls.json',
        'docs/evidence/v0.3-engine/moment-angular-feasibility.json','scripts/angular_feasibility.py',
        'docs/evidence/v0.3-engine/fixed-depth-order-controls.json',
        'docs/v0.3-depth-order-diagnostic.md',
        'docs/v0.3-permutation-investigation.md',
        'docs/evidence/v0.3-engine/fixed-reference-permutation.json',
        'scripts/permutation-proposal.json','scripts/permutation-plan.json',
        '.github/workflows/reference-permutation.yml',
        'docs/evidence/v0.3-ci/permutation-pre-dispatch/review-decision.json',
        'docs/evidence/v0.3-engine/historical-finite-recovery-audit.json',
        'docs/evidence/v0.3-engine/MF8-annual-size-lower-bound.json',
        'docs/evidence/v0.3-engine/hosted-twelve-case-qualification.json',
        'docs/evidence/v0.3-ci/final-main-596fde6.json',
        'docs/evidence/v0.3-ci/final-main-082ce64.json',
        'docs/evidence/v0.3-ci/final-main-890166b.json',
        'docs/evidence/v0.3-ci/final-main-19f0bbe.json',
        'docs/evidence/v0.3-ci/final-main-5cde6dc.json',
        'docs/evidence/v0.3-native/candidate06-selected-hour-observation.json',
        'docs/evidence/v0.3-native/candidate06-selected-hour-audit.json',
        'docs/evidence/v0.3-native/candidate06-selected-hour-exported.png',
        'docs/evidence/v0.3-native/candidate06-selected-hour-study-undo-annual.png',
        'docs/evidence/v0.3-native/candidate06-selected-hour-study-redo.png',
        'docs/evidence/v0.3-integration/bounded-manifest-read.json',
        'docs/evidence/v0.3-integration/build-provenance-worktree.json',
        'docs/evidence/v0.3-integration/candidate06-selected-hour-replay.json',
        'docs/evidence/v0.3-engine/hosted-four-A-qualification.json',
        'docs/evidence/v0.3-engine/hosted-three-autumn-A-qualification.json',
        'docs/evidence/v0.3-native/candidate06-save-retains-result.png',
        'docs/evidence/v0.3-native/candidate06-retained-series-1912.png',
        'docs/evidence/v0.3-native/candidate06-hour-request-loading.png',
        'docs/evidence/v0.3-native/candidate06-hour24-retained-context.png',
        'docs/evidence/v0.3-native/candidate06-save-busy-retains-series.png',
        'docs/evidence/v0.3-ci/method5-6cf036e-convergence-independent.json',
        'docs/evidence/v0.3-engine/method5-higher-MF2-A-partial-audit.json',
        'docs/evidence/v0.3-native/candidate05-installed-binary-audit.json',
        'docs/evidence/v0.3-native/candidate05-cancellation.json',
        'docs/evidence/v0.3-native/candidate05-camera-after-A.json',
        'docs/evidence/v0.3-native/candidate05-weather-import.png',
        'docs/evidence/v0.3-native/candidate05-annual-cancelled.png',
        'docs/evidence/v0.3-native/candidate05-annual-A-completed.png',
        'docs/evidence/v0.3-native/candidate05-annual-B-completed.png',
        'docs/evidence/v0.3-native/candidate05-annual-cda600-current-vs-history.png',
        'docs/evidence/v0.3-native/candidate05-annual-cold-export.png',
        'docs/evidence/v0.3-native/candidate05-annual-cold-reopen.png',
        'docs/evidence/v0.3-native/candidate05-annual-sensor1912.png',
        'docs/evidence/v0.3-native/candidate05-annual-window-edit-stale.png',
        'docs/evidence/v0.3-native/candidate05-annual-window-undo-current.png',
        'docs/evidence/v0.3-native/candidate05-annual-integrity.json',
        'docs/evidence/v0.3-native/candidate05-copy-integrity.json',
        'docs/evidence/v0.3-native/candidate05-currentness-camera.json',
        'docs/evidence/v0.3-native/candidate05-compact-layout.json',
        'docs/evidence/v0.3-native/candidate05-compact-diagnostics.png',
        'docs/evidence/v0.3-native/candidate05-artifact-build-license.json',
        'docs/evidence/v0.3-native/candidate05-package-members.json',
        'docs/evidence/v0.3-integration/method5-selected-hour-replay-audit.json',
        'docs/evidence/v0.3-integration/method5-selected-hour-export-members.json',
        'docs/evidence/v0.3-integration/method5-legacy-static-replay-audit.json',
        'docs/evidence/v0.3-integration/method5-native-annual-replay-audit.json',
        'docs/evidence/v0.3-engine/method5-selected-hour-independent-audit.json',
        'docs/evidence/v0.3-engine/controlled-reference-winter8504-timeouts.json',
        'docs/evidence/v0.3-engine/controlled-reference-spring-summer-timeouts.json',
        'docs/evidence/v0.3-ci/historical-method4-and-pilot-checkpoint.json',
        'docs/evidence/v0.3-ci/method5-and-qualified-pilot-1417.json',
        'docs/evidence/v0.3-ci/method5-main-and-seventeen-reference-startup.json',
        'docs/evidence/v0.3-ci/method5-package-replay-checkpoint-manifest.json',
        'docs/evidence/v0.3-engine/hosted-A8504-qualified-audit.json',
        'docs/v0.3-derived-metric-verification.md',
        'docs/evidence/v0.3-native/candidate04-derived-metrics.json',
        'docs/profile-investigation.json','docs/engine-feasibility.md',
        'docs/gate-0-evidence.json','docs/science-fixtures-evidence.json','docs/science-evidence.json',
        'docs/evidence/v0.3-engine/method2-actual-ab-twelve-hours-failed.json',
        'docs/evidence/v0.3-engine/explicit-actual-ab-diffuse262144-solar8192.json',
        'docs/evidence/v0.3-engine/explicit-actual-ab-diffuse1048576-solar8192.json',
        'docs/evidence/v0.3-engine/high-sampling-uniform-diagnostics.json',
        'docs/evidence/v0.3-engine/accumulation-receiver-point-diagnostics.json',
        'docs/evidence/v0.3-engine/scaled-reference-point-failed.json',
        'docs/evidence/v0.3-engine/path-weight-point-diagnostic.json',
        'docs/evidence/v0.3-engine/moderate-two-sensor-transport-pilot.json',
        'docs/evidence/v0.3-engine/method4-annual-negative-residuals.json',
        'docs/evidence/v0.3-engine/paired-primary-coupling.json',
        'dependencies/radiance.json','dependencies/weather.json',
        'schemas/primary-streams-v1.schema.json',
        'third-party/Radiance-LICENSE.txt','third-party/Radiance-gendaymtx-NOTICE.txt'
    )
}

function Assert-PackageDocumentLinks([string]$Root) {
    $packageRoot = [IO.Path]::GetFullPath($Root)
    $prefix = $packageRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    $checked = 0
    foreach ($file in (Get-PackageDocumentPaths | Where-Object { $_.EndsWith('.md') })) {
        $path = Join-Path $packageRoot $file
        foreach ($match in [regex]::Matches([IO.File]::ReadAllText($path), '!?\[[^\]]*\]\((?:<)?([^)>]+)(?:>)?\)')) {
            $reference = ($match.Groups[1].Value -split '#', 2)[0]
            if (!$reference) { continue }
            if ([IO.Path]::IsPathRooted($reference)) { throw "Packaged document has an absolute local path: $file" }
            if ($reference -match '^[a-zA-Z][a-zA-Z0-9+.-]*:') { continue }
            $target = [IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $path) $reference))
            if (!$target.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or !(Test-Path -LiteralPath $target -PathType Leaf)) {
                throw "Packaged document link is missing or outside the package: $file -> $reference"
            }
            $checked++
        }
    }
    return $checked
}

function Copy-PackageDocuments([string]$Source, [string]$Destination) {
    $paths = @(Get-PackageDocumentPaths)
    $bytes = 0L
    foreach ($path in $paths) {
        $file = Get-Item -LiteralPath (Join-Path $Source $path)
        if ($file.PSIsContainer -or $file.Length -gt 2MB) { throw "Expected a compact documentation file: $path" }
        $bytes += $file.Length
    }
    if ($bytes -gt 8MB) { throw 'Review the explicit documentation selection before exceeding 8 MiB.' }
    foreach ($path in $paths) {
        $destinationPath = Join-Path $Destination $path
        New-Item -ItemType Directory -Path (Split-Path -Parent $destinationPath) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $Source $path) -Destination $destinationPath
    }
    $links = Assert-PackageDocumentLinks $Destination
    @"
# Packaged documentation

The selected guides, method/reference declarations, data inventories and compact linked numerical records are included for offline reading. Local inline Markdown links in these guides were checked before packaging ($links links).

This is not a source checkout or a complete historical evidence archive. Backtick command examples, source-only verification scripts, historical screenshot filenames, upstream URLs and local build/output paths can require the repository or separately retained artifacts. The linked research script is retained as readable source; its development dependencies are not a runtime requirement or a bundled verification toolchain. Large matrices, octrees, evidence ZIPs and engine binaries are excluded.
"@ | Set-Content -LiteralPath (Join-Path $Destination 'docs/PACKAGE-CONTENTS.md') -Encoding utf8
}

if ($FunctionsOnly) { return }
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
Copy-Item -LiteralPath (Join-Path $sourceRoot 'dependencies/license-supplements.json') -Destination (Join-Path $cliDirectory 'dependencies/license-supplements.json')
Copy-Item -LiteralPath (Join-Path $sourceRoot 'examples') -Destination (Join-Path $cliDirectory 'examples') -Recurse
Copy-PackageDocuments -Source $sourceRoot -Destination $cliDirectory
Compress-Archive -LiteralPath $cliDirectory -DestinationPath (Join-Path $OutputDirectory "GreenLens-CLI-$applicationVersion-windows-x64.zip")
foreach ($installer in $installers) { Copy-Item -LiteralPath $installer.FullName -Destination (Join-Path $OutputDirectory $installer.Name) }
$hashes = Get-ChildItem -LiteralPath $OutputDirectory -File | ForEach-Object { @{file=$_.Name; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()} }
@{schema_version='0.1.0'; application_version=$applicationVersion; generated_utc=[DateTime]::UtcNow.ToString('o'); signed=$false; artifacts=@($hashes)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'checksums.json') -Encoding utf8
Write-Output "Unsigned artifacts with retained notices: $OutputDirectory"
