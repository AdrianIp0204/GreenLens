#Requires -Version 7.2
# Recombine already measured transport to distinguish diffuse sampling from
# solar-patch bias. This centroid diagnostic is not final finite-sun acceptance.
param(
 [Parameter(Mandatory)][string]$MatchedDirectory,
 [Parameter(Mandatory)][string]$CoefficientDirectory,
 [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
. "$PSScriptRoot/annual-matched-evidence.ps1"
$old=Get-Content -LiteralPath (Join-Path $MatchedDirectory 'declaration.json') -Raw|ConvertFrom-Json
$coeff=Get-Content -LiteralPath (Join-Path $CoefficientDirectory 'declaration.json') -Raw|ConvertFrom-Json
foreach($name in @('scene.rad','sensors.pts')){if((Get-MatchedHash (Join-Path $MatchedDirectory $name)) -ne (Get-MatchedHash (Join-Path $CoefficientDirectory $name))){throw 'Coefficient geometry/sensors differ from retained explicit solar transport'}}
Copy-Item -LiteralPath (Join-Path $CoefficientDirectory 'coefficients.mtx') -Destination (Join-Path $OutputDirectory 'coefficients.mtx')
foreach($name in @('selected-skies.mtx','solar-rgb.mtx','scene.rad','sensors.pts')){Copy-Item -LiteralPath (Join-Path $MatchedDirectory $name) -Destination (Join-Path $OutputDirectory $name)}
$declaration=@{scope='Retained centroid-sun transport recombination diagnostic, not final corrected-method acceptance';declared_utc=[DateTime]::UtcNow.ToString('o');original_matched=$old;diffuse_coefficients=$coeff;coefficients_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'coefficients.mtx'));sky_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'selected-skies.mtx'));solar_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'solar-rgb.mtx'));tolerance='max(5.0 lx, 5% of reference) for all retained sensors and intervals';direct_policy='Both sides retain the same centroid direct beam. Finite-disc correction must be applied and final high-quality references checked before any profile decision.'}
$declaration|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'dctimestep' @('-oa','coefficients.mtx','selected-skies.mtx') 'diffuse-rgb.mtx'
$diffuse=Read-Matrix 'diffuse-rgb.mtx';$solar=Read-Matrix 'solar-rgb.mtx';$indices=@($old.intervals.index)
if($diffuse.rows -ne $solar.rows -or $diffuse.columns -ne $solar.columns -or $solar.columns -ne $indices.Count){throw 'Retained matrix dimensions mismatch'}
function Lux([double[]]$Rgb){foreach($v in $Rgb){if(![double]::IsFinite($v) -or $v -lt 0){throw 'Invalid diagnostic RGB'}};return 179*(.265074126*$Rgb[0]+.670114631*$Rgb[1]+.064811243*$Rgb[2])}
$checks=[Collections.Generic.List[object]]::new()
for($column=0;$column -lt $indices.Count;$column++){
 $index=$indices[$column];$name="hour-$index.rgb"
 Copy-Item -LiteralPath (Join-Path $MatchedDirectory $name) -Destination (Join-Path $OutputDirectory $name)
 $reference=@(([IO.File]::ReadAllText((Join-Path $OutputDirectory $name))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries)|ForEach-Object{[double]::Parse($_,$culture)})
 if($reference.Count -ne $solar.rows*3){throw 'Retained reference dimensions mismatch'}
 $values=[Collections.Generic.List[object]]::new();$failures=0;$maxRatio=0.0
 for($sensor=0;$sensor -lt $solar.rows;$sensor++){
  $offset=($sensor*$indices.Count+$column)*3;$rgb=[double[]]::new(3)
  for($k=0;$k -lt 3;$k++){$rgb[$k]=$diffuse.values[$offset+$k]+$solar.values[$offset+$k]}
  $candidate=Lux $rgb;$expected=Lux $reference[($sensor*3)..($sensor*3+2)];$error=[Math]::Abs($candidate-$expected);$allowed=[Math]::Max(5.0,.05*$expected);$pass=$error -le $allowed
  if(!$pass){$failures++};$maxRatio=[Math]::Max($maxRatio,$error/$allowed)
  $values.Add(@{sensor_index=$sensor;candidate_lux=$candidate;reference_lux=$expected;absolute_error_lux=$error;allowed_error_lux=$allowed;pass=$pass;diffuse_lux=(Lux $diffuse.values[$offset..($offset+2)]);explicit_solar_lux=(Lux $solar.values[$offset..($offset+2)])})
 }
 $checks.Add(@{interval_index=$index;pass=($failures -eq 0);failure_count=$failures;maximum_error_over_tolerance=$maxRatio;sensors=$values;reference_sha256=(Get-MatchedHash (Join-Path $OutputDirectory $name))})
 Write-Host "Explicit-sun recombination interval ${index}: $failures failures; maximum error/tolerance $maxRatio"
}
$result=@{declaration=$declaration;checks=$checks;pass=(@($checks|Where-Object{!$_.pass}).Count -eq 0);processes=$records}
$result|ConvertTo-Json -Depth 18|Set-Content -LiteralPath (Join-Path $OutputDirectory 'explicit-recombination.json') -Encoding utf8
if(!$result.pass){throw 'Centroid diagnostic tolerance failed; all values retained.'}
