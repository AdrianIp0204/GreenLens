#Requires -Version 7.2
# Isolate path-weight termination from source scaling and accumulation.
param([Parameter(Mandatory)][string]$SourceDirectory,[Parameter(Mandatory)][string]$OutputDirectory,[switch]$HighEndpoint)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
$inputs=@('scene.rad','one-sensor.pts','hour-8512.dat','matched-basis.cal','ad1048576-c1-scale100.rad');$hashes=[ordered]@{}
foreach($file in $inputs){Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file);$hashes[$file]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $file)).Hash.ToLowerInvariant()}
$sensor=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'one-sensor.pts'));$sky=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'ad1048576-c1-scale100.rad'));$match=[regex]::Match($sky,'void light (solar\d+)');if(!$match.Success){throw 'Missing exact explicit source.'};$solar=$match.Groups[1].Value
$base=@('-I+','-h','-faa','-n','1','-y','1','-ab','9','-ad','8192','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$cases=@(@(8192,64,-16,'1e-9'),@(262144,4,-16,'1e-9'),@(1048576,1,-16,'1e-9'),@(8192,64,16,'1e-10'))
if($HighEndpoint){$cases=,@(4194304,1,-16,'1e-9')}
[ordered]@{scope='One-point numerical path-weight termination diagnostic; not production profile acceptance';declared_utc=[DateTime]::UtcNow.ToString('o');sensor_index=22;sensor_id='s000023';interval_index=8512;cases=$cases;arguments=$base;input_sha256=$hashes;source_scale=100;unchanged_ambient_depth=9;reflection_depth_cap=16;negative_lr_meaning='The pinned rayorigin uses Russian roulette below the weight threshold with a hard absolute-lr reflection cap; positive lr deterministically terminates below-weight paths.';cross_sampling_relative_difference_limit=.01;harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();release_acceptance_complete=$false}|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'oconv' @('-f','scene.rad','ad1048576-c1-scale100.rad') 'point.oct'
$rows=[Collections.Generic.List[object]]::new()
foreach($case in $cases){
 $ad=[int]$case[0];$repeat=[int]$case[1];$lr=[int]$case[2];$lw=[string]$case[3];$name="ad$ad-c$repeat-lr$lr-lw$lw";$options=@($base)
 $options[[Array]::IndexOf($options,'-ad')+1]="$ad";$options[[Array]::IndexOf($options,'-lr')+1]="$lr";$options[[Array]::IndexOf($options,'-lw')+1]=$lw
 Invoke-Engine 'rcontrib' ($options+@('-c',"$repeat",'-V+','-m','sky_glow','-m',$solar,'point.oct')) "$name.rgb" ($sensor*$repeat)
 $values=@([IO.File]::ReadAllText((Join-Path $OutputDirectory "$name.rgb")).Trim() -split '\s+'|ForEach-Object{[double]::Parse($_,$culture)/100});if($values.Count -ne 6 -or @($values|Where-Object{![double]::IsFinite($_) -or $_ -lt 0}).Count){throw 'Invalid native output.'}
 [double[]]$rgb=@(($values[0]+$values[3]),($values[1]+$values[4]),($values[2]+$values[5]));$lux=179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])
 $rows.Add([ordered]@{ad=$ad;repeat=$repeat;lr=$lr;lw=$lw;normalized_bins=$values;normalized_rgb=$rgb;lux=$lux;raw_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name.rgb")).Hash.ToLowerInvariant()})
 $rows|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
 Write-Host "$name $lux lx"
}
