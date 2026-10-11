#Requires -Version 7.2
# Coupling/positivity investigation only. This is not an accepted profile.
param(
 [Parameter(Mandatory)][string]$OriginalA,
 [Parameter(Mandatory)][string]$OriginalB,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [ValidateSet(32,64)][int[]]$Sides=@(32,64),
 [ValidateRange(1,8192)][int]$SecondarySamples=64
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained investigation directory.'}
if(@($Sides|Sort-Object -Unique).Count -ne $Sides.Count){throw 'Primary quadrature sides must be unique.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
. "$PSScriptRoot/annual-reference-process.ps1"
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-process.ps1" -Destination (Join-Path $OutputDirectory 'process-source.ps1')
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
Write-Input 'sensors.pts' (($indices|ForEach-Object{$sensorLines[$_]}) -join "`n")
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
$declaration=[ordered]@{schema_version='greenlens.paired-primary-investigation.v1';declared_utc=[DateTime]::UtcNow.ToString('o');scope='Coupling and per-bin positivity only; secondary sampling is not an accuracy profile';sides=$Sides;secondary_ambient_divisions=$SecondarySamples;full_ambient_bounces=7;direct_ambient_bounces=0;depth_accounting='Radiance radiance-response primaries replace one I+ hemispherical bounce; full ab7 corresponds in maximum diffuse depth to I+ ab8';receiver_scale=100;sensor_indices=$indices;sensor_ids=@('s000003','s000023');input_sha256=$inputHashes;primary_rule='For each sensor, row-major radial/azimuth cells: r=sqrt((radial+0.5)/side), phi=2*pi*(azimuth+0.5)/side, z=sqrt(1-r*r); origin=sensor+1e-4 up; directions fixed identically for full/direct';conversion='pi*mean radiance/100 per channel';base_arguments=$base;checks=@('Every full-minus-direct coefficient is finite and nonnegative without clamping','Black opaque room retains glass; every full-minus-direct coefficient is exactly zero','Unobstructed uniform full and direct irradiance agree with pi*L within1%','Fixed A and B two-sensor RGB coefficient shapes and direct pairing');harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();process_helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-reference-process.ps1").Hash.ToLowerInvariant();release_acceptance_complete=$false}
$declaration.engine_sha256=$engineHashes
$declaration|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'oconv' @('-f','receiver.rad') 'uniform.oct'
foreach($name in @('black','A','B')){Invoke-Engine 'oconv' @('-f',"$name.rad",'receiver.rad') "$name.oct"}
$results=[Collections.Generic.List[object]]::new()
foreach($side in $Sides){
 $count=$side*$side;$rays=[Text.StringBuilder]::new()
 foreach($sensor in $sensors){for($i=0;$i -lt $side;$i++){for($j=0;$j -lt $side;$j++){
  $radius=[Math]::Sqrt(($i+.5)/$side);$phi=2*[Math]::PI*($j+.5)/$side
  $row=@($sensor[0],$sensor[1],($sensor[2]+.0001),($radius*[Math]::Cos($phi)),($radius*[Math]::Sin($phi)),[Math]::Sqrt(1-$radius*$radius))
  $null=$rays.AppendLine((($row|ForEach-Object{$_.ToString('G17',$culture)}) -join ' '))
 }}}
 Write-Input "rays-$side.pts" $rays.ToString()
 foreach($name in @('uniform','black','A','B')){
  foreach($mode in @('full','direct')){
   $pairedBounceCount=if($mode -eq 'full'){7}else{0}
   Invoke-Engine 'rcontrib' ($base+@('-ab',"$pairedBounceCount",'-c',"$count","$name.oct")) "$name-$side-$mode.mtx" $rays.ToString()
  }
  $full=Read-Matrix "$name-$side-full.mtx";$direct=Read-Matrix "$name-$side-direct.mtx"
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
  $results.Add([ordered]@{fixture=$name;side=$side;primary_count_per_sensor=$count;negative_channel_bins=$negative.Count;minimum_residual_rgb=$minimum;nonzero_residual_channel_bins=$nonzero;uniform_within_one_percent=$uniformPass;totals=$totals;negative_bins=$negative.ToArray();passed=$passed;full_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name-$side-full.mtx")).Hash.ToLowerInvariant();direct_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name-$side-direct.mtx")).Hash.ToLowerInvariant();rays_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "rays-$side.pts")).Hash.ToLowerInvariant()})
  [ordered]@{declaration=$declaration;results=$results.ToArray();completed_cases=$results.Count;expected_cases=$Sides.Count*4;passed=($results.Count -eq $Sides.Count*4 -and @($results|Where-Object{!$_.passed}).Count -eq 0);release_acceptance_complete=$false}|ConvertTo-Json -Depth 14|Set-Content -LiteralPath (Join-Path $OutputDirectory 'paired-primary.json') -Encoding utf8
  Write-Host "$name side$side negative bins$($negative.Count), passed$passed"
 }
}
if(@($results|Where-Object{!$_.passed}).Count){throw 'Paired-primary investigation failed; all completed cases retained.'}
