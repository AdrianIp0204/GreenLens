#Requires -Version 7.2
# Research only: double contribution bins and exactly normalized source scale.
param([Parameter(Mandatory)][string]$SourceDirectory,[Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
$hashes=[ordered]@{}
foreach($file in @('scene.rad','one-sensor.pts','hour-8512.dat','hour-8512.rad','matched-basis.cal')){Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file);$hashes[$file]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $file)).Hash.ToLowerInvariant()}
$sensor=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'one-sensor.pts'));$originalSky=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'hour-8512.rad'))
$pattern='(?s)(void light (solar\d+)\s+0\s+0\s+3)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)';$match=[regex]::Match($originalSky,$pattern)
if(!$match.Success -or [regex]::Matches($originalSky,'4 1 1 1 0').Count -ne 1){throw 'Expected the fixed one-sun, neutral-glow fixture.'};$solar=$match.Groups[2].Value
$base=@('-I+','-h','-faa','-n','1','-y','1','-ab','9','-ad','8192','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$cases=@(@(8192,64,100),@(262144,4,100),@(1048576,1,100),@(1048576,1,10000),@(4194304,1,100))
[ordered]@{scope='One-point scaled-sky contribution reference; earlier failed full-grid and high-ad rtrace references remain immutable';declared_utc=[DateTime]::UtcNow.ToString('o');sensor_index=22;sensor_id='s000023';interval_index=8512;cases=$cases;arguments=$base;input_sha256=$hashes;source_scaling='Multiply the existing sky glow RGB and explicit solar light RGB by the declared scalar. Preserve geometry, sky distribution data, solar direction, sensor and all numerical settings; divide both RGB contribution bins by that exact scalar in f64.';last_successive_relative_change_limit=.01;same_sampling_scale_relative_difference_limit=1e-5;harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();release_acceptance_complete=$false}|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
$rows=[Collections.Generic.List[object]]::new()
foreach($case in $cases){
 $ad=[int]$case[0];$repeat=[int]$case[1];$scale=[double]$case[2];$name="ad$ad-c$repeat-scale$scale";$scaled=$originalSky.Replace('4 1 1 1 0',"4 $scale $scale $scale 0")
 $scaled=[regex]::Replace($scaled,$pattern,{param($m) $rgb=@(3..5|ForEach-Object{([double]::Parse($m.Groups[$_].Value,$culture)*$scale).ToString('R',$culture)});$m.Groups[1].Value+"`n"+($rgb -join ' ')})
 Write-Input "$name.rad" $scaled;Invoke-Engine 'oconv' @('-f','scene.rad',"$name.rad") "$name.oct"
 $options=@($base);$options[[Array]::IndexOf($options,'-ad')+1]="$ad"
 Invoke-Engine 'rcontrib' ($options+@('-c',"$repeat",'-V+','-m','sky_glow','-m',$solar,"$name.oct")) "$name.rgb" ($sensor*$repeat)
 $values=@([IO.File]::ReadAllText((Join-Path $OutputDirectory "$name.rgb")).Trim() -split '\s+'|ForEach-Object{[double]::Parse($_,$culture)/$scale});if($values.Count -ne 6 -or @($values|Where-Object{![double]::IsFinite($_) -or $_ -lt 0}).Count){throw 'Invalid native output.'}
 [double[]]$rgb=@(($values[0]+$values[3]),($values[1]+$values[4]),($values[2]+$values[5]));$lux=179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])
 $rows.Add([ordered]@{ad=$ad;repeat=$repeat;scale=$scale;normalized_bins=$values;normalized_rgb=$rgb;lux=$lux;scaled_sky_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name.rad")).Hash.ToLowerInvariant();raw_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name.rgb")).Hash.ToLowerInvariant()})
 $rows|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
 Write-Host "$name $lux lx"
}
$change=[Math]::Abs($rows[4].lux-$rows[2].lux)/$rows[4].lux;$linearity=[Math]::Abs($rows[2].lux-$rows[3].lux)/$rows[3].lux
[ordered]@{last_successive_relative_change=$change;reference_convergence_pass=($change -le .01);same_sampling_scale_relative_difference=$linearity;scale_linearity_pass=($linearity -le 1e-5);release_acceptance_complete=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8
