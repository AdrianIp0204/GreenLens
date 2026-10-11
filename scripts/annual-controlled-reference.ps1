#Requires -Version 7.2
# Reference qualification only. No candidate arrays are read or compared here.
param(
 [Parameter(Mandatory)][string]$OriginalJobDirectory,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [Parameter(Mandatory)][ValidateSet('A','B')][string]$Alternative,
 [int[]]$IntervalIndices=@(8512,8504,8508,1904,1908,1912,4112,4116,4120,6320,6324,6328),
 [switch]$PrepareOnly
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained reference directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
. "$PSScriptRoot/annual-reference-process.ps1"
. "$PSScriptRoot/annual-matched-evidence.ps1"
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs" -Destination (Join-Path $OutputDirectory 'aggregation-source.cs')
Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-process.ps1" -Destination (Join-Path $OutputDirectory 'process-source.ps1')
Add-Type -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs"
$kernelSource=[IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1")
$kernel=[regex]::Match($kernelSource,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
if(!$kernel){throw 'Missing reviewed finite-direct kernel.'}
Add-Type -TypeDefinition $kernel
Write-Input 'finite-kernel.cs' $kernel
$recipe=Get-Content -LiteralPath (Join-Path $OriginalJobDirectory 'annual-recipe.json') -Raw|ConvertFrom-Json
$model=Get-Content -LiteralPath (Join-Path $OriginalJobDirectory 'input.json') -Raw|ConvertFrom-Json
$study=Get-Content -LiteralPath (Join-Path $OriginalJobDirectory 'study.json') -Raw|ConvertFrom-Json
if($model.room.width_m -ne 4 -or $model.room.depth_m -ne 6 -or $model.room.height_m -ne 3 -or $model.room.rotation_deg -ne 0 -or $model.sensors.requested_spacing_m -ne 1 -or $null -ne $model.obstruction){throw 'Not the predeclared annual example.'}
if(($Alternative -eq 'A' -and $null -ne $model.overhang) -or ($Alternative -eq 'B' -and $model.overhang.projection_m -ne .6)){throw 'Alternative label differs from actual model.'}
if($study.calculation.weather.original_sha256 -ne '60719262a01e8827e8984537800f97f508ab4ae5ae8abc55fce571893c55b2d8' -or $study.calculation.weather.normalized_sha256 -ne '5dec99cf1e82f5492f62640241618d2b591bc8852145a5c7a56ea5d60c4e82ed' -or $study.calculation.weather.time_axis_sha256 -ne 'f11e5fbc0273736b9a9f08afa8ea06503706a1c6489ca123ff5b2e260b893d2f' -or $study.calculation.weather.interpretation -ne 'greenlens.pvgis_tmy_epw_midpoint.v1'){throw 'Weather binding differs from declared reference.'}
if($recipe.sensor_count -ne 24 -or $recipe.profile.sky_basis_mf -ne 2 -or ($recipe.interval_indices -join ',') -ne ((0..8759) -join ',')){throw 'This declaration requires the documented 24-sensor, MF2 full-year inputs.'}
$declared=@(1904,1908,1912,4112,4116,4120,6320,6324,6328,8504,8508,8512)
if(!$IntervalIndices.Count -or @($IntervalIndices|Sort-Object -Unique).Count -ne $IntervalIndices.Count -or @($IntervalIndices|Where-Object{$_ -notin $declared}).Count){throw 'Intervals must be a unique subset of the fixed plan.'}
$inputHashes=[ordered]@{}
foreach($pair in @(@('annual-scene.rad','scene.rad'),@('annual-sensors.pts','sensors.pts'),@('annual-sky.mtx','identical-sky.mtx'),@('annual-suns.rad','suns.rad'),@('annual-weather.epw','weather.epw'),@('annual-recipe.json','source-recipe.json'),@('input.json','model.json'),@('study.json','study.json'))){
 Copy-Item -LiteralPath (Join-Path $OriginalJobDirectory $pair[0]) -Destination (Join-Path $OutputDirectory $pair[1])
 $inputHashes[$pair[1]]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $pair[1])).Hash.ToLowerInvariant()
}
if($inputHashes['scene.rad'] -ne $recipe.scene_sha256 -or $inputHashes['sensors.pts'] -ne $recipe.sensor_sha256 -or $inputHashes['weather.epw'] -ne $recipe.normalized_weather_sha256){throw 'Generated input hashes differ from immutable recipe.'}
if($inputHashes['weather.epw'] -ne $study.calculation.weather.normalized_sha256){throw 'Generated weather differs from the study binding.'}
foreach($pair in @(@('original.epw','original_sha256'),@('normalized.epw','normalized_sha256'))){if((Get-FileHash -LiteralPath (Join-Path $OriginalJobDirectory "weather/$($pair[0])")).Hash.ToLowerInvariant() -ne $study.calculation.weather.($pair[1])){throw 'Retained weather bytes differ from source binding.'}}
Copy-Item -LiteralPath (Join-Path $OriginalJobDirectory 'weather/asset.json') -Destination (Join-Path $OutputDirectory 'weather-asset.json')
$inputHashes['weather-asset.json']=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'weather-asset.json')).Hash.ToLowerInvariant()
$weatherRows=@([IO.File]::ReadAllLines((Join-Path $OutputDirectory 'weather.epw'))|Select-Object -Skip 8)
if($weatherRows.Count -ne 8760){throw 'Expected complete canonical weather.'}
function Condition([int]$Index){$f=$weatherRows[$Index].Split(',');$dni=[double]::Parse($f[14],$culture);$dhi=[double]::Parse($f[15],$culture);if($dhi -lt 50){return 'low_diffuse'};if($dni/$dhi -ge 3){return 'clear'};if($dni/$dhi -ge .5){return 'mixed'};return 'overcast'}
$conditions=@($declared|ForEach-Object{[ordered]@{interval_index=$_;condition=(Condition $_)}})
foreach($class in @('clear','mixed','overcast')){if($class -notin $conditions.condition){throw 'The fixed weather requires an appended condition-coverage interval; seal a complete amended declaration before execution.'}}
$scene=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'scene.rad'));if($scene -match '(?m)^\s*!'){throw 'Scene shell directives are prohibited.'}
$sensors=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'sensors.pts'))
if(!$sensors.EndsWith("`n")){throw 'Repeated sensor input requires the original trailing newline.'}
if(@($sensors.Trim() -split '\r?\n').Count -ne 24){throw 'Sensor count mismatch.'}
$inventory=Get-Content -LiteralPath "$PSScriptRoot/../dependencies/radiance.json" -Raw|ConvertFrom-Json
$engine=[ordered]@{}
foreach($name in @('rtrace','rcontrib','oconv')){$hash=(Get-FileHash -LiteralPath (Join-Path $EngineRoot "bin/$name.exe")).Hash.ToLowerInvariant();$expected=if($name -eq 'rcontrib'){$inventory.annual_workflow_review.executables.$name.sha256}else{$inventory.executables.$name.sha256};if($hash -ne $expected){throw 'Pinned executable differs.'};$engine["$name.exe"]=$hash}
foreach($name in @('rayinit.cal','skybright.cal')){$hash=(Get-FileHash -LiteralPath (Join-Path $EngineRoot "lib/$name")).Hash.ToLowerInvariant();if($hash -ne $inventory.critical_libraries.$name.sha256){throw 'Pinned library differs.'};$engine[$name]=$hash}
$basisHash=(Get-FileHash -LiteralPath (Join-Path $EngineRoot 'lib/reinhart.cal')).Hash.ToLowerInvariant()
if($basisHash -ne $inventory.annual_workflow_review.additional_critical_libraries.'reinhart.cal'.sha256){throw 'Pinned basis differs.'};$engine['reinhart.cal']=$basisHash
$sky=Read-Matrix 'identical-sky.mtx';if($sky.rows -ne 578 -or $sky.columns -ne 8760){throw 'Invalid MF2 year.'}
$sunByIndex=@{};foreach($match in [regex]::Matches([IO.File]::ReadAllText((Join-Path $OutputDirectory 'suns.rad')),'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')){$sunByIndex[[int](([int]$match.Groups[1].Value-30)/60)]=$match.Value}
$fullOptions=@('-I+','-h','-faa','-n','1','-ab','9','-ad','8192','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-c','1','-V+')
$rayOptions=@('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$declaration=[ordered]@{schema_version='greenlens.controlled-reference.v1';scope='Reference-only qualification before candidate comparison; earlier failures remain immutable';declared_utc=[DateTime]::UtcNow.ToString('o');alternative=$Alternative;declared_interval_indices=$declared;executed_interval_indices=$IntervalIndices;sensor_count=24;input_sha256=$inputHashes;engine_sha256=$engine;source_scale=100;primary_order='Round-major: all24 immutable ordered sensors once per round';ambient_divisions=8192;ambient_bounces=9;ray_depth=-16;ambient_weight=1e-9;initial_rungs=@(64,128,256);optional_final_rung=512;initial_process_rounds=256;escalation='Run512 only if initial qualification fails; require exact first256 output records by sensor/round equal the original deterministic prefix; do not count that prefix twice';qualification='Each sensor passes both last two adjacent nested-mean transitions and disjoint equal-half means, abs difference<=max(0.1lx,1% higher mean)';full_arguments=$fullOptions;direct_arguments=$rayOptions;finite_samples=4096;maximum_process_seconds=7200;maximum_scratch_bytes=2147483648;maximum_single_file_bytes=536870912;harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();aggregation_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs").Hash.ToLowerInvariant();finite_kernel_sha256=(Get-MatchedKernelHash $kernelSource);protocol_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/../docs/v0.3-reference-protocol.md").Hash.ToLowerInvariant();release_acceptance_complete=$false}
$declaration.process_helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-reference-process.ps1").Hash.ToLowerInvariant()
$declaration.maximum_runner_plus_engine_working_set_bytes=2147483648
$declaration.weather_binding=$study.calculation.weather
$declaration.condition_coverage=$conditions
$declaration.normalized_model=$model
$declaration.sky_basis_mf=2
$declaration.block_tolerance='max(0.1lx,1% complete final mean), applied to the two equal disjoint halves'
$declaration.sensitivities=[ordered]@{fixed_cases=@('B:8512','A:4116');scope='All24 sensors of each fixed case; limited sensitivity evidence only';repeat_policy='Match qualified baseline final rung';scale_variant=10000;scale_tolerance='1e-5 relative per normalized RGB bin; exact-zero agreement';weight_variant=1e-10;depth_variant=@{ambient_bounces=10;ray_depth=-24};weight_depth_tolerance='max(0.1lx,1% baseline mean)';execution='Separate follow-up harness; required before reference qualification complete'}
$declaration|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
if($PrepareOnly){Write-Host 'Prepared declaration only; native reference NOT EXECUTED.';return}
Write-Input 'matched-basis.cal' ("MF:2;`n"+[IO.File]::ReadAllText((Join-Path $EngineRoot 'lib/reinhart.cal')))
Write-Input 'receiver.rad' "void glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'transmission.oct'
$checks=[Collections.Generic.List[object]]::new()
foreach($index in $IntervalIndices){
 if(!$sunByIndex.ContainsKey([int]$index)){throw 'Declared reference interval has no explicit source.'}
 $name="hour-$index";$sun=$sunByIndex[[int]$index]
 Write-Input "$name-sun.rad" $sun
 $data=[Text.StringBuilder]::new("1`n0 577 578`n")
 for($patch=0;$patch -lt 578;$patch++){$offset=($patch*8760+$index)*3;$rgb=$sky.values[$offset..($offset+2)];if($rgb[0] -ne $rgb[1] -or $rgb[0] -ne $rgb[2]){throw 'Non-gray sky needs explicit per-channel distributions.'};$null=$data.Append($rgb[0].ToString('R',$culture)).Append("`n")}
 Write-Input "$name.dat" $data.ToString()
 $pattern='(?s)(void light (solar\d+)\s+0\s+0\s+3)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)'
 $modifier=[regex]::Match($sun,$pattern).Groups[2].Value;if(!$modifier){throw 'Malformed generated source.'}
 $scaledSun=[regex]::Replace($sun,$pattern,{param($m) $rgb=@(3..5|ForEach-Object{([double]::Parse($m.Groups[$_].Value,$culture)*100).ToString('R',$culture)});$m.Groups[1].Value+"`n"+($rgb -join ' ')})
 Write-Input "$name-scaled.rad" ("void brightdata distribution`n4 noop $name.dat matched-basis.cal rbin`n0`n0`ndistribution glow sky_glow`n0`n0`n4 100 100 100 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"+$scaledSun)
 Invoke-Engine 'oconv' @('-f','scene.rad',"$name-scaled.rad") "$name.oct"
 Invoke-Engine 'oconv' @('-f','scene.rad',"$name-sun.rad") "$name-sun.oct"
 Invoke-Engine 'rtrace' (@('-I+')+$rayOptions+@("$name-sun.oct")) "$name-direct.rgb" $sensors
 Invoke-Engine 'rtrace' ($rayOptions+@('transmission.oct')) "$name-finite.rgb" ([GreenLensMatched.Direct]::Rays($sensors,$sun))
 $direct=[GreenLensMatched.Direct]::Rgb((Join-Path $OutputDirectory "$name-direct.rgb"),24)
 $finite=[GreenLensMatched.Direct]::Finite($sensors,$sun,(Join-Path $OutputDirectory "$name-finite.rgb"))
 $attempts=[Collections.Generic.List[object]]::new();$previous=$null;$qualified=$false
 foreach($rounds in @(256,512)){
  Write-Input "$name-rounds$rounds.pts" ($sensors*$rounds)
  Invoke-Engine 'rcontrib' ($fullOptions+@('-y',"$(24*$rounds)",'-m','sky_glow','-m',$modifier,"$name.oct")) "$name-rounds$rounds.rgb" ($sensors*$rounds)
  $values=[GreenLensReference.Aggregate]::Read((Join-Path $OutputDirectory "$name-rounds$rounds.rgb"),(24*$rounds*6))
  if($null -ne $previous){[GreenLensReference.Aggregate]::EqualPrefix($previous,$values)}
  $rungs=@();foreach($n in @($rounds/4,$rounds/2,$rounds)){$rungs+=[GreenLensReference.Aggregate]::Calculate($values,24,0,[int]$n,100,$direct,$finite)}
  $first=[GreenLensReference.Aggregate]::Compare($rungs[0],$rungs[1]);$second=[GreenLensReference.Aggregate]::Compare($rungs[1],$rungs[2])
  $block1=[GreenLensReference.Aggregate]::Calculate($values,24,0,($rounds/2),100,$direct,$finite)
  $block2=[GreenLensReference.Aggregate]::Calculate($values,24,($rounds/2),($rounds/2),100,$direct,$finite)
  $blocks=[GreenLensReference.Aggregate]::CompareWithScale($block1,$block2,$rungs[2])
  $qualified=@(@($first)+@($second)+@($blocks)|Where-Object{!$_.passed}).Count -eq 0
  $attempts.Add([ordered]@{rounds=$rounds;rungs=$rungs;first_transition=$first;second_transition=$second;block1=$block1;block2=$block2;block_comparison=$blocks;qualified=$qualified;raw_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name-rounds$rounds.rgb")).Hash.ToLowerInvariant();primary_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name-rounds$rounds.pts")).Hash.ToLowerInvariant()})
  $attempts|ConvertTo-Json -Depth 20|Set-Content -LiteralPath (Join-Path $OutputDirectory "$name-convergence.json") -Encoding utf8
  Write-Host "Reference $Alternative interval$index $rounds rounds qualified=$qualified"
  if($qualified){break};$previous=$values
 }
 $checks.Add([ordered]@{interval_index=$index;qualified=$qualified;attempts=$attempts;direct_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name-direct.rgb")).Hash.ToLowerInvariant();finite_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name-finite.rgb")).Hash.ToLowerInvariant();sky_scene_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name-scaled.rad")).Hash.ToLowerInvariant()})
 [ordered]@{declaration=$declaration;checks=$checks;completed=($checks.Count -eq $IntervalIndices.Count);all_executed_references_qualified=($checks.Count -eq $IntervalIndices.Count -and @($checks|Where-Object{!$_.qualified}).Count -eq 0);sensitivity_checks_complete=$false;release_acceptance_complete=$false}|ConvertTo-Json -Depth 25|Set-Content -LiteralPath (Join-Path $OutputDirectory 'controlled-reference.json') -Encoding utf8
}
if(@($checks|Where-Object{!$_.qualified}).Count){throw 'Reference remains unqualified at the predeclared cap; all failed cases are retained.'}
