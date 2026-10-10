# Progress and acceptance evidence

## 2026-10-11 — initial inspection
- Implemented: sanitized specification, durable instructions, architecture, roadmap and gate checklist.
- Verified: requested source directory exists and is empty; no parent AGENTS.md found in inspected ancestry; no Git repository/remotes. Git/Node/npm/Python/uv/GitHub CLI available. GitHub authentication is active (account-specific details intentionally omitted).
- Unverified: native Rust/MSVC and engine execution, all scientific thresholds, all desktop/package acceptance, CI.
- Blocked: none established; dependency inspection/acquisition is next.
- Next: native feasibility; then shared contracts/core and headless application service. Do not count this documentation baseline as release completion.

## 2026-10-11 — native feasibility and initial implementation
- Verified native Radiance 6.0.2 asset digest against upstream release API and downloaded bytes. `gensky`, `oconv`, `rtrace` all exited 0 in a working path with spaces and Chinese characters. Prescribed 10,000 lx produced 9,984.14923 lx (0.158508% error). See `docs/gate-0-evidence.json`, `docs/engine-feasibility.md` and scripts for exact arguments and runtime inventory.
- Remedy: initially missing `rayinit.cal` resolved by child-only `RAYPATH=.;<engine>/lib`. No global engine environment changed.
- Verified a native Rust/MSVC smoke executable in a path with spaces and non-ASCII characters. Rustup finalization then encountered concurrent incomplete-component recovery; toolchain owner is repairing one installation with other Cargo invocations held. This is a setup failure under investigation, not release completion.
- Implemented, not yet compiled/verified: authoritative geometry/sensor/glazing core; pinned adapter with supervised processes; shared runner, immutable run records, safe project promotion/manifest replacement, lock/conflict detection, checked ZIP import/export, offline report and CLI. Frontend implementation follows native feasibility.
- Authorized GitHub setup completed after immediate account/repository recheck. Sanitized specification baseline committed and pushed. Repository: https://github.com/AdrianIp0204/GreenLens .
- Unverified: all full scientific thresholds A–F, CLI end-to-end, native desktop states/workflow/portability, package and CI.

## 2026-10-11 — Gate 0 complete, headless integration underway
- Verified: clean repair of the newly installed Rust 1.99.0 MSVC toolchain followed by an actual dependency-free Cargo program using std, native execution in a path containing spaces/non-ASCII, rustfmt and Clippy with warnings denied. The earlier partial installation is resolved. See `docs/toolchain.md`.
- Verified frontend supplement: TypeScript check, Vite production build, and presentation tests for stale-result suppression, shared color scales and terminal job states. These do not count as native desktop acceptance.
- Implemented review repairs: exact manifest-byte revision pairing, repeated external-change check before replacement, complete ZIP checksum coverage, Windows reserved-name/alias/path rejection, raw RGB-to-lux consistency, successful process provenance, UTC validation and rotated-model float roundtrip preservation.
- Gate 0 checklist completed using the native execution and inspection records. Gate 1 remains open: initial cold Cargo resolution of the Tauri workspace and real numerical profile investigation are underway.
- Dependency review: npm versions/notices inventoried; Rust inventory awaits completed dependency resolution. Separate engine acquisition remains required; archive rebundling is not approved by the incomplete upstream notice set.
- Native desktop/package verification and GitHub CI have not yet executed. No release-complete claim is made.

## 2026-10-11 — dependency resolution and numerical investigation
- Resolved the full Cargo workspace lockfile after diagnosing slow delivery of large crates.io sparse-index files. TLS and archive requests were healthy. A process-local longer HTTP timeout allowed resolution; no global Cargo settings or dependency source substitutions were retained. Native CLI compilation then proceeded from the original registry cache offline.
- Independent direct-engine diagnostic: the five-sensor aperture fixture agreed with converged numerical quadrature within 1.99% at every sensor. Quadrature refinement changed results by at most 0.000609%. These exploratory measurements do not replace the required Rust adapter suite.
- The first faster room profile passed the mean convergence limit but failed the per-sensor limit at one sensor. Increased sampling is under investigation. Release thresholds remain unchanged and profiles are not frozen yet.
- Frontend and per-user installer implementation are present; native installed workflow and packaging evidence remain pending. Gate 1 remains open.

## 2026-10-11 — native unit checks and adapter integration remedy
- Verified native Rust tests: 14 core tests, 5 runner unit tests and 15 persistence integration tests passed. Native fixtures include junction rejection and a directory guard that prevents rename while held. Mock evidence is explicitly labeled and counts only toward file/contract tests.
- A native guard test exposed insufficient access on a retained Windows directory handle; requesting directory-list access fixed it, and the test passed. Hydrated cloud-tag classification is tested; a live cloud-provider hydration cycle has not been tested.
- The first application cancellation check failed because real `rtrace` exited 2 before cancellation: its C runtime rejected an extended Windows library path while locating `rayinit.cal`. Both prior engine stages exited 0. Adapter path normalization is being corrected; this failure is not counted as cancellation acceptance.
- The final direct-engine profile investigation passed unchanged F thresholds (0.7591% mean absolute difference, all 77 sensor limits). Frozen profile details and rejected candidate evidence are in `docs/profile-investigation.json`; adapter execution remains required.
- Large official crate archives downloaded slowly through Cargo multiplexed transfers. Direct downloads from the same official archive service completed quickly; their bytes were checked against the committed lockfile SHA-256 before atomic cache placement. No registry trust, TLS verification or global configuration was weakened.

## 2026-10-11 — committed implementation and production candidate
- Implemented and pushed the complete bounded Rust/CLI/Tauri workflow, schemas, portable evidence export, native Windows CI and dependency notices. Source revisions `019e5ff`, `e5e05eb` and `845506a` record implementation and packaging changes. Public staged files were checked for credentials, private paths and unintended outputs before committing.
- Verified `cargo fmt --all -- --check`, workspace Clippy with warnings denied, 14 core tests, 5 runner unit tests, 15 persistence integration tests, 2 real application lifecycle tests and 5 adapter tests. Two ignored child fixture entry points are deliberately invoked by the passing native process-tree test; they are not skipped acceptance checks.
- Resolved the extended-path Radiance library issue. Actual native cancellation now passes and does not create completed evidence. Real analytical fixtures B–E, intensity scaling, symmetry and nonzero engine exits pass; see `science-fixtures-evidence.json`. Full A/F and saved-package replay remain in progress.
- Verified strict dependency inventory: 357 entries, 195 retained notice texts, zero review gaps. All nine actual NSIS compiler/stub/plugin hashes matched. Read-only inventory verification preserves committed evidence bytes.
- Production NSIS builds succeeded. Import inspection exposed Tauri's default static VC support-library override; the final candidate explicitly uses the external Microsoft runtime already required by the installation route. The candidate is built from clean revision `845506a`; installed native acceptance and final imports remain pending.
- A real CLI A/B project has been copied with exact result/provenance/raw/checksum hashes and exported while its original scratch directories were unavailable. Its 27-entry evidence ZIP contains no detected personal absolute paths or account tokens. Replay is running; this is not yet recorded as a passed end-to-end gate.
- Native Windows GitHub CI is running. Installed desktop A/B/state/portability verification follows the final candidate build. No current external blocker is established; Gates 1–3 remain open.

## 2026-10-11 — portable CLI acceptance and final review repairs
- Verified `scripts/cli-integration.ps1`: actual doctor/validate/A/B/run/compare, portable copy, unchanged result/provenance/raw/checksum files, compare/export without original scratch and both exported run replays passed. All 154 readings met max(0.1 lx, 0.1% of reference); this invocation measured zero absolute lux difference. See `docs/cli-evidence.json`. This establishes same-engine integration, not independent physical validation or a promise of byte-identical stochastic calculations.
- Final native release review identified exact edge contact between an obstruction and overhang, incomplete required version provenance on imported records, and isolated core test feature inheritance. Focused validation and regression repairs are being completed; default scientific scenes and frozen numerical profiles are unchanged.
- A CI runner hit GitHub's unauthenticated release-API rate limit. Engine acquisition now accepts the masked CI token only for the fixed official release API endpoint, with redirects disabled for that request. Authenticated and unauthenticated local verification both passed; no token is printed, persisted or sent to asset-download redirects. The release digest remains mandatory.
- Final dynamic-runtime candidate imports were inspected: desktop uses VCRUNTIME140.dll/VCRUNTIME140_1.dll and Windows UCRT libraries; CLI uses VCRUNTIME140.dll and UCRT. Final installed-copy identity remains part of native acceptance.
