# Scientific verification scope

The exact product calculation is **Static daylight illuminance under a prescribed CIE standard overcast sky.** It represents a prescribed sky distribution, not measured weather, a selected date, an annual sequence or a certification criterion. All geometry and sensors use metres; results use lux.

## Reproduce the required suite

After installing the pinned native engine and development prerequisites:

```powershell
. ./scripts/dev-env.ps1
cargo test -p greenlens-radiance --test scientific -- --nocapture
```

The required test fails when the engine is missing or does not match the pinned hashes. It does not silently skip. For a standalone report:

```powershell
cargo run -p greenlens-radiance --bin greenlens-science -- --output "$env:LOCALAPPDATA/GreenLens/verification/science"
```

The output includes `science-report.json`, generated scene/sky/sensors, raw RGB, process records, arguments, engine/library identities and numerical profiles. Large/disposable octrees remain local. A diagnostic `--fixtures-only` option omits A and F and cannot satisfy the full gate. Do not infer acceptance from source code or test names: only a completed report records measured outcomes.

## Checks and unchanged release thresholds

| Check | Reference and threshold |
| --- | --- |
| A | Regenerate an octree and run the same pinned engine/settings by a direct invocation independently of the adapter process runner; every sensor must differ by no more than max(0.1 lx, 0.1% of reference). This is integration verification. Export/CLI replay has its own required evidence. |
| B | A constant upper luminous hemisphere with radiance 1 in each RGB channel has expected illuminance `179*pi` lx. Require relative error <=1%. |
| C | An unobstructed upward sensor under the prescribed sky must reproduce 10,000 lx within 1%. |
| D | Replace every surface, including the aperture, with black opaque material; all five interior sensors must remain below 0.1 lx. |
| E | Remove the glass, make all room surfaces black, prescribe black ground and independently integrate sky luminance over the rectangular opening. Each sensor must agree within max(0.5 lx, 3% of reference). |
| F | Compare the shipping profile against a profile with increased sampling and bounce depth. Mean absolute difference divided by reference mean must be <=2%; each sensor must meet max(2 lx, 5% of reference). This mean measure avoids cancellation of positive and negative sensor errors. |

Additional checks cover x2 sky-intensity scaling, a mirror-symmetric aperture/sensor pair, nonzero native exits with retained diagnostics, malformed/truncated/nonfinite output, unavailable engine and actual rtrace cancellation. A separate process-supervision unit test launches explicitly synthetic child/grandchild processes and verifies both terminate when the Windows Job Object closes. Its two ignored harness entry points are invoked by that required unit test; they are not skipped scientific acceptance tests.

## Independent aperture integral

The fixture uses the default 4 m × 6 m × 3 m room and unglazed south opening from x=1.1 to 2.9 m, z=0.9 to 2.4 m at y=0. The five horizontal sensor positions are `(2,0.5,0.8)`, `(2,1.5,0.8)`, `(2,3,0.8)`, `(1,3,0.8)`, `(3,3,0.8)` metres. Their normals point upward.

For each rectangular area element, compute the vector from sensor to element, its squared distance r² and unit vector omega. Integrate:

`Lsky(omega) * max(0, omega_z) * abs(omega_y) / r² dA`

For upper-sky directions, `Lsky = Lz*(1+2*omega_z)/3`, with `Lz=9*E_out/(7*pi)`. Lower-hemisphere luminance is zero. Midpoint quadrature uses 128², 256² and 512² cells; require the relative 256²-to-512² change to be <=0.1% at each sensor. This code does not call Radiance or reuse its ray traversal/material implementation.

The pinned `skybright.cal` blends sky and ground close to the horizon, whereas the reference integral uses the analytical upper-sky expression and zero lower hemisphere. The release tolerances are retained when checking this implementation difference. Neither the comparison nor the integration reference proves measured-building accuracy.

## Numerical interpretation

All profiles use one worker, explicit ASCII RGB output, zero initial ambient value and no reused ambient file. In-memory ambient interpolation is allowed by the profile. Each completed result records every numerical argument and the exact executable/library identity. Repeated same-engine checks assess their stated tolerance; they do not establish universal byte-identical stochastic results.

The frozen shipping profile uses `-ab 5 -ad 4096 -as 1024 -aa 0.075 -ar 192 -lw 0.00005 -lr 12`; the reference increases sampling and bounce depth with `-ab 6 -ad 8192 -as 2048 -aa 0.05 -ar 256 -lw 0.00001 -lr 12`. Both also use `-I+ -h -faa -ov -n 1 -av 0 0 0 -dt 0 -dc 1 -dj 0 -ds 0.05 -st 0 -ss 0 -u-`. Analytical fixtures use no ambient interpolation and 131,072 ambient divisions.

The [profile investigation](profile-investigation.json) retains a rejected cheaper candidate: its 2,048 ambient divisions failed one of 77 sensors against the reference. Doubling shipping sampling produced 0.7591% mean absolute difference divided by reference mean, with every sensor inside the unchanged tolerance in the exploratory direct runs. The default example took approximately 349 seconds for this shipping candidate and 1,229 seconds for the reference on the development machine. These timings are observations, not guarantees. Full Rust adapter results are recorded separately; this investigation alone is not the release acceptance record.

Original-code unit tests validate the bounded geometry, split aperture, sensor tiling, coordinate transforms, finite input bounds, transmittance conversion and comparison/staleness rules. The equivalent single-pane, neutral diffuse surfaces, rectangular geometry and prescribed azimuth-symmetric sky limit interpretation. A simulation metric is not a criterion evaluation, and neither constitutes formal certification.

## Measured status

Native Gate 0 and the initial exterior normalization result are recorded in [engine-feasibility.md](engine-feasibility.md) and [gate-0-evidence.json](gate-0-evidence.json). The numerical profiles are frozen after the documented investigation. The [Rust fixture record](science-fixtures-evidence.json) confirms B–E, scaling, symmetry, failed-exit handling and actual rtrace cancellation in a working directory containing spaces and non-ASCII characters. B differed from pi*L by 0.13310%; C differed from prescribed exterior illuminance by 0.16905%; D produced zero at all five sensors; E's largest error was 1.98187%, with maximum final quadrature change 0.00060843%. Full A/F Rust suite evidence is pending execution; this document does not claim Gates 1–3 passed.
