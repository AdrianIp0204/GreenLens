#Requires -Version 7.5
# Native sky checks plus focused fault-injection and mathematical regressions.
[CmdletBinding()]
param(
    [string]$OutputDirectory,
    [string]$EngineRoot = (Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package')
)
$ErrorActionPreference='Stop'
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/sky normalization regressions 空間 '+[guid]::NewGuid().ToString('N'))}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Regression destination must be new.'}
$verificationOutput=[IO.Path]::GetFullPath($OutputDirectory)
$requestedEngine=$EngineRoot
New-Item -ItemType Directory -Path $verificationOutput|Out-Null
. "$PSScriptRoot/annual-sky-normalization.ps1" -FunctionsOnly
$checks=[Collections.Generic.List[string]]::new()
function Require-SkyRejection([string]$Name,[scriptblock]$Action){
    $rejected=$false
    try{& $Action|Out-Null}catch{$rejected=$true}
    if(!$rejected){throw "Expected rejection was absent: $Name"}
    $checks.Add($Name)
}
foreach($mf in @(2,4)){
    $weights=Get-SkyProjectedWeights $mf
    $expectedCount=if($mf -eq 2){578}else{2306}
    if($weights.Count -ne $expectedCount -or $weights[0] -ne 0 -or [Math]::Abs(($weights|Measure-Object -Sum).Sum-[Math]::PI) -gt 1e-12){throw 'Hemisphere partition failed'}
    $rgb=[double[]]::new(3*$expectedCount)
    for($p=0;$p -lt $expectedCount;$p++){$rgb[3*$p]=0.25;$rgb[3*$p+1]=1;$rgb[3*$p+2]=2}
    Assert-SkyChecks @((New-SkyRgbCheck 'Independent uniform patch sum' @((0.25*[Math]::PI),[Math]::PI,(2*[Math]::PI)) (Get-SkyProjectedRgb $mf $rgb)))
    $checks.Add("MF$mf patch partition integrates pi and unequal-channel uniform field")
    $ground=[double[]]::new(3*$expectedCount);$ground[0]=0.25;$ground[1]=1;$ground[2]=2
    if(@((Get-SkyProjectedRgb $mf $ground)|Where-Object{$_ -ne 0}).Count){throw 'Ground leaked into upward integral'}
    Assert-SkyChecks @((New-SkyRgbCheck 'Independent ground pi L' @((0.25*[Math]::PI),[Math]::PI,(2*[Math]::PI)) (Get-SkyProjectedRgb $mf $ground -Downward)))
    $checks.Add("MF$mf ground contributes once to downward and zero to upward sensor")
}
if([Math]::Abs((Convert-SkyRgbToLux @(0,1,0))-119.950518949) -gt 1e-10){throw 'Pinned RGB photometry coefficient differs'}
$checks.Add('Green-only irradiance uses the pinned weighted RGB convention')
Require-SkyRejection 'Two-percent normalization perturbation fails fixed one-percent limit' {Assert-SkyChecks @((New-SkyRgbCheck 'Perturbed' @(1,2,3) @(1.02,2.04,3.06)))}
Require-SkyRejection 'Lux-preserving RGB perturbation is still rejected per channel' {Assert-SkyChecks @((New-SkyRgbCheck 'Color swap' @(1,2,3) @(1.1,(2-0.1*0.265074126/0.670114631),3)))}
Require-SkyRejection 'Gray shortcut rejects unequal RGB data' {Assert-SkyChecks @((New-SkyRgbCheck 'Gray shortcut' @(1,2,3) @(1,1,1)))}
Require-SkyRejection 'Truncated patch input rejects' {Get-SkyProjectedRgb 2 @(1,2,3)}
Require-SkyRejection 'Nonfinite channel rejects' {New-SkyRgbCheck 'NaN' @(1,2,3) @([double]::NaN,2,3)}
Require-SkyRejection 'Negative channel rejects' {Convert-SkyRgbToLux @(-1,2,3)}
$cardinal=[double[]]@(2,9,14,17,39,59,32,69,104,47,99,149,578,1161,1742,1,7,11)
Assert-SkyChecks (Get-SkyCardinalChecks 2 $cardinal)
$checks.Add('Hand-enumerated MF2 north/east/south/west/zenith/ground observations agree')
$cardinal[0]+=1
Require-SkyRejection 'One-bin north perturbation fails ordering' {Assert-SkyChecks (Get-SkyCardinalChecks 2 $cardinal)}
Require-SkyRejection 'Unsupported MF3 rejects' {Get-SkyProjectedWeights 3}

$nativeDirectory=Join-Path $verificationOutput 'native sky 空間'
& "$PSScriptRoot/annual-sky-normalization.ps1" -OutputDirectory $nativeDirectory -EngineRoot $requestedEngine
$native=Get-Content -LiteralPath (Join-Path $nativeDirectory 'sky-normalization.json') -Raw|ConvertFrom-Json -DateKind String -Depth 50
if(!$native.passed -or @($native.checks).Count -ne 23){throw 'Required native sky observations are incomplete'}
if($native.resources.processes -ne 19 -or $native.resources.wall_seconds -le 0 -or $native.resources.sampled_peak_child_working_set_bytes -le 0 -or $native.resources.scratch_bytes -le 0){throw 'Required native observation resource summary is missing'}
$checks.Add('Pinned native sky suite passes every one of the 23 predeclared observations')
# These are genuinely changed native observations, not a second simulation.
$nativeUniform=$native.checks|Where-Object name -CEQ 'Unequal RGB uniform upper hemisphere E=pi*L'
$perturbed=[double[]]@($nativeUniform.actual_rgb_irradiance|ForEach-Object{$_*1.05})
Require-SkyRejection 'Five-percent perturbation of actual native uniform output fails' {Assert-SkyChecks @((New-SkyRgbCheck 'Native perturbation' $nativeUniform.expected_rgb_irradiance $perturbed))}

$missingDirectory=Join-Path $verificationOutput 'missing-engine'
$missingOutput=Join-Path $verificationOutput 'missing-engine-check'
$info=[Diagnostics.ProcessStartInfo]::new()
$info.FileName=(Get-Command pwsh).Source;$info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
foreach($argument in @('-NoProfile','-File',"$PSScriptRoot/annual-sky-normalization.ps1",'-OutputDirectory',$missingOutput,'-EngineRoot',$missingDirectory)){$info.ArgumentList.Add($argument)}
$process=[Diagnostics.Process]::new();$process.StartInfo=$info
try{
    if(!$process.Start()){throw 'Cannot start isolated missing-engine check'}
    $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
    if(!$process.WaitForExit(30000)){$process.Kill($true);$process.WaitForExit();throw 'Missing-engine check timed out'}
    $text=$stderr.GetAwaiter().GetResult();[IO.File]::WriteAllText((Join-Path $verificationOutput 'missing-engine.stderr.txt'),$text)
    [IO.File]::WriteAllText((Join-Path $verificationOutput 'missing-engine.stdout.txt'),$stdout.GetAwaiter().GetResult())
    if($process.ExitCode -eq 0 -or $text -notmatch 'Required pinned native gendaymtx is unavailable'){throw 'Missing required engine did not fail clearly'}
    $checks.Add('Missing native engine fails clearly rather than skipping')
}finally{$process.Dispose()}
@{schema_version='greenlens.sky-normalization-regressions.v1';passed=$true;checks=$checks;native_report_relative='native sky 空間/sky-normalization.json';native_report_sha256=(Get-FileHash -LiteralPath (Join-Path $nativeDirectory 'sky-normalization.json')).Hash.ToLowerInvariant();scope='Required native sky-only observations plus deliberately perturbed/malformed checks; no annual transport or installed desktop acceptance.'}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $verificationOutput 'regressions.json') -Encoding utf8NoBOM
Write-Output $verificationOutput
