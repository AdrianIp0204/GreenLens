#Requires -Version 7.2
# Sealed experimental candidates. Never treats unqualified references as truth.
param(
 [Parameter(Mandatory)][string]$OriginalJobDirectory,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [Parameter(Mandatory)][ValidateSet('A','B')][string]$Alternative,
 [ValidateSet('candidate','higher','secondary-sensitivity')][string]$Variant='candidate',
 [switch]$PrepareOnly
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
. "$PSScriptRoot/annual-paired-process.ps1"
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
Copy-Item -LiteralPath "$PSScriptRoot/annual-paired-grid.cs" -Destination (Join-Path $OutputDirectory 'primary-source.cs')
Copy-Item -LiteralPath "$PSScriptRoot/annual-paired-process.ps1" -Destination (Join-Path $OutputDirectory 'process-source.ps1')
Copy-Item -LiteralPath "$PSScriptRoot/annual-paired-native-process.cs" -Destination (Join-Path $OutputDirectory 'native-process-source.cs')
Add-Type -LiteralPath "$PSScriptRoot/annual-paired-grid.cs"
$sourceKernel=[regex]::Match([IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1"),"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
if(!$sourceKernel){throw 'Missing reviewed finite kernel.'}
$primarySide=if($Variant -eq 'higher'){256}else{128}
$secondaryCount=if($Variant -eq 'candidate'){64}else{128}
$fullBounceCount=if($Variant -eq 'higher'){8}else{7}
$finiteSide=if($Variant -eq 'higher'){64}else{32}
$finiteCount=$finiteSide*$finiteSide
$kernel=$sourceKernel.Replace('GreenLensMatched','GreenLensPairedFinite') -replace '\b4096\b',"$finiteCount" -replace '\b64\b',"$finiteSide"
Write-Input 'finite-kernel.cs' $kernel
Add-Type -TypeDefinition $kernel
$originalRecipe=Get-Content -LiteralPath (Join-Path $OriginalJobDirectory 'annual-recipe.json') -Raw|ConvertFrom-Json
$model=Get-Content -LiteralPath (Join-Path $OriginalJobDirectory 'input.json') -Raw|ConvertFrom-Json
$study=Get-Content -LiteralPath (Join-Path $OriginalJobDirectory 'study.json') -Raw|ConvertFrom-Json
if($model.room.width_m -ne 4 -or $model.room.depth_m -ne 6 -or $model.room.height_m -ne 3 -or $model.room.rotation_deg -ne 0 -or $model.sensors.requested_spacing_m -ne 1 -or $null -ne $model.obstruction){throw 'Not the fixed annual model.'}
if(($Alternative -eq 'A' -and $null -ne $model.overhang) -or ($Alternative -eq 'B' -and $model.overhang.projection_m -ne .6)){throw 'Alternative differs from model.'}
if($originalRecipe.sensor_count -ne 24 -or $originalRecipe.profile.sky_basis_mf -ne 2){throw 'Expected original24 sensor MF2 inputs.'}
$fixedIndices=@(1904,1908,1912,4112,4116,4120,6320,6324,6328,8504,8508,8512)
$selectedIndices=if($Variant -ne 'secondary-sensitivity'){$fixedIndices}elseif($Alternative -eq 'A'){@(4116)}else{@(8512)}
$inputHashes=[ordered]@{}
foreach($pair in @(@('annual-scene.rad','scene.rad'),@('annual-sensors.pts','sensors.pts'),@('annual-sky.mtx','diffuse-year.mtx'),@('annual-solar-sky.mtx','solar-year.mtx'),@('annual-suns.rad','suns.rad'),@('annual-weather.epw','weather.epw'),@('annual-recipe.json','source-recipe.json'),@('input.json','model.json'),@('study.json','study.json'))){
 Copy-Item -LiteralPath (Join-Path $OriginalJobDirectory $pair[0]) -Destination (Join-Path $OutputDirectory $pair[1])
 $inputHashes[$pair[1]]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $pair[1])).Hash.ToLowerInvariant()
}
if($inputHashes['scene.rad'] -ne $originalRecipe.scene_sha256 -or $inputHashes['sensors.pts'] -ne $originalRecipe.sensor_sha256 -or $inputHashes['weather.epw'] -ne $originalRecipe.normalized_weather_sha256 -or $inputHashes['weather.epw'] -ne $study.calculation.weather.normalized_sha256){throw 'Immutable physical input hashes differ.'}
$scene=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'scene.rad'));if($scene -match '(?m)^\s*!'){throw 'Scene shell directive prohibited.'}
$sensorText=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'sensors.pts'))
$sunByIndex=@{};foreach($match in [regex]::Matches([IO.File]::ReadAllText((Join-Path $OutputDirectory 'suns.rad')),'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')){$sunByIndex[[int](([int]$match.Groups[1].Value-30)/60)]=$match.Value}
foreach($index in $fixedIndices){if(!$sunByIndex.ContainsKey($index)){throw 'Fixed daylight interval lacks emitted source.'}}
$inventory=Get-Content -LiteralPath "$PSScriptRoot/../dependencies/radiance.json" -Raw|ConvertFrom-Json
$engineHashes=[ordered]@{}
foreach($name in @('rtrace','oconv','rcontrib','dctimestep')){
 $hash=(Get-FileHash -LiteralPath (Join-Path $EngineRoot "bin/$name.exe")).Hash.ToLowerInvariant()
 $expected=if($name -in @('rcontrib','dctimestep')){$inventory.annual_workflow_review.executables.$name.sha256}else{$inventory.executables.$name.sha256}
 if($hash -ne $expected){throw 'Pinned executable differs.'};$engineHashes["$name.exe"]=$hash
}
foreach($name in @('rayinit.cal','skybright.cal','reinhart.cal')){
 $hash=(Get-FileHash -LiteralPath (Join-Path $EngineRoot "lib/$name")).Hash.ToLowerInvariant()
 $expected=if($name -eq 'reinhart.cal'){$inventory.annual_workflow_review.additional_critical_libraries.$name.sha256}else{$inventory.critical_libraries.$name.sha256}
 if($hash -ne $expected){throw 'Pinned library differs.'};$engineHashes[$name]=$hash
}
$weatherRows=[IO.File]::ReadAllLines((Join-Path $OutputDirectory 'weather.epw'))
$conditions=@($fixedIndices|ForEach-Object{$fields=$weatherRows[8+$_].Split(',');$dni=[double]::Parse($fields[14],$culture);$dhi=[double]::Parse($fields[15],$culture);$condition=if($dhi -lt 50){'low_diffuse'}elseif($dni/$dhi -ge 3){'clear'}elseif($dni/$dhi -ge .5){'mixed'}else{'overcast'};@{interval_index=$_;condition=$condition}})
foreach($class in @('overcast','mixed','clear')){if($class -notin $conditions.condition){throw 'Fixed condition coverage changed; investigate before execution.'}}
$base=@('-n','1','-faa','-y','24','-ad',"$secondaryCount",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-V+','-c',"$($primarySide*$primarySide)",'-e','MF:2','-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow')
$rayOptions=@('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$declaration=[ordered]@{schema_version='greenlens.paired-primary-grid.v1';declared_utc=[DateTime]::UtcNow.ToString('o');scope='Experimental full-grid candidates; no comparison with unqualified controlled references';alternative=$Alternative;variant=$Variant;fixed_interval_indices=$fixedIndices;executed_interval_indices=$selectedIndices;sensor_count=24;condition_coverage=$conditions;weather_binding=$study.calculation.weather;normalized_model=$model;input_sha256=$inputHashes;engine_sha256=$engineHashes;sky_basis_mf=2;primary_side=$primarySide;primary_count=$primarySide*$primarySide;secondary_divisions=$secondaryCount;full_ambient_bounces=$fullBounceCount;direct_ambient_bounces=0;receiver_scale=100;finite_samples=$finiteCount;primary_rule='Identical cosine-weighted radial/azimuth midpoint rays for each full/direct pair; sensor-major, then radial-major and azimuth; sensor origin+1e-4 upward';conversion='pi/100 times full coefficient diffuse product plus pi/100 times full-minus-direct solar product plus finite direct';base_arguments=$base;ray_arguments=$rayOptions;paired_only=$true;nonnegative_policy='Reject negative coefficient and assembled reflected terms; retain every failed value, never clamp';reference_policy='Only qualified controlled references may establish max(5lx,5%) matched agreement; MF2 sky is identical, annual MF4 convergence remains separate';sensitivities=@{scope='All24 sensors at fixed B8512 and A4116';primary_side=128;secondary_divisions=128;ambient_bounces=7;finite_samples=1024;status='Predeclared separate follow-up'};maximum_process_seconds=7200;maximum_runner_plus_engine_working_set_bytes=2147483648;maximum_scratch_bytes=2147483648;maximum_single_file_bytes=536870912;harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();primary_helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-grid.cs").Hash.ToLowerInvariant();finite_kernel_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'finite-kernel.cs')).Hash.ToLowerInvariant();process_helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-process.ps1").Hash.ToLowerInvariant();release_acceptance_complete=$false}
$declaration.native_process_helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-native-process.cs").Hash.ToLowerInvariant()
$declaration|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
if($PrepareOnly){Write-Host 'Prepared sealed experiment inputs; no engine execution.';return}
foreach($kind in @('diffuse','solar')){
 $year=Read-Matrix "$kind-year.mtx";if($year.rows -ne 578 -or $year.columns -ne 8760){throw 'Wrong annual matrix shape.'}
 $selected=[Text.StringBuilder]::new("#?RADIANCE`nNROWS=578`nNCOLS=$($selectedIndices.Count)`nNCOMP=3`nFORMAT=ascii`n`n")
 for($patch=0;$patch -lt 578;$patch++){foreach($index in $selectedIndices){$offset=($patch*8760+$index)*3;for($channel=0;$channel -lt 3;$channel++){$null=$selected.Append($year.values[$offset+$channel].ToString('R',$culture)).Append(' ')}};$null=$selected.Append("`n")}
 Write-Input "$kind-selected.mtx" $selected.ToString()
}
$year=$null;$selected=$null
Write-Input 'scaled-receiver.rad' "void glow sky_glow`n0`n0`n4 100 100 100 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"
Write-Input 'unit-receiver.rad' "void glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"
Invoke-Engine 'oconv' @('-f','scene.rad','scaled-receiver.rad') 'coefficient.oct'
Invoke-Engine 'oconv' @('-f','scene.rad','unit-receiver.rad') 'transmission.oct'
[GreenLensPairedGrid.Primary]::WriteRays($sensorText,$primarySide,(Join-Path $OutputDirectory 'primary-rays.pts'))
Invoke-PairedProcess -Executable (Join-Path $EngineRoot 'bin/rcontrib.exe') -Name 'rcontrib' -Arguments ($base+@('-ab',"$fullBounceCount",'coefficient.oct')) -Output 'full-coefficients.mtx' -InputFile 'primary-rays.pts'
Invoke-PairedProcess -Executable (Join-Path $EngineRoot 'bin/rcontrib.exe') -Name 'rcontrib' -Arguments ($base+@('-ab','0','coefficient.oct')) -Output 'direct-coefficients.mtx' -InputFile 'primary-rays.pts'
$full=Read-Matrix 'full-coefficients.mtx';$direct=Read-Matrix 'direct-coefficients.mtx'
if($full.rows -ne 24 -or $full.columns -ne 578 -or $direct.rows -ne 24 -or $direct.columns -ne 578){throw 'Coefficient dimensions differ.'}
$negativeBins=[Collections.Generic.List[object]]::new()
for($offset=0;$offset -lt $full.values.Length;$offset++){if($full.values[$offset] -lt $direct.values[$offset]){$negativeBins.Add(@{sensor_index=[int][Math]::Floor($offset/(578*3));patch_index=[int][Math]::Floor(($offset%(578*3))/3);channel=$offset%3;full=$full.values[$offset];direct=$direct.values[$offset];residual=$full.values[$offset]-$direct.values[$offset]})}}
Invoke-Engine 'dctimestep' @('-oa','full-coefficients.mtx','diffuse-selected.mtx') 'diffuse-rgb.mtx'
Invoke-Engine 'dctimestep' @('-oa','full-coefficients.mtx','solar-selected.mtx') 'solar-rgb.mtx'
Invoke-Engine 'dctimestep' @('-oa','direct-coefficients.mtx','solar-selected.mtx') 'direct-solar-rgb.mtx'
$diffuse=Read-Matrix 'diffuse-rgb.mtx';$solar=Read-Matrix 'solar-rgb.mtx';$solarDirect=Read-Matrix 'direct-solar-rgb.mtx'
foreach($matrix in @($diffuse,$solar,$solarDirect)){if($matrix.rows -ne 24 -or $matrix.columns -ne $selectedIndices.Count){throw 'Product shape mismatch.'}}
$cases=[Collections.Generic.List[object]]::new()
for($hour=0;$hour -lt $selectedIndices.Count;$hour++){
 $index=$selectedIndices[$hour];$sun=$sunByIndex[$index];Write-Input "hour-$index-sun.rad" $sun
 Invoke-Engine 'rtrace' ($rayOptions+@('transmission.oct')) "hour-$index-finite.rgb" ([GreenLensPairedFinite.Direct]::Rays($sensorText,$sun))
 $finite=[GreenLensPairedFinite.Direct]::Finite($sensorText,$sun,(Join-Path $OutputDirectory "hour-$index-finite.rgb"))
 $rows=@();for($sensor=0;$sensor -lt 24;$sensor++){
  $offset=($sensor*$selectedIndices.Count+$hour)*3;$d=@();$s=@();$c=@();$r=@();$total=@();$valid=$true
  for($channel=0;$channel -lt 3;$channel++){
   $d+=$diffuse.values[$offset+$channel]*[Math]::PI/100;$s+=$solar.values[$offset+$channel]*[Math]::PI/100;$c+=$solarDirect.values[$offset+$channel]*[Math]::PI/100
   $r+=$s[$channel]-$c[$channel];$total+=$d[$channel]+$r[$channel]+$finite[$sensor][$channel]
   if(![double]::IsFinite($total[$channel]) -or $total[$channel] -lt 0 -or $r[$channel] -lt 0){$valid=$false}
  }
  $lux=179*(.265074126*$total[0]+.670114631*$total[1]+.064811243*$total[2])
  $rows+=@{sensor_index=$sensor;sensor_id=('s'+($sensor+1).ToString('D6'));diffuse_rgb=$d;full_solar_rgb=$s;patch_direct_rgb=$c;reflected_solar_rgb=$r;finite_direct_rgb=$finite[$sensor];total_rgb=$total;lux=$lux;valid=$valid}
 }
 $cases.Add(@{interval_index=$index;sensors=$rows;finite_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "hour-$index-finite.rgb")).Hash.ToLowerInvariant();sun_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "hour-$index-sun.rad")).Hash.ToLowerInvariant()})
 $hashes=[ordered]@{};foreach($name in @('full-coefficients.mtx','direct-coefficients.mtx','primary-rays.pts','diffuse-selected.mtx','solar-selected.mtx','diffuse-rgb.mtx','solar-rgb.mtx','direct-solar-rgb.mtx')){$hashes[$name]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $name)).Hash.ToLowerInvariant()}
 [ordered]@{declaration=$declaration;output_sha256=$hashes;negative_coefficient_bins=$negativeBins.ToArray();cases=$cases.ToArray();completed=($cases.Count -eq $selectedIndices.Count);all_values_finite_nonnegative=($negativeBins.Count -eq 0 -and @($cases.ToArray().sensors|Where-Object{!$_.valid}).Count -eq 0);matched_reference_qualified=$false;candidate_accuracy_accepted=$false;release_acceptance_complete=$false}|ConvertTo-Json -Depth 25|Set-Content -LiteralPath (Join-Path $OutputDirectory 'paired-grid.json') -Encoding utf8
 Write-Host "$Alternative $Variant interval$index complete; negative coefficient bins$($negativeBins.Count)"
}
if($negativeBins.Count -or @($cases.ToArray().sensors|Where-Object{!$_.valid}).Count){throw 'Paired full-grid experiment contains invalid residuals; all values retained.'}
