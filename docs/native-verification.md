# Native desktop verification

This record distinguishes implemented interface behavior from observed native acceptance. Browser previews and frontend unit tests are not native engine evidence.

## Method

Use the production per-user Windows installation with its embedded frontend assets and no development server. The installed application invokes the same Rust application service as the CLI, and that service launches the pinned native Radiance executables.

The reproducible interaction sequence is:

1. Launch GreenLens, create the synthetic example, enter a fresh project directory, inspect the validated room, aperture and sensor cells, then run A.
2. Observe queued/preparing/running/completed states and logs. Verify saved sensor counts and finite lux values against the sensor table. Select B, inspect/edit its 0.60 m overhang, run it, and inspect the shared A/B heatmap scale and absolute differences.
3. Edit a scientific input after completion. Confirm the historical result is marked stale and its lux values are not drawn on edited geometry. Restore the original input or inspect the immutable snapshot in Evidence.
4. Enter an invalid geometry/material value and verify actionable validation and disabled Run. Inspect an explicitly missing engine directory and verify a visible error. Start a failed engine job and a cancellable real job; confirm failures/cancellation never add a completed result, and check the Windows process tree has ended.
5. Save, close/reopen and inspect the saved A/B arrays/provenance. Export the evidence ZIP through the UI.
6. Copy the complete portable project to a different path. Make only its owned original scratch directories unavailable. Reopen the copy in the installed application and compare exact result/provenance hashes; export again. Replay the export with the CLI, regenerating disposable octrees, and enforce the integration tolerance.

Record exact build identity, UTC event times, local evidence artifact locations and outcomes. Keep screenshots and machine-specific paths outside public source; summarize only sanitized evidence here.

## Current evidence

| Item | Status | Evidence |
| --- | --- | --- |
| Native Rust/MSVC toolchain | Verified | Standard-library Cargo smoke in spaces/non-ASCII path; rustfmt and Clippy passed after installation repair; see `toolchain.md`. |
| Frontend production assets | Verified | TypeScript check and Vite production build passed on native Windows. No mocked simulation data is present. |
| Frontend presentation checks | Verified, not native acceptance | Tests cover stale-result suppression, shared-scale color mapping and terminal job presentation. |
| Installed native A/B workflow | Not yet executed | Awaiting production binary/package. |
| Invalid, missing, failed, cancelled and stale native states | Not yet executed | Must be observed in the native application. |
| Native copy/reopen/export without original scratch | Not yet executed | Must use a saved real A/B project. |
| Export replay from native-created evidence | Not yet executed | Must pass every sensor within max(0.1 lx, 0.1% of reference). |

This file does not yet certify completion of Gates 2 or 3.
