#Requires -Version 7.2
# Bounded component isolation for the retained winter mismatch. This is a
# research diagnostic against trusted local generated files, not acceptance.
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [string]$OutputDirectory=(Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 winter sensor4 convergence 空間'),
    [int]$Interval=8504,
    [int]$SensorIndex=4,
    [int[]]$SampleCounts=@(262144,1048576),
    [int[]]$SolarBases=@(2,4,8)
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
foreach($file in @('scene.rad','receiver.rad','matched-basis.cal',"hour-$Interval.dat")) {
    Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file)
}
$sensor=[IO.File]::ReadAllLines((Join-Path $SourceDirectory 'sensors.pts'))[$SensorIndex]+"`n"
Write-Input 'sensor.pts' $sensor
$source=[IO.File]::ReadAllText((Join-Path $SourceDirectory "hour-$Interval.rad"))
$offset=$source.IndexOf('void light solar')
if($offset -lt 0){throw 'The predeclared diagnostic requires an emitted sun'}
Write-Input 'diffuse.rad' $source.Substring(0,$offset)
Write-Input 'solar.rad' $source.Substring($offset)
$weather=[IO.File]::ReadAllLines((Join-Path $SourceDirectory 'weather.epw'))
Write-Input 'weather.epw' ((($weather[0..7]+$weather[($Interval+6)..($Interval+8)]) -join "`n")+"`n")
$declaration=@{purpose='Winter component convergence diagnostic; not release acceptance';interval=$Interval;sensor_index=$SensorIndex;sensor=$sensor.Trim();sample_counts=$SampleCounts;solar_bases=$SolarBases;full_bounces=8;reference_bounces=9;weight=1e-9;ray_depth=16;declared_utc=[DateTime]::UtcNow.ToString('o');reference_policy='Identical cached MF2 diffuse sky and explicit continuous-centroid solar source. Report components separately; no finite-disc correction is needed to compare the indirect residual.'}
$declaration|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'coefficients.oct'
Invoke-Engine 'oconv' @('-f','scene.rad','diffuse.rad') 'diffuse.oct'
Invoke-Engine 'oconv' @('-f','scene.rad','solar.rad') 'solar.oct'
function Options([int]$Depth,[int]$Count) {
    return @('-I+','-n','1','-faa','-ab',"$Depth",'-ad',"$Count",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
}
function Lux([double[]]$Rgb) { return 179*(.265074126*$Rgb[0]+.670114631*$Rgb[1]+.064811243*$Rgb[2]) }
function Read-Ray([string]$Name) {
    $values=@(([IO.File]::ReadAllText((Join-Path $OutputDirectory $Name))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries)|ForEach-Object{[double]::Parse($_,$culture)})
    if($values.Count -ne 3 -or @($values|Where-Object{![double]::IsFinite($_) -or $_ -lt 0}).Count){throw 'Expected one finite nonnegative RGB ray'}
    return ,$values
}
function Matrix-Lux([string]$Name) {
    $matrix=Read-Matrix $Name
    if($matrix.rows -ne 1 -or $matrix.columns -ne 3){throw 'Expected a one-sensor three-context-interval RGB matrix'}
    return Lux $matrix.values[6..8]
}
Invoke-Engine 'rtrace' ((Options 0 1)+@('-h','-ov','solar.oct')) 'direct-centroid.rgb' $sensor
$direct=Lux (Read-Ray 'direct-centroid.rgb')
$checks=[Collections.Generic.List[object]]::new()
foreach($count in $SampleCounts) {
    Invoke-Engine 'rtrace' ((Options 9 $count)+@('-h','-ov','diffuse.oct')) "diffuse-$count.rgb" $sensor
    Invoke-Engine 'rtrace' ((Options 9 $count)+@('-h','-ov','solar.oct')) "solar-$count.rgb" $sensor
    $checks.Add(@{kind='continuous_reference';samples=$count;diffuse_lux=(Lux (Read-Ray "diffuse-$count.rgb"));solar_full_lux=(Lux (Read-Ray "solar-$count.rgb"));solar_direct_lux=$direct;solar_indirect_lux=((Lux (Read-Ray "solar-$count.rgb"))-$direct)})
    foreach($mf in $SolarBases) {
        Invoke-Engine 'gendaymtx' @('-O0','-m',"$mf",'-d','weather.epw') "sun-$mf.mtx"
        $coefficient=@('-y','1','-V-','-e',"MF:$mf",'-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow','coefficients.oct')
        Invoke-Engine 'rcontrib' ((Options 8 $count)+$coefficient) "full-$mf-$count.mtx" $sensor
        Invoke-Engine 'rcontrib' ((Options 1 $count)+$coefficient) "direct-$mf-$count.mtx" $sensor
        Invoke-Engine 'dctimestep' @('-oa',"full-$mf-$count.mtx", "sun-$mf.mtx") "full-sun-$mf-$count.mtx"
        Invoke-Engine 'dctimestep' @('-oa',"direct-$mf-$count.mtx", "sun-$mf.mtx") "direct-sun-$mf-$count.mtx"
        $full=Matrix-Lux "full-sun-$mf-$count.mtx";$directPatch=Matrix-Lux "direct-sun-$mf-$count.mtx"
        $checks.Add(@{kind='solar_patch';samples=$count;mf=$mf;solar_full_lux=$full;solar_direct_lux=$directPatch;solar_indirect_lux=($full-$directPatch)})
        $checks|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $OutputDirectory 'diagnostic.json') -Encoding utf8
        $checks|Select-Object -Last 1|ConvertTo-Json -Compress|Write-Host
    }
}
