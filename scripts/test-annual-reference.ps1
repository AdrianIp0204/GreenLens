#Requires -Version 7.2
# Synthetic helper tests plus a bounded native stream check; not science acceptance.
param([switch]$SkipNative)
$ErrorActionPreference='Stop'
Add-Type -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs"
function Assert([bool]$Condition,[string]$Message){if(!$Condition){throw $Message}}
[double[]]$values=0..47|ForEach-Object{0.}
for($round=0;$round -lt 4;$round++){for($sensor=0;$sensor -lt 2;$sensor++){for($channel=0;$channel -lt 3;$channel++){$values[($round*2+$sensor)*6+$channel]=2*($round+1+$sensor);$values[($round*2+$sensor)*6+3+$channel]=4*($round+1+$sensor)}}}
[double[][]]$zero=@(@(0.,0.,0.),@(0.,0.,0.))
$mean=[GreenLensReference.Aggregate]::Calculate($values,2,0,4,2,$zero,$zero)
Assert ($mean.sensors[0].diffuse_rgb[0] -eq 2.5 -and $mean.sensors[1].diffuse_rgb[0] -eq 3.5) 'Round-major sensor identity or normalization differs.'
Assert ([Math]::Abs($mean.sensors[0].lux-1342.5) -lt 1e-9) 'RGB-to-lux aggregation differs.'
$first=[GreenLensReference.Aggregate]::Calculate($values,2,0,2,2,$zero,$zero)
$second=[GreenLensReference.Aggregate]::Calculate($values,2,2,2,2,$zero,$zero)
$comparison=[GreenLensReference.Aggregate]::CompareWithScale($first,$second,$mean)
Assert ([Math]::Abs($comparison[0].allowed_difference_lux-13.425) -lt 1e-9 -and !$comparison[0].passed) 'Disjoint-block comparison did not use complete final mean.'
$changed=[double[]]$values.Clone();$changed[0]+=.1;$rejected=$false
try{[GreenLensReference.Aggregate]::EqualPrefix($values,$changed)}catch{$rejected=$true}
Assert $rejected 'Changed deterministic prefix accepted.'
[GreenLensReference.Aggregate]::EqualPrefix($values,$values)
$rejected=$false;try{[GreenLensReference.Aggregate]::Calculate($values,2,0,5,2,$zero,$zero)|Out-Null}catch{$rejected=$true};Assert $rejected 'Truncated primary count accepted.'
Write-Host 'Aggregation tests passed (synthetic arrays, not native science evidence).'
if($SkipNative){Write-Host 'Native bounded stream check NOT EXECUTED.';return}
$scratch=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/reference supervisor '+[Guid]::NewGuid().ToString('N')+' 空間')
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $scratch -FunctionsOnly
. "$PSScriptRoot/annual-reference-process.ps1"
Write-Input 'uniform.rad' "void glow sky`n0`n0`n4 1 1 1 0`nsky source dome`n0`n0`n4 0 0 1 180`n"
Invoke-Engine 'oconv' @('-f','uniform.rad') 'uniform.oct'
$primaries=12000
Invoke-Engine 'rcontrib' @('-I+','-h','-faa','-n','1','-y',"$primaries",'-c','1','-ab','1','-ad','16','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','-16','-u-','-V+','-m','sky','uniform.oct') 'uniform.rgb' ("0 0 0 0 0 1`n"*$primaries)
$rgb=[GreenLensReference.Aggregate]::Read((Join-Path $scratch 'uniform.rgb'),($primaries*3))
Assert ($rgb.Count -eq 36000 -and [Math]::Abs($rgb[0]-[Math]::PI) -lt 1e-5) 'Native async stream output invalid.'
Write-Host "Native async stdin/output check passed: $primaries primaries, retained $scratch"
