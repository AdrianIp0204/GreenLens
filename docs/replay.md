# Portable evidence and replay

GreenLens replays immutable saved results, using their recorded scientific method and settings. Static overcast and climate results have different evidence formats and replay tolerances. The current climate method is [under investigation](v0.3-scientific-method.md); successful replay does not establish its scientific acceptance. See [progress](progress.md) for observed results and failures.

## Static prescribed overcast

The calculation label is **Static daylight illuminance under a prescribed CIE standard overcast sky.** There is no simulated date, weather sequence or annual occupancy schedule in this mode.

A completed static run retains normalized input JSON; generated `scene.rad`, `sky.rad`, `sensors.pts`; raw ASCII `raw.rgb`; processed `results.json` and `results.csv`; `provenance.json`; engine process records in `logs.json`; a bounded `recipe.json`; and SHA-256 checksums. The legacy export includes all retained static runs, compatible A/B comparison conditions/design differences, `project.json`, a root checksum manifest, `REPLAY.txt` and an offline `report.html`.

```powershell
greenlens export ./saved-project ./evidence.zip
greenlens replay ./evidence.zip --engine-dir <directory-containing-bin-and-lib>
```

For static runs, **every sensor** must agree within `max(0.1 lx, 0.1% of the stored reference)`. This preserves the published v0.1/v0.2 threshold and scientific identity. Legacy static evidence inspection does not require an installed engine; static replay requires the pinned `gensky`, `oconv`, `rtrace` and libraries, without requiring annual executables.

## Selected weather hour and annual climate studies

Climate exports require explicit completed run IDs. The selected export can also include static run IDs in the same ZIP:

```powershell
$a = 'replace-with-completed-A-run-id'
$b = 'replace-with-completed-B-run-id'
greenlens export ./saved-project ./climate-evidence.zip --run $a --run $b
greenlens replay ./climate-evidence.zip --engine-dir <directory-containing-bin-and-lib>
```

The package retains the selected immutable model/study snapshots, generated text inputs and sensors, original and normalized weather, time-axis/source/rights records, chunked parsed RGB and lux values, annual metrics where applicable, recipes, process/resource records, provenance and checksums. It also contains the portable project, comparison conditions/design differences and offline report. A selected-hour result contains one sensor vector at its original weather interval index; an annual result contains all 8,760 hourly intervals. The numeric chunk format is documented in [climate contracts](v0.3-contracts.md).

Climate replay regenerates disposable sky/transport matrices and octrees from validated retained weather and geometry through the recognized compiled adapter. It requires the complete matching engine/library identity for that method. **Every retained interval and sensor** must agree within `max(0.5 lx, 0.5% of the stored reference)`. Annual replay covers the full year, not only the offline report's explicitly labeled hourly preview. Inspect other saved hours or a sensor's series through GreenLens or the [CLI result commands](v0.3-usage.md).

The recognized historical method-2 and current candidate method-3 recipes dispatch to their own complete allowlisted profiles and transport branches. Old fingerprints and evidence bytes are preserved. Recognition and successful replay do not make a historical result current, nor prove that a provisional profile meets matched-sky or annual-convergence requirements. Unknown versions, mixed method identities and altered profiles fail explicitly; recorded command strings are never executed. See [compatibility rules](v0.3-contracts.md) and the [provisional method contract](v0.3-scientific-method.md).

Climate timestamps describe the retained weather's fixed local-standard intervals and representative midpoints. They do not use the computer's timezone or daylight-saving rules. Typical-year source years remain provenance; they are not a forecast or an inferred weekday schedule. Run execution timestamps are UTC in both modes.

## Integrity, portability and interpretation

Replay accepts only contained relative paths, rejects Windows device names and duplicate/case aliases, checks archive seals and the validated scientific records, and enforces the shared 5,000-entry, 64 MiB-per-file and 256 MiB-uncompressed limits. A missing engine, invalid package, unsupported recipe, failed process, incomplete result or failed numerical comparison exits nonzero. A mixed package applies each run's own static or climate tolerance.

Open `report.html` in a local browser. Reports are available offline without an engine or original scratch. Climate reports retain the selected snapshots' metric conventions; changing current display thresholds does not rewrite exported history. Hourly values/differences use lux, annual occupied-time metrics use percent, and metric differences use percentage points. Compatible A/B maps share scales. A mixed selected export links its static evidence through `static-report.html`.

Copy the whole saved project, including its referenced run and weather directories. Reopening, inspection and export use retained evidence without the original import path or job scratch. Engines, octrees, coefficient caches and large intermediate matrices are excluded from packages. Acquire the compatible engine separately; see [installation](installation.md) and `dependencies/radiance.json`. Personal engine paths and whole environment blocks are excluded from portable provenance.

Replay is same-engine integration verification, not independent physical validation, certification or a claim of byte-identical stochastic output. Checksums detect corruption, not authorship; no cryptographic signing is claimed. Analytical and convergence acceptance remain separate requirements with their own unchanged limits.
