# Portable evidence and replay

A completed run retains normalized input JSON; generated `scene.rad`, `sky.rad`, `sensors.pts`; raw ASCII `raw.rgb`; processed `results.json` and `results.csv`; `provenance.json`; engine process records in `logs.json`; a bounded `recipe.json`; and SHA-256 checksums. Octrees and ambient files are disposable and excluded.

The ZIP adds `project.json`, compatible comparison conditions/design differences, a root checksum manifest, `REPLAY.txt`, and an offline `report.html`. Open the report in any local browser. Sensor heatmaps share a scale, values are lux, and tables/provenance remain inspectable without scripts or network access.

```powershell
greenlens export ./saved-project ./evidence.zip
greenlens replay ./evidence.zip --engine-dir <directory-containing-bin-and-lib>
```

Replay accepts only contained relative paths, rejects Windows device names and duplicate/case aliases, limits entry sizes and total expansion, checks every exported file, verifies the immutable model/results/provenance, requires the same engine/library identity and frozen numerical profile, then regenerates disposable inputs/octrees through the adapter. It does not execute commands supplied by the package.

Every sensor must agree within `max(0.1 lx, 0.1% of the stored reference)`. Failure exits nonzero. This is same-engine integration verification and does not claim independent physical validation or byte-identical stochastic behavior. Checksums detect corruption, not authorship; no cryptographic signing is claimed.

The engine is acquired separately from its upstream release; see `dependencies/radiance.json`. Personal engine paths and whole environment blocks are excluded. The replay recipe records necessary library configuration using placeholders. Run timestamps are UTC; there is no simulated date in this sky calculation.
