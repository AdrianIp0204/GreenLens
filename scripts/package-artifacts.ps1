[CmdletBinding()]
param([string]$OutputDirectory, [switch]$FunctionsOnly)
$ErrorActionPreference = 'Stop'

function Get-PackageDocumentPaths {
    # Explicit compact evidence only: no directory-wide evidence/archive copies.
    @(
        'docs/v0.6-spec.md','docs/v0.6-implementation-plan.md','docs/v0.6-acceptance.md',
        'docs/v0.6-reference-contracts.md','docs/v0.6-intake-licenses.md','docs/v0.6-delivery.md',
        'docs/evidence/v0.6/frontend-development-checkpoint.json',
        'docs/evidence/v0.6/opening-development-checkpoint.json',
        'docs/evidence/v0.6/candidate-opening-d19.json','docs/evidence/v0.6/cold-reports-d19.json',
        'docs/evidence/v0.6/report-visual-coverage-d19.json','docs/evidence/v0.6/reference-resource-d19.json',
        'docs/evidence/v0.6/replay-d19.json','docs/evidence/v0.6/ci-d19-and-93a58e9.json',
        'docs/evidence/v0.6/workspace-interaction-repair.json',
        'docs/evidence/v0.6/ci-aa7-success.json','docs/evidence/v0.6/cold-reports-aa7.json',
        'docs/evidence/v0.6/native-aa7-checkpoint.json',
        'docs/evidence/v0.6/opening-baseline.json','scripts/profile-project-opening.ps1',
        'schemas/reference-model-v0.6.schema.json','schemas/architectural-scope-v0.6.schema.json',
        'scripts/reference-synthetic-fixture.mjs',
        'docs/installation.md','docs/replay.md','docs/persistence.md','docs/model-contract.md',
        'docs/evidence/v0.5/ci-29adf02-failure.json',
        'docs/evidence/v0.5/ci-5c507f7-diagnostic.json',
        'docs/evidence/v0.5/atomic-draft-repair.json',
        'docs/evidence/v0.5/repaired-candidate-artifacts.json',
        'docs/evidence/v0.5/repaired-installed-payload.json',
        'docs/evidence/v0.5/repaired-cold-audit.json',
        'docs/evidence/v0.5/ci-94a80e0-success.json',
        'docs/scientific-validation.md','docs/dependencies.md','docs/windows-runtime.md',
        'docs/windows-installer.md','docs/workspace-guide.md','docs/rhino-interaction-matrix.md',
        'docs/v0.2-native-verification.md','docs/images/v0.2/README.md','docs/v0.3-spec.md',
        'docs/v0.3-implementation-plan.md','docs/v0.3-scientific-method.md','docs/v0.3-qualification-continuation.md',
        'docs/v0.3-weather-sources.md','docs/v0.3-contracts.md','docs/v0.3-usage.md','docs/v0.3-status.md',
        'docs/v0.3-native-verification.md','docs/v0.3-ci-package-validation.md','docs/progress.md',
        'docs/v0.3-reference-protocol.md','docs/v0.3-engine-investigation.md',
        'scripts/annual-reference-prerequisite/README.md',
        'scripts/annual-reference-prerequisite/proposal.json',
        'scripts/annual-reference-prerequisite/decision.md',
        'scripts/annual-reference-prerequisite/runtime-pins.json',
        'scripts/annual-reference-prerequisite/guard-review.json',
        'scripts/annual-reference-prerequisite/prepare-check.json',
        'scripts/annual-reference-prerequisite/portable-test-check.json',
        'docs/evidence/v0.3-engine/reference-prerequisite-rejection-checkpoint.json',
        'docs/evidence/v0.3-engine/reference-prerequisite-ordinal13.json',
        'docs/evidence/v0.3-engine/reference-prerequisite-reader-recovery.json',
        'docs/evidence/v0.3-engine/reference-prerequisite-remaining-checkpoints.json',
        'docs/evidence/v0.3-engine/reference-prerequisite-terminal.json',
        'LICENSE','scripts/annual-reference-aggregation.cs',
        'tests/fixtures/reference-depth/README.md','tests/fixtures/reference-depth/manifest.json',
        'tests/fixtures/reference-depth/external-primary/manifest.json',
        'tests/fixtures/reference-depth/external-primary/A4116-finite-original.json',
        'tests/fixtures/reference-depth/external-primary/B8512-finite-original.json',
        'tests/fixtures/reference-depth/shared/sensors.pts',
        'tests/fixtures/reference-depth/shared/matched-basis.cal',
        'tests/fixtures/reference-depth/shared/weather-asset.json',
        'tests/fixtures/reference-depth/A4116/scene.rad',
        'tests/fixtures/reference-depth/A4116/hour-4116-scaled.rad',
        'tests/fixtures/reference-depth/A4116/hour-4116.dat',
        'tests/fixtures/reference-depth/A4116/baseline.rgb.gz',
        'tests/fixtures/reference-depth/A4116/depth.rgb.gz',
        'tests/fixtures/reference-depth/B8512/scene.rad',
        'tests/fixtures/reference-depth/B8512/hour-8512-scaled.rad',
        'tests/fixtures/reference-depth/B8512/hour-8512.dat',
        'tests/fixtures/reference-depth/B8512/baseline.rgb.gz',
        'tests/fixtures/reference-depth/B8512/depth.rgb.gz',
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
        'docs/evidence/v0.3-native/candidate08-build-audit.json',
        'docs/evidence/v0.3-native/candidate08-native-qualification.json',
        'docs/evidence/v0.3-native/candidate08-hourly-qualification.jpg',
        'docs/evidence/v0.3-native/candidate08-comparison-qualification.jpg',
        'docs/evidence/v0.3-native/candidate09-build-audit.json',
        'docs/evidence/v0.3-native/candidate09-native-qualification.json',
        'docs/evidence/v0.3-native/candidate09-open-qualification.jpg',
        'docs/evidence/v0.3-native/candidate09-comparison-qualification.jpg',
        'docs/evidence/v0.3-native/candidate10-build-audit.json',
        'docs/evidence/v0.3-native/candidate10-native-qualification.json',
        'docs/evidence/v0.3-native/candidate10-current-annual-export.json',
        'docs/evidence/v0.3-native/candidate10-export-audit-utc-correction.json',
        'docs/evidence/v0.3-native/candidate10-current-annual-export-independent.json',
        'docs/evidence/v0.3-native/candidate10-current-metric-qualification.jpg',
        'docs/evidence/v0.3-native/candidate10-stale-metric-qualification.jpg',
        'docs/evidence/v0.3-native/candidate10-undo-metric-qualification.jpg',
        'docs/evidence/v0.3-native/candidate10-comparison-qualification.jpg',
        'docs/evidence/v0.3-integration/candidate08-selected-hour-replay.json',
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
        'docs/evidence/v0.3-ci/final-main-572f0b7.json',
        'docs/evidence/v0.3-ci/superseded-main-d7d74a3.json',
        'docs/evidence/v0.3-ci/final-main-b813b06.json',
        'docs/evidence/v0.3-ci/selected-hour-b813b06.json',
        'docs/evidence/v0.3-ci/final-main-34a3d53.json',
        'docs/evidence/v0.3-ci/selected-hour-34a3d53.json',
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
        'schemas/qualification-v1.schema.json','crates/greenlens-runner/src/qualification-registry.json',
        'third-party/Radiance-LICENSE.txt','third-party/Radiance-gendaymtx-NOTICE.txt',
        'docs/roadmap.md','docs/v0.4-energy-spec.md','docs/v0.4-energy-implementation-plan.md',
        'docs/v0.4-energy-contracts.md','docs/v0.4-energy-method.md','docs/v0.4-energy-status.md','docs/v0.4-native-verification.md',
        'docs/v0.4-energy-usage.md','docs/v0.4-report-method.md',
        'docs/architecture.md','docs/build-provenance.md','docs/v0.2-build-provenance-evidence.json',
        'docs/v0.5-spec.md','docs/v0.5-implementation-plan.md','docs/v0.5-acceptance.md',
        'docs/v0.5-contracts.md','docs/v0.5-delivery.md','docs/v0.5-native-verification.md','docs/v0.5-real-weather-protocol.md',
        'docs/evidence/v0.5/baseline-ci-supplement.json',
        'docs/evidence/v0.5/npm-license-review.json',
        'docs/evidence/v0.5/report-development-review.json',
        'docs/evidence/v0.5/frontend-final-development.json',
        'docs/evidence/v0.5/sensitivity-development.json',
        'docs/evidence/v0.5/final-cold-replay.json','docs/evidence/v0.5/installed-payload.json',
        'docs/evidence/v0.5/legacy-replay.json','docs/evidence/v0.5/native-lifecycle.json',
        'docs/evidence/v0.5/candidate-artifacts.json',
        'docs/evidence/v0.5/carrier-curve-domain-proof.json','docs/evidence/v0.5/carrier-curve-observability.json',
        'dependencies/energyplus.json','dependencies/typst.json','dependencies/energy-qualification.json',
        'schemas/thermal-model-v0.4.schema.json','schemas/energy-study-v0.4.schema.json',
        'schemas/energy-weather-v0.4.schema.json','schemas/energy-report-v0.4.schema.json',
        'schemas/energy-report-presentation-v0.4.schema.json',
        'examples/energy/README.md','examples/energy/revision1-inputs.json','examples/carbon/README.md',
        'examples/carbon/clp-hong-kong-2025.json','examples/carbon/synthetic-fixture.json',
        'scripts/install-energyplus.ps1','scripts/install-typst.ps1',
        'scripts/energy-gate0-audit.py','scripts/energy-gate0-fixture.py','scripts/energy-gate0-run.ps1',
        'scripts/energy-interzone-protocol.json','scripts/energy-control-protocol.json','scripts/energy-zero-cutout-protocol.json','third-party/EnergyPlus-LICENSE.txt',
        'crates/greenlens-runner/src/energy_report.rs','reports/energy/feasibility.typ','reports/energy/report.typ',
        'reports/energy/notices/NotoSansCJK-OFL.txt','reports/energy/notices/Typst-LICENSE.txt',
        'reports/energy/notices/Typst-NOTICE.txt','reports/energy/notices/Typst-assets-NOTICE.txt',
        'reports/energy/notices/Typst-target-NOTICES.txt',
        'docs/evidence/v0.4-energy/interzone.json','docs/evidence/v0.4-energy/native-gate0.json',
        'docs/evidence/v0.4-energy/production-timestep.json',
        'docs/evidence/v0.4-energy/revision2-independent.json',
        'docs/evidence/v0.4-energy/revision2-clean-report-audit.json',
        'docs/evidence/v0.4-energy/revision2-clean-delivery-audit.json',
        'docs/evidence/v0.4-energy/candidate-2e11-build.json',
        'docs/evidence/v0.4-energy/candidate-2e11-package-audit.json',
        'docs/evidence/v0.4-energy/candidate-2e11-ci-checkpoint.json',
        'docs/v0.4-delivery.md',
        'docs/evidence/v0.4-energy/candidate-d144-build.json',
        'docs/evidence/v0.4-energy/candidate-00cf-build.json',
        'docs/evidence/v0.4-energy/design-difference-regression.json',
        'docs/evidence/v0.4-energy/design-difference-frontend.json',
        'docs/evidence/v0.4-energy/design-difference-report-audit.json',
        'docs/evidence/v0.4-energy/design-difference-cold-audit.json',
        'docs/evidence/v0.4-energy/candidate-fce896d-build.json',
        'docs/evidence/v0.4-energy/candidate-fce896d-package-audit.json',
        'docs/evidence/v0.4-energy/candidate-fce896d-delivery-audit.json',
        'docs/evidence/v0.4-energy/ci-9b658-success.json',
        'docs/evidence/v0.4-energy/ci-fce896d.json',
        'docs/evidence/v0.4-energy/candidate-9b658-build.json',
        'docs/evidence/v0.4-energy/candidate-9b658-package-audit.json',
        'docs/evidence/v0.4-energy/candidate-9b658-delivery-audit.json',
        'docs/evidence/v0.4-energy/final-report-identity.json',
        'docs/evidence/v0.4-energy/final-cold-lifecycle-audit.json',
        'docs/evidence/v0.4-energy/disk-recovery.json',
        'docs/evidence/v0.4-energy/native-post-disk-availability.json',
        'docs/evidence/v0.4-energy/ci-9b658.json',
        'docs/evidence/v0.4-energy/candidate-00cf-package-audit.json',
        'docs/evidence/v0.4-energy/repaired-report-audit.json',
        'docs/evidence/v0.4-energy/repaired-cold-delivery-audit.json',
        'docs/evidence/v0.4-energy/scratch-sampling-race.json',
        'docs/evidence/v0.4-energy/ci-d144-success.json',
        'docs/evidence/v0.4-energy/eight-zone-capacity.json',
        'docs/evidence/v0.4-energy/eight-zone-initial-report-audit.json',
        'docs/evidence/v0.4-energy/candidate-d144-package-audit.json',
        'docs/evidence/v0.4-energy/revision3-clean-report-audit.json',
        'docs/evidence/v0.4-energy/revision3-clean-delivery-audit.json',
        'scripts/energy-fan-example-protocol.json',
        'docs/evidence/v0.4-energy/revision3-protocol-review.json',
        'docs/evidence/v0.4-energy/revision3-guard-review.json',
        'docs/evidence/v0.4-energy/revision3-regression.json',
        'docs/evidence/v0.4-energy/revision3-qualification-regression.json',
        'docs/evidence/v0.4-energy/revision3-independent.json',
        'docs/evidence/v0.4-energy/revision3-production-timestep.json',
        'docs/evidence/v0.4-energy/revision3-direct-native-parity.json',
        'docs/evidence/v0.4-energy/revision2-production-timestep.json',
        'docs/evidence/v0.4-energy/revision2-direct-native-parity.json',
        'docs/evidence/v0.4-energy/direct-native-parity.json',
        'docs/evidence/v0.4-energy/fixed-control-diagnostic.json',
        'docs/evidence/v0.4-energy/legacy-static-regression.json',
        'docs/evidence/v0.4-energy/lifecycle-initial-failure.json',
        'docs/evidence/v0.4-energy/native-lifecycle.json',
        'docs/evidence/v0.4-energy/cold-portability.json',
        'docs/evidence/v0.4-energy/historical-energy-replay.json',
        'docs/evidence/v0.4-energy/report-preflight-guards.json',
        'docs/evidence/v0.4-energy/report-production-renderer.json',
        'docs/evidence/v0.4-energy/report-production-results.json',
        'docs/evidence/v0.4-energy/report-cold-failed-assessment.json',
        'docs/evidence/v0.4-energy/report-renderer-feasibility.json',
        'docs/evidence/v0.4-energy/report-renderer-fixture.pdf',
        'docs/evidence/v0.4-energy/report-renderer-preview.png',
        'docs/evidence/v0.4-energy/report-typst-target-inventory.json'
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

function Assert-PackageDocumentSizes([long[]]$FileBytes, [long]$GeneratedIndexBytes) {
    # Offline distribution documentation only. Scientific archive/process bounds
    # are separate Rust policies and are not changed by this selection budget.
    $bytes = 0L
    foreach ($size in @($FileBytes) + @($GeneratedIndexBytes)) {
        if ($size -lt 0 -or $size -gt 2MB) { throw 'Expected a compact documentation file within 2 MiB.' }
        if ($bytes -gt 16MB - $size) { throw 'Review the explicit documentation selection before exceeding 16 MiB, including its generated index.' }
        $bytes += $size
    }
    return $bytes
}

function Get-PackageContentsBytes([int]$Links) {
    $contents = @"
# Packaged documentation

The selected guides, method/reference declarations, data inventories and compact linked numerical records are included for offline reading. Local inline Markdown links in these guides were checked before packaging ($Links links).

Documentation policy v2 limits this explicit selection, including this index, to 16 MiB total and 2 MiB per file. The increase from 12 MiB retains the historical evidence and adds v0.4 method/schema records and audited renderer notices without removing old outcomes. Scientific evidence archive, process and runtime limits are unchanged.

This is not a source checkout or a complete historical evidence archive. Backtick command examples, source-only verification scripts, historical screenshot filenames, upstream URLs and local build/output paths can require the repository or separately retained artifacts. Selected research scripts are retained as readable source; their development dependencies are not a runtime requirement or a bundled verification toolchain. The reference-prerequisite declarations, source pins and preparation records are included for inspection; executing their commands requires the separately reviewed source checkout and inputs. Large matrices, octrees, evidence ZIPs and engine binaries are excluded.
"@
    return ,([Text.UTF8Encoding]::new($false).GetBytes($contents.Replace("`r`n", "`n") + "`n"))
}

function Copy-PackageDocuments([string]$Source, [string]$Destination) {
    $paths = @(Get-PackageDocumentPaths)
    $files = @($paths | ForEach-Object {
        $file = Get-Item -LiteralPath (Join-Path $Source $_)
        if ($file.PSIsContainer) { throw "Expected a compact documentation file: $_" }
        $file
    })
    # Reject oversize source selections before creating/copying any destination.
    $sizes = @($files | ForEach-Object { $_.Length })
    $null = Assert-PackageDocumentSizes -FileBytes $sizes -GeneratedIndexBytes 0
    $links = Assert-PackageDocumentLinks $Source
    $indexBytes = Get-PackageContentsBytes $links
    $null = Assert-PackageDocumentSizes -FileBytes $sizes -GeneratedIndexBytes $indexBytes.Length
    foreach ($path in $paths) {
        $destinationPath = Join-Path $Destination $path
        New-Item -ItemType Directory -Path (Split-Path -Parent $destinationPath) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $Source $path) -Destination $destinationPath
    }
    if ((Assert-PackageDocumentLinks $Destination) -ne $links) { throw 'Packaged documentation link count changed while copying.' }
    [IO.File]::WriteAllBytes((Join-Path $Destination 'docs/PACKAGE-CONTENTS.md'), $indexBytes)
    $copiedSizes = @($paths | ForEach-Object { (Get-Item -LiteralPath (Join-Path $Destination $_)).Length })
    $null = Assert-PackageDocumentSizes -FileBytes $copiedSizes -GeneratedIndexBytes $indexBytes.Length
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
