#Requires -Version 7.2
# Isolate diffuse coefficient sampling at one already identified sensor.
# Single-sensor sampling can use a different deterministic ray sequence from a
# full grid, so this diagnostic cannot replace the full-grid acceptance checks.
param(
 [Parameter(Mandatory)][string]$SourceDirectory,
 [Parameter(Mandatory)][string]$MatchedDirectory,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [int]$SensorIndex=15,
 [int[]]$SampleCounts=@(262144,1048576)
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
. "$PSScriptRoot/annual-matched-evidence.ps1"
$report=Get-Content -LiteralPath (Join-Path $MatchedDirectory 'explicit-matched.json') -Raw|ConvertFrom-Json
$positions=[IO.File]::ReadAllLines((Join-Path $SourceDirectory 'sensors.pts'))
if($SensorIndex -lt 0 -or $SensorIndex -ge $positions.Length){throw 'Sensor index is outside the original grid'}
foreach($file in @('scene.rad','receiver.rad','selected-skies.mtx')){Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file)}
Write-Input 'sensors.pts' ($positions[$SensorIndex]+"`n")
Copy-Item -LiteralPath (Join-Path $MatchedDirectory 'diffuse-rgb.mtx') -Destination (Join-Path $OutputDirectory 'original-diffuse.mtx')
$original=Read-Matrix 'original-diffuse.mtx';$indices=[int[]]@($report.declaration.declared_interval_indices)
if($original.rows -ne $positions.Length -or $original.columns -ne $indices.Count){throw 'Original diffuse dimensions differ'}
$declaration=[ordered]@{scope='Single-sensor diffuse sampling isolation; not full-grid acceptance';declared_utc=[DateTime]::UtcNow.ToString('o');sensor_index=$SensorIndex;sensor=$positions[$SensorIndex];sample_counts=$SampleCounts;ambient_bounces=8;sky_basis_mf=2;original_report_sha256=(Get-MatchedHash (Join-Path $MatchedDirectory 'explicit-matched.json'));single_sensor_sequence_may_differ=$true;fixed_solar_and_finite_terms=$true;tolerance='max(5.0 lx, 5% of reference), unchanged diagnostic comparison'}
$declaration|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
function Lux([double[]]$rgb){foreach($value in $rgb){if(![double]::IsFinite($value) -or $value -lt 0){throw 'Invalid RGB'}};return 179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])}
$checks=[Collections.Generic.List[object]]::new()
foreach($probeCount in $SampleCounts){
 $child=Join-Path $OutputDirectory "samples-$probeCount"
 & "$PSScriptRoot/annual-coefficient-probe.ps1" -SourceDirectory $OutputDirectory -OutputDirectory $child -CoefficientSamples $probeCount -CoefficientBounces 8 -CoefficientBasis 2
 Invoke-Engine 'dctimestep' @('-oa',"samples-$probeCount/coefficients.mtx",'selected-skies.mtx') "diffuse-$probeCount.mtx"
 $matrix=Read-Matrix "diffuse-$probeCount.mtx";if($matrix.rows -ne 1 -or $matrix.columns -ne $indices.Count){throw 'Single-sensor diffuse dimensions differ'}
 foreach($check in $report.checks){$index=[int]$check.interval_index;$column=[Array]::IndexOf($indices,$index);if($column -lt 0){throw 'Unknown report interval'};$value=$check.sensors[$SensorIndex];if($value.sensor_index -ne $SensorIndex){throw 'Original sensor order differs'};$offset=($SensorIndex*$indices.Count+$column)*3;$oldDiffuse=Lux $original.values[$offset..($offset+2)];$newDiffuse=Lux $matrix.values[($column*3)..($column*3+2)];$candidate=$value.candidate_lux-$oldDiffuse+$newDiffuse;$error=[Math]::Abs($candidate-$value.reference_lux);$allowed=[Math]::Max(5.0,.05*$value.reference_lux);$checks.Add(@{samples=$probeCount;interval_index=$index;original_diffuse_lux=$oldDiffuse;single_sensor_diffuse_lux=$newDiffuse;original_candidate_lux=$value.candidate_lux;candidate_lux=$candidate;reference_lux=$value.reference_lux;absolute_error_lux=$error;allowed_error_lux=$allowed;pass=($error -le $allowed);error_over_tolerance=$error/$allowed})}
 @{declaration=$declaration;checks=$checks;completed=($probeCount -eq $SampleCounts[-1])}|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'diffuse-sampling.json') -Encoding utf8
 $checks|Where-Object{$_.samples -eq $probeCount}|Select-Object interval_index,original_diffuse_lux,single_sensor_diffuse_lux,error_over_tolerance|Format-Table|Out-Host
}
