#Requires -Version 7.2
# Read-only diagnostic against explicitly UNQUALIFIED historical reference values.
param([Parameter(Mandatory)][string]$CandidateDirectory,[Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Choose a new diagnostic directory'}
$historicalPath=Join-Path $PSScriptRoot '../docs/evidence/v0.3-engine/explicit-actual-ab-diffuse1048576-solar8192.json'
$candidate=Get-Content -LiteralPath (Join-Path $CandidateDirectory 'paired-grid.json') -Raw|ConvertFrom-Json
if(!$candidate.completed -or !$candidate.all_values_finite_nonnegative -or $candidate.candidate_accuracy_accepted){throw 'Expected a complete, unaccepted paired-grid experiment'}
$historical=Get-Content -LiteralPath $historicalPath -Raw|ConvertFrom-Json
$record=@($historical.records|Where-Object{$_.alternative -ceq $candidate.declaration.alternative})
if($record.Count -ne 1){throw 'Historical alternative is ambiguous'}
$reference=$record[0].result;$source=$reference.declaration.source
foreach($pair in @(@('scene.rad','scene_sha256'),@('sensors.pts','sensor_sha256'),@('diffuse-year.mtx','sky_sha256'),@('suns.rad','sun_sha256'),@('source-recipe.json','source_recipe_sha256'))){
 if($candidate.declaration.input_sha256.($pair[0]) -cne $source.($pair[1]) -or (Get-FileHash -LiteralPath (Join-Path $CandidateDirectory $pair[0])).Hash.ToLowerInvariant() -cne $source.($pair[1])){throw 'Candidate and historical physical input bytes differ'}
}
foreach($property in $candidate.output_sha256.PSObject.Properties){
 if($property.Name -notmatch '^[A-Za-z0-9_.-]+$' -or (Get-FileHash -LiteralPath (Join-Path $CandidateDirectory $property.Name)).Hash.ToLowerInvariant() -cne $property.Value){throw 'Retained candidate numeric bytes changed'}
}
if(@($candidate.cases).Count -ne 12 -or @($reference.checks).Count -ne 12){throw 'This diagnostic requires all12 fixed intervals'}
$rows=@();$hours=@()
foreach($case in $candidate.cases){
 $old=@($reference.checks|Where-Object{$_.interval_index -eq $case.interval_index})
 if($old.Count -ne 1 -or @($case.sensors).Count -ne 24 -or @($old[0].sensors).Count -ne 24){throw 'Unexpected reference shape/order'}
 $hour=@()
 for($i=0;$i -lt 24;$i++){
  if($case.sensors[$i].sensor_index -ne $i -or $old[0].sensors[$i].sensor_index -ne $i){throw 'Sensor order differs'}
  $actual=[double]$case.sensors[$i].lux;$prior=[double]$old[0].sensors[$i].reference_lux
  if(![double]::IsFinite($actual) -or ![double]::IsFinite($prior) -or $prior -le 0){throw 'Diagnostic values must be positive finite'}
  $delta=$actual-$prior;$band=[Math]::Max(5.,.05*$prior)
  $row=[ordered]@{interval_index=$case.interval_index;sensor_index=$i;sensor_id=('s'+($i+1).ToString('D6'));paired_lux=$actual;historical_unqualified_lux=$prior;delta_lux=$delta;relative_delta_percent=100*$delta/$prior;historical_comparison_band_lux=$band;outside_historical_band=([Math]::Abs($delta) -gt $band)}
  $rows+=,$row;$hour+=,$row
 }
 $hours+=,[ordered]@{interval_index=$case.interval_index;mean_delta_lux=($hour.delta_lux|Measure-Object -Average).Average;minimum_relative_delta_percent=($hour.relative_delta_percent|Measure-Object -Minimum).Minimum;maximum_relative_delta_percent=($hour.relative_delta_percent|Measure-Object -Maximum).Maximum;outside_historical_band_count=@($hour|Where-Object{$_.outside_historical_band}).Count}
}
$result=[ordered]@{schema_version='greenlens.paired-unqualified-history-diagnostic.v1';captured_utc=[DateTime]::UtcNow.ToString('o');scope='Diagnostic only: historical high-sampling reference is NOT convergence-qualified and is not an accuracy oracle. Counts outside its original comparison band are descriptive, never acceptance.';alternative=$candidate.declaration.alternative;variant=$candidate.declaration.variant;candidate_result_sha256=(Get-FileHash -LiteralPath (Join-Path $CandidateDirectory 'paired-grid.json')).Hash.ToLowerInvariant();historical_evidence_sha256=(Get-FileHash -LiteralPath $historicalPath).Hash.ToLowerInvariant();harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();physical_identity_verified=$true;rows=$rows;hours=$hours;outside_historical_band_count=@($rows|Where-Object{$_.outside_historical_band}).Count;qualified_reference=$false;candidate_accuracy_accepted=$false;release_acceptance_complete=$false}
[IO.Directory]::CreateDirectory($OutputDirectory)|Out-Null
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'diagnostic.json'),(ConvertTo-Json -InputObject $result -Depth 15),[Text.UTF8Encoding]::new($false))
$hours|Format-Table
Write-Host "Diagnostic only: $($result.outside_historical_band_count)/288 outside historical band; qualified reference=false"
