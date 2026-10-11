#Requires -Version 7.2
# Exploratory accuracy isolation, not an acceptance substitute. Input is a
# previously generated trusted matched-sky research directory, never an import.
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 diffuse isolation 空間'),
    [int]$Interval = 1904,
    [int[]]$SensorIndices = @(15,23),
    [int[]]$SampleCounts = @(8192,32768,131072),
    [double]$Weight = 0.000000001,
    [int]$Bounces = 8
)
$ErrorActionPreference='Stop'
$isolationBounces=$Bounces
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
$sensorLines = [IO.File]::ReadAllLines((Join-Path $SourceDirectory 'sensors.pts'))
$sensors = (($SensorIndices | ForEach-Object { $sensorLines[$_] }) -join "`n")+"`n"
foreach ($file in @('scene.rad','matched-basis.cal',"hour-$Interval.dat")) {
    Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file)
}
Write-Input 'sensors.pts' $sensors
$source = [IO.File]::ReadAllText((Join-Path $SourceDirectory "hour-$Interval.rad"))
$offset = $source.IndexOf('void light solar')
if ($offset -lt 0) { throw 'Trusted source must include the expected explicit solar source' }
Write-Input 'diffuse.rad' $source.Substring(0,$offset)
Write-Input 'solar.rad' $source.Substring($offset)
Write-Input 'receiver.rad' "void glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"
$dataTokens = ([IO.File]::ReadAllText((Join-Path $OutputDirectory "hour-$Interval.dat"))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries)
$count=[int]$dataTokens[3]
$matrix="#?RADIANCE`nNROWS=$count`nNCOLS=1`nNCOMP=3`nFORMAT=ascii`n`n"
for($index=4;$index -lt $dataTokens.Count;$index++) { $matrix+="$($dataTokens[$index]) $($dataTokens[$index]) $($dataTokens[$index])`n" }
Write-Input 'sky.mtx' $matrix
Invoke-Engine 'oconv' @('-f','scene.rad','diffuse.rad') 'diffuse.oct'
Invoke-Engine 'oconv' @('-f','scene.rad','solar.rad') 'solar.oct'
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'coefficients.oct'
$declaration=[ordered]@{ purpose='Exploratory isolated transport convergence; concurrent host activity allowed, not resource acceptance'; interval=$Interval;original_sensor_indices=$SensorIndices;samples=$SampleCounts;limit_weight=$Weight;bounces=$isolationBounces;declared_utc=[DateTime]::UtcNow.ToString('o') }
$declaration | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
$checks=[Collections.Generic.List[object]]::new()
foreach($sampleCount in $SampleCounts) {
    $shared=@('-I+','-n','1','-faa','-ab',"$isolationBounces",'-ad',"$sampleCount",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw',$Weight.ToString('R',$culture),'-lr','20','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
    Invoke-Engine 'rcontrib' ($shared+@('-y',"$($SensorIndices.Count)",'-V-','-e','MF:2','-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow','coefficients.oct')) "coeff-$sampleCount.mtx" $sensors
    Invoke-Engine 'dctimestep' @('-oa',"coeff-$sampleCount.mtx",'sky.mtx') "matrix-$sampleCount.mtx"
    Invoke-Engine 'rtrace' ($shared+@('-h','-ov','diffuse.oct')) "diffuse-$sampleCount.rgb" $sensors
    Invoke-Engine 'rtrace' ($shared+@('-h','-ov','solar.oct')) "solar-$sampleCount.rgb" $sensors
    Invoke-Engine 'rcontrib' ($shared+@('-y',"$($SensorIndices.Count)",'-V+','-m',"solar$($Interval*60+30)",'solar.oct')) "sunmatrix-$sampleCount.mtx" $sensors
    $matrix=Read-Matrix "matrix-$sampleCount.mtx"; $sun=Read-Matrix "sunmatrix-$sampleCount.mtx"
    $diffuse=@(([IO.File]::ReadAllText((Join-Path $OutputDirectory "diffuse-$sampleCount.rgb"))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { [double]::Parse($_,$culture) })
    $solar=@(([IO.File]::ReadAllText((Join-Path $OutputDirectory "solar-$sampleCount.rgb"))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { [double]::Parse($_,$culture) })
    for($i=0;$i -lt $SensorIndices.Count;$i++) {
        $checks.Add(@{samples=$sampleCount;sensor_index=$SensorIndices[$i];matrix_diffuse_lux=179*$matrix.values[$i*3];ray_diffuse_lux=179*$diffuse[$i*3];matrix_solar_lux=179*$sun.values[$i*3];ray_solar_lux=179*$solar[$i*3]})
    }
    $checks | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'isolation.json') -Encoding utf8
    $checks | Select-Object -Last $SensorIndices.Count | ConvertTo-Json -Compress | Write-Host
}
