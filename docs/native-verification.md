# Native desktop verification

The installed Windows application completed the real A/B workflow on 11 October 2026 (UTC events on 10 October). This record concerns the production NSIS installation, embedded frontend assets, shared Rust application service and pinned native Radiance 6.0.2. No browser preview, mock result or development server is used as native acceptance evidence. Machine-readable observations are in [native-desktop-evidence.json](native-desktop-evidence.json).

## Tested artifacts and environment

The application was built from clean source revision `2c3de39e05271353a3818a2fd10e81c423c27b78`; saved provenance records that revision and `source_worktree_dirty: false`. The observed platform was Windows 11 x64, build 26200, with WebView2 154.0.4258.62. Rust 1.99.0 and the native MSVC toolchain produced the executables. Runtime imports and prerequisites are documented in [windows-runtime.md](windows-runtime.md). This was a development workstation, not a separate clean-machine test.

| Artifact | SHA-256 |
| --- | --- |
| Initial NSIS installer used for calculations | `abf57fde5c0f4e5e85c68809c079d04f5f47b9d73ece96c4620480da7ee8e149` |
| Refreshed NSIS installer, installed and reopened after calculations | `fc215f3a3ae61189ddcd01418e615cc3f33efce109c1dff0be4ce7233e5e62b5` |
| Installed desktop in both installations | `66196ab72212617a2232043b8dd71e2cacd4ef3dc4d357d46385c41f9fc7986d` |
| Unbundled desktop | `befaa5e113618d8139948f3c83314935e3a4043a14f4050cac50feb4a60c3fa6` |
| Release CLI | `cc9d7a228cfe4cfd7f386d9f8f66346edea4e7d68b9f08b54ad717795b826dc8` |

Both per-user installations used the NSIS `/S` option, exited 0 and installed under `%LOCALAPPDATA%\Programs\GreenLens`. The refreshed installer changed packaged runtime documentation only; its installed executable was byte-identical to the native-tested payload. Installed runtime documentation matched the source file. LICENSE, NOTICE, dependency inventory, license supplements, engine manifest and generated third-party notices also matched their source hashes. Installer, desktop and CLI signatures were inspected and reported `NotSigned`.

The unbundled and installed desktop differ at exactly three bytes: Tauri changes its documented bundle marker from `UNK` to `NSS` for NSIS. The inspected pinned `tauri-utils` source confirms that behavior. This explains their different hashes; no other byte differed.

## Observed workflow

Native accessibility actions and screenshots exercised the installed app. Independent filesystem and process inspection checked immutable evidence and process termination. The private local evidence folder contains screenshots `01-initial.png` through `14-relocated-export.png`, table checks, process snapshots and artifact hashes. Screenshots are retained locally rather than published because the UI displays user-selected absolute paths.

| Acceptance item | Observation | Status |
| --- | --- | --- |
| Launch and inspect geometry | Production app launched with embedded `tauri.localhost` assets, real Three.js geometry, aperture and 77 workplane sensors; exact calculation label and metre/lux units visible. | Verified |
| Actionable invalid input | Room width 0 produced the backend finite 0.5–100 m validation message and disabled Run. Restoring 4 m restored valid geometry. | Verified |
| Missing engine | Check engine with a nonexistent explicit directory displayed the missing-Radiance message and installation remedy. | Verified |
| Failed job | Run with that explicit missing directory entered Failed during real backend preparation, displayed its log, and produced no completed result. Nonzero engine exits are separately tested headlessly. | Verified |
| Cancellation | A real native `rtrace.exe` process was observed during Running. Cancel displayed Cancelled; the observed process and all engine children ended; completed-run count stayed zero. | Verified |
| Real A/B calculation | A with no overhang and B with a 0.60 m overhang completed gensky, oconv and rtrace with exit 0, 77 finite outputs each, and automatic evidence saving. | Verified |
| Heatmap, units and table | Shared 0–1454.4 lx scale, sensor IDs, equal-area cells and signed B minus A statistics were visible. All 77 A/B/delta table rows matched saved arrays within 0.000498 lx display rounding. | Verified |
| Stale result | Editing B projection from 0.60 to 0.70 m displayed Stale, removed the old heatmap and blanked result values on the edited model. Restoring 0.60 m restored the matching historical result. | Verified |
| Save and reopen | Save project followed by Open project displayed checksummed-evidence confirmation and restored completed B results. | Verified |
| Application shutdown | Closing the native window during another real rtrace job terminated application and child process. The saved manifest retained exactly the two completed runs. | Verified |
| Portable copy | Copied the whole A/B project to a separate path containing spaces and non-ASCII text, moved only its two original scratch directories away, then reopened it in the refreshed installed application. | Verified |
| Immutable copied evidence | Both result and provenance files remained hash-identical; all 77 reopened A table rows matched saved values within 0.000497 lx rounding. Original scratch locations were absent. | Verified |
| Native export | Both original and relocated projects exported through the installed UI. The two 94,116-byte ZIPs have the same SHA-256, `c53dcc4094630f3bac5c3f1f76b7e3c452f015c0e819d8713e4744f3e0aedcec`. | Verified |
| CLI replay of native export | Release CLI replay passed both runs, all 77 sensors each, maximum absolute difference 0.0 lx. All 27 original/relocated ZIP entries and the complete archive hashes are identical, so this replay verifies both packages. | Verified |

The frontend presentation tests and TypeScript/Vite checks passed separately. They do not replace these installed native observations.

## Numerical observations

| Alternative | Run ID | Sensors | Minimum lx | Mean lx | Maximum lx | rtrace time |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| A, no overhang | `78b6e8e6-8a46-4a6a-9e75-8931ae5d3528` | 77 | 45.964712 | 237.212445 | 1454.363365 | 195.395 s |
| B, 0.60 m overhang | `270984ca-ecf9-482e-b8e5-d13b851eb4d0` | 77 | 40.526280 | 175.623912 | 1079.445896 | 163.562 s |

The backend comparison reports B minus A: minimum −374.917469 lx, mean −61.588533 lx and maximum −5.438432 lx. Percentage deltas are omitted. These values describe this synthetic example only; reflective shading is not assumed to reduce every sensor in all models. The comparison and sensor fingerprints match across A/B, and window/glazing remain unchanged.

The synthetic room, prescribed sky and simplified glass limit interpretation. Native workflow verification is distinct from the analytical and convergence checks in [scientific-validation.md](scientific-validation.md); replay agreement is integration verification, not independent physical validation.

## Reproduce the installed sequence

1. Install the documented Microsoft runtime prerequisites and separately acquire the pinned engine with `scripts/install-radiance.ps1`. Install `GreenLens_0.1.0_x64-setup.exe`, then launch `%LOCALAPPDATA%\Programs\GreenLens\greenlens-desktop.exe`.
2. Start the synthetic example and enter a fresh portable project path. Exercise invalid width and a missing explicit engine directory, then clear the engine override to use automatic discovery. Start and cancel one real run; inspect the process tree and absence of completed evidence.
3. Run A, select B with a 0.60 m overhang, run B and inspect the common scale and all table values. Edit B to 0.70 m to observe staleness, then restore 0.60 m. Save, reopen and export the original project.
4. Start another run and close the app while Radiance is running. Confirm its process tree ends and no new result is completed. Copy the saved project elsewhere and move only the two owned original scratch directories out of their expected locations.
5. Open the copy in the installed app, verify unchanged results/provenance and export it. Use `greenlens replay <evidence.zip> --engine-dir <directory-containing-bin-and-lib>` with the compatible pinned engine; every sensor must be within `max(0.1 lx, 0.1% of reference)`.

## Remaining verification limits

The exported 85,091-byte HTML report was inspected as source: two inline SVG heatmaps, one shared scale, exact calculation label, both alternatives, units, immutable assumptions and no scripts or external resource references. Automated visual rendering of the extracted local HTML was not executed because the browser tool blocks `file:` URLs and prohibits workarounds. No bypass was attempted. Users can open `report.html` directly in their browser; the installed native maps and comparison were visually inspected and independently reviewed.

A separate clean-machine installation was not tested. The documented prerequisite route and inspected imports identify what that environment needs. This release is unsigned. No claim of measured-building validation, annual analysis or certification follows from these checks.
