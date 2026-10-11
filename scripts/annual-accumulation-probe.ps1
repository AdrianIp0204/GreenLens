#Requires -Version 7.2
# Research only: diagnose sampling/accumulation limits of the pinned engine.
param(
 [Parameter(Mandatory)][string]$OutputDirectory,
 [ValidateSet('uniform','point')][string]$Fixture='uniform',
 [string]$PointSourceDirectory=''
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
function Hash([string]$Name){(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $Name) -Algorithm SHA256).Hash.ToLowerInvariant()}
function Numbers([string]$Name,[int]$Expected){
 $values=@([IO.File]::ReadAllText((Join-Path $OutputDirectory $Name)).Trim() -split '\s+'|ForEach-Object{[double]::Parse($_,$culture)})
 if($values.Count -ne $Expected -or @($values|Where-Object{![double]::IsFinite($_) -or $_ -lt 0}).Count){throw "Invalid native output $Name"}
 return ,$values
}
$base=@('-I+','-h','-faa','-n','1','-ab','1','-ad','8192','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$rows=[Collections.Generic.List[object]]::new()
if($Fixture -eq 'uniform'){
 $cases=@(@(8192,1,1),@(8192,128,1),@(8192,512,1),@(8192,128,.001),@(8192,128,100),@(4194304,1,100),@(8388608,1,100))
 $declaration=[ordered]@{scope='Independent uniform hemisphere sampling and linearity diagnostic; no building acceptance';declared_utc=[DateTime]::UtcNow.ToString('o');fixture=$Fixture;cases=$cases;expected_normalized_rgb=@([Math]::PI,[Math]::PI,[Math]::PI);relative_tolerance=.01;arguments=$base;harness_sha256=(Hash 'harness-source.ps1');helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-feasibility.ps1").Hash.ToLowerInvariant();release_acceptance_complete=$false}
 $declaration|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
 foreach($case in $cases){
  $ad=[int]$case[0];$repeat=[int]$case[1];$scale=[double]$case[2];$name="ad$ad-c$repeat-scale$($scale.ToString($culture))"
  Write-Input "$name.rad" "void glow unit`n0`n0`n4 $scale $scale $scale 0`nunit source sky`n0`n0`n4 0 0 1 180`n"
  Invoke-Engine 'oconv' @('-f',"$name.rad") "$name.oct"
  $options=@($base);$options[[Array]::IndexOf($options,'-ad')+1]="$ad"
  Invoke-Engine 'rcontrib' ($options+@('-y','1','-c',"$repeat",'-V+','-m','unit',"$name.oct")) "$name.rgb" ("0 0 0 0 0 1`n"*$repeat)
  $rgb=Numbers "$name.rgb" 3;[double[]]$normalized=@($rgb|ForEach-Object{$_/$scale});$error=[Math]::Abs($normalized[0]-[Math]::PI)/[Math]::PI
  $rows.Add([ordered]@{ad=$ad;repeat=$repeat;source_scale=$scale;rgb=$rgb;normalized_rgb=$normalized;relative_error=$error;pass=($error -le .01);raw_sha256=(Hash "$name.rgb")})
  $rows|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
  Write-Host "Uniform ${name}: relative error $error"
 }
}else{
 if(!$PointSourceDirectory){throw 'PointSourceDirectory is required.'}
 $files=@('scene.rad','matched-basis.cal','hour-8512.dat','hour-8512.rad','hour-8512-sun.rad','one-sensor.pts')
 $hashes=[ordered]@{};foreach($file in $files){Copy-Item -LiteralPath (Join-Path $PointSourceDirectory $file) -Destination (Join-Path $OutputDirectory $file);$hashes[$file]=Hash $file}
 $sensor=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'one-sensor.pts'));$sky=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'hour-8512.rad'));$match=[regex]::Match($sky,'void light (solar\d+)');if(!$match.Success){throw 'Missing one explicit sun.'};$solar=$match.Groups[1].Value
 $base[[Array]::IndexOf($base,'-ab')+1]='9'
 $repeats=@(16,64,256)
 $declaration=[ordered]@{scope='Repeated moderate-sampling reference diagnostic, retaining prior full-grid and high-ad failures';declared_utc=[DateTime]::UtcNow.ToString('o');fixture=$Fixture;sensor_index=22;sensor_id='s000023';interval_index=8512;input_sha256=$hashes;ad=8192;repeated_primary_counts=$repeats;arguments=$base;modifiers=@('sky_glow',$solar);last_successive_relative_change_limit=.01;per_record_sample_sequence='The pinned engine increments samplendx for each identical primary; sensor coordinates remain unchanged.';harness_sha256=(Hash 'harness-source.ps1');release_acceptance_complete=$false}
 $declaration|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
 Invoke-Engine 'oconv' @('-f','scene.rad','hour-8512.rad') 'point.oct'
 foreach($repeat in $repeats){
  $name="contribution-c$repeat";Invoke-Engine 'rcontrib' ($base+@('-y','1','-c',"$repeat",'-V+','-m','sky_glow','-m',$solar,'point.oct')) "$name.rgb" ($sensor*$repeat)
  $bins=Numbers "$name.rgb" 6;[double[]]$rgb=@(($bins[0]+$bins[3]),($bins[1]+$bins[4]),($bins[2]+$bins[5]));$lux=179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])
  $rows.Add([ordered]@{engine='rcontrib';ad=8192;repeat=$repeat;bins=$bins;rgb=$rgb;lux=$lux;raw_sha256=(Hash "$name.rgb")})
  $rows|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
  Write-Host "Point $name $lux lx"
 }
 $repeat=256;$name='rtrace-records';Invoke-Engine 'rtrace' ($base+@('-ov','point.oct')) "$name.rgb" ($sensor*$repeat)
 $values=Numbers "$name.rgb" ($repeat*3);[double[]]$sum=@(0,0,0);$unique=[Collections.Generic.HashSet[string]]::new()
 for($index=0;$index -lt $repeat;$index++){for($channel=0;$channel -lt 3;$channel++){$sum[$channel]+=$values[$index*3+$channel]};$null=$unique.Add(($values[($index*3)..($index*3+2)] -join ','))}
 [double[]]$rgb=@($sum|ForEach-Object{$_/$repeat});$lux=179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])
 $rows.Add([ordered]@{engine='rtrace';ad=8192;repeat=$repeat;unique_rgb_records=$unique.Count;rgb=$rgb;lux=$lux;raw_sha256=(Hash "$name.rgb")})
 $rows|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
 Write-Host "Point rtrace f64 average $lux lx, $($unique.Count) unique records"
}
