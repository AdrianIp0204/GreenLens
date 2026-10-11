# Native Windows workspace captures

These are unaltered application-window screenshots from installed GreenLens 0.2.0, without a development server. They show synthetic models and contain no private paths. The physical display was 1920×1080 with 125% Windows scaling. Capture tooling returns display-scaled pixels; the application's Diagnostics measurements distinguish native client size from CSS size.

- `four-view-window-editing.png`: final installed clean build `bb0456c`, four views, selected window and constrained Move handles. Actual source and package hashes are in `../../v0.2-release-artifacts.json`. This is the saved, idle document prepared for the outstanding manual gesture observations; a screenshot does not establish that those gestures passed.
- `native-ab-comparison.png`: earlier installed candidate, actual real Radiance A/B results and shared lux scale. Native client 1920×991; CSS client 1536×794; DPR 1.25. The run snapshots retain actual `b7b8d32` dirty-build provenance. Final-CLI replay passed both runs with zero maximum lux difference.
- `compact-workspace.png`: corrected compact layout in that earlier installed candidate, with command history, result drawer and Analysis panel visible. Native client 1125×625; CSS client 900×500; DPR 1.25. This smaller-than-1366×768 usable window is evidence for the compact layout; it is not a claim that the physical monitor resolution was changed to 1366×768.

See `../../v0.2-native-verification.md` for executed procedures, identities and remaining coverage limits. Historical v0.1 screenshots and evidence are preserved separately.
