# Bounded implementation plan

Gates are sequential acceptance barriers; independent preparatory work can overlap. Status is recorded per item in progress.md. No future roadmap work is part of completion.

## Gate 0 — feasibility
- [x] Native Rust/MSVC, Node/npm, WebView2 verified
- [x] Official Radiance artifact/digest/archive/licenses/libraries inspected
- [x] gensky/oconv/rtrace real finite calculation, child library lookup
- [x] Spaces and non-ASCII working paths executed
- [x] Versions, commands and feasibility evidence recorded

## Gate 1 — headless science
- [x] Domain schema, validation, geometry/aperture/transforms/sensors/glazing
- [x] Adapter and supervised jobs, cancellation and shutdown trees
- [x] Shared service and doctor/validate/run/compare/export/replay CLI
- [x] Immutable runs, staleness, fingerprints, comparison rules
- [x] Safe portable persistence, locking/conflicts/checksums/package validation
- [x] Offline report, ZIP and replay
- [x] A direct/adapter/replay tolerance
- [x] B pi-L hemisphere
- [x] C prescribed exterior illuminance
- [x] D black enclosure
- [x] E converged independent aperture quadrature and Radiance agreement
- [x] F frozen shipping/reference convergence
- [x] Rejection, scaling/symmetry, malformed output/exits/missing engine/cancel/stale tests

## Gate 2 — native workflow
- [x] Native desktop real Rust/Radiance A/B edit/run/compare
- [x] Heatmaps shared scale, units, identity and table values
- [x] Invalid/missing/failed/cancelled/stale visible states
- [x] Save/reopen/export and relocated project without original scratch
- [x] Export replay tolerance; cancellation tree and no completed partial result

## Gate 3 — release
- [x] Per-user NSIS package and CLI artifact
- [x] Installed production application real run/save/export without dev server
- [x] Clean-machine installation, method, validation, limitations/replay docs
- [x] Dependency inventory and third-party notices
- [x] Windows CI fmt/clippy/tests/TS/frontend/CLI/real engine/desktop production
- [x] Inspected public Git repository/commits/push and CI results

Dependencies: engine identity and conventions precede frozen adapter; domain contracts precede frontend; acceptance science precedes release claims; shared service/persistence precede native workflow; workflow precedes packaged acceptance. Required failures remain failures, not skips. Use synthetic fixture data only.
