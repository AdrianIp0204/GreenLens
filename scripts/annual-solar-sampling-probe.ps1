#Requires -Version 7.2
# Bounded explicit-sun sampling investigation using trusted retained inputs.
param(
 [Parameter(Mandatory)][string]$MatchedDirectory,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [ValidateRange(16,1048576)][int]$SolarSamples=2048,
 [ValidateRange(1,16)][int]$SolarBounces=8,
 [switch]$TransportOnly
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
. "$PSScriptRoot/annual-matched-evidence.ps1"
foreach($name in @('scene.rad','sensors.pts','selected-suns.rad','selected-modifiers.txt','selected-skies.mtx','declaration.json')){Copy-Item -LiteralPath (Join-Path $MatchedDirectory $name) -Destination (Join-Path $OutputDirectory $name)}
$original=Get-Content -LiteralPath (Join-Path $MatchedDirectory 'declaration.json') -Raw|ConvertFrom-Json
if(!$TransportOnly){foreach($index in $original.intervals.index){Copy-Item -LiteralPath (Join-Path $MatchedDirectory "hour-$index.rgb") -Destination (Join-Path $OutputDirectory "hour-$index.rgb")}}
$sensors=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'sensors.pts'));$count=@($sensors.Trim() -split '\r?\n').Count
$options=@('-I+','-n','1','-faa','-y',"$count",'-ab',"$SolarBounces",'-ad',"$SolarSamples",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-V+','-M','selected-modifiers.txt','solar.oct')
@{scope='Explicit-sun sampling investigation; final profile acceptance outstanding';declared_utc=[DateTime]::UtcNow.ToString('o');solar_samples=$SolarSamples;solar_bounces=$SolarBounces;scene_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'scene.rad'));sensor_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'sensors.pts'));sources_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'selected-suns.rad'));interval_indices=@($original.intervals.index);full_solar_arguments=$options}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'solar-declaration.json') -Encoding utf8
Invoke-Engine 'oconv' @('-f','scene.rad','selected-suns.rad') 'solar.oct'
Invoke-Engine 'rcontrib' $options 'solar-rgb.mtx' $sensors
$options[[Array]::IndexOf($options,'-ab')+1]='0'
Invoke-Engine 'rcontrib' $options 'solar-centroid-direct.mtx' $sensors
foreach($name in @('solar-rgb.mtx','solar-centroid-direct.mtx')){$matrix=Read-Matrix $name;if($matrix.rows -ne $count -or $matrix.columns -ne @($original.intervals.index).Count){throw 'Explicit solar matrix dimensions mismatch'}}
