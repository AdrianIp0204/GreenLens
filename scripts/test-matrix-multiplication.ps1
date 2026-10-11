#Requires -Version 7.5
# Hand-calculated RGB matrices plus deliberately invalid arithmetic fixtures.
# Executes pinned native dctimestep; missing engine is a failure, never a skip.
[CmdletBinding()]
param(
    [string]$OutputDirectory,
    [string]$EngineRoot = (Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package')
)
$ErrorActionPreference = 'Stop'
if (!$OutputDirectory) { $OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/matrix arithmetic 空間 '+[guid]::NewGuid().ToString('N')) }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Matrix test destination must be new.' }
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -EngineRoot $EngineRoot -FunctionsOnly
$inventory = Get-Content -LiteralPath "$PSScriptRoot/../dependencies/radiance.json" -Raw | ConvertFrom-Json
$binary = Join-Path $EngineRoot 'bin/dctimestep.exe'
if (!(Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Required pinned native dctimestep is unavailable.' }
$digest = (Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant()
if ($digest -cne $inventory.annual_workflow_review.executables.dctimestep.sha256) { throw 'Native dctimestep digest differs from the reviewed dependency.' }
$coefficients=[pscustomobject]@{rows=2;columns=3;components=3;values=[double[]]@(1,2,3,4,5,6,7,8,9,2,0,1,0,3,2,4,1,0)}
$sky=[pscustomobject]@{rows=3;columns=2;components=3;values=[double[]]@(1,2,3,4,5,6,2,3,4,5,6,7,3,4,5,6,7,8)}
# Manually expanded sums, independently recorded before engine invocation:
# e.g. first RGB=(1*1+4*2+7*3, 2*2+5*3+8*4, 3*3+6*4+9*5).
$expected=[pscustomobject]@{rows=2;columns=2;components=3;values=[double[]]@(30,51,78,66,96,132,14,13,11,32,25,20)}
function Write-FixtureMatrix([string]$Name,$Matrix) {
    $values = ($Matrix.values | ForEach-Object { $_.ToString('R',[Globalization.CultureInfo]::InvariantCulture) }) -join ' '
    Write-Input $Name "#?RADIANCE`nNROWS=$($Matrix.rows)`nNCOLS=$($Matrix.columns)`nNCOMP=3`nFORMAT=ascii`n`n$values`n"
}
Write-FixtureMatrix 'coefficients.mtx' $coefficients
Write-FixtureMatrix 'sky.mtx' $sky
Write-FixtureMatrix 'hand-expected.mtx' $expected
$checks=[Collections.Generic.List[string]]::new()
$null=Assert-IndependentRgbProduct $coefficients $sky $expected
$checks.Add('Hand-expanded non-square RGB product matches every component')
Invoke-Engine 'dctimestep' @('-oa','coefficients.mtx','sky.mtx') 'native-product.mtx'
$native=Read-Matrix 'native-product.mtx'
$nativeResult=Assert-IndependentRgbProduct $coefficients $sky $native -EvidencePath (Join-Path $OutputDirectory 'native-product-check.json')
if (($native.values -join ',') -cne ($expected.values -join ',')) { throw 'Native integer fixture differs from independently written exact sums.' }
$checks.Add('Pinned native dctimestep matches all twelve hand-calculated RGB values exactly')

function Clone-Fixture($Value) { return ($Value | ConvertTo-Json -Depth 8 | ConvertFrom-Json -DateKind String) }
function Require-Rejection([string]$Name,[scriptblock]$Action) {
    $failed=$false
    try { & $Action | Out-Null } catch { $failed=$true }
    if (!$failed) { throw "Deliberately invalid arithmetic fixture was accepted: $Name" }
    $checks.Add('Rejects '+$Name)
}
$rounded=Clone-Fixture $expected
$rounded.values[0]+=0.0001
$null=Assert-IndependentRgbProduct $coefficients $sky $rounded
$checks.Add('Documented ASCII-rounding tolerance permits a bounded last-digit change')
$bad=Clone-Fixture $expected; $bad.values[0]+=0.001
$failurePath=Join-Path $OutputDirectory 'expected-threshold-failure.json'
Require-Rejection 'above-tolerance product' { Assert-IndependentRgbProduct $coefficients $sky $bad -EvidencePath $failurePath }
$failure=Get-Content -LiteralPath $failurePath -Raw | ConvertFrom-Json
if ($failure.passed -or $failure.failures.Count -ne 1 -or $failure.maximum_error_over_tolerance -le 1) { throw 'Arithmetic failure evidence was not retained accurately.' }
$checks.Add('Failure evidence retains the offending sensor, time, RGB channel and fixed tolerance')
$bad=Clone-Fixture $expected; $bad.values[0]=51; $bad.values[1]=30
Require-Rejection 'RGB channel permutation' { Assert-IndependentRgbProduct $coefficients $sky $bad }
$bad=Clone-Fixture $expected; $bad.values=@($expected.values[3..5]) + @($expected.values[0..2]) + @($expected.values[6..11])
Require-Rejection 'time-column permutation' { Assert-IndependentRgbProduct $coefficients $sky $bad }
$bad=Clone-Fixture $expected; $bad.columns=1
Require-Rejection 'wrong exact shape' { Assert-IndependentRgbProduct $coefficients $sky $bad }
$bad=Clone-Fixture $expected; $bad.values=$bad.values[0..10]
Require-Rejection 'truncated RGB payload' { Assert-IndependentRgbProduct $coefficients $sky $bad }
foreach ($invalid in @([double]::NaN,[double]::PositiveInfinity,-1.0)) {
    $bad=Clone-Fixture $expected; $bad.values[0]=$invalid
    Require-Rejection "invalid output $invalid" { Assert-IndependentRgbProduct $coefficients $sky $bad }
}
$badSky=Clone-Fixture $sky; $badSky.values[0]=[double]::NaN
Require-Rejection 'nonfinite multiplicand' { Assert-IndependentRgbProduct $coefficients $badSky $expected }
$zero=Clone-Fixture $coefficients; $zero.values=[double[]]::new(18)
$zeroExpected=Clone-Fixture $expected; $zeroExpected.values=[double[]]::new(12)
$null=Assert-IndependentRgbProduct $zero $sky $zeroExpected
$zeroExpected.values[0]=5e-8
$null=Assert-IndependentRgbProduct $zero $sky $zeroExpected
$checks.Add('Zero products use the fixed absolute rounding bound without division by zero')
$zeroExpected.values[0]=2e-7
Require-Rejection 'above-tolerance nonzero value for zero product' { Assert-IndependentRgbProduct $zero $sky $zeroExpected }
Write-Input 'truncated.mtx' "#?RADIANCE`nNROWS=3`nNCOLS=2`nNCOMP=3`nFORMAT=ascii`n`n1 2 3`n"
Invoke-Engine 'dctimestep' @('-oa','coefficients.mtx','truncated.mtx') 'expected-engine-failure.mtx' '' $true
$checks.Add('Pinned native dctimestep rejects a truncated matrix with a nonzero exit')
@{schema_version='greenlens.matrix-arithmetic-verification.v1';passed=$true;checks=$checks;native_component_count=$nativeResult.component_checks;native_maximum_absolute_rgb_error=$nativeResult.maximum_absolute_rgb_error;dctimestep_sha256=$digest;scope='Native arithmetic integration and explicit synthetic rejection fixtures only; no annual transport/profile acceptance.'} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8NoBOM
Write-Host "Independent matrix verification passed ($($checks.Count) checks): $OutputDirectory"
