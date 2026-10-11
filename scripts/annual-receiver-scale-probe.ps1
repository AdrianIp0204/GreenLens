#Requires -Version 7.2
# Research only: isolate contribution cutoffs from transport sampling.
param([Parameter(Mandatory)][string]$SourceDirectory,[Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
$hashes=[ordered]@{}
foreach($file in @('scene.rad','one-sensor.pts','hour-8512.dat')){Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file);$hashes[$file]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $file)).Hash.ToLowerInvariant()}
$sensor=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'one-sensor.pts'))
$sky=@([IO.File]::ReadAllLines((Join-Path $OutputDirectory 'hour-8512.dat'))|Select-Object -Skip 2|ForEach-Object{[double]::Parse($_,$culture)})
if($sky.Count -ne 578){throw 'Expected the retained neutral MF2 sky data.'}
$cases=@(@('coefficient',1048576,1,1),@('contribution',1048576,1,1),@('contribution',1048576,1,100),@('contribution',8192,128,100),@('contribution',8192,128,10000))
$base=@('-I+','-faa','-n','1','-y','1','-ab','9','-ad','8192','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
[ordered]@{scope='One-point exact receiver-scale and accumulation diagnostic, not release acceptance';declared_utc=[DateTime]::UtcNow.ToString('o');sensor_index=22;sensor_id='s000023';interval_index=8512;input_sha256=$hashes;cases=$cases;arguments=$base;harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();release_acceptance_complete=$false}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
$rows=[Collections.Generic.List[object]]::new()
foreach($case in $cases){
 $mode=[string]$case[0];$ad=[int]$case[1];$repeat=[int]$case[2];$scale=[double]$case[3];$name="$mode-ad$ad-c$repeat-scale$scale"
 Write-Input "$name.rad" "void glow sky_glow`n0`n0`n4 $scale $scale $scale 0`nsky_glow source upper_sky`n0`n0`n4 0 0 1 180`nsky_glow source lower_ground`n0`n0`n4 0 0 -1 180`n"
 Invoke-Engine 'oconv' @('-f','scene.rad',"$name.rad") "$name.oct"
 $options=@($base);$options[[Array]::IndexOf($options,'-ad')+1]="$ad";$v=if($mode -eq 'coefficient'){'-V-'}else{'-V+'}
 Invoke-Engine 'rcontrib' ($options+@('-c',"$repeat",$v,'-e','MF:2','-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow',"$name.oct")) "$name.mtx" ($sensor*$repeat)
 $matrix=Read-Matrix "$name.mtx";if($matrix.rows -ne 1 -or $matrix.columns -ne 578){throw 'Coefficient shape differs from one sensor and MF2.'}
 [double[]]$rgb=@(0,0,0);$nonzero=0
 for($bin=0;$bin -lt 578;$bin++){if($matrix.values[$bin*3] -gt 0){$nonzero++};for($channel=0;$channel -lt 3;$channel++){$rgb[$channel]+=$matrix.values[$bin*3+$channel]*$sky[$bin]/$scale}}
 $lux=179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])
 $rows.Add([ordered]@{mode=$mode;ad=$ad;repeat=$repeat;source_scale=$scale;nonzero_bins=$nonzero;rgb=$rgb;lux=$lux;raw_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$name.mtx")).Hash.ToLowerInvariant()})
 $rows|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
 Write-Host "$name $lux lx ($nonzero nonzero bins)"
}
