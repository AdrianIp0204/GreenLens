#Requires -Version 7.2
# Research only: reuse a sealed moderate-sampling pilot's full coefficients.
param([Parameter(Mandatory)][string]$CoefficientDirectory,[Parameter(Mandatory)][string]$OriginalJobDirectory,[Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
$prior=Get-Content -LiteralPath (Join-Path $CoefficientDirectory 'declaration.json') -Raw|ConvertFrom-Json
if($prior.schema_version -ne 'greenlens.moderate-transport-probe.v1' -or $prior.phase -ne 'diffuse' -or $prior.source_receiver_scale -ne 100){throw 'Unsupported pilot coefficients.'}
if((Get-FileHash -LiteralPath (Join-Path $OriginalJobDirectory 'annual-recipe.json')).Hash.ToLowerInvariant() -ne (Get-Content -LiteralPath (Join-Path $CoefficientDirectory 'source-declaration.json') -Raw|ConvertFrom-Json).source_recipe_sha256){throw 'Original job identity mismatch.'}
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
$hashes=[ordered]@{}
foreach($file in @('scene.rad','sensors.pts','repeated-sensors.pts','coefficient-receiver.rad','coefficients-scaled.mtx','declaration.json')){Copy-Item -LiteralPath (Join-Path $CoefficientDirectory $file) -Destination (Join-Path $OutputDirectory "source-$file");$hashes[$file]=(Get-FileHash -LiteralPath (Join-Path $CoefficientDirectory $file)).Hash.ToLowerInvariant()}
foreach($file in @('scene.rad','sensors.pts','repeated-sensors.pts','coefficient-receiver.rad','coefficients-scaled.mtx')){Copy-Item -LiteralPath (Join-Path $CoefficientDirectory $file) -Destination (Join-Path $OutputDirectory $file)}
if($hashes['sensors.pts'] -ne $prior.selected_sensor_sha256 -or $hashes['repeated-sensors.pts'] -ne $prior.primary_input_sha256 -or $hashes['scene.rad'] -ne $prior.source_sha256.'scene.rad'){throw 'Pilot inputs differ from declaration.'}
$solar= [GreenLensFeasibility.Matrix]::Read((Join-Path $OriginalJobDirectory 'annual-solar-sky.mtx'))
$indices=@($prior.intervals.index);$count=@($prior.sensor_ids).Count
if($solar.rows -ne 578 -or $solar.columns -ne 8760){throw 'Expected original MF2 solar-only matrix.'}
$matrix=[Text.StringBuilder]::new("#?RADIANCE`nNROWS=$($solar.rows)`nNCOLS=$($indices.Count)`nNCOMP=3`nFORMAT=ascii`n`n")
for($row=0;$row -lt $solar.rows;$row++){foreach($index in $indices){for($channel=0;$channel -lt 3;$channel++){$value=$solar.values[($row*8760+$index)*3+$channel];if($row -eq 0 -and $value -ne 0){throw 'Solar-only lower hemisphere must be black.'};$null=$matrix.Append($value.ToString('R',$culture)).Append(' ')}};$null=$matrix.Append("`n")}
Write-Input 'selected-solar-sky.mtx' $matrix.ToString()
$options=@($prior.ray_arguments);$options[[Array]::IndexOf($options,'-ab')+1]='1'
$declaration=[ordered]@{schema_version='greenlens.moderate-patch-probe.v1';scope='Diagnostic patch-indirect reuse; no production or physical acceptance';declared_utc=[DateTime]::UtcNow.ToString('o');prior=$prior;input_sha256=$hashes;original_solar_matrix_sha256=(Get-FileHash -LiteralPath (Join-Path $OriginalJobDirectory 'annual-solar-sky.mtx')).Hash.ToLowerInvariant();selected_solar_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'selected-solar-sky.mtx')).Hash.ToLowerInvariant();direct_arguments=$options;formula='(scaled_full_coefficients - scaled_ab1_coefficients) * pure_solar_matrix / 100; diffuse/ground and finite-disc direct remain separate';harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();release_acceptance_complete=$false}
$declaration|ConvertTo-Json -Depth 18|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'oconv' @('-f','scene.rad','coefficient-receiver.rad') 'direct.oct'
Invoke-Engine 'rcontrib' ($options+@('-e','MF:2','-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow','direct.oct')) 'direct-scaled.mtx' ([IO.File]::ReadAllText((Join-Path $OutputDirectory 'repeated-sensors.pts')))
Invoke-Engine 'dctimestep' @('-oa','coefficients-scaled.mtx','selected-solar-sky.mtx') 'solar-full-scaled.mtx'
Invoke-Engine 'dctimestep' @('-oa','direct-scaled.mtx','selected-solar-sky.mtx') 'solar-direct-scaled.mtx'
$full=Read-Matrix 'solar-full-scaled.mtx';$direct=Read-Matrix 'solar-direct-scaled.mtx'
if($full.rows -ne $count -or $direct.rows -ne $count -or $full.columns -ne $indices.Count -or $direct.columns -ne $indices.Count){throw 'Shape mismatch.'}
$rows=@(for($sensor=0;$sensor -lt $count;$sensor++){
 for($hour=0;$hour -lt $indices.Count;$hour++){
  $offset=($sensor*$indices.Count+$hour)*3;$rgb=@(0.,0.,0.)
  for($channel=0;$channel -lt 3;$channel++){
   $rgb[$channel]=($full.values[$offset+$channel]-$direct.values[$offset+$channel])/100
   if(![double]::IsFinite($rgb[$channel]) -or $rgb[$channel] -lt 0){throw 'Negative or invalid indirect patch RGB; no clamping.'}
  }
  [ordered]@{sensor_index=$prior.original_sensor_indices[$sensor];sensor_id=$prior.sensor_ids[$sensor];interval_index=$indices[$hour];rgb=$rgb;lux=179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])}
 }
})
$rows|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
Write-Host 'Completed diagnostic patch-indirect transport; no acceptance inferred.'
