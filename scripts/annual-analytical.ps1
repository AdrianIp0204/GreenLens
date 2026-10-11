#Requires -Version 7.2
# Original limited analytical fixtures; no measured-building accuracy claim.
param(
    [Parameter(Mandatory)][string]$MatchedDirectory,
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 analytical annual 空間'),
    [int]$Interval = 1904
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
$checks=[Collections.Generic.List[object]]::new()
function Read-Rgb([string]$Name) {
    return @(([IO.File]::ReadAllText((Join-Path $OutputDirectory $Name))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { [double]::Parse($_,$culture) })
}
$base=@('-h','-faa','-ov','-n','1','-ab','0','-ad','262144','-as','0','-aa','0','-av','0','0','0','-lw','1e-10','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.01','-st','0','-ss','0','-u-')
Copy-Item -LiteralPath (Join-Path $MatchedDirectory 'matched-basis.cal') -Destination (Join-Path $OutputDirectory 'matched-basis.cal')
Copy-Item -LiteralPath (Join-Path $MatchedDirectory "hour-$Interval.dat") -Destination (Join-Path $OutputDirectory "hour-$Interval.dat")
$source=[IO.File]::ReadAllText((Join-Path $MatchedDirectory "hour-$Interval.rad"))
Write-Input 'sky.rad' $source.Substring(0,$source.IndexOf('void light solar'))
Invoke-Engine 'oconv' @('-f','sky.rad') 'sky.oct'
$skyOptions=@($base);$skyOptions[[Array]::IndexOf($skyOptions,'-ab')+1]='1'
Invoke-Engine 'rtrace' (@('-I+')+$skyOptions+@('sky.oct')) 'sky.rgb' "0 0 0 0 0 1`n"
$tokens=([IO.File]::ReadAllText((Join-Path $OutputDirectory "hour-$Interval.dat"))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries)
$mf=2;$alpha=[Math]::PI/2/(7*$mf+.5);$bin=1;$expected=0.0;$solid=0.0
$ringCounts=@(30,30,24,24,18,12,6)
for($ring=0;$ring -lt 7*$mf;$ring++) {
    $n=$mf*$ringCounts[[int][Math]::Floor($ring/$mf)]
    $projection=[Math]::PI/$n*([Math]::Pow([Math]::Sin(($ring+1)*$alpha),2)-[Math]::Pow([Math]::Sin($ring*$alpha),2))
    for($azimuth=0;$azimuth -lt $n;$azimuth++) {
        $expected+=179*[double]::Parse($tokens[4+$bin],$culture)*$projection
        $solid+=$projection;$bin++
    }
}
$projection=[Math]::PI*(1-[Math]::Pow([Math]::Sin(7*$mf*$alpha),2))
$expected+=179*[double]::Parse($tokens[4+$bin],$culture)*$projection;$solid+=$projection
$actual=179*(Read-Rgb 'sky.rgb')[0]
$checks.Add(@{name='Exterior identical piecewise sky';expected_lux=$expected;actual_lux=$actual;relative_error=[Math]::Abs($actual-$expected)/$expected;projected_solid_angle_sum=$solid;passed=([Math]::Abs($actual-$expected)/$expected -le .01)})
Write-Input 'uniform.rad' "void glow unit`n0`n0`n4 1 1 1 0`nunit source sky`n0`n0`n4 0 0 1 180`n"
Invoke-Engine 'oconv' @('-f','uniform.rad') 'uniform.oct'
Invoke-Engine 'rtrace' (@('-I+')+$skyOptions+@('uniform.oct')) 'uniform.rgb' "0 0 0 0 0 1`n"
$actual=179*(Read-Rgb 'uniform.rgb')[0];$expected=179*[Math]::PI
$checks.Add(@{name='Uniform hemisphere pi L';expected_lux=$expected;actual_lux=$actual;relative_error=[Math]::Abs($actual-$expected)/$expected;passed=([Math]::Abs($actual-$expected)/$expected -le .01)})
# Original bin-valued sky exposes sign/order directly rather than relying on a
# nearly symmetric weather sky. Rays are at cardinal patch centers.
$values=0..577 -join "`n"
Write-Input 'bins.dat' "1`n0 577 578`n$values`n"
Write-Input 'bins.rad' "void brightdata values`n4 noop bins.dat matched-basis.cal rbin`n0`n0`nvalues glow sky`n0`n0`n4 1 1 1 0`nsky source sphere`n0`n0`n4 0 0 1 360`n"
Invoke-Engine 'oconv' @('-f','bins.rad') 'bins.oct'
$cos=[Math]::Cos($alpha/2).ToString('R',$culture);$sin=[Math]::Sin($alpha/2).ToString('R',$culture)
Invoke-Engine 'rtrace' ($base+@('bins.oct')) 'bins.rgb' "0 0 0 0 $cos $sin`n0 0 0 $cos 0 $sin`n0 0 0 0 -$cos $sin`n0 0 0 -$cos 0 $sin`n0 0 0 0 0 1`n0 0 0 0 0 -1`n"
$rgb=Read-Rgb 'bins.rgb';$bins=@(1,16,31,46,577,0)
for($i=0;$i -lt $bins.Count;$i++) { $checks.Add(@{name="North-east-south-west-zenith-ground bin$i";expected=$bins[$i];actual=$rgb[3*$i];passed=($rgb[3*$i] -eq $bins[$i])}) }
$sunLum=10000.0;$radius=.533/2*[Math]::PI/180;$normalE=179*$sunLum*[Math]::PI*[Math]::Pow([Math]::Sin($radius),2)
Write-Input 'sun.rad' "void light sun`n0`n0`n3 10000 10000 10000`nsun source disc`n0`n0`n4 0.8660254037844386 0 0.5 0.533`n"
Invoke-Engine 'oconv' @('-f','sun.rad') 'sun.oct'
Invoke-Engine 'rtrace' (@('-I+')+$base+@('sun.oct')) 'sun.rgb' "0 0 0 0 0 1`n0 0 0 1 0 0`n0 0 0 -1 0 0`n"
$rgb=Read-Rgb 'sun.rgb';$factors=@(.5,[Math]::Sqrt(.75),0)
for($i=0;$i -lt 3;$i++) {
    $expected=$normalE*$factors[$i];$actual=179*$rgb[3*$i]
    $checks.Add(@{name="Explicit beam direction $i";expected_lux=$expected;actual_lux=$actual;passed=([Math]::Abs($actual-$expected) -le [Math]::Max(.001,.01*$expected))})
}
Write-Input 'block.rad' "void plastic black`n0`n0`n5 0 0 0 0 0`nblack polygon cover`n0`n0`n12 -10 -10 1 10 -10 1 10 10 1 -10 10 1`n"
Invoke-Engine 'oconv' @('-f','sun.rad','block.rad') 'blocked.oct'
Invoke-Engine 'rtrace' (@('-I+')+$base+@('blocked.oct')) 'blocked.rgb' "0 0 0 0 0 1`n"
$actual=179*(Read-Rgb 'blocked.rgb')[0]
$checks.Add(@{name='Opaque blocked beam';expected_lux=0;actual_lux=$actual;passed=($actual -lt .001)})
# Vertical sun and a black rectangular opening: the central edge bisects the
# circular beam; sensors outside the angular radius must be wholly lit/shaded.
Write-Input 'vertical.rad' "void light sun`n0`n0`n3 10000 10000 10000`nsun source disc`n0`n0`n4 0 0 1 0.533`n"
$aperture="void plastic black`n0`n0`n5 0 0 0 0 0`n"
$rectangles=@(@(-10,-10,-.1,10),@(.1,-10,10,10),@(-.1,-10,.1,-.1),@(-.1,.1,.1,10))
for($i=0;$i -lt $rectangles.Count;$i++) { $r=$rectangles[$i];$aperture+="black polygon wall$i`n0`n0`n12 $($r[0]) $($r[1]) 1 $($r[2]) $($r[1]) 1 $($r[2]) $($r[3]) 1 $($r[0]) $($r[3]) 1`n" }
Write-Input 'aperture.rad' $aperture
Invoke-Engine 'oconv' @('-f','vertical.rad','aperture.rad') 'aperture.oct'
# Enable deterministic source subdivision sufficiently fine to resolve the
# finite solar disc at the separately identified boundary sensor.
$edgeOptions=@($base);$edgeOptions[[Array]::IndexOf($edgeOptions,'-ds')+1]='0.0001'
Invoke-Engine 'rtrace' (@('-I+')+$edgeOptions+@('aperture.oct')) 'aperture.rgb' "0 0 0 0 0 1`n0.09 0 0 0 0 1`n0.1 0 0 0 0 1`n0.11 0 0 0 0 1`n0.2 0 0 0 0 1`n"
$rgb=Read-Rgb 'aperture.rgb';$expectedFractions=@(1,1,.5,0,0)
for($i=0;$i -lt 5;$i++) {
    $actual=179*$rgb[3*$i];$expected=$normalE*$expectedFractions[$i]
    $checks.Add(@{name="Separately declared aperture shadow sensor$i";expected_lux=$expected;actual_lux=$actual;passed=([Math]::Abs($actual-$expected) -le [Math]::Max(5.0,.05*$expected))})
}
$result=@{schema_version='greenlens.annual-analytical.v1';checks=$checks;all_passed=(@($checks | Where-Object passed -eq $false).Count -eq 0);limits='Limited exact patch-integral, cosine-projection, original beam/aperture fixtures; not measured-building validation.'}
$result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'analytical.json') -Encoding utf8
$result | ConvertTo-Json -Depth 8 | Write-Host
if(!$result.all_passed) { throw 'Analytical fixture failed; preserve evidence and investigate' }
