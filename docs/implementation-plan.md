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
- [ ] Adapter and supervised jobs, cancellation and shutdown trees
- [ ] Shared service and doctor/validate/run/compare/export/replay CLI
- [ ] Immutable runs, staleness, fingerprints, comparison rules
- [ ] Safe portable persistence, locking/conflicts/checksums/package validation
- [ ] Offline report, ZIP and replay
- [ ] A direct/adapter/replay tolerance
- [ ] B pi-L hemisphere
- [ ] C prescribed exterior illuminance
- [ ] D black enclosure
- [ ] E converged independent aperture quadrature and Radiance agreement
- [ ] F frozen shipping/reference convergence
- [ ] Rejection, scaling/symmetry, malformed output/exits/missing engine/cancel/stale tests

## Gate 2 — native workflow
- [ ] Native desktop real Rust/Radiance A/B edit/run/compare
- [ ] Heatmaps shared scale, units, identity and table values
- [ ] Invalid/missing/failed/cancelled/stale visible states
- [ ] Save/reopen/export and relocated project without original scratch
- [ ] Export replay tolerance; cancellation tree and no completed partial result

## Gate 3 — release
- [ ] Per-user NSIS package and CLI artifact
- [ ] Installed production application real run/save/export without dev server
- [ ] Clean-machine installation, method, validation, limitations/replay docs
- [ ] Dependency inventory and third-party notices
- [ ] Windows CI fmt/clippy/tests/TS/frontend/CLI/real engine/desktop production
- [ ] Inspected public Git repository/commits/push and CI results

Dependencies: engine identity and conventions precede frozen adapter; domain contracts precede frontend; acceptance science precedes release claims; shared service/persistence precede native workflow; workflow precedes packaged acceptance. Required failures remain failures, not skips. Use synthetic fixture data only.
