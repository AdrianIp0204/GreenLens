# Retained reference depth discrepancy

This data-only fixture reproduces the existing A4116 and B8512 comparison of `-ab 9 -lr -16` with `-ab 10 -lr -24`, using all 24 sensors and 256 original irradiance samples per sensor. Each case failed 10 of 24 unchanged sensitivity bounds, `max(0.1 lx, 1% of baseline total)`. These are historical native outputs; packaging started no engine. Internal repeat stability did not resolve the depth discrepancy. The reference family remains **unqualified**.

Eight unique physical input files total 10,373 bytes. Shared weather attribution adds 2,711 bytes. Four complete original ASCII outputs (6,144 rows × 6 channels each) are retained as lossless gzip, totaling 277,556 bytes. `manifest.json` pins each original/compressed byte stream, the exact eight process roles, helper code, engines/libraries and published component evidence. No private baseline archive is needed for this narrow reproduction.

The scene/sensor code is Apache-2.0: retain the repository [LICENSE](../../../LICENSE). `matched-basis.cal` derives from Radiance: retain [Radiance-LICENSE.txt](../../../third-party/Radiance-LICENSE.txt). The two hourly sky fields derive from the normalized PVGIS/JRC/ERA5 typical-year source; retain the complete original [weather attribution](shared/weather-asset.json), including the modification and Copernicus notices. No engine binaries are bundled.

## Inspect all retained values without running Radiance

Use a fresh native PowerShell 7.2+ process, starting in the repository root. This reuses the existing C# aggregator. It verifies every bundled member/helper and the component records, copies only physical inputs into a new local cache, bounded-decompresses all four raw streams and reproduces all 48 comparisons. It does not run native transport.

```powershell
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path '.').Path
$fixture = Join-Path $repo 'tests/fixtures/reference-depth'
$m = Get-Content -LiteralPath "$fixture/manifest.json" -Raw | ConvertFrom-Json -AsHashtable
function Hash([string]$p) { (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() }
foreach ($p in $m.files.GetEnumerator()) {
    $f = Join-Path $fixture $p.Key
    if ((Hash $f) -cne $p.Value.sha256 -or (Get-Item -LiteralPath $f).Length -ne $p.Value.bytes) { throw "Fixture differs: $($p.Key)" }
}
foreach ($p in $m.helper_sha256.GetEnumerator()) {
    if ((Hash (Join-Path $repo $p.Key)) -cne $p.Value) { throw "Helper differs: $($p.Key)" }
}
function New-CaseInputs([string]$root, $case) {
    $dir = Join-Path $root $case.case
    New-Item -ItemType Directory -Path $dir | Out-Null
    foreach ($name in $case.members.Keys) { Copy-Item -LiteralPath (Join-Path "$fixture/$($case.case)" $name) -Destination (Join-Path $dir $name) }
    foreach ($name in @('sensors.pts','matched-basis.cal')) { Copy-Item -LiteralPath "$fixture/shared/$name" -Destination "$dir/$name" }
    Copy-Item -LiteralPath (Join-Path $dir $case.depth_source_alias.copy_exact_from) -Destination "$dir/depth10-24.rad"
    $sensorBytes = [IO.File]::ReadAllBytes("$dir/sensors.pts")
    $pts = Join-Path $dir $case.commands[1].input
    $stream = [IO.File]::Create($pts)
    try { for ($i=0; $i -lt 256; $i++) { $stream.Write($sensorBytes) } } finally { $stream.Dispose() }
    if ((Hash $pts) -cne $case.repeated_input_sha256 -or (Get-Item -LiteralPath $pts).Length -ne 192512) { throw 'Repeated sensor order differs' }
    return $dir
}
$inspectRoot = Join-Path $env:LOCALAPPDATA ("GreenLens/verification/retained depth inspect " + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $inspectRoot | Out-Null
Add-Type -LiteralPath (Join-Path $repo 'scripts/annual-reference-aggregation.cs')
foreach ($case in $m.cases) {
    $dir = New-CaseInputs $inspectRoot $case
    foreach ($raw in $case.retained_outputs) {
        $source = [IO.File]::OpenRead((Join-Path $fixture $raw.path))
        $gzip = [IO.Compression.GZipStream]::new($source,[IO.Compression.CompressionMode]::Decompress)
        $dest = "$dir/$($raw.role).rgb"
        $target = [IO.File]::Create($dest); $count = 0L; $buffer = [byte[]]::new(65536)
        try {
            while (($n=$gzip.Read($buffer,0,$buffer.Length)) -gt 0) {
                $count += $n
                if ($count -gt 491520) { throw 'Decompressed output exceeds fixed bound' }
                $target.Write($buffer,0,$n)
            }
        } finally { $target.Dispose(); $gzip.Dispose(); $source.Dispose() }
        if ($count -ne $raw.raw_bytes -or (Hash $dest) -cne $raw.raw_sha256) { throw 'Original output differs' }
    }
    $componentPath = Join-Path $repo $case.published_expected_components.path
    if ((Hash $componentPath) -cne $case.published_expected_components.sha256) { throw 'Published components differ' }
    $e = Get-Content -LiteralPath $componentPath -Raw | ConvertFrom-Json
    $direct = [double[][]]::new(24); $finite = [double[][]]::new(24)
    for ($i=0; $i -lt 24; $i++) {
        if ($e.rows[$i].sensor_id -cne ('s{0:D6}' -f ($i+1))) { throw 'Component sensor order differs' }
        $direct[$i] = [double[]]$e.rows[$i].components.centroid_direct_rgb.baseline_rgb
        $finite[$i] = [double[]]$e.rows[$i].components.finite_direct_rgb.baseline_rgb
    }
    $baseRaw = [GreenLensReference.Aggregate]::Read("$dir/baseline.rgb",36864)
    $depthRaw = [GreenLensReference.Aggregate]::Read("$dir/depth.rgb",36864)
    $base = [GreenLensReference.Aggregate]::Calculate($baseRaw,24,0,256,100,$direct,$finite)
    $depth = [GreenLensReference.Aggregate]::Calculate($depthRaw,24,0,256,100,$direct,$finite)
    $checks = [GreenLensReference.Aggregate]::CompareWithScale($base,$depth,$base)
    if (@($checks | Where-Object { !$_.passed }).Count -ne 10) { throw 'Retained failure count differs' }
    Write-Output $case.case
    $checks | Format-Table sensor_index,lower_lux,higher_lux,absolute_difference_lux,allowed_difference_lux,passed
}
```

The direct/finite arrays above are authenticated reused components from the original native audit, not newly traced values. `-I+ -V+` output already represents irradiance: divide the two source bins by `256*100` and use `diffuse + fullSolar - centroidDirect + finite4096`. **No additional pi multiplier.** The exact fixed baseline denominator is preserved.

## Optional explicit native replay

This section is an expensive manual reproduction, not part of inspection or tests. It can take hours. Install the pinned Radiance package separately through the repository's documented engine setup. Use a fresh case directory, one process at a time and the existing atomic Windows Job supervisor, with its unchanged 7,200-second / 2 GiB memory / 2 GiB scratch / 512 MiB per-file bounds. Do not run alongside the active scientific prerequisite. No retry or altered settings is prescribed.

After the verification/setup definitions above, execute the following only when intentionally requesting native replay. The existing supervisor sets each child's working directory to `$OutputDirectory` and child-only `RAYPATH=.;<EngineRoot>/lib`, so the relative hourly `.dat` and shared `.cal` resolve correctly. Each case deliberately retains both original octree roles, even though their source text is identical.

```powershell
$EngineRoot = Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'
foreach ($p in $m.engine_sha256.GetEnumerator()) {
    $subdir = if ($p.Key.EndsWith('.exe')) { 'bin' } else { 'lib' }
    if ((Hash (Join-Path $EngineRoot "$subdir/$($p.Key)")) -cne $p.Value) { throw 'Pinned engine/library differs' }
}
. (Join-Path $repo 'scripts/annual-paired-process.ps1')
$nativeRoot = Join-Path $env:LOCALAPPDATA ("GreenLens/verification/retained depth native " + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $nativeRoot | Out-Null
foreach ($setting in @(@{id='A4116';hour=4116;sun='solar246990'},@{id='B8512';hour=8512;sun='solar510750'})) {
    $case = @($m.cases | Where-Object { $_.case -ceq $setting.id })[0]
    $OutputDirectory = New-CaseInputs $nativeRoot $case
    $records = [Collections.Generic.List[object]]::new()
    $h = $setting.hour
    Invoke-PairedProcess -Executable "$EngineRoot/bin/oconv.exe" -Name 'oconv' -Arguments @('-f','scene.rad',"hour-$h-scaled.rad") -Output "hour-$h.oct"
    $argsBase = @('-I+','-h','-faa','-n','1','-ab','9','-ad','8192','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-c','1','-V+','-y','6144','-m','sky_glow','-m',$setting.sun,"hour-$h.oct")
    Invoke-PairedProcess -Executable "$EngineRoot/bin/rcontrib.exe" -Name 'rcontrib' -Arguments $argsBase -InputFile "hour-$h-rounds256.pts" -Output "hour-$h-rounds256.rgb"
    Invoke-PairedProcess -Executable "$EngineRoot/bin/oconv.exe" -Name 'oconv' -Arguments @('-f','scene.rad','depth10-24.rad') -Output 'depth10-24.oct'
    $argsDepth = @('-I+','-h','-faa','-n','1','-ab','10','-ad','8192','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-24','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-c','1','-V+','-y','6144','-m','sky_glow','-m',$setting.sun,'depth10-24.oct')
    Invoke-PairedProcess -Executable "$EngineRoot/bin/rcontrib.exe" -Name 'rcontrib' -Arguments $argsDepth -InputFile "hour-$h-rounds256.pts" -Output 'depth10-24.rgb'
}
```

Require successful process completion, exact 6,144×6 finite nonnegative output and every hash/identity check before calculating a new result. Use the same `Read`, `Calculate` and `CompareWithScale` calls on the newly completed file paths. Preserve any changed raw hash and every comparison; bitwise historical equality must be tested, not assumed on other platforms/builds. No negative correction is clamped.

This reduced fixture replays already sealed hourly angular fields. It does not regenerate annual weather, establish angular solar visibility, requalify the reference family, or replace the separate external-primary prerequisite. The complete original audits remain [A components](../../../docs/evidence/v0.3-engine/reference-A-depth-components.json) and [B components](../../../docs/evidence/v0.3-engine/reference-B-depth-components.json). Their full 24-row arrays preserve all components and failures; the compact fixture adds the physical input and complete original raw-output closure.

## Separate external-primary transport reproduction

This optional manual reproduction is **not** the eight-process historical `-I+` replay above, the original strict 42-stage driver, or a new qualification. It reproduces exactly the first 13 native stages of the declared external-primary protocol: both cases at N=16,384, followed by A at N=32,768, with all 24 sensors, both complete primary blocks and both full depths plus matched black controls. Those stages supply the fixed 144 A adjacent-N comparisons. A passing subset grants nothing; the other 864 R1 predicates and every R2 predicate remain unevaluated here. Do not run concurrently with an active scientific job.

Use the existing setup/verification definitions above, pinned Radiance installation and a fresh native PowerShell 7.2+ process. The physical scenes, hourly sky fields, sensors and notices are already in this fixture. The exact black scenes and source-scale aliases are in `scripts/annual-reference-prerequisite/derived-inputs`. The two additional `external-primary/*-finite-original.json` files total 2,016 bytes: they are exact copies of the immutable finite4096 inputs, with all 144 RGB channels also present in the published historical component records. `external-primary/manifest.json` pins those bytes and the interpreter. Scene/sensor/finite files are Apache-2.0; the existing Radiance and weather attribution requirements remain unchanged.

The following uses existing supervisor, producer and archive helpers. It keeps both original ASCII and verified gzip in this deliberately small prefix: at most 566,231,040 raw bytes for A and 188,743,680 for B, before gzip/logs. It does **not** simulate the original driver's duplicate-prune records. All files still count toward the same 2 GiB owned-case cap; insufficient reserve, any failure, or the 12-hour deadline stops the manual reproduction without retries.

```powershell
$src = Join-Path $repo 'scripts/annual-reference-prerequisite'
if ((Hash "$src/proposal.json") -cne 'a74e9e574cae8ad12806c8b0caadb22a3b7dd99eb30c7af916147517d1e3c6bf' -or
    (Hash "$src/runtime-pins.json") -cne '34664d92b10b5d9e0661701c692ab1fb3b9cad99517ce49d3b4e46b5e3239fad') { throw 'Protocol differs' }
$plan = Get-Content "$src/proposal.json" -Raw | ConvertFrom-Json -AsHashtable
$pins = Get-Content "$src/runtime-pins.json" -Raw | ConvertFrom-Json -AsHashtable
foreach ($p in $pins.draft.GetEnumerator()) { if ((Hash (Join-Path $src $p.Key)) -cne $p.Value) { throw 'Runtime differs' } }
foreach ($p in $pins.repository.GetEnumerator()) { if ((Hash (Join-Path $repo $p.Key)) -cne $p.Value) { throw 'Trusted source differs' } }
$extra = Get-Content "$fixture/external-primary/manifest.json" -Raw | ConvertFrom-Json -AsHashtable
foreach ($p in $extra.files.GetEnumerator()) { if ((Hash "$fixture/external-primary/$($p.Key)") -cne $p.Value.sha256) { throw 'Finite input differs' } }
$Python = (Get-Command python.exe -ErrorAction Stop).Source
if ((Hash $Python) -cne $extra.python.sha256) { throw 'Use the declared research Python interpreter' }
$EngineRoot = Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'
foreach ($p in $m.engine_sha256.GetEnumerator()) {
    $sub = if ($p.Key.EndsWith('.exe')) { 'bin' } else { 'lib' }
    if ((Hash "$EngineRoot/$sub/$($p.Key)") -cne $p.Value) { throw 'Engine/library differs' }
}
. (Join-Path $repo 'scripts/annual-paired-process.ps1')
Add-Type -LiteralPath "$src/reference_primary.cs"
$prefixRoot = Join-Path $env:LOCALAPPDATA ("GreenLens/verification/reference prefix reproduction " + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $prefixRoot | Out-Null
$cache = Join-Path $prefixRoot 'absent-bytecode-cache'
$native = @{}; $post = @{}; $folders = @{}
foreach ($id in @('A4116','B8512')) {
    $case = @($m.cases | Where-Object { $_.case -ceq $id })[0]
    $folders[$id] = New-CaseInputs $prefixRoot $case
    $declared = @($plan.cases | Where-Object { "$($_.alternative)$($_.interval_index)" -ceq $id })[0]
    foreach ($p in $declared.derived_members.GetEnumerator()) {
        $from = "$src/derived-inputs/$id/$($p.Key)"
        if ((Hash $from) -cne $p.Value.sha256) { throw 'Derived input differs' }
        Copy-Item -LiteralPath $from -Destination "$($folders[$id])/$($p.Key)"
    }
    Copy-Item -LiteralPath "$fixture/external-primary/$id-finite-original.json" -Destination "$($folders[$id])/finite-original.json"
    $native[$id] = [Collections.Generic.List[object]]::new(); $post[$id] = [Collections.Generic.List[object]]::new()
}
$sensorText = [IO.File]::ReadAllText("$fixture/shared/sensors.pts")
$expectedReceipts=@{}
foreach ($n in @(16384,32768)) {
    $expectedReceipts[$n]=[GreenLensReferenceStream.HammersleyProducer]::new($sensorText,$n).GenerateAsync([IO.Stream]::Null,[Threading.CancellationToken]::None).GetAwaiter().GetResult()
}
# Fixed bounded receipt preparation precedes native timing, as in the original driver.
$clock = [Diagnostics.Stopwatch]::StartNew()
function Remaining { $v=43200-$clock.Elapsed.TotalSeconds; if ($v -le 0) { throw '12-hour deadline' }; [Math]::Min(7200.,$v) }
function CheckCaseBudget([string]$dir) {
    $size=0L
    foreach ($f in [IO.Directory]::EnumerateFiles($dir)) {
        $bytes=[GreenLensPairedProcess.NativeProcess]::FileSize($f)
        if ($bytes -gt 536870912) { throw 'Single-file cap' }; $size += $bytes
    }
    if ($size -gt 2147483648) { throw 'Owned-case cap' }
}
# Exact original order and output names, restricted to ordinals 1 through 13.
foreach ($stage in @($plan.stages | Where-Object { $_.ordinal -le 13 })) {
    $OutputDirectory = $folders[$stage.case]; $records = $native[$stage.case]
    $size=0L; foreach ($f in [IO.Directory]::EnumerateFiles($OutputDirectory)) { $size += [GreenLensPairedProcess.NativeProcess]::FileSize($f) }
    if ($size+1073741824 -gt 2147483648) { throw 'Insufficient raw-plus-gzip reserve' }
    $h = if ($stage.case -ceq 'A4116') { 4116 } else { 8512 }
    $sun = if ($h -eq 4116) { 'solar246990' } else { 'solar510750' }
    if ($stage.executable -ceq 'oconv.exe') {
        $scene = if ($stage.ordinal -in @(2,4)) { 'scene-black.rad' } else { 'scene.rad' }
        $argv = @('-f',$scene,"hour-$h-scale100.rad")
    } else {
        $n = [int]$stage.primary_count_per_sensor_per_block
        $ab = $stage.id.Substring($stage.id.LastIndexOf('-ab')+3)
        $octree = if ($ab -ceq '0') { "hour-$h-scale100-black.oct" } else { "hour-$h-scale100.oct" }
        $argv = @('-h','-faa','-n','1','-ab',$ab,'-ad','128','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-c','1','-V+','-y',"$(48L*$n)",'-m','sky_glow','-m',$sun,$octree)
    }
    if (($argv | ConvertTo-Json -Compress) -cne ($stage.argv | ConvertTo-Json -Compress)) { throw 'Literal argument order differs' }
    $call = @{Executable="$EngineRoot/bin/$($stage.executable)";Name=[IO.Path]::GetFileNameWithoutExtension($stage.executable);Arguments=$argv;Output=$stage.output;TimeoutSeconds=(Remaining)}
    if ($stage.executable -ceq 'rcontrib.exe') { $call.InputProducer=[GreenLensReferenceStream.HammersleyProducer]::new($sensorText,$n) }
    $receipt = Invoke-PairedProcess @call
    Copy-Item "$OutputDirectory/processes.json" "$OutputDirectory/native-processes.json"
    CheckCaseBudget $OutputDirectory
    if ($stage.executable -ceq 'rcontrib.exe') {
        $expected=$expectedReceipts[$n]
        if ($receipt.Sha256 -cne $expected.Sha256 -or $receipt.Bytes -ne $expected.Bytes -or $receipt.Rays -ne 48L*$n) { throw 'Primary stream differs' }
        if (Test-Path -LiteralPath $cache) { throw 'Bytecode cache must be absent' }
        $records=$post[$stage.case]; $seconds=Remaining
        Invoke-PairedProcess -Executable $Python -Name 'research-python' -Arguments @('-B','-X','utf8','-X',"pycache_prefix=$cache","$src/reference_stream.py",'archive','--root',$OutputDirectory,'--name',$stage.output,'--N',"$n",'--scale','100','--seconds',([Math]::Max(.001,$seconds-1)).ToString('R',[Globalization.CultureInfo]::InvariantCulture)) -Output "$($stage.output).archive.json" -TimeoutSeconds $seconds
        if (Test-Path -LiteralPath $cache) { throw 'Bytecode cache must remain absent' }
        Copy-Item "$OutputDirectory/processes.json" "$OutputDirectory/postprocesses.json"
        CheckCaseBudget $OutputDirectory
    }
}
# Inline use of the existing readers, under the SAME decreasing global budget.
$audit=@'
import json,pathlib,sys,time
sys.path.insert(0,sys.argv[1])
import reference_stream as S, reference_report as R
root=pathlib.Path(sys.argv[2]); end=time.monotonic()+float(sys.argv[3])
folder=root/'A4116'; S.inventory(folder); summaries={}; assemblies={}; checks=[]
finite=R.load(folder/'finite-original.json')
for n in (16384,32768):
    for ab in (0,8,9):
        receipt=R.load(folder/f'A4116-base-N{n}-ab{ab}.rgb.archive.json')
        summaries[n,ab]=R.expected(S.read_archive(folder,receipt,end),n,100)
    for ab in (8,9): assemblies[n,ab]=S.assemble(summaries[n,ab],summaries[n,0],finite)
for ab in (8,9):
    checks += S.compare(assemblies[16384,ab]['mean_rows'],assemblies[32768,ab]['mean_rows'],label=f'A4116/N16384-32768/ab{ab}')
S.require(len(checks)==144,'Exact subset count'); S.inventory(folder); S.deadline(end)
print(json.dumps({'scope':'manual physical prefix reproduction, not the strict provenance audit','assemblies':[{'N':n,'ab':ab,**v} for (n,ab),v in assemblies.items()],'checks':checks,'negative_results':[{'N':n,'ab':ab,'blocks':v['negative_sensor_blocks']} for (n,ab),v in assemblies.items() if not v['completed']],'R1_complete':False,'remaining_R1_predicates_not_evaluated':864,'reference_family_qualified':False,'accuracy_accepted':False},allow_nan=False))
'@
if (Test-Path -LiteralPath $cache) { throw 'Bytecode cache must remain absent' }
$OutputDirectory=$prefixRoot; $records=[Collections.Generic.List[object]]::new(); $seconds=Remaining
Invoke-PairedProcess -Executable $Python -Name 'research-python' -Arguments @('-B','-X','utf8','-X',"pycache_prefix=$cache",'-c',$audit,$src,$prefixRoot,([Math]::Max(.001,$seconds-1)).ToString('R',[Globalization.CultureInfo]::InvariantCulture)) -Output 'prefix-checks.json' -TimeoutSeconds $seconds
foreach ($dir in $folders.Values) { CheckCaseBudget $dir }
CheckCaseBudget $prefixRoot
if (Test-Path -LiteralPath $cache) { throw 'Bytecode cache must remain absent' }
$null=Remaining
```

The producer supplies block→sensor→ray order, each block covering the **entire** cosine hemisphere. N=16,384 is not a prefix of N=32,768. For each closed archive, use the pinned `reference_stream.read_archive` and `reference_report.expected(summary,N,100)`, then `reference_stream.assemble(full,black,finite)`. The assembly is `full.diffuse + full.solar − black.solar + original finite4096`, with pi/100 applied once by the streaming collector. Do not subtract the black diffuse channel or use the old irradiance centroid control. Keep negative residuals as failures.

The inline comparison preserves A's equal-two-block mean rows for N=16,384 versus N=32,768 at ab=8 and ab=9, every sensor/component row and the later-count component denominator. It yields exactly 144 predicates at `max(0.1 lx, 1% of the later component)`. B's complete N=16,384 streams are retained without inventing a B adjacent-N result. Record all primary/native/archive receipts and all raw hashes; disagreement with the retained run is evidence, not a reason to choose other settings. Recheck copied input and helper hashes before interpreting a result; this snippet does not provide the original driver's full identity closure.

This compact reproduction intentionally cannot invoke the original strict inspector or `run-reference.ps1`: their exact 24-member-per-case original baseline closure includes full-year matrices, weather/model, historical raw outputs and source/process declarations (64,379,016 bytes for A; 64,385,476 for B, before derived files). Those files and hashes remain enumerated in the sealed proposal. A compact transport replay does not replace that provenance check, complete R1/R2, qualify the reference family, or validate solar angular physics.

## Minimal terminal B8512 depth-failure variant

The original sealed driver subsequently evaluated all 1,008 R1 predicates and stopped naturally: 36 failed, all in B8512 at N=65,536, comparing depth 8 with depth 9. The breakdown is 8 diffuse, 17 reflected-solar and 11 total checks across 17 sensors; no negative assembly occurred. R2 and the conditional candidate were not executed. The complete [terminal record](../../../docs/evidence/v0.3-engine/reference-prerequisite-terminal.json) retains every result and source seal. The sign of the difference does not establish its cause.

This **reviewed but unexecuted manual recipe** isolates those 72 depth predicates. Choose it **instead of** the first-13-stage recipe, before executing any of that recipe's code. It uses the same existing helpers and five original native stages, in order: ordinals **3, 4, 20, 21, 22** (B full octree, black octree, full depth 8, full depth 9, black control). It does not reuse a selected sensor, a primary prefix, or another direct estimator.

Apply only these literal substitutions to the preceding external-primary PowerShell block:

- In case preparation, replace `@('A4116','B8512')` with `@('B8512')`.
- In expected-receipt preparation **before the native clock starts**, replace `@(16384,32768)` with `@(65536)`.
- Replace the stage filter `Where-Object { $_.ordinal -le 13 }` with `Where-Object { $_.ordinal -in @(3,4,20,21,22) }`. Preserve the proposal order and exact argument-array equality check. The existing literal construction then supplies `-y 3145728`, `-ab 8`, `-ab 9` or `-ab 0`, unchanged scale 100 and all other flags.
- Replace only the `$audit` here-string with the following. Keep the existing supervised invocation and its same decreasing `Remaining` budget, cache/source checks, and inventory checks after native and collector metadata writes.

```python
import json,pathlib,sys,time
sys.path.insert(0,sys.argv[1])
import reference_stream as S, reference_report as R
root=pathlib.Path(sys.argv[2]); end=time.monotonic()+float(sys.argv[3])
folder=root/'B8512'; S.inventory(folder); summaries={}; assemblies={}
finite=R.load(folder/'finite-original.json')
for ab in (0,8,9):
    receipt=R.load(folder/f'B8512-base-N65536-ab{ab}.rgb.archive.json')
    summaries[ab]=R.expected(S.read_archive(folder,receipt,end),65536,100)
for ab in (8,9): assemblies[ab]=S.assemble(summaries[ab],summaries[0],finite)
checks=S.compare(assemblies[8]['mean_rows'],assemblies[9]['mean_rows'],label='B8512/depth8-9')
S.require(len(checks)==72,'Exact subset count'); S.inventory(folder); S.deadline(end)
print(json.dumps({'scope':'manual B8512 depth subset, not strict full-protocol qualification','assemblies':[{'ab':ab,**v} for ab,v in assemblies.items()],'checks':checks,'failed_count':sum(not c['passed'] for c in checks),'negative_results':[{'ab':ab,'blocks':v['negative_sensor_blocks']} for ab,v in assemblies.items() if not v['completed']],'R1_complete':False,'remaining_R1_predicates_not_evaluated':936,'R2_predicates_not_evaluated':1296,'reference_family_qualified':False,'accuracy_accepted':False,'release_accepted':False},allow_nan=False))
```

Each transport retains both complete hemisphere blocks and all 24 ordered receivers, with 3,145,728 rows × 6 channels. The primary receipt is 3,145,728 rays / 322,831,568 input bytes / SHA-256 `00c7fc5323d9c9a08eac29ac47700474b8a5feb5e841cda14632f4d105c05a98`. Three observed ASCII outputs total 754,974,720 bytes before gzip/logs; keep the existing reserve and inventory checks, 512 MiB file / 2 GiB case / 2 GiB memory / 7,200-second child / 12-hour global limits. This manual variant retains raw ASCII as well as verified gzip, so its storage differs from the original driver's verified duplicate pruning. A resource failure stops it; no retry is prescribed.

The comparison uses each deeper component as denominator: `max(0.1 lx, 1% of depth-9 component)`. The retained run has 36 failures out of these 72 checks. Preserve every new result even if reproduction differs; do not assert the expected failure count before saving the evidence. The first retained failure is receiver `s000001`, reflected solar 91.08753878332723 → 92.13936935635618 lx, difference 1.0518305730289512 lx versus 0.9213936935635618 lx allowed. IDs here are one-based ordered receiver labels; `sensor_index` is zero-based. The other 936 R1 and all 1,296 R2 predicates are outside this subset. The large original current-protocol raw archives are retained externally and hash-bound in the terminal record, not bundled in this fixture; its four bundled gzip files remain the separate historical `-I+` experiment.
