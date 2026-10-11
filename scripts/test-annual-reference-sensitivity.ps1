# Pure synthetic regressions for the exact functions used by the research
# sensitivity harness. No engine or reference qualification is claimed.
$ErrorActionPreference='Stop'
$parseErrors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'annual-reference-sensitivity.ps1'),[ref]$null,[ref]$parseErrors)
if($parseErrors.Count){throw ($parseErrors|Out-String)}
foreach($name in @('New-SensitivitySky','Compare-SensitivityBins')){
 $function=$ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name},$false)
 if(!$function){throw "Missing production helper $name"}
 . ([scriptblock]::Create($function.Extent.Text))
}
$checks=0
function Assert([bool]$Condition,[string]$Message){if(!$Condition){throw $Message};$script:checks++}
function Reject([scriptblock]$Action,[string]$Message){$failed=$false;try{&$Action}catch{$failed=$true};Assert $failed $Message}
$sky="void brightdata distribution`n4 noop hour.dat matched-basis.cal rbin`n0`n0`ndistribution glow sky_glow`n0`n0`n4 100 100 100 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`nvoid light solar510750`n0`n0`n3 2.5 0 7e-3`nsolar510750 source sun`n0`n0`n4 0 -1 1 .533`n"
$scaled=New-SensitivitySky $sky
Assert ($scaled.Contains("3`n250 0 0.7000000000000001") -or $scaled.Contains("3`n250 0 0.7")) 'Each non-gray solar channel must scale independently.'
Assert ($scaled.Contains('4 10000 10000 10000 0')) 'Sky scale missing.'
Assert ($scaled.Contains('4 noop hour.dat matched-basis.cal rbin') -and $scaled.Contains('4 0 -1 1 .533')) 'Scaling changed physical sky or sun geometry.'
Reject {New-SensitivitySky ($sky+$sky)} 'Duplicate sources must reject.'
Reject {New-SensitivitySky ($sky.Replace('2.5 0 7e-3','-1 0 1'))} 'Negative source values must reject.'
$baseline=[pscustomobject]@{sensors=@([pscustomobject]@{diffuse_rgb=@(0.,2.,3.);full_solar_rgb=@(4.,5.,6.)})}
$equal=[pscustomobject]@{sensors=@([pscustomobject]@{diffuse_rgb=@(0.,2.,3.);full_solar_rgb=@(4.,5.,6.)})}
$result=@(Compare-SensitivityBins $equal $baseline)
Assert ($result.Count -eq 6 -and @($result|Where-Object{!$_.passed}).Count -eq 0) 'Both RGB bins must be compared.'
$equal.sensors[0].diffuse_rgb[0]=1e-20
Assert (!(@(Compare-SensitivityBins $equal $baseline)[0].passed)) 'Exact-zero baseline must reject any nonzero output.'
$equal.sensors[0].diffuse_rgb[0]=0
$equal.sensors[0].full_solar_rgb[2]=6.000061
Assert (!(@(Compare-SensitivityBins $equal $baseline)[5].passed)) 'Per-channel scale drift must fail without lux cancellation.'
$equal.sensors[0].full_solar_rgb[2]=[double]::NaN
Reject {Compare-SensitivityBins $equal $baseline} 'Nonfinite normalized bins must reject.'
$equal.sensors[0].full_solar_rgb=@(1.,2.)
Reject {Compare-SensitivityBins $equal $baseline} 'Truncated RGB bins must reject.'
Write-Host "$checks synthetic sensitivity checks passed; no native qualification executed."
