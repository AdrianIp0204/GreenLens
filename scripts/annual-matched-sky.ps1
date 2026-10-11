#Requires -Version 7.2
# Predeclared same-generated-sky investigation. Keeps every tested failure.
param(
    [Parameter(Mandatory)][string]$SceneFile,
    [Parameter(Mandatory)][string]$SensorsFile,
    [Parameter(Mandatory)][string]$WeatherFile,
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 matched skies 空間'),
    [int]$CandidateSamples = 2048,
    [int]$CandidateSolarSamples = 0,
    [int]$CandidateBounces = 5,
    [int]$ReferenceSamples = 16384,
    [int]$ReferenceBounces = 7,
    [int]$CandidateBasis = 2,
    [string]$ReferenceDirectory = '',
    [double]$CandidateWeight = 0.0000001,
    [double]$ReferenceWeight = 0.000001
)
$ErrorActionPreference = 'Stop'
$inputScene = [IO.File]::ReadAllText($SceneFile)
$inputSensors = [IO.File]::ReadAllText($SensorsFile)
$inputWeather = [IO.File]::ReadAllText($WeatherFile)
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
Write-Input 'scene.rad' $inputScene
Write-Input 'sensors.pts' $inputSensors
Write-Input 'weather.epw' $inputWeather
$sensorCount = @($inputSensors.Trim() -split '\r?\n').Length
if ($CandidateSolarSamples -eq 0) { $CandidateSolarSamples=$CandidateSamples }
$plan = Get-Content -LiteralPath "$PSScriptRoot/annual-validation-plan.json" -Raw | ConvertFrom-Json
$weatherRows = @($inputWeather.Trim() -split '\r?\n' | Select-Object -Skip 8)
$indices = [Collections.Generic.List[int]]::new()
foreach ($interval in $plan.fixed_intervals) {
    $date = [DateTime]::new(2001,$interval.month,$interval.day)
    $indices.Add([int](($date.DayOfYear-1)*24+$interval.hour_ending-1))
}
function Condition([int]$Index) {
    $fields=$weatherRows[$Index].Split(',')
    $dni=[double]::Parse($fields[14],$culture); $dhi=[double]::Parse($fields[15],$culture)
    if ($dhi -lt 50) { return 'low_diffuse' }
    $ratio=$dni/$dhi
    if ($ratio -ge 3) { return 'clear' }
    if ($ratio -ge 0.5) { return 'mixed' }
    return 'overcast'
}
$classes=@($indices | ForEach-Object { Condition $_ })
foreach ($class in @('clear','mixed','overcast')) {
    if ($classes -notcontains $class) {
        for ($index=0; $index -lt 8760; $index++) {
            if ((Condition $index) -eq $class -and $index%24 -ge 8 -and $index%24 -le 16) { $indices.Add($index); break }
        }
    }
}
$declaration=[ordered]@{declared_utc=[DateTime]::UtcNow.ToString('o');weather_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'weather.epw')).Hash.ToLowerInvariant();sensor_count=$sensorCount;intervals=@($indices | ForEach-Object { @{index=$_;condition=(Condition $_)} });candidate=@{ab=$CandidateBounces;ad=$CandidateSamples;solar_ad=$CandidateSolarSamples;mf=$CandidateBasis;lw=$CandidateWeight};reference=@{ab=$ReferenceBounces;ad=$ReferenceSamples;lw=$ReferenceWeight};tolerance='max(5lx,5%reference)';results='not executed at declaration'}
$declaration | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
if ($ReferenceDirectory) {
    $previous = Get-Content -LiteralPath (Join-Path $ReferenceDirectory 'declaration.json') -Raw | ConvertFrom-Json
    $previousWeight = $(if ($null -eq $previous.reference.lw) {0.000001} else {$previous.reference.lw})
    if ($previousWeight -ne $ReferenceWeight) { throw 'Reference cache cutoff differs' }
    if ($previous.weather_sha256 -ne $declaration.weather_sha256 -or $previous.reference.ab -ne $ReferenceBounces -or $previous.reference.ad -ne $ReferenceSamples -or $previous.candidate.mf -ne $CandidateBasis -or (($previous.intervals.index -join ',') -ne ($indices -join ','))) { throw 'Reference cache recipe mismatch' }
    foreach ($file in @('scene.rad','sensors.pts')) { if ((Get-FileHash -LiteralPath (Join-Path $ReferenceDirectory $file)).Hash -ne (Get-FileHash -LiteralPath (Join-Path $OutputDirectory $file)).Hash) { throw "Reference cache $file mismatch" } }
}
Invoke-Engine 'gendaymtx' @('-O0','-m',"$CandidateBasis",'-c','1','1','1','-g','0.2','0.2','0.2','-s','weather.epw') 'all-skies.mtx'
Invoke-Engine 'gendaymtx' @('-O0','-n','-D','all-suns.rad','-M','all-modifiers.txt','weather.epw') 'solar-generation.txt'
$sky=Read-Matrix 'all-skies.mtx'
$selectedSky=[Text.StringBuilder]::new("#?RADIANCE`nNROWS=$($sky.rows)`nNCOLS=$($indices.Count)`nNCOMP=3`nFORMAT=ascii`n`n")
for ($patch=0; $patch -lt $sky.rows; $patch++) { foreach ($index in $indices) { $offset=($patch*$sky.columns+$index)*3; $null=$selectedSky.Append(($sky.values[$offset..($offset+2)] | ForEach-Object { $_.ToString('R',$culture) }) -join ' ').Append("`n") } }
Write-Input 'selected-skies.mtx' $selectedSky.ToString()
$sunByIndex=@{}
$allSunText=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'all-suns.rad'))
foreach ($match in [regex]::Matches($allSunText,'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')) {
    $minute=[int]$match.Groups[1].Value
    $sunByIndex[[int](($minute-30)/60)]=$match.Value
}
$chosenSuns=''; $chosenModifiers=''
foreach ($index in $indices) {
    if (!$sunByIndex.ContainsKey($index)) { throw "Predeclared daylight interval $index has no sun above the horizon; investigate, never silently drop" }
    $chosenSuns+=$sunByIndex[$index]+"`n"
    $chosenModifiers+="solar$($index*60+30)`n"
}
Write-Input 'selected-suns.rad' $chosenSuns
Write-Input 'selected-modifiers.txt' $chosenModifiers
Write-Input 'receiver.rad' "void glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'diffuse.oct'
Invoke-Engine 'oconv' @('-f','scene.rad','selected-suns.rad') 'solar.oct'
$options=@('-I+','-n','1','-faa','-y',"$sensorCount",'-ab',"$CandidateBounces",'-ad',"$CandidateSamples",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw',$CandidateWeight.ToString('R',$culture),'-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
Invoke-Engine 'rcontrib' ($options+@('-V-','-e',"MF:$CandidateBasis",'-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow','diffuse.oct')) 'coefficients.mtx' $inputSensors
Invoke-Engine 'dctimestep' @('-oa','coefficients.mtx','selected-skies.mtx') 'diffuse-rgb.mtx'
$solarOptions=@($options)
$adIndex=[Array]::IndexOf($solarOptions,'-ad')
$solarOptions[$adIndex+1]="$CandidateSolarSamples"
Invoke-Engine 'rcontrib' ($solarOptions+@('-V+','-M','selected-modifiers.txt','solar.oct')) 'solar-rgb.mtx' $inputSensors
$diffuse=Read-Matrix 'diffuse-rgb.mtx'; $solar=Read-Matrix 'solar-rgb.mtx'
# Reference uses the identical discontinuous patch luminance. Engine library is
# copied only into disposable research scratch, retaining its source comment.
Write-Input 'matched-basis.cal' ("MF:$CandidateBasis;`n"+[IO.File]::ReadAllText((Join-Path $EngineRoot 'lib/reinhart.cal')))
$checks=[Collections.Generic.List[object]]::new()
for ($column=0; $column -lt $indices.Count; $column++) {
    $index=$indices[$column]
    $name="hour-$index"
    $data=[Text.StringBuilder]::new("1`n0 $($sky.rows-1) $($sky.rows)`n")
    for ($patch=0; $patch -lt $sky.rows; $patch++) { $null=$data.Append($sky.values[($patch*$sky.columns+$index)*3].ToString('R',$culture)).Append("`n") }
    Write-Input "$name.dat" $data.ToString()
    $exactSky="void brightdata distribution`n4 noop $name.dat matched-basis.cal rbin`n0`n0`ndistribution glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n" + $sunByIndex[$index]
    Write-Input "$name.rad" $exactSky
    $trace=@('-I+','-h','-faa','-ov','-n','1','-ab',"$ReferenceBounces",'-ad',"$ReferenceSamples",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw',$ReferenceWeight.ToString('R',$culture),'-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-',"$name.oct")
    if ($ReferenceDirectory) {
        if ((Get-FileHash -LiteralPath (Join-Path $ReferenceDirectory "$name.rad")).Hash -ne (Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name.rad")).Hash) { throw 'Reference sky differs from cached trace' }
        Copy-Item -LiteralPath (Join-Path $ReferenceDirectory "$name.rgb") -Destination (Join-Path $OutputDirectory "$name.rgb")
    } else {
        Invoke-Engine 'oconv' @('-f','scene.rad',"$name.rad") "$name.oct"
        Invoke-Engine 'rtrace' $trace "$name.rgb" $inputSensors
    }
    $rgb=([IO.File]::ReadAllText((Join-Path $OutputDirectory "$name.rgb")).Trim() -split '\s+') | ForEach-Object { [double]::Parse($_,$culture) }
    if ($rgb.Length -ne $sensorCount*3) { throw 'Truncated matched trace' }
    $maximumRatio=0.0; $failures=[Collections.Generic.List[object]]::new(); $sensorValues=[Collections.Generic.List[object]]::new()
    for ($sensor=0; $sensor -lt $sensorCount; $sensor++) {
        $offset=($sensor*$indices.Count+$column)*3
        $candidate=179*($diffuse.values[$offset]+$solar.values[$offset])
        $reference=179*(0.265074126*$rgb[$sensor*3]+0.670114631*$rgb[$sensor*3+1]+0.064811243*$rgb[$sensor*3+2])
        $error=[Math]::Abs($candidate-$reference); $limit=[Math]::Max(5.0,0.05*$reference)
        $maximumRatio=[Math]::Max($maximumRatio,$error/$limit)
        $value=@{sensor_index=$sensor;candidate_lux=$candidate;reference_lux=$reference;absolute_error_lux=$error;allowed_error_lux=$limit}
        $sensorValues.Add($value)
        if ($error -gt $limit) { $failures.Add($value) }
    }
    $checks.Add(@{interval_index=$index;condition=(Condition $index);pass=($failures.Count -eq 0);maximum_error_over_tolerance=$maximumRatio;failures=$failures;sensors=$sensorValues;reference_reused_from=$(if ($ReferenceDirectory) { Split-Path -Leaf $ReferenceDirectory } else {$null});reference_rgb_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name.rgb")).Hash.ToLowerInvariant()})
    @{declaration=$declaration;checks=$checks} | ConvertTo-Json -Depth 14 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'matched-evidence.json') -Encoding utf8
    Write-Host "MATCHED interval $index failures=$($failures.Count) max_error/tolerance=$maximumRatio"
}
Write-Output (@{checks=$checks.Count;failed_checks=@($checks | Where-Object { !$_.pass }).Count;output='matched-evidence.json'} | ConvertTo-Json)
