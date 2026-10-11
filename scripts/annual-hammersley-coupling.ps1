#Requires -Version 7.2
# Coupling/positivity investigation only. This is not an accepted profile.
param(
 [Parameter(Mandatory)][string]$OriginalA,
 [Parameter(Mandatory)][string]$OriginalB,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [ValidateSet('candidate','higher')][string]$Variant='candidate'
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained investigation directory.'}
$count=if($Variant -eq 'higher'){65536}else{16384}
$SecondarySamples=if($Variant -eq 'higher'){128}else{64}
$FullBounces=if($Variant -eq 'higher'){8}else{7}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
. "$PSScriptRoot/annual-paired-process.ps1"
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
Copy-Item -LiteralPath "$PSScriptRoot/annual-paired-process.ps1" -Destination (Join-Path $OutputDirectory 'process-source.ps1')
Copy-Item -LiteralPath "$PSScriptRoot/annual-paired-native-process.cs" -Destination (Join-Path $OutputDirectory 'native-process-source.cs')
Copy-Item -LiteralPath "$PSScriptRoot/annual-paired-primary-stream.cs" -Destination (Join-Path $OutputDirectory 'primary-source.cs')
Add-Type -LiteralPath "$PSScriptRoot/annual-paired-primary-stream.cs"
$sensorText=[IO.File]::ReadAllText((Join-Path $OriginalA 'annual-sensors.pts'))
if($sensorText -ne [IO.File]::ReadAllText((Join-Path $OriginalB 'annual-sensors.pts'))){throw 'A/B sensor definitions differ.'}
$sensorLines=$sensorText.Trim() -split '\r?\n'
if($sensorLines.Count -ne 24){throw 'Expected original24 sensor fixture.'}
$indices=@(2,22)
$sensors=@($indices|ForEach-Object{ $tokens=$sensorLines[$_].Split(' ',[StringSplitOptions]::RemoveEmptyEntries); if($tokens.Count -ne 6){throw 'Invalid sensor.'}; ,@($tokens|ForEach-Object{[double]::Parse($_,$culture)}) })
foreach($sensor in $sensors){if($sensor[3] -ne 0 -or $sensor[4] -ne 0 -or $sensor[5] -ne 1){throw 'This bounded investigation requires upward sensors.'}}
$inputHashes=[ordered]@{}
foreach($alternative in @('A','B')){
 $source=if($alternative -eq 'A'){$OriginalA}else{$OriginalB}
 Copy-Item -LiteralPath (Join-Path $source 'annual-scene.rad') -Destination (Join-Path $OutputDirectory "$alternative.rad")
 $inputHashes["$alternative.rad"]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$alternative.rad")).Hash.ToLowerInvariant()
}
$black=[regex]::Replace([IO.File]::ReadAllText((Join-Path $OutputDirectory 'A.rad')),'(?m)^5 [^\r\n]+$','5 0 0 0 0 0')
Write-Input 'black.rad' $black
Write-Input 'receiver.rad' "void glow sky_glow`n0`n0`n4 100 100 100 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"
$selectedSensorText=(($indices|ForEach-Object{$sensorLines[$_]}) -join "`n")+"`n"
Write-Input 'sensors.pts' $selectedSensorText
$producer=[GreenLensPairedStream.HammersleyProducer]::new($selectedSensorText,$count)
$inputHashes['sensors.pts']=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'sensors.pts')).Hash.ToLowerInvariant()
foreach($name in @('black.rad','receiver.rad')){$inputHashes[$name]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $name)).Hash.ToLowerInvariant()}
$inventory=Get-Content -LiteralPath "$PSScriptRoot/../dependencies/radiance.json" -Raw|ConvertFrom-Json
$engineHashes=[ordered]@{}
foreach($name in @('oconv','rcontrib')){
 $hash=(Get-FileHash -LiteralPath (Join-Path $EngineRoot "bin/$name.exe")).Hash.ToLowerInvariant()
 $expected=if($name -eq 'rcontrib'){$inventory.annual_workflow_review.executables.$name.sha256}else{$inventory.executables.$name.sha256}
 if($hash -ne $expected){throw 'Pinned executable differs.'};$engineHashes["$name.exe"]=$hash
}
foreach($name in @('rayinit.cal','skybright.cal','reinhart.cal')){
 $hash=(Get-FileHash -LiteralPath (Join-Path $EngineRoot "lib/$name")).Hash.ToLowerInvariant()
 $expected=if($name -eq 'reinhart.cal'){$inventory.annual_workflow_review.additional_critical_libraries.$name.sha256}else{$inventory.critical_libraries.$name.sha256}
 if($hash -ne $expected){throw 'Pinned library differs.'};$engineHashes[$name]=$hash
}
$base=@('-n','1','-faa','-y','2','-ad',"$SecondarySamples",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-V+','-e','MF:2','-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow')
$declaration=[ordered]@{schema_version='greenlens.hammersley-coupling-investigation.v1';declared_utc=[DateTime]::UtcNow.ToString('o');scope='Coupling and per-bin positivity only; secondary sampling is not an accuracy profile';variant=$Variant;primary_count=$count;secondary_ambient_divisions=$SecondarySamples;full_ambient_bounces=$FullBounces;direct_ambient_bounces=0;depth_accounting='Radiance radiance-response primaries replace one I+ hemispherical bounce; full ab7/8 corresponds in maximum diffuse depth to I+ ab8/9';receiver_scale=100;sensor_indices=$indices;sensor_ids=@('s000003','s000023');input_sha256=$inputHashes;primary_rule='Continuous Hammersley sensor-major stream: r=sqrt((i+0.5)/N), phi=2*pi*base2_radical_inverse(i), z=sqrt(1-r*r); i=0..N-1; no scramble/rotation; origin=sensor+1e-4 up; identical full/direct bytes';conversion='pi*mean radiance/100 per channel';base_arguments=$base;checks=@('Every full-minus-direct coefficient is finite and nonnegative without clamping','Black opaque room retains glass; every full-minus-direct coefficient is exactly zero','Unobstructed uniform full and direct irradiance agree with pi*L within1%','Fixed A and B two-sensor RGB coefficient shapes and direct pairing');harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();process_helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-process.ps1").Hash.ToLowerInvariant();native_process_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-native-process.cs").Hash.ToLowerInvariant();primary_helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-primary-stream.cs").Hash.ToLowerInvariant();release_acceptance_complete=$false}
$declaration.engine_sha256=$engineHashes
$declaration|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'oconv' @('-f','receiver.rad') 'uniform.oct'
foreach($name in @('black','A','B')){Invoke-Engine 'oconv' @('-f',"$name.rad",'receiver.rad') "$name.oct"}
$results=[Collections.Generic.List[object]]::new()
 foreach($name in @('uniform','black','A','B')){
  $receipts=@{}
  foreach($mode in @('full','direct')){
   $pairedBounceCount=if($mode -eq 'full'){$FullBounces}else{0}
   $receipts[$mode]=Invoke-PairedProcess -Executable (Join-Path $EngineRoot 'bin/rcontrib.exe') -Name 'rcontrib' -Arguments ($base+@('-ab',"$pairedBounceCount",'-c',"$count","$name.oct")) -Output "$name-$count-$mode.mtx" -InputProducer $producer
  }
  if($receipts.full.Rays -ne 2L*$count -or $receipts.full.Sha256 -cne $receipts.direct.Sha256 -or $receipts.full.Bytes -ne $receipts.direct.Bytes -or $receipts.full.Rays -ne $receipts.direct.Rays){throw 'Full/direct streams differ'}
  $full=Read-Matrix "$name-$count-full.mtx";$direct=Read-Matrix "$name-$count-direct.mtx"
  if($full.rows -ne 2 -or $full.columns -ne 578 -or $direct.rows -ne 2 -or $direct.columns -ne 578){throw 'Wrong paired coefficient shape.'}
  $negative=[Collections.Generic.List[object]]::new();$nonzero=0;$minimum=0.;$totals=@()
  for($sensor=0;$sensor -lt 2;$sensor++){
   $sumFull=@(0.,0.,0.);$sumDirect=@(0.,0.,0.)
   for($patch=0;$patch -lt 578;$patch++){for($channel=0;$channel -lt 3;$channel++){
    $offset=($sensor*578+$patch)*3+$channel;$a=$full.values[$offset]*[Math]::PI/100;$b=$direct.values[$offset]*[Math]::PI/100;$residual=$a-$b
    $sumFull[$channel]+=$a;$sumDirect[$channel]+=$b
    if($residual -ne 0){$nonzero++};if($residual -lt 0){$minimum=[Math]::Min($minimum,$residual);$negative.Add(@{sensor_index=$indices[$sensor];patch_index=$patch;channel=$channel;full=$a;direct=$b;residual=$residual})}
   }}
   $totals+=@{sensor_index=$indices[$sensor];full_rgb=$sumFull;direct_rgb=$sumDirect}
  }
  $uniformPass=$true;if($name -eq 'uniform'){foreach($row in $totals){foreach($value in ($row.full_rgb+$row.direct_rgb)){if([Math]::Abs($value-[Math]::PI)/[Math]::PI -gt .01){$uniformPass=$false}}}}
  $passed=$negative.Count -eq 0 -and ($name -ne 'black' -or $nonzero -eq 0) -and $uniformPass
  $results.Add([ordered]@{fixture=$name;variant=$Variant;primary_count_per_sensor=$count;negative_channel_bins=$negative.Count;minimum_residual_rgb=$minimum;nonzero_residual_channel_bins=$nonzero;uniform_within_one_percent=$uniformPass;totals=$totals;negative_bins=$negative.ToArray();passed=$passed;full_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name-$count-full.mtx")).Hash.ToLowerInvariant();direct_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name-$count-direct.mtx")).Hash.ToLowerInvariant();primary_streams=$receipts})
  [ordered]@{declaration=$declaration;results=$results.ToArray();completed_cases=$results.Count;expected_cases=4;passed=($results.Count -eq 4 -and @($results|Where-Object{!$_.passed}).Count -eq 0);release_acceptance_complete=$false}|ConvertTo-Json -Depth 14|Set-Content -LiteralPath (Join-Path $OutputDirectory 'paired-primary.json') -Encoding utf8
  Write-Host "$name count$count negative bins$($negative.Count), passed$passed"
 }
if(@($results|Where-Object{!$_.passed}).Count){throw 'Paired-primary investigation failed; all completed cases retained.'}
