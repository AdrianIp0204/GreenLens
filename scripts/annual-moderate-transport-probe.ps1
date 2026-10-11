#Requires -Version 7.2
# Research only: moderate per-primary sampling and explicit, recorded repetitions.
param(
 [Parameter(Mandatory)][string]$SourceDirectory,
 [Parameter(Mandatory)][string]$OriginalJobDirectory,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [ValidateSet('diffuse','solar')][string]$Phase,
 [ValidateRange(16,262144)][int]$AmbientDivisions=8192,
 [ValidateRange(1,16)][int]$AmbientBounces=8,
 [ValidateRange(1,4096)][int]$PrimaryCount=1,
 [int[]]$SensorIndices=@(15,22)
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
$source=Get-Content -LiteralPath (Join-Path $SourceDirectory 'declaration.json') -Raw|ConvertFrom-Json
$hashes=[ordered]@{}
$bindings=[ordered]@{'scene.rad'='scene_sha256';'sensors.pts'='sensor_sha256'}
foreach($file in @('scene.rad','sensors.pts','selected-skies.mtx','selected-suns.rad','selected-modifiers.txt','declaration.json')){
 $hash=(Get-FileHash -LiteralPath (Join-Path $SourceDirectory $file)).Hash.ToLowerInvariant()
 if($bindings.Contains($file) -and $hash -ne $source.($bindings[$file])){throw "Source declaration mismatch: $file"}
 Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory "source-$file")
 $hashes[$file]=$hash
}
foreach($pair in @(@('annual-sky.mtx','sky_sha256'),@('annual-suns.rad','sun_sha256'),@('annual-recipe.json','source_recipe_sha256'))){if((Get-FileHash -LiteralPath (Join-Path $OriginalJobDirectory $pair[0])).Hash.ToLowerInvariant() -cne $source.($pair[1])){throw 'Original job identity differs from prepared inputs.'}}
$fullSky=[GreenLensFeasibility.Matrix]::Read((Join-Path $OriginalJobDirectory 'annual-sky.mtx'))
$selectedSky=[GreenLensFeasibility.Matrix]::Read((Join-Path $SourceDirectory 'selected-skies.mtx'))
if($fullSky.columns -ne 8760 -or $selectedSky.rows -ne $fullSky.rows -or $selectedSky.columns -ne @($source.intervals).Count){throw 'Prepared sky shape mismatch.'}
for($row=0;$row -lt $fullSky.rows;$row++){for($hour=0;$hour -lt $selectedSky.columns;$hour++){for($channel=0;$channel -lt 3;$channel++){if($selectedSky.values[($row*$selectedSky.columns+$hour)*3+$channel] -ne $fullSky.values[($row*8760+$source.intervals[$hour].index)*3+$channel]){throw 'Prepared sky value differs from original.'}}}}
$sunByIndex=@{};foreach($match in [regex]::Matches([IO.File]::ReadAllText((Join-Path $OriginalJobDirectory 'annual-suns.rad')),'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')){$sunByIndex[[int](([int]$match.Groups[1].Value-30)/60)]=$match.Value.Replace("`r`n","`n")}
$expectedSources=[Text.StringBuilder]::new();$expectedModifiers=[Text.StringBuilder]::new()
foreach($index in $source.intervals.index){if(!$sunByIndex.ContainsKey([int]$index)){throw 'Missing original source.'};$null=$expectedSources.Append($sunByIndex[[int]$index]).Append("`n");$null=$expectedModifiers.Append("solar$($index*60+30)`n")}
if([IO.File]::ReadAllText((Join-Path $SourceDirectory 'selected-suns.rad')) -cne $expectedSources.ToString() -or [IO.File]::ReadAllText((Join-Path $SourceDirectory 'selected-modifiers.txt')) -cne $expectedModifiers.ToString()){throw 'Prepared source sequence differs from original.'}
$fullSky=$null
foreach($file in @('scene.rad','selected-skies.mtx','selected-suns.rad','selected-modifiers.txt')){Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file)}
if([IO.File]::ReadAllText((Join-Path $OutputDirectory 'scene.rad')) -match '(?m)^\s*!'){throw 'Shell directives are prohibited.'}
$allSensors=@([IO.File]::ReadAllLines((Join-Path $SourceDirectory 'sensors.pts'))|Where-Object{$_})
if(@($SensorIndices|Sort-Object -Unique).Count -ne $SensorIndices.Count -or @($SensorIndices|Where-Object{$_ -lt 0 -or $_ -ge $allSensors.Count}).Count){throw 'Invalid unique sensor selection.'}
$sensorText=($SensorIndices|ForEach-Object{$allSensors[$_]}) -join "`n"
$sensorText+="`n"
$primaryText=($SensorIndices|ForEach-Object{($allSensors[$_]+"`n")*$PrimaryCount}) -join ''
Write-Input 'sensors.pts' $sensorText
Write-Input 'repeated-sensors.pts' $primaryText
$count=$SensorIndices.Count
$options=@('-I+','-n','1','-faa','-y',"$count",'-c',"$PrimaryCount",'-ab',"$AmbientBounces",'-ad',"$AmbientDivisions",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-V+')
$engine=[ordered]@{}
foreach($name in @('oconv','rcontrib','dctimestep')){$engine[$name]=(Get-FileHash -LiteralPath (Join-Path $EngineRoot "bin/$name.exe")).Hash.ToLowerInvariant()}
$declaration=[ordered]@{schema_version='greenlens.moderate-transport-probe.v1';scope='Bounded transport pilot; not accepted scientific profile';declared_utc=[DateTime]::UtcNow.ToString('o');phase=$Phase;ambient_divisions=$AmbientDivisions;ambient_bounces=$AmbientBounces;primary_repetitions=$PrimaryCount;primary_order='Consecutive repetitions per selected sensor; output one averaged row per sensor';source_receiver_scale=$(if($Phase -eq 'diffuse'){100}else{1});ray_depth=-16;original_sensor_indices=$SensorIndices;sensor_ids=@($SensorIndices|ForEach-Object{'s{0:D6}' -f ($_+1)});selected_sensor_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'sensors.pts')).Hash.ToLowerInvariant();primary_input_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'repeated-sensors.pts')).Hash.ToLowerInvariant();intervals=@($source.intervals);source_sha256=$hashes;engine_sha256=$engine;ray_arguments=$options;harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();release_acceptance_complete=$false}
$declaration|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
$intervalCount=@($source.intervals).Count
if($Phase -eq 'diffuse'){
 Write-Input 'coefficient-receiver.rad' "void glow sky_glow`n0`n0`n4 100 100 100 0`nsky_glow source upper_sky`n0`n0`n4 0 0 1 180`nsky_glow source lower_ground`n0`n0`n4 0 0 -1 180`n"
 Invoke-Engine 'oconv' @('-f','scene.rad','coefficient-receiver.rad') 'coefficients.oct'
 Invoke-Engine 'rcontrib' ($options+@('-e',"MF:$($source.sky_basis_mf)",'-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow','coefficients.oct')) 'coefficients-scaled.mtx' $primaryText
 Invoke-Engine 'dctimestep' @('-oa','coefficients-scaled.mtx','selected-skies.mtx') 'diffuse-scaled.mtx'
 $matrix=Read-Matrix 'diffuse-scaled.mtx'
 if($matrix.rows -ne $count -or $matrix.columns -ne $intervalCount){throw 'Diffuse matrix shape mismatch.'}
 $values=[double[]]$matrix.values.Clone();for($i=0;$i -lt $values.Count;$i++){$values[$i]/=100}
}else{
 if($PrimaryCount -ne 1){throw 'This solar pilot freezes one primary per sensor.'}
 Invoke-Engine 'oconv' @('-f','scene.rad','selected-suns.rad') 'solar.oct'
 Invoke-Engine 'rcontrib' ($options+@('-M','selected-modifiers.txt','solar.oct')) 'solar-full.mtx' $primaryText
 $options[[Array]::IndexOf($options,'-ab')+1]='0'
 Invoke-Engine 'rcontrib' ($options+@('-M','selected-modifiers.txt','solar.oct')) 'solar-direct.mtx' $primaryText
 $matrix=Read-Matrix 'solar-full.mtx';$direct=Read-Matrix 'solar-direct.mtx'
 if($matrix.rows -ne $count -or $matrix.columns -ne $intervalCount -or $direct.rows -ne $count -or $direct.columns -ne $intervalCount){throw 'Solar matrix shape mismatch.'}
 $values=[double[]]$matrix.values.Clone();for($i=0;$i -lt $values.Count;$i++){$values[$i]-=$direct.values[$i];if(![double]::IsFinite($values[$i]) -or $values[$i] -lt 0){throw 'Negative or invalid indirect solar output; no clamping.'}}
}
$rows=@(for($sensor=0;$sensor -lt $count;$sensor++){for($hour=0;$hour -lt $intervalCount;$hour++){
 $offset=($sensor*$intervalCount+$hour)*3;$rgb=@($values[$offset],$values[$offset+1],$values[$offset+2])
 [ordered]@{sensor_index=$SensorIndices[$sensor];sensor_id=('s{0:D6}' -f ($SensorIndices[$sensor]+1));interval_index=$source.intervals[$hour].index;rgb=$rgb;lux=179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])}
}})
$rows|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
Write-Host "$Phase completed $count sensors x $intervalCount intervals; no acceptance inferred."
