#Requires -Version 7.2
# Separate finite-sun visibility fixtures. No weather, diffuse reflection,
# glazing or full-building accuracy claim. Original synthetic geometry only.
param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new fixture directory to preserve earlier evidence.'}
. "$PSScriptRoot/annual-matched-evidence.ps1"
$matchedSource=[IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1")
$kernel=[regex]::Match($matchedSource,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
if(!$kernel){throw 'Missing reviewed finite-sun reference kernel'}
# The same directional kernel is evaluated with the two declared production
# counts. Count/side substitutions are explicit, with unchanged cap geometry.
foreach($samples in @(1024,4096)){
    $side=[int][Math]::Sqrt($samples)
    $code=$kernel.Replace('GreenLensMatched',"GreenLensShadow$samples") -replace '\b4096\b',"$samples" -replace '\b64\b',"$side"
    Add-Type -TypeDefinition $code
}
Add-Type -TypeDefinition @'
using System;
namespace GreenLensShadow {
public static class Analytic {
// Independent one-dimensional projected-disc integration. x,y are the
// orthographic coordinates on the solar cap, so dOmega=dxdy/sqrt(1-x*x-y*y).
// Boundary roots come from the exact ray/plane visibility inequality.
public static double Lux(bool context,double sensorOffset,int slices){
 double radius=.533*Math.PI/360,s=Math.Sin(radius),a=Math.Sqrt(.5),sum=0,dy=2*s/slices;
 for(int i=0;i<slices;i++){
  double y=-s+(i+.5)*dy,h=Math.Sqrt(s*s-y*y);
  if(!context){
   double c=-sensorOffset/(1-.0001),edge=c*Math.Sqrt((1-y*y)/(1+c*c));
   sum+=Math.Max(0,Math.Min(h,edge)+h)*dy;
  }else{
   double b=sensorOffset*a,edge=b*(y+Math.Sqrt(1+b*b-(1+2*b*b)*y*y))/(1+b*b);
   double lo=Math.Max(-h,Math.Min(h,edge)),den=Math.Sqrt(1-y*y);
   sum+=a*((h-y*Math.Asin(h/den))-(lo-y*Math.Asin(lo/den)))*dy;
  }
 }
 return 179*1e6*sum;
}
}}
'@
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 600
$fractions=@(-1.25,-.75,-.5,-.25,0.0,.25,.5,.75,1.25)
$declaration=[ordered]@{schema_version='greenlens.finite-blocker-edge-fixture.v1';declared_utc=[DateTime]::UtcNow.ToString('o');fractions=$fractions;samples=@(1024,4096);reference_slices=@(16384,65536);reference_convergence_relative=.001;per_sensor_tolerance='max(5.0 lx, 5% of reference)';source_radiance_rgb=@(1e6,1e6,1e6);source_diameter_degrees=.533;effective_sensor_offset_m=.0001;finite_kernel_sha256=(Get-MatchedKernelHash $matchedSource);scope='Finite direct visibility through an isolated horizontal overhang edge and a detached vertical context edge. No diffuse transport, glazing, weather or general building validation.';policy='No sensor exclusion, no negative clamping, all failed values retained.'}
$declaration|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Copy-Item -LiteralPath "$PSScriptRoot/annual-shadow-corrected.ps1" -Destination (Join-Path $OutputDirectory 'fixture-source.ps1')
Copy-Item -LiteralPath "$PSScriptRoot/annual-matched-corrected.ps1" -Destination (Join-Path $OutputDirectory 'finite-kernel-source.ps1')
$options=@('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$checks=[Collections.Generic.List[object]]::new()
$radius=.533*[Math]::PI/360
foreach($kind in @('overhang','context')){
    $context=$kind -eq 'context'
    $a=[Math]::Sqrt(.5)
    $vertices=if($context){'1 0 -10 1 10 -10 1 10 10 1 0 10'}else{'0 -10 1 10 -10 1 10 10 1 0 10 1'}
    $scene="void plastic black`n0`n0`n5 0 0 0 0 0`nblack polygon $kind`n0`n0`n12 $vertices`nvoid glow unit`n0`n0`n4 1 1 1 0`nunit source dome`n0`n0`n4 0 0 1 360`n"
    Write-Input "$kind.rad" $scene
    $direction=if($context){"$($a.ToString('R',$culture)) 0 $($a.ToString('R',$culture))"}else{'0 0 1'}
    $sun="void light solar30`n0`n0`n3 1000000 1000000 1000000`nsolar30 source sun`n0`n0`n4 $direction 0.533`n"
    Write-Input "$kind-sun.rad" $sun
    $offsets=@($fractions|ForEach-Object{$_*[Math]::Tan($radius)/$(if($context){$a}else{1.0})})
    $sensorText=($offsets|ForEach-Object{if($context){"0 $($_.ToString('R',$culture)) 0 0 0 1"}else{"$($_.ToString('R',$culture)) 0 0 0 0 1"}}) -join "`n"
    $sensorText+="`n";Write-Input "$kind-sensors.pts" $sensorText
    Invoke-Engine 'oconv' @('-f',"$kind.rad") "$kind.oct"
    foreach($samples in @(1024,4096)){
        $type="GreenLensShadow$samples.Direct" -as [type]
        $rays=$type::Rays($sensorText,$sun)
        $output="$kind-$samples.rgb"
        Invoke-Engine 'rtrace' ($options+@("$kind.oct")) $output $rays
        $rgb=$type::Finite($sensorText,$sun,(Join-Path $OutputDirectory $output))
        for($i=0;$i -lt $offsets.Count;$i++){
            $coarse=[GreenLensShadow.Analytic]::Lux($context,$offsets[$i],16384)
            $reference=[GreenLensShadow.Analytic]::Lux($context,$offsets[$i],65536)
            $quadrature=[Math]::Abs($reference-$coarse)/[Math]::Max(1.0,$reference)
            $actual=$type::Lux($rgb[$i]);$error=[Math]::Abs($actual-$reference);$allowed=[Math]::Max(5.0,.05*$reference)
            $checks.Add(@{kind=$kind;samples=$samples;sensor_index=$i;edge_fraction=$fractions[$i];sensor_offset_m=$offsets[$i];actual_lux=$actual;reference_lux=$reference;coarse_reference_lux=$coarse;reference_relative_change=$quadrature;absolute_error_lux=$error;allowed_error_lux=$allowed;pass=($quadrature -le .001 -and $error -le $allowed)})
        }
    }
}
$result=@{declaration=$declaration;checks=$checks;pass=(@($checks|Where-Object{!$_.pass}).Count -eq 0);fixture_source_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'fixture-source.ps1'));finite_source_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'finite-kernel-source.ps1'));processes=$records}
$result|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'shadow-edges.json') -Encoding utf8
Write-Host "Finite blocker edges: $(@($checks|Where-Object{!$_.pass}).Count) failures of $($checks.Count) checks."
if(!$result.pass){throw 'Finite blocker-edge fixture failed; preserve all results and investigate.'}
