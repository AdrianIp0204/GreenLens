# Visual design checkpoint — source 2d98b03

Recorded 2026-10-18. This is a source-only evidence receipt, excluded from the packaged documentation selection. It supplements the [bounded continuation](../../v0.8-report-design-continuation.md) and the prospectively fixed [VD1 protocol](../../v0.8-visual-design-protocol.md). It does not accept incomplete v0.8 or earlier release requirements. External receipts remain authoritative for complete runtime details; this record omits local paths and personal identifiers.

## Status at this checkpoint

| Work | Status and scope |
|---|---|
| B-only design proposals and matched analytical report figures | Implemented; focused source checks passed. Six retained-model SVGs independently checked and individually reviewed. Installed interaction and PDF embedding/page review remain unverified. |
| VD1 real annual executions | Verified: the four predeclared A10/B10/A5/B5 slots completed once each, with exact shapes, finite outputs and integrity-checked retained evidence. No retry or input tuning. |
| Exact-pair numerical sensitivity | Verified: all 20 fixed criteria passed, comprising four electricity/peak comparisons and 16 zone-comfort checks. This does not promote scientific qualification. |
| Independent retained-data arithmetic | Verified within the stated accounting scope: 1,912,300 checks passed in 7.019 seconds. This is not independent building-physics validation. |
| Production build and installation | Release CLI and unsigned NSIS package built; installation returned exit code 0 on the existing workstation. No clean-machine claim. |
| Final-source hosted verification | [Run 39099636177](https://github.com/AdrianIp0204/GreenLens/actions/runs/39099636177) was in progress when this initial receipt was prepared. Its outcome must be refreshed before native acceptance. |
| VD1 packaged report and conditional cold regeneration | Failed during the one installed issuance, before rendering. No new issue accepted; conditional cold regeneration remains unused and its prerequisite failed. See the stopped-outcome supplement below. |
| RC1 complete retained reporting closure | Failed and consumed before rendering; no R2 or conditional regeneration. The subsequent observer source repair does not reopen that allowance. |
| CT1 containment investigation | Source-blocked before execution; zero synthetic containment cases consumed. |
| OpenSKP matrix, app adoption and private probe | Gated and unexecuted. The private-probe allowance remains unused; no private geometry was decoded. |

## Application and package identities

Application source: `2d98b03d96d844750608cc879a585d4ab20290af`, branch `codex/v0.8-report-design`, version 0.8.0. The receipt `installed-identity.json` was recorded at `2026-10-18T05:22:51.4819940Z`, before the native observation; its SHA-256 is `5ef44fc871f42f24bb87911207031f06fce6f1e8b65e5f7f1069f9a524c25234`.

| Artifact | SHA-256 |
|---|---|
| Release CLI executable | `22f74f3571d1de024cc5065b1203ee77d97a7803cd4fd3701377cfe19abaa6dd` |
| Unbundled desktop executable | `e27a255b7e9779ebef7d1b3eabb1255240b7fe950fc2a327548e485fa2845b6c` |
| Installed desktop executable | `8f78dfca0720d547bb98b901c9f1b5b8d10861c78ab9cd2c97ea9d713d650c3b` |
| `GreenLens_0.8.0_x64-setup.exe` | `8c60945db67cb6e17a51f2941befeca6b0dea4900787daf29d657aecf0d43795` |
| `GreenLens-CLI-0.8.0-windows-x64.zip` | `c5fa323d996d83932465bb3afe25e553ede41aa08860bfd54788b1d3360998c0` |

The desktop build log records a successful optimized release build and NSIS bundle. The installed executable differs from the unbundled executable only at offsets 27,180,016–27,180,018, where the Tauri bundle marker changes from `UNK` to `NSS`; remaining bytes were identical in the retained installation check. These are distinct artifact identities, not interchangeable hashes. The installer is unsigned. Installation success does not establish that a PDF was displayed, a report issued, or the complete desktop workflow completed.

## Fixed analytical proposal and comparison conditions

VD1 is an original synthetic four-zone, two-storey example, with four 8 × 6 × 3 m zones, 192 m² declared gross/conditioned floor area and 576 m³ volume. A and B were first made exactly equal through the shared validated service. The single proposal adds an overhang to `zone3_window` on `zone3_South`: projection 0.60 m, side extension 0.10 m, gap above opening 0.10 m. Baseline A has no overhang there. Independently reconstructed overhang vertices are `[1.9,0,5.6]`, `[5.1,0,5.6]`, `[5.1,-0.6,5.6]`, `[1.9,-0.6,5.6]` metres in the declared analytical coordinates.

The typed comparison reports four field rows for this one object addition: object presence and its three dimensions. It reports unchanged service inputs and compatible zoning, weather and calendar. No other envelope, glazing, construction, HVAC, fan, occupancy, lighting, equipment or schedule input was changed. The geometry audit does not infer a real building's north or location.

| Binding | Identity |
|---|---|
| Baseline model | `cefc5cd4c98d0ef50d1e1343a5df1583337ef0e37e8a7365565c154873b9f7fd` |
| Proposed model | `e50ed49bee1fd0991a172cd1ff35c3259c1a0b20c53d90ebb406fc0876985798` |
| Common ten-minute study | `114646b1c45d824e447d1080fbcecc37d899af82fb2276dcb1040cf9d15ac9ec` |
| Original synthetic weather bytes | `d648c9ab29f9b4c7953d153f15085e62c9e726ade50f0c1a65d94cdc5574618a` |
| Normalized weather | `f4b2b66e599a6c4951c5ef24ff39cf1e705c748f6fe7322ff791ff5249acd5ae` |
| Weather time axis | `bc39582473077ca6bcaf3a8cf172197a623c7d898aff6f21dbecacafc04112e4` |

Weather admission remains `greenlens.energy-epw-explicit-fields.v1`. The weather is explicitly non-climatic synthetic data at a fictional equatorial location. It does not describe the private architectural case. The calendar remains Monday start, January 1 holiday and fixed UTC offset zero; no operating-system timezone or daylight-saving interpretation is introduced.

The design hypothesis investigates fixed external shading. Daylight, view, material use, embodied carbon and construction impacts were not calculated. Typed input differences alone do not establish causal attribution, service adequacy or environmental superiority.

## Native engine execution and retained bindings

The exact source/executable/input declaration was retained before the first launch. `execution-declaration.json` SHA-256 is `0d4a2fe5806f7409043c7e4c2d2ba49062c3a6d7745afaa27d93e1abb19d9204`; the prospectively declared protocol bytes have SHA-256 `abb8e9227983d859a1986c352ae9730e041b6b6c225a1f9a980b5cbf9aa66140`.

Engine identity:

| Field | Value |
|---|---|
| Method / adapter | `greenlens.energyplus.explicit-dx/1` / `1` |
| EnergyPlus | `26.2.0` |
| Package SHA-256 | `15ab698803f459444ec3e97ee52ab9f99db93a583819c7821b17dfcd7b4461e7` |
| Executable SHA-256 | `71278a62c9c3871922b3f7ee28a3c7c9f658caa995a934fccb92e8f8965a3977` |
| Schema SHA-256 | `28ba2e4267d79e69331c9988ec52f0211a86077629f0336c46b1cc9103e63c44` |
| Recipe SHA-256 | `b487b38659180648924b81d9348795cad204efdff5c26848ee3f332bbf68498c` |
| Ten-minute numerical settings | `58a8dbf3bcf52df9c626b3e3ec608ebdbb5b824f125c0f11dced662c2ba2ec82` |
| Five-minute numerical settings | `90227a6bf39574b0f1872a18361bae811780a3804530fd11c8c5f857f138cd63` |

Each engine launch used this argument array in its owned working directory:

```json
["--jobs","1","--weather","weather.epw","--output-directory",".","--output-prefix","eplus","../model.epJSON"]
```

| Slot | Run ID | Canonical retained record SHA-256 | Shape |
|---|---|---|---|
| A10 | `51ff925a-755d-437d-a3f8-a615a55e50cb` | `545372f539fdbf30133191f02a4f49cd21623be7be96a0c6df0b4472cd1310c8` | 52,560 × 34 |
| B10 | `4e66b264-24b2-426c-a335-78e6f622931f` | `dea78cdc5fe7603f4006da34426dfbf6d55e0b551708a4933ed1ac5d1c3db3ec` | 52,560 × 34 |
| A5 | `7e47460e-9eca-4072-92a9-52ef8de7a46d` | `4042d2255505fd88a0b16a0460c8d89bb9ae2eecc06fa63877c621416a1a4671` | 105,120 × 34 |
| B5 | `e1615e5d-842b-492a-a379-ddfa75a2f82d` | `2412b832f721af3eea75a15378507b712a1936bef347e6fe2a9cae42134f7968` | 105,120 × 34 |

All four executions returned exit code zero, completed evidence collection, and recorded an empty owned process tree at normal completion. Each retains one native warning; warnings were not discarded. No live-cancellation experiment was performed. The final shared-service inspection reprocessed immutable series, original weather, recipe, inputs and process receipts and reported every declared run integrity-checked, current inputs matching, no missing slot and no interruption.

| Slot | Shared-service call wall time (s) | Engine process wall time (s) | Sampled working-set peak (B) | Scratch peak (B) | Retained run members / bytes |
|---|---:|---:|---:|---:|---:|
| A10 | 27.5723787 | 12.231 | 420,708,352 | 19,753,241 | 72 / 22,548,683 |
| B10 | 49.4884317 | 16.835 | 421,916,672 | 19,754,881 | 72 / 22,549,489 |
| A5 | 97.6393740 | 19.075 | 603,648,000 | 37,544,158 | 72 / 36,845,022 |
| B5 | 70.5968946 | 24.203 | 606,314,496 | 37,545,019 | 72 / 36,845,824 |

Call wall times include preparation/collection and are distinct from engine-process times. The resource receipt measures sampled runner-plus-Job **working set**, not raw aggregate committed-memory containment. It does not satisfy the separate converter/report committed-memory acceptance criteria. Hardware context: Intel Core i7-13650HX, 14 cores/20 logical processors, 25,463,480,320 bytes installed RAM; results are local observations, not a general benchmark promise.

## Measured outcomes and numerical sensitivity

Five-minute demand was aligned to common ten-minute intervals before peak comparison. The table deliberately does not compare unrelated end-use maxima or a five-minute instantaneous peak with a ten-minute peak.

| Slot | Annual facility electricity (kWh) | Common ten-minute coincident facility peak (kW) | Maximum zone occupied cooling-unmet (%) |
|---|---:|---:|---:|
| A10 | 16,307.089802617933 | 5.516090229475 | 0 |
| B10 | 16,304.927259000930 | 5.511688754590 | 0 |
| A5 | 16,305.190534702979 | 5.514467140553 | 0 |
| B5 | 16,302.937448191064 | 5.510096967142 | 0 |

All common ten-minute facility peaks occurred on December 28, 17:20–17:30 local standard time, interval 52,088. Peak end-use contributions are taken at each case's facility peak. The declared area denominator is 192 m². Zone 1/3 each have 5,096 occupied hours; zone 2/4 each have 4,576. All 16 zone/run cooling-unmet counts are zero at the fixed 0.2°C threshold. The building occupied-time union is 6,136 hours and is descriptive; it does not replace individual zone acceptance.

| Frozen criterion | Measured difference (%) | Limit (%) | Outcome |
|---|---:|---:|---|
| A annual electricity, 10 versus 5 minutes | 0.011648241159 | 1 | Pass |
| B annual electricity, 10 versus 5 minutes | 0.012205228758 | 1 | Pass |
| A aligned ten-minute facility peak | 0.029433286669 | 3 | Pass |
| B aligned ten-minute facility peak | 0.028888556015 | 3 | Pass |
| Every conditioned zone in every run, occupied cooling-unmet | 0 (16 checks) | 1 | Pass |

Sensitivity session `e4545025-f740-49cc-af2a-7ff9555dae6c` binds exactly these four runs. Plan SHA-256: `1b61b40db1312528e29366382bedbb291e3b83dfcbf5a8fa82d9fab4f2fe792d`. Plan-file SHA-256: `be1575474543a89ab5a6eb122ec61f1122d3ad8713c1b48ea77a71ed087a49f4`. The finalized result retains all 13 orchestration event bindings; `sensitivity-final.json` SHA-256 is `1757d78465e0bb60ef5a39e30c7b1a89065df4a98be576d5819d5f49d5c8da81`. Its `qualification_promoted` value is false.

A10 minus B10 is **2.162543617003 kWh/year (0.013261370626%)** and **0.004401474884 kW peak (0.079793380841%)**. These small modeled changes are not a net environmental-benefit finding. The annual ten-/five-minute absolute differences are 1.899267914954 kWh for A and 1.989810809866 kWh for B, of comparable magnitude to the proposed shade's annual change. Passing the frozen sensitivity limits does not establish that such a small design difference is physically meaningful. No calibrated accuracy, equipment-curve-domain qualification, daylight benefit, embodied-carbon benefit or certification is established. The original synthetic weather and declared simplifications limit interpretation.

## Independent accounting audit and capacity

`independent-arithmetic-audit-01.json` SHA-256: `75b04497aa080351c543c24476b31a54e9ecab971275e4833f68326523a07049`. Audit-script SHA-256: `433631d6967c2f11e4bc63be27674cdf3ca5e2a0b812dff0b5f7d2b0c079be19`. The read-only external audit passed 1,912,300 checks in 7.019 seconds. It independently read complete source rows and 34-channel dimensions, recomputed retained byte checksums, arithmetic sums/means, coincident peaks, alignment and occupied comfort, and checked the single declared geometry difference. Canonical Rust JSON identity claims were matched to fixed declarations; that does not substitute for the shared Rust integrity inspection. No engine, renderer or report generation occurred in the audit.

Before the native report operation, the capacity service inventoried **308 retained members / 127,151,438 bytes**, with **4,692 entries / 141,284,018 bytes** remaining under the unchanged 5,000-entry/256 MiB aggregate limits. The largest retained entry is 5,300,731 bytes, below 64 MiB. This inventory is not an integrity claim or a guarantee that an unknown final render will fit. There is no prior design report from which to estimate incremental product bytes; exact completed-product admission remains mandatory before publication. No history was pruned and no assets omitted to obtain headroom. The disk-manifest and bounded-history policies remain unchanged.

## Source checks and distinct acceptance gaps

The source implementation checkpoint retains successful workspace checks (123 core and 277 runner unit checks at that checkpoint), final changed-feature checks (13 core, 31 runner, strict Clippy), 41 focused frontend component/draft/response checks, and actual workspace composition covering one B-only transaction, undo/redo and invalid/stale preservation. The full frontend checkpoint passed 472 cases before three additional detached-shade checks; the final focused proposal component file passed 19 cases. These source tests exercise real components where specified but do not establish native gestures, engine execution or rendered-page quality.

A concurrent Radiance unit run retained a failure in the 450 ms synthetic descendant-startup assertion (46 passed, one failed). The unchanged isolated case passed; the suite then passed 47 checks with seven explicit ignores at the hosted two-thread setting. Both failure and follow-up receipts remain. Ignored native checks are not acceptance, and this is not a restart of stopped daylight qualification. Observer helper checks passed 54 new/135 supervisor/26 cold cases; the controlled synthetic lifecycle harness passed 80 checks with 12 harmless children and empty owned Jobs. Strict dependency inventory recorded 415 packages, 231 notices and zero gaps. Packaged-document selection recorded 414 files, 804 local links and 16,776,789 expanded bytes including its generated index; this additional detailed receipt is source-only.

RC1 remains a separately consumed failure before rendering: its short-lived approved runtime child exited before a later managed PID lookup, producing an observer lifetime race. No R2, Typst or EnergyPlus execution followed. All 580 predeclared evidence members / 225,550,938 bytes were preserved; only the excluded zero-byte runtime lock was added. The held-process-handle source repair passed [observer CI 39098580701](https://github.com/AdrianIp0204/GreenLens/actions/runs/39098580701), without retrying RC1 or establishing its reporting acceptance.

CT1 remains stopped before execution for the allocation/serialization and raw-peak gaps documented in the [prospective containment decision](../../v0.8-containment-prospective.md). The historical 384 MiB raw-peak failure remains 438,923,264 observed bytes against 402,653,184 bytes. No subtraction, weaker memory measure or larger limit resolves it. Pinned OpenSKP main remains `dceb9fff0efc9668961530b535d5375c001343dc`; the independent legacy/modern fixtures remain unparsed by the candidate. No CT1 case, fixed matrix conversion, adopted app worker or private probe was executed. This is neither proof of source-file corruption nor a claim that future bounded containment is impossible.

The prospectively declared VD1 report operation retained the 600-second operation deadline, raw 2 GiB aggregate committed-memory acceptance ceiling, stricter 1 GiB Job guard and 16-process limit, with zero EnergyPlus during rendering. Its stopped result is recorded below. Every changed page/figure, offline PDF delivery, full Save As/export, cold inspection and deterministic regeneration remain unverified for this candidate. Prior native reimport/document-change, physical modifier, exact client-size/DPI, clean-machine and live EnergyPlus cancellation gaps remain separately open. The release is incomplete.

## VD1 installed issuance — consumed and stopped

The one VD1 packaged observation was reserved before launch at `2026-10-18T05:25:49.3126256Z`. It used the exact installed 2d98b03 executable and unchanged genuine A10/B10 pair above. The source-bound [observer run 39099636112](https://github.com/AdrianIp0204/GreenLens/actions/runs/39099636112) had passed; broader Windows CI 39099636177 remained pending. The prospective native declaration pinned renderer `greenlens.analytical-design-report-renderer/1`, Typst executable SHA-256 `081217a463adb006f8894b44227fe4b9c9e91fc85f5463d5948e8370db9bb31e`, template SHA-256 `be937ae7386ff76a9e0e3ae245742047134667043e158eb1ff4f901b8832fe55`, and runtime declaration SHA-256 `ada6d02d29e5f64eadd2b6a36a9fb9a13c1056d46bff37fe4f47601899e2213c`.

The observer armed the exact analytical scope and selected pair at `2026-10-18T05:33:31.4297994Z`, with manifest SHA-256 `b5d851485075cfee767843e47f960cd8fab8c44ea4037ac148bef431d64f4ea6`. The native UI operator clicked **Issue** once. The app reported **“No simulation has been started.”** No new report was accepted. The report action's existing pre-action Save recorded display state automatically; no separate post-failure Save click is claimed. This is a substantive application/workflow failure before rendering, not a failed annual execution or evidence that the retained runs are missing. Subsequent source inspection identifies the session-readiness check described below.

The operator wrote the declared stop marker and requested close with Alt+F4. The collector then exited with code 1. Its failure finalization records `normal_close=false`, `forced_owned_job_kill=true` and `cleanup_job_empty=true`; the observed close request must not be reported as a passed normal-close acceptance. The collector's success flag is false, with stage `armed-report` and a diagnostic containing the visible application error. All eight retained identity handles were closed; output draining recorded zero stdout/stderr bytes and no overflow. No unresolved identity or cleanup failure was reported.

| Observed quantity | Value |
|---|---:|
| Total observation wall time, including navigation | 503.110732 s |
| Completed samples | 2,406 |
| Maximum simultaneous members | 8 |
| Raw aggregate Job committed-memory peak | 490,807,296 B |
| Sampled aggregate private-memory maximum | 465,842,176 B |
| Observed Typst processes | 0 |
| Observed EnergyPlus processes | 0 |
| Accepted new issues | 0 |

No renderer ran, so the recorded total observation duration and memory peak are not report-render performance measurements. No PDF page/figure was generated or reviewed, and the single conditional cold regeneration was not attempted. The issuance allowance is consumed; an ordinary source fix does not authorize an automatic native retry. RC1 remains independently failed and is not replaced by this synthetic case.

The actual collector files are `report.json`, `protocol.json`, `armed.json`, `armed-members.json`, `before-members.json`, `arm-render.json`, `arm-render.template.json`, `state.json`, `stop-observation.json`, `stop-observation.template.json`, `stdout.bin` and `stderr.bin`. There is no `failure.json`; the failed outcome and diagnostics are in `report.json`. Complete diagnostic files remain external and are not packaged as successful report products.

| Receipt | Bytes | SHA-256 |
|---|---:|---|
| `report.json` | 5,680,354 | `4104d3438477e47ccc2ed133fca1e1371a54eca6616e46e6e8bb289d23942192` |
| `protocol.json` | 3,938 | `1c4ce237944dff6fca8bd92e0837a7fb3608f78ceb7404240642ce97d7a53f40` |
| `armed.json` | 925 | `6f67bff3d29d05978a92c0470a8484a1f0959cfd04208258e9c97245397c4932` |
| `stop-observation.json` | 473 | `1ec2dd395c239cf5f23bb4d8ee006478072d2da8805cc89eb7dc9edff7bb9ef6` |

The final saved manifest hash is `149717252b82df71a163f005cefd153f23bcc80a89fbc158004d2a0ad25ceffe`; it is not byte-identical to the armed manifest and must not be described as such. Independent preservation audit `native-failure-preservation-audit-01.json`, SHA-256 `a926593fdd84149d304b3332be5d1d95455ee555660eb37970403bb346d53393`, verified all **307 non-manifest members unchanged**, no missing or added retained member, unchanged genuine run IDs and no accepted design-report directory/products. Only `/display_state` was added to `project.json`: 985 bytes, from 210,288 to 211,273 bytes. Removing that display-only field reconstructed the exact old manifest bytes. The complete retained inventory is therefore 308 members / **127,152,423 bytes** after the save. Three zero-byte mutable lock files were explicitly excluded from the immutable inventory. The audit took 0.244 seconds and the manifest remained stable during it. It establishes filesystem preservation, not renderer execution or viewing.

The four completed native simulations and their prior arithmetic/sensitivity evidence remain separately recorded; no new simulation was requested by the failed report operation.

## Source-only readiness repair — focused checks passed

Source inspection found that the new Tauri analytical-report action called `Application::status()` to reject active calculations. That method intentionally returns “No simulation has been started” in a fresh application session. Its error propagated before report preparation even when the opened project contained the exact genuine retained A/B evidence. A fresh session and absent retained evidence are different conditions.

The source repair introduces a shared `Application::ensure_report_ready()` guard and calls it from the Tauri design-report action. It permits no active session job, rejects queued/preparing/running jobs, rejects pending completed evidence, and fails closed on poisoned state. It does not collect/discard evidence, change ordinary status semantics or start a simulation. Added focused regressions cover cold-session readiness, live/terminal states, pending evidence preservation and poisoned mutexes. A separately explicit retained-evidence regression prepares the exact genuine pair without engines/rendering and exercises complete-copy/pending collection with byte preservation; it requires supplied retained records rather than fabricated accepted runs.

All four control-state checks passed. The genuine retained-evidence case, explicitly invoked separately from its ordinary ignored-by-default entry, also passed in 210.32 seconds with the exact VD1 A/B records: cold report preparation, pending-evidence refusal, complete-copy collection and original/copied byte-preservation assertions. This test launched zero engine or renderer processes; restoring a job control state from a verified retained run is not a new native calculation. Strict workspace Clippy for all targets passed in 16.59 seconds; formatting checks also passed.

| Source-repair check receipt | SHA-256 |
|---|---|
| `readiness-tests.txt` — four passed; genuine case initially not invoked | `2ea4fc0d98b9b4f99c624be36419f7b69698ee4ad2d0d2952210a1032969f4a0` |
| `genuine-retained-readiness.txt` — explicit genuine case passed | `1cba0145de086c419496642599a399c62bb1fb9c6fb2c60c35ae99ca879fbd97` |
| `clippy.txt` — strict workspace/all-targets check passed | `d6574752170d4cb1ddff2e9f1faed9c186529850e8927a54da78462f6bdf8262` |

The refreshed packaged-document check passed 414 selected files / 804 links / 16,776,870 expanded bytes including the generated index, leaving 346 bytes below the unchanged 16 MiB limit. This detailed receipt remains source-only. The readiness change is newer than installed application source 2d98b03 and has no installed acceptance or replacement package identity here. No source-only check, read-only figure extraction or later successful build can turn the consumed VD1 issuance into a pass or authorize a retry. Final-source CI and the blocked native report/conditional regeneration status must remain separately visible.

## Standalone retained-geometry figures — verified separately

After the native stop, the read-only CLI `design figures` action inspected the same retained A10/B10 evidence and emitted `retained-paired-figures.json`. This invokes the shared integrity and drawing services, not report issuance, Typst or EnergyPlus. Its figure method is `greenlens.analytical-paired-drawing/1`, content identity `a9da3e36859c982135951604a91573cc81b5f175b936706f15020d2ffc5ac748`, and complete file SHA-256 `1e97fdc803ba535fc752feb22eb57d5d69101e7a617026908872770bbc06f1dd` (345,363 bytes). It binds the exact retained model/run identities above, with no invented reference or scope revision. A read-only reproduction command is:

```powershell
greenlens design figures "$Project" --a 51ff925a-755d-437d-a3f8-a615a55e50cb --b 4e66b264-24b2-426c-a335-78e6f622931f
```

The independent projection audit passed **7,461 checks**, comparing reconstructed analytical endpoints with the retained three-decimal SVG coordinates within **0.000501 SVG units**. It checked all six SVG hashes, complete object changes, matching A/B frames and scale, and the declared overhang. The axonometric frame uses eye `[9,1.7,3.8]` m, target `[8,2.7,3]` m, Z-up and vertical span 15.446954878884803 m. The plan uses eye `[8,2.7,4]` m, the same target, Y-up and span 11.8 m. Both aspect ratios are 1.6; neither has a section. Each A/B/overlay set shares its exact frame, extent and 2 m scale bar. All eight zone/window labels are present in each drawing, with zero omitted labels.

| SVG | SHA-256 | Analytical edges |
|---|---|---:|
| `design-axonometric-a.svg` | `5f9ebf423280e35ace0eabd3564b659df3e94c84dc688d22057e655d0e035323` | 112 |
| `design-axonometric-b.svg` | `09545dee84b3442a47b0114f6e733fd5f265165a5419eb4c91520f638a8cd9d6` | 116 |
| `design-axonometric-overlay.svg` | `8c0ee906c4c59bddf0f37ad3723f8094bcea47ab64610318ea5442d0ec6283e0` | 116 |
| `design-plan-a.svg` | `849e72330ffa3409e6405a2d0985666ecc3c711eab20f8aed73e8207dc4229a9` | 72 |
| `design-plan-b.svg` | `d3ba40649ad4b16b1e414e2e87a24003772318dff6cc004bb33e6c6ecdee1b1a` | 76 |
| `design-plan-overlay.svg` | `1f94b378fcc1295a2c89226d82dafdf346d65eccd8cb544f0233bd14e4e39fa1` | 76 |

The complete change data identifies `overhang:zone3_window` as added and its containing zone/surface/window as property-modified. The additional four outline edges are not invented changes to walls or zone volumes. True north remains explicitly unknown in these report figures. Input-change colours are not spatial thermal results. The baseline's inherited LimitedQualified status and the proposed case's Unassessed status remain separate; passing the exact-pair sensitivity assessment does not transfer qualification.

The existing Chromium 148.0.7778.96 installation, controlled through Playwright 1.62.1, produced external 1200 × 1140 review PNGs using the pinned reviewed font. Browser executable SHA-256: `290fa7018fda22c52ada5eddb0113baf3ebc41fd0fde6085eddb19793606c635`; font SHA-256: `dce08bd4fd91aa8aa76ed8fea4b694c2dfb8550f67871e326843212ddbeb88b4`. Page requests were intercepted: only the in-memory pinned font was fulfilled; all other requests were blocked. No source URL was fetched. Automated browser review took 8.214 seconds and detected no text overflow beyond the whole canvas. That check is narrower than visual page review.

The reviewer inspected all six individual PNGs and their contact sheet. Run/scientific status, metre scale, legends, unknown north and scope-excerpt disclosures were visible. No text-to-text collisions or canvas-clipped text were observed at that size. The A/B/overlay axes and extents were consistent and the green added shade appeared only on B/overlay. Transparent plan projection intentionally superposes both storeys; all labels remain present and separated. Some wireframe lines cross labels, whose text halos preserve readability. These are technical outline drawings, not hidden-surface architectural renderings. The initial inspection script expected the word “metres”; it was corrected to recognize the actual explicit `2.0000 m` scale label without changing geometry, evidence or tolerance.

| Review receipt | SHA-256 |
|---|---|
| `standalone-figure-audit-01.json` | `962d9c4bc8f73055d6370fb52f8705745816e4b59f04cf70136ff119e99d6ff1` |
| `standalone-figure-browser-review-01.json` | `9a6a488964dc09c131389e99db316d69757ab0e121c64c2e500cc525d9430011` |
| `standalone-figure-visual-review-01.json` | `b0c879a4b1d42453332a36ffd4e53109908de6db7b9ffe48bb3e54f045b63453` |
| Contact-sheet review image | `55c374882f84de8b4315c1fe331d1c60d5f30c6fd97ce82c41d56a710d477850` |

All review images remain external; none is an issued artifact or included in public source. This supplement establishes standalone analytical figure content and screen-size legibility only. It does **not** establish PDF embedding, printed-page readability, an accepted report issue, native PDF viewing, cold regeneration, private geometry compatibility or physical validation. The failed native issuance and unused conditional regeneration remain unchanged.
