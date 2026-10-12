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
