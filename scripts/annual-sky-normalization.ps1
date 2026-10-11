#Requires -Version 7.5
# Sky-only analytical/native checks. No room, solar disc or transport acceptance.
[CmdletBinding()]
param(
    [string]$OutputDirectory,
    [string]$EngineRoot = (Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'),
    [switch]$FunctionsOnly
)
$ErrorActionPreference = 'Stop'

function Convert-SkyRgbToLux([double[]]$Rgb) {
    if ($Rgb.Count -ne 3 -or @($Rgb | Where-Object { ![double]::IsFinite($_) -or $_ -lt 0 }).Count) { throw 'Expected exactly three finite nonnegative RGB values.' }
    return 179 * (0.265074126*$Rgb[0] + 0.670114631*$Rgb[1] + 0.064811243*$Rgb[2])
}
function Get-SkyProjectedWeights([ValidateSet(2,4)][int]$Mf) {
    # Integral cos(theta) dOmega, independently integrating each altitude ring.
    # Bin 0 is the lower hemisphere and has zero weight for an upward sensor.
    $weights = [double[]]::new(144*$Mf*$Mf+2)
    $alpha = [Math]::PI/2/(7*$Mf+0.5)
    $baseCounts = @(30,30,24,24,18,12,6)
    $bin = 1
    for ($ring=0; $ring -lt 7*$Mf; $ring++) {
        $count = $Mf*$baseCounts[[int][Math]::Floor($ring/$Mf)]
        $weight = [Math]::PI/$count * ([Math]::Pow([Math]::Sin(($ring+1)*$alpha),2)-[Math]::Pow([Math]::Sin($ring*$alpha),2))
        for ($azimuth=0; $azimuth -lt $count; $azimuth++) { $weights[$bin++]=$weight }
    }
    $weights[$bin] = [Math]::PI*(1-[Math]::Pow([Math]::Sin(7*$Mf*$alpha),2))
    if ($bin -ne $weights.Count-1) { throw 'Independent patch count is inconsistent.' }
    return ,$weights
}
function Get-SkyProjectedRgb([ValidateSet(2,4)][int]$Mf,[double[]]$Radiance,[switch]$Downward) {
    $weights = Get-SkyProjectedWeights $Mf
    if ($Radiance.Count -ne 3*$weights.Count -or @($Radiance | Where-Object { ![double]::IsFinite($_) -or $_ -lt 0 }).Count) { throw 'Truncated, excessive or invalid patch RGB radiance.' }
    $rgb = [double[]]::new(3)
    if ($Downward) { for ($c=0;$c -lt 3;$c++) { $rgb[$c]=[Math]::PI*$Radiance[$c] } }
    else { for ($p=1;$p -lt $weights.Count;$p++) { for ($c=0;$c -lt 3;$c++) { $rgb[$c]+=$weights[$p]*$Radiance[3*$p+$c] } } }
    return ,$rgb
}
function New-SkyRgbCheck([string]$Name,[double[]]$Expected,[double[]]$Actual) {
    $expectedLux = Convert-SkyRgbToLux $Expected
    $actualLux = Convert-SkyRgbToLux $Actual
    $channels = @(); $passed = $true
    for ($c=0;$c -lt 3;$c++) {
        $error = [Math]::Abs($Actual[$c]-$Expected[$c])
        $limit = 0.01*$Expected[$c]
        $ok = $error -le $limit
        $passed = $passed -and $ok
        $channels += @{channel=@('R','G','B')[$c];expected=$Expected[$c];actual=$Actual[$c];absolute_error=$error;allowed_error=$limit;passed=$ok}
    }
    $luxError = [Math]::Abs($actualLux-$expectedLux)
    $luxPassed = $luxError -le 0.01*$expectedLux
    return @{name=$Name;kind='projected_hemisphere_integral';expected_rgb_irradiance=$Expected;actual_rgb_irradiance=$Actual;channels=$channels;expected_lux=$expectedLux;actual_lux=$actualLux;relative_lux_error=$(if($expectedLux -gt 0){$luxError/$expectedLux}else{$null});relative_tolerance=0.01;passed=($passed -and $luxPassed)}
}
function Get-SkyCardinalChecks([ValidateSet(2,4)][int]$Mf,[double[]]$Actual) {
    if ($Actual.Count -ne 18) { throw 'Expected six cardinal RGB observations.' }
    $bins = @(1,(1+30*$Mf/4),(1+30*$Mf/2),(1+3*30*$Mf/4),(144*$Mf*$Mf+1),0)
    $names = @('north','east','south','west','zenith','ground')
    $checks = @()
    for ($i=0;$i -lt 6;$i++) {
        $bin=[int]$bins[$i]; $expected=@([double]($bin+1),[double](2*$bin+7),[double](3*$bin+11))
        $rgb=@($Actual[3*$i],$Actual[3*$i+1],$Actual[3*$i+2]);$passed=$true
        for($c=0;$c -lt 3;$c++) { if(![double]::IsFinite($rgb[$c]) -or [Math]::Abs($rgb[$c]-$expected[$c]) -gt 0.000001){$passed=$false} }
        $checks+=@{name="MF$Mf $($names[$i]) patch ordering";kind='cardinal_patch_order';expected_bin=$bin;expected_rgb_radiance=$expected;actual_rgb_radiance=$rgb;absolute_rgb_tolerance=0.000001;passed=$passed}
    }
    return $checks
}
function Assert-SkyChecks($Checks) {
    if (@($Checks).Count -eq 0 -or @($Checks | Where-Object { !$_.passed }).Count) { throw 'Sky normalization or ordering check failed; retain all evidence and investigate.' }
}
if ($FunctionsOnly) { return }

if (!$OutputDirectory) { $OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/annual sky normalization 空間 '+[guid]::NewGuid().ToString('N')) }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new sky verification directory; existing evidence is preserved.' }
$requestedEngine = $EngineRoot
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -EngineRoot $requestedEngine -FunctionsOnly -ProcessTimeoutSeconds 120
$checks=[Collections.Generic.List[object]]::new()
$result=[ordered]@{
    schema_version='greenlens.annual-sky-normalization.v1';started_utc=[DateTime]::UtcNow.ToString('o');passed=$false
    scope='Sky-only analytical projection and native piecewise-sky integration, plus directional patch order. No explicit solar disc, room transport, full-year convergence, matched-room or desktop acceptance.'
    tolerance='Each RGB irradiance channel and converted illuminance within 1% of the independently integrated identical sky; cardinal RGB absolute error <=1e-6.'
    bases=@(2,4);real_interval_index=1908;real_context_indices=@(1906,1907,1908)
    units=@{matrix='visible RGB radiance W/(sr m2)';rtrace_I='visible RGB irradiance W/m2';illuminance='lx using 179*(0.265074126R+0.670114631G+0.064811243B)'}
    original_fixture='Unequal-channel uniform hemisphere, bin-coded sphere, and three original non-climatic EPW records.'
    verification_scripts=@('annual-sky-normalization.ps1','annual-feasibility.ps1','verify-matrix-multiplication.ps1')|ForEach-Object{@{file=$_;sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot $_)).Hash.ToLowerInvariant()}}
    commands_record='processes.json';checks=$checks
}
function Read-SkyRgb([string]$Name,[int]$Expected) {
    $values=@([IO.File]::ReadAllText((Join-Path $OutputDirectory $Name)).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object {[double]::Parse($_,$culture)})
    if($values.Count -ne $Expected -or @($values|Where-Object{![double]::IsFinite($_) -or $_ -lt 0}).Count){throw 'Malformed native RGB observation.'}
    return ,([double[]]$values)
}
function Write-SkyData([string]$Name,[double[]]$Radiance,[int]$Mf) {
    $count=144*$Mf*$Mf+2
    if($Radiance.Count -ne 3*$count){throw 'Cannot write malformed RGB sky.'}
    foreach($c in 0..2){
        $data=[Text.StringBuilder]::new("1`n0 $($count-1) $count`n")
        for($p=0;$p -lt $count;$p++){$null=$data.Append($Radiance[3*$p+$c].ToString('R',$culture)).Append("`n")}
        Write-Input "$Name-$c.dat" $data.ToString()
    }
    Write-Input "$Name.rad" "void colordata distribution`n8 noop noop noop $Name-0.dat $Name-1.dat $Name-2.dat basis-mf$Mf.cal rbin`n0`n0`ndistribution glow dome`n0`n0`n4 1 1 1 0`ndome source sphere`n0`n0`n4 0 0 1 360`n"
}
try {
    $inventory=Get-Content -LiteralPath "$PSScriptRoot/../dependencies/radiance.json" -Raw|ConvertFrom-Json -DateKind String
    $identities=[ordered]@{}
    foreach($name in @('gendaymtx','oconv','rtrace')){
        $file=Join-Path $EngineRoot "bin/$name.exe"
        if(!(Test-Path -LiteralPath $file -PathType Leaf)){throw "Required pinned native $name is unavailable; this verification does not skip."}
        $expected=if($name -eq 'gendaymtx'){$inventory.annual_workflow_review.executables.$name.sha256}else{$inventory.executables.$name.sha256}
        $actual=(Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant()
        if($actual -cne $expected){throw "Pinned $name identity differs."}
        $identities["$name.exe"]=$actual
    }
    foreach($name in @('rayinit.cal','reinhart.cal')){
        $file=Join-Path $EngineRoot "lib/$name"
        $expected=if($name -eq 'reinhart.cal'){$inventory.annual_workflow_review.additional_critical_libraries.$name.sha256}else{$inventory.critical_libraries.$name.sha256}
        $actual=(Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant()
        if($actual -cne $expected){throw "Pinned $name identity differs."}
        $identities[$name]=$actual
    }
    $result.engine=@{release=$inventory.release;tag=$inventory.tag;identities=$identities;library_environment='RAYPATH=.;<verified engine>/lib (child processes only)'}
    Invoke-Engine 'rtrace' @('-version') 'engine-version.txt'
    if([IO.File]::ReadAllText((Join-Path $OutputDirectory 'engine-version.txt')).Trim() -cne $inventory.version_output){throw 'Engine version output differs.'}

    # Reproduce only the reviewed source's documented byte normalization and
    # require the whole normalized byte digest. This is not a general EPW parser.
    $dataset=(Get-Content -LiteralPath "$PSScriptRoot/../dependencies/weather.json" -Raw|ConvertFrom-Json -DateKind String).datasets[0]
    $original=Join-Path "$PSScriptRoot/.." $dataset.repository_path
    if((Get-FileHash -LiteralPath $original).Hash.ToLowerInvariant() -cne $dataset.original_sha256){throw 'Required reviewed original EPW differs.'}
    $sourceLines=[IO.File]::ReadAllText($original).TrimStart([char]0xFEFF).TrimEnd("`r","`n") -split '\r?\n'
    if($sourceLines.Count -ne 8768){throw 'Reviewed EPW must contain eight headers and 8760 records.'}
    $normalized=[Collections.Generic.List[string]]::new()
    for($i=0;$i -lt $sourceLines.Count;$i++){
        $fields=@($sourceLines[$i].Split(',')|ForEach-Object{$_.Trim()})
        if($i -eq 4 -and $fields[0] -ceq 'HOLIDAYS/DAYLIGHT SAVING'){$fields[0]='HOLIDAYS/DAYLIGHT SAVINGS'}
        if($i -ge 8){$fields[4]='60'}
        $normalized.Add(($fields -join ','))
    }
    $canonical=($normalized -join "`n")+"`n"
    $canonicalHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($utf8.GetBytes($canonical))).ToLowerInvariant()
    if($canonicalHash -cne $dataset.normalized_sha256){throw 'Canonical weather bytes differ from the reviewed provider output.'}
    $selected=@($normalized[1914],$normalized[1915],$normalized[1916])
    $last=$selected[2].Split(',')
    if($last[1] -ne '3' -or $last[2] -ne '21' -or $last[3] -ne '13'){throw 'Predeclared real interval mapping changed.'}
    Write-Input 'real-selected.epw' (($normalized[0..6] -join "`n")+"`nDATA PERIODS,1,1,GreenLens sky context,Monday,3/21,3/21`n"+($selected -join "`n")+"`n")
    $result.weather=@{original_sha256=$dataset.original_sha256;normalized_sha256=$canonicalHash;time_axis_sha256=$dataset.time_axis_sha256;interpretation=$dataset.interpretation;citation=$dataset.title;source_url=$dataset.source_url;utc_offset_minutes=480;month=3;day=21;hour_ending=13;representative_local_standard_time='12:30';source_year=[int]$last[0];no_dst=$true;dew_point_context='Requested record plus preceding two original records; final matrix column selected.';radiation_policy='Hourly EPW DNI/DHI Wh/m2 numerically equal interval-average W/m2. Output is visible radiance from the pinned efficacy model, not broadband-to-lux conversion.'}
    $synthetic="LOCATION,Synthetic,None,None,GreenLens,000000,22.3,114.17,8,10`nDESIGN CONDITIONS,0`nTYPICAL/EXTREME PERIODS,0`nGROUND TEMPERATURES,0`nHOLIDAYS/DAYLIGHT SAVINGS,No,0,0,0`nCOMMENTS 1,Original non-climatic sky-only fixture`nCOMMENTS 2,Three hourly records preserve varying dew point context`nDATA PERIODS,1,1,Original fixture,Monday,6/21,6/21`n"
    for($i=0;$i -lt 3;$i++){$synthetic+=(@(2001,6,21,(11+$i),60,'?9?9?9?9',25,(12+4*$i),60,101325,0,0,300,(600+100*$i),(500+150*$i),(120+20*$i),0,0,0,0,180,1,5,5,20,77777,9,999999999,20,0.1,0,88,0.2,0,0) -join ',')+"`n"}
    Write-Input 'synthetic-selected.epw' $synthetic
    $base=@('-h','-faa','-ov','-n','1','-ab','1','-ad','262144','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-10','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.01','-st','0','-ss','0','-u-')
    $directional=@($base);$directional[[Array]::IndexOf($directional,'-ab')+1]='0'
    $result.trace_settings=$base
    $result.ground_policy='The -s matrix already contains the luminous ground term, including reflected direct-horizontal radiation. Reconstruct bin0 once; no ground polygon or added ground illumination. This sky-only check excludes explicit solar-disc irradiance.'
    Write-Input 'uniform.rad' "void glow uniform`n0`n0`n4 0.25 1 2 0`nuniform source upper`n0`n0`n4 0 0 1 180`n"
    Invoke-Engine 'oconv' @('-f','uniform.rad') 'uniform.oct'
    Invoke-Engine 'rtrace' (@('-I+')+$base+@('uniform.oct')) 'uniform.rgb' "0 0 0 0 0 1`n"
    $checks.Add((New-SkyRgbCheck 'Unequal RGB uniform upper hemisphere E=pi*L' @((0.25*[Math]::PI),[Math]::PI,(2*[Math]::PI)) (Read-SkyRgb 'uniform.rgb' 3)))
    foreach($mf in @(2,4)){
        Write-Input "basis-mf$mf.cal" ("MF:$mf;`n"+[IO.File]::ReadAllText((Join-Path $EngineRoot 'lib/reinhart.cal')))
        $weights=Get-SkyProjectedWeights $mf
        $sum=($weights|Measure-Object -Sum).Sum
        $checks.Add(@{name="MF$mf independent projected solid-angle partition";kind='analytical_partition';expected=[Math]::PI;actual=$sum;absolute_tolerance=1e-12;passed=([Math]::Abs($sum-[Math]::PI) -le 1e-12)})
        foreach($fixture in @('real','synthetic')){
            $skyColor=if($fixture -eq 'real'){@('1','1','1')}else{@('0.960','1.004','1.118')}
            $groundColor=if($fixture -eq 'real'){@('0.2','0.2','0.2')}else{@('0.1','0.2','0.3')}
            $name="$fixture-mf$mf"
            Invoke-Engine 'gendaymtx' (@('-O0','-m',"$mf",'-c')+$skyColor+@('-g')+$groundColor+@('-s',"$fixture-selected.epw")) "$name.mtx"
            $matrix=Read-Matrix "$name.mtx"
            if($matrix.rows -ne $weights.Count -or $matrix.columns -ne 3 -or $matrix.components -ne 3){throw 'Generated sky matrix shape differs from the declared context/basis.'}
            $radiance=[double[]]::new(3*$matrix.rows)
            for($p=0;$p -lt $matrix.rows;$p++){for($c=0;$c -lt 3;$c++){$radiance[3*$p+$c]=$matrix.values[($p*3+2)*3+$c]}}
            Write-SkyData $name $radiance $mf
            Invoke-Engine 'oconv' @('-f',"$name.rad") "$name.oct"
            Invoke-Engine 'rtrace' (@('-I+')+$base+@("$name.oct")) "$name.rgb" "0 0 0 0 0 1`n0 0 0 0 0 -1`n"
            $actual=Read-SkyRgb "$name.rgb" 6
            $checks.Add((New-SkyRgbCheck "$name exact upper patch integral" (Get-SkyProjectedRgb $mf $radiance) $actual[0..2]))
            $checks.Add((New-SkyRgbCheck "$name exact lower-ground integral" (Get-SkyProjectedRgb $mf $radiance -Downward) $actual[3..5]))
        }
        $coded=[double[]]::new(3*$weights.Count)
        for($p=0;$p -lt $weights.Count;$p++){$coded[3*$p]=$p+1;$coded[3*$p+1]=2*$p+7;$coded[3*$p+2]=3*$p+11}
        Write-SkyData "cardinal-mf$mf" $coded $mf
        Invoke-Engine 'oconv' @('-f',"cardinal-mf$mf.rad") "cardinal-mf$mf.oct"
        $alt=[Math]::PI/2/(7*$mf+0.5)/2;$cos=[Math]::Cos($alt).ToString('R',$culture);$sin=[Math]::Sin($alt).ToString('R',$culture)
        Invoke-Engine 'rtrace' ($directional+@("cardinal-mf$mf.oct")) "cardinal-mf$mf.rgb" "0 0 0 0 $cos $sin`n0 0 0 $cos 0 $sin`n0 0 0 0 -$cos $sin`n0 0 0 -$cos 0 $sin`n0 0 0 0 0 1`n0 0 0 0 0 -1`n"
        foreach($check in (Get-SkyCardinalChecks $mf (Read-SkyRgb "cardinal-mf$mf.rgb" 18))){$checks.Add($check)}
    }
    Assert-SkyChecks $checks
    $result.passed=$true
} catch {
    $result.error=$_.Exception.Message
    throw
} finally {
    $result.finished_utc=[DateTime]::UtcNow.ToString('o')
    $result.resources=@{processes=$records.Count;wall_seconds=($records|ForEach-Object{$_.wall_seconds}|Measure-Object -Sum).Sum;sampled_peak_child_working_set_bytes=($records|ForEach-Object{$_.sampled_peak_working_set_bytes}|Measure-Object -Maximum).Maximum;measurement_policy='Per-child working set sampled at 100 ms; short processes may have no sample. Scratch is completion size before this summary, not a continuously sampled peak. These small fixtures do not measure production annual peak memory.';scratch_bytes=(Get-ChildItem -LiteralPath $OutputDirectory -File|Measure-Object Length -Sum).Sum}
    $result.files=@(Get-ChildItem -LiteralPath $OutputDirectory -File|Sort-Object Name|ForEach-Object{@{name=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}})
    $result|ConvertTo-Json -Depth 30|Set-Content -LiteralPath (Join-Path $OutputDirectory 'sky-normalization.json') -Encoding utf8NoBOM
}
Write-Output $OutputDirectory
