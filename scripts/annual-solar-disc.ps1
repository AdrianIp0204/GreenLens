#Requires -Version 7.2
# Exploratory deterministic Radiance source-jitter averaging of the original
# finite-disc diagnostic. No production method/profile change is implied.
param(
    [Parameter(Mandatory)][string]$FixtureDirectory,
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 finite disc sampling 空間'),
    [int[]]$SampleCounts = @(64,256,1024),
    [switch]$ExtendedEdges
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
foreach($name in @('vertical.rad','aperture.rad')) { Copy-Item -LiteralPath (Join-Path $FixtureDirectory $name) -Destination (Join-Path $OutputDirectory $name) }
Invoke-Engine 'oconv' @('-f','vertical.rad','aperture.rad') 'aperture.oct'
$sensors=@('0 0 0 0 0 1','0.09 0 0 0 0 1','0.1 0 0 0 0 1','0.11 0 0 0 0 1','0.2 0 0 0 0 1')
$normalE=179*10000*[Math]::PI*[Math]::Pow([Math]::Sin(.533/2*[Math]::PI/180),2)
$expectedFractions=@(1,1,.5,0,0);$checks=[Collections.Generic.List[object]]::new()
if($ExtendedEdges) {
    $radius=[Math]::Tan(.533/2*[Math]::PI/180)
    foreach($fraction in @(-.75,-.5,-.25,.25,.5,.75)) {
        $x=(.1+$radius*$fraction).ToString('R',$culture)
        $sensors+= "$x 0 0 0 0 1"
        $t=-$fraction
        $expectedFractions+=.5+([Math]::Asin($t)+$t*[Math]::Sqrt(1-$t*$t))/[Math]::PI
    }
}
foreach($sampleCount in $SampleCounts) {
    $inputRays=[Text.StringBuilder]::new()
    foreach($sensor in $sensors) { for($repeat=0;$repeat -lt $sampleCount;$repeat++) { $null=$inputRays.Append($sensor).Append("`n") } }
    $options=@('-I+','-faa','-n','1','-y',"$($sensors.Count)",'-c',"$sampleCount",'-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','1','-ds','0','-st','0','-ss','0','-u-','-V+','-m','sun','aperture.oct')
    Invoke-Engine 'rcontrib' $options "disc-$sampleCount.mtx" $inputRays.ToString()
    $matrix=Read-Matrix "disc-$sampleCount.mtx"
    if($matrix.rows -ne $sensors.Count -or $matrix.columns -ne 1) { throw 'Incorrect averaged sensor shape' }
    for($index=0;$index -lt $sensors.Count;$index++) {
        $actual=179*$matrix.values[$index*3];$expected=$normalE*$expectedFractions[$index]
        $checks.Add(@{samples=$sampleCount;sensor_index=$index;expected_lux=$expected;actual_lux=$actual;absolute_error_lux=[Math]::Abs($actual-$expected);passed=([Math]::Abs($actual-$expected) -le [Math]::Max(5.0,.05*$expected))})
    }
}
$checks | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'disc-sampling.json') -Encoding utf8
$checks | ConvertTo-Json -Depth 6 | Write-Host
