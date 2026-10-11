#Requires -Version 7.2
# Exploratory equal-solid-angle midpoint quadrature, traced through the bounded
# diffuse/one-pane scene by Radiance. This does not change production settings.
param(
    [Parameter(Mandatory)][string]$FixtureDirectory,
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 directional disc 空間'),
    [int[]]$SideCounts = @(16,32,64)
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
Copy-Item -LiteralPath (Join-Path $FixtureDirectory 'aperture.rad') -Destination (Join-Path $OutputDirectory 'aperture.rad')
Write-Input 'unit.rad' "void glow unit`n0`n0`n4 1 1 1 0`nunit source sphere`n0`n0`n4 0 0 1 360`n"
Invoke-Engine 'oconv' @('-f','unit.rad','aperture.rad') 'aperture.oct'
$radius=.533/2*[Math]::PI/180;$omega=2*[Math]::PI*(1-[Math]::Cos($radius));$luminance=1000000.0
$normalE=179*$luminance*[Math]::PI*[Math]::Pow([Math]::Sin($radius),2)
$locations=@(0,.09,.1,.11,.2);$fractions=@(1,1,.5,0,0)
foreach($fraction in @(-.75,-.5,-.25,.25,.5,.75)) {
    $locations+=.1+[Math]::Tan($radius)*$fraction
    $t=-$fraction;$fractions+=.5+([Math]::Asin($t)+$t*[Math]::Sqrt(1-$t*$t))/[Math]::PI
}
$base=@('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0','-st','0','-ss','0','-u-')
$checks=[Collections.Generic.List[object]]::new()
function Directions([int]$Side,[double]$Angle) {
    # Center=(sinAngle,0,cosAngle), u=(0,1,0), v=(-cosAngle,0,sinAngle).
    # Uniform cos(theta) gives equal solid angle; no square-to-disc mapping.
    $sinAngle=[Math]::Sin($Angle);$cosAngle=[Math]::Cos($Angle)
    $directions=[Collections.Generic.List[object]]::new()
    for($ring=0;$ring -lt $Side;$ring++) {
        $mu=1-(1-[Math]::Cos($radius))*($ring+.5)/$Side
        $radial=[Math]::Sqrt(1-$mu*$mu)
        for($az=0;$az -lt $Side;$az++) {
            $phi=2*[Math]::PI*($az+.5)/$Side
            $directions.Add(@(($mu*$sinAngle-$radial*[Math]::Sin($phi)*$cosAngle),($radial*[Math]::Cos($phi)),($mu*$cosAngle+$radial*[Math]::Sin($phi)*$sinAngle)))
        }
    }
    return ,$directions
}
foreach($side in $SideCounts) {
    $directions=Directions $side 0
    $rays=[Text.StringBuilder]::new()
    foreach($x in $locations) {
        foreach($direction in $directions) { $null=$rays.Append(($x,0,0,$direction[0],$direction[1],$direction[2] | ForEach-Object { ([double]$_).ToString('R',$culture) }) -join ' ').Append("`n") }
    }
    Invoke-Engine 'rtrace' ($base+@('aperture.oct')) "aperture-$side.rgb" $rays.ToString()
    $tokens=([IO.File]::ReadAllText((Join-Path $OutputDirectory "aperture-$side.rgb"))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries)
    for($sensor=0;$sensor -lt $locations.Count;$sensor++) {
        $actual=0.0
        for($ray=0;$ray -lt $directions.Count;$ray++) { $actual += [double]::Parse($tokens[($sensor*$directions.Count+$ray)*3],$culture)*$directions[$ray][2] }
        $actual*=179*$luminance*$omega/$directions.Count
        $expected=$normalE*$fractions[$sensor]
        $checks.Add(@{fixture='Equal-solid-angle finite-disc aperture';samples=$side*$side;sensor_index=$sensor;expected_lux=$expected;actual_lux=$actual;absolute_error_lux=[Math]::Abs($actual-$expected);passed=([Math]::Abs($actual-$expected) -le [Math]::Max(5.0,.05*$expected))})
    }
}
# A single neutral glass pane plus an opaque back plane eliminates reflected
# hemisphere leakage. The same angular glass law is evaluated by the engine.
$transmittance=.6
$tn=([Math]::Sqrt(.8402528435+.0072522239*$transmittance*$transmittance)-.9166530661)/(.0036261119*$transmittance)
$glass="void glass pane`n0`n0`n3 $($tn.ToString('R',$culture)) $($tn.ToString('R',$culture)) $($tn.ToString('R',$culture))`npane polygon glazing`n0`n0`n12 -100 -100 1 100 -100 1 100 100 1 -100 100 1`nvoid plastic black`n0`n0`n5 0 0 0 0 0`nblack polygon back`n0`n0`n12 -100 -100 -1 -100 100 -1 100 100 -1 100 -100 -1`n"
Write-Input 'glass.rad' $glass
Invoke-Engine 'oconv' @('-f','unit.rad','glass.rad') 'glass.oct'
foreach($degrees in @(0,30,60)) {
    $angle=$degrees*[Math]::PI/180;$directions=Directions 32 $angle
    $rays=[Text.StringBuilder]::new()
    foreach($direction in $directions) { $null=$rays.Append((0,0,0,$direction[0],$direction[1],$direction[2] | ForEach-Object { ([double]$_).ToString('R',$culture) }) -join ' ').Append("`n") }
    Invoke-Engine 'rtrace' ($base+@('glass.oct')) "glass-$degrees.rgb" $rays.ToString()
    $tokens=([IO.File]::ReadAllText((Join-Path $OutputDirectory "glass-$degrees.rgb"))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries)
    $actual=0.0
    for($ray=0;$ray -lt $directions.Count;$ray++) { $actual += [double]::Parse($tokens[$ray*3],$culture)*$directions[$ray][2] }
    $actual*=179*$luminance*$omega/$directions.Count
    $sin=[Math]::Sin($angle).ToString('R',$culture);$cos=[Math]::Cos($angle).ToString('R',$culture)
    Write-Input "sun-$degrees.rad" "void light sun`n0`n0`n3 $luminance $luminance $luminance`nsun source disc`n0`n0`n4 $sin 0 $cos 0.533`n"
    Invoke-Engine 'oconv' @('-f',"sun-$degrees.rad",'glass.rad') "glass-source-$degrees.oct"
    Invoke-Engine 'rtrace' (@('-I+')+$base+@("glass-source-$degrees.oct")) "glass-source-$degrees.rgb" "0 0 0 0 0 1`n"
    $reference=179*[double]::Parse(([IO.File]::ReadAllText((Join-Path $OutputDirectory "glass-source-$degrees.rgb"))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries)[0],$culture)
    $checks.Add(@{fixture='Angle-dependent glass, finite quadrature vs explicit source';incidence_degrees=$degrees;normal_transmittance=$transmittance;reference_lux=$reference;actual_lux=$actual;relative_error=[Math]::Abs($actual-$reference)/$reference;passed=([Math]::Abs($actual-$reference)/$reference -le .01)})
}
$checks | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'transmission-quadrature.json') -Encoding utf8
$checks | ConvertTo-Json -Depth 6 | Write-Host
