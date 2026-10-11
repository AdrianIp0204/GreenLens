#Requires -Version 7.2
# Bounded convergence diagnostic; a different single-sensor random sequence is
# explicitly not a replacement for the retained full-grid acceptance record.
param(
 [Parameter(Mandatory)][string]$ReferenceDirectory,
 [Parameter(Mandatory)][string]$ExplicitDirectory,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [ValidateRange(0,255)][int]$SensorIndex=22,
 [ValidateRange(0,8759)][int]$IntervalIndex=8512
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 3600
. "$PSScriptRoot/annual-matched-evidence.ps1"
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
$report=Get-Content -LiteralPath (Join-Path $ReferenceDirectory 'matched-corrected.json') -Raw|ConvertFrom-Json
Assert-MatchedReference $report.declaration
$check=@($report.checks|Where-Object interval_index -EQ $IntervalIndex)
if($check.Count -ne 1){throw 'Expected one retained reference hour.'}
Assert-MatchedRaw $ReferenceDirectory $check[0]
$candidate=Get-Content -LiteralPath (Join-Path $ExplicitDirectory 'explicit-matched.json') -Raw|ConvertFrom-Json
if($candidate.declaration.reference_identity_sha256 -cne $report.declaration.reference_identity_sha256){throw 'Reference/candidate identities differ.'}
$candidateCheck=@($candidate.checks|Where-Object interval_index -EQ $IntervalIndex)
if($candidateCheck.Count -ne 1){throw 'Expected one candidate hour.'}
$sensorLines=@([IO.File]::ReadAllLines((Join-Path $ReferenceDirectory 'sensors.pts'))|Where-Object{$_.Trim()})
if($SensorIndex -ge $sensorLines.Count){throw 'Sensor index is outside the recorded grid.'}
$sensor=$sensorLines[$SensorIndex]+"`n";Write-Input 'one-sensor.pts' $sensor
$name="hour-$IntervalIndex"
foreach($file in @('scene.rad','matched-basis.cal',"$name.dat","$name.rad","$name-sun.rad")){Copy-Item -LiteralPath (Join-Path $ReferenceDirectory $file) -Destination (Join-Path $OutputDirectory $file)}
$sky=[IO.File]::ReadAllText((Join-Path $OutputDirectory "$name.rad"));$sunStart=$sky.IndexOf('void light solar');if($sunStart -lt 1){throw 'Expected separate generated solar source after the identical patch sky.'};Write-Input 'diffuse-only.rad' $sky.Substring(0,$sunStart)
$base=@('-I+','-h','-faa','-ov','-n','1','-ab','9','-ad','262144','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$counts=@(262144,1048576,4194304)
$inputs=[ordered]@{};foreach($file in @('scene.rad','matched-basis.cal',"$name.dat","$name.rad","$name-sun.rad",'one-sensor.pts')){$inputs[$file]=Get-MatchedHash (Join-Path $OutputDirectory $file)}
$declaration=[ordered]@{scope='One-sensor reference convergence and component diagnosis; full-grid failure is preserved';declared_utc=[DateTime]::UtcNow.ToString('o');sensor_index=$SensorIndex;interval_index=$IntervalIndex;reference_identity_sha256=$report.declaration.reference_identity_sha256;input_sha256=$inputs;harness_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'harness-source.ps1'));combined_reference_samples=$counts;component_samples=1048576;ambient_bounces=9;ray_arguments=$base;last_successive_reference_relative_change_limit=0.01;original_full_grid_reference=$check[0].sensors[$SensorIndex];original_full_grid_candidate=$candidateCheck[0].sensors[$SensorIndex];limitation='Single-sensor sampling has a different random sequence and does not replace full-grid acceptance.'}
$declaration|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
function Read-ProbeRgb([string]$File){$text=[IO.File]::ReadAllText((Join-Path $OutputDirectory $File));$values=@($text.Trim() -split '\s+'|ForEach-Object{[double]::Parse($_,$culture)});if($values.Count -ne 3 -or @($values|Where-Object{![double]::IsFinite($_) -or $_ -lt 0}).Count){throw 'Invalid native RGB shape/value.'};return ,$values}
function Probe-Lux([double[]]$Rgb){return 179*(.265074126*$Rgb[0]+.670114631*$Rgb[1]+.064811243*$Rgb[2])}
Invoke-Engine 'oconv' @('-f','scene.rad',"$name.rad") 'combined.oct'
$combined=[Collections.Generic.List[object]]::new()
foreach($division in $counts){$options=@($base);$options[[Array]::IndexOf($options,'-ad')+1]="$division";$file="combined-$division.rgb";Invoke-Engine 'rtrace' ($options+@('combined.oct')) $file $sensor;$rgb=Read-ProbeRgb $file;$combined.Add([ordered]@{samples=$division;rgb=$rgb;lux=(Probe-Lux $rgb);raw_sha256=(Get-MatchedHash (Join-Path $OutputDirectory $file))});$combined|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $OutputDirectory 'combined-progress.json') -Encoding utf8}
$components=[ordered]@{}
foreach($part in @(@('diffuse','diffuse-only.rad'),@('solar',"$name-sun.rad"))){$label=$part[0];Invoke-Engine 'oconv' @('-f','scene.rad',$part[1]) "$label.oct";$options=@($base);$options[[Array]::IndexOf($options,'-ad')+1]='1048576';Invoke-Engine 'rtrace' ($options+@("$label.oct")) "$label.rgb" $sensor;$rgb=Read-ProbeRgb "$label.rgb";$components[$label]=[ordered]@{rgb=$rgb;lux=(Probe-Lux $rgb);raw_sha256=(Get-MatchedHash (Join-Path $OutputDirectory "$label.rgb"))}}
$column=[Array]::IndexOf([int[]]$candidate.declaration.declared_interval_indices,$IntervalIndex)
$candidateComponents=[ordered]@{}
foreach($file in @('diffuse-rgb.mtx','solar-rgb.mtx','solar-centroid-direct.mtx')){$matrix=[GreenLensFeasibility.Matrix]::Read((Join-Path $ExplicitDirectory $file));$offset=($SensorIndex*$matrix.columns+$column)*3;[double[]]$rgb=$matrix.values[$offset..($offset+2)];$candidateComponents[$file]=[ordered]@{rgb=$rgb;lux=(Probe-Lux $rgb)}}
$change=[Math]::Abs($combined[-1].lux-$combined[-2].lux)/[Math]::Max(1e-12,$combined[-1].lux)
$result=[ordered]@{declaration=$declaration;combined=$combined;native_components=$components;candidate_components=$candidateComponents;last_successive_relative_change=$change;reference_convergence_pass=($change -le .01);release_acceptance_complete=$false}
$result|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'reference-sensor.json') -Encoding utf8
Write-Host "Reference sensor investigation complete. Last successive relative change: $change"
