#Requires -Version 7.2
# Finite-correct explicit source transport against retained high-quality native
# references. Partial execution is explicitly incomplete, never release evidence.
param(
 [Parameter(Mandatory)][string]$SourceDirectory,
 [Parameter(Mandatory)][string]$CoefficientDirectory,
 [Parameter(Mandatory)][string]$SolarDirectory,
 [Parameter(Mandatory)][string[]]$ReferenceDirectories,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [switch]$AllowPartial
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new evidence directory.'}
. "$PSScriptRoot/annual-matched-evidence.ps1"
$source=[IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1")
$kernelHash=Get-MatchedKernelHash $source
$kernel=[regex]::Match($source,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
foreach($samples in @(1024,4096)){$side=[int][Math]::Sqrt($samples);$code=$kernel.Replace('GreenLensMatched',"GreenLensExplicit$samples") -replace '\b4096\b',"$samples" -replace '\b64\b',"$side";Add-Type -TypeDefinition $code}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
$prior=Get-Content -LiteralPath (Join-Path $SourceDirectory 'declaration.json') -Raw|ConvertFrom-Json
$indices=@($prior.intervals.index);$available=@{};$referenceIdentity=$null
foreach($pair in @(@('scene.rad','scene_sha256'),@('sensors.pts','sensor_sha256'))){if((Get-MatchedHash (Join-Path $SourceDirectory $pair[0])) -cne $prior.($pair[1])){throw 'Prepared source bytes do not match their declaration'}}
$receiverText="void glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source upper_sky`n0`n0`n4 0 0 1 180`nsky_glow source lower_ground`n0`n0`n4 0 0 -1 180`n"
if([IO.File]::ReadAllText((Join-Path $SourceDirectory 'receiver.rad')) -cne $receiverText){throw 'Unsupported coefficient/transmission receiver'}
$selectedSky=[GreenLensFeasibility.Matrix]::Read((Join-Path $SourceDirectory 'selected-skies.mtx'))
$verifiedSource=$false
foreach($directory in $ReferenceDirectories){
 $report=Get-Content -LiteralPath (Join-Path $directory 'matched-corrected.json') -Raw|ConvertFrom-Json
 Assert-MatchedReference $report.declaration;Assert-MatchedSource $directory $kernelHash
 if(!$referenceIdentity){$referenceIdentity=$report.declaration.reference_identity_sha256}else{if($referenceIdentity -cne $report.declaration.reference_identity_sha256){throw 'Mixed reference identities'}}
 foreach($field in @('scene_sha256','sensor_sha256','sky_sha256','sun_sha256')){if($report.declaration.reference.$field -cne $prior.$field){throw "Reference input mismatch: $field"}}
 if(($report.declaration.reference.interval_indices -join ',') -ne ($indices -join ',')){throw 'Reference plan differs'}
 if(!$verifiedSource){
  if((Get-MatchedHash (Join-Path $directory 'identical-sky.mtx')) -cne $prior.sky_sha256 -or (Get-MatchedHash (Join-Path $directory 'suns.rad')) -cne $prior.sun_sha256){throw 'Retained full reference sky/source bytes differ from identity'}
  $fullSky=[GreenLensFeasibility.Matrix]::Read((Join-Path $directory 'identical-sky.mtx'))
  if($fullSky.columns -ne 8760 -or $fullSky.rows -ne $selectedSky.rows -or $selectedSky.columns -ne $indices.Count){throw 'Prepared sky dimensions differ'}
  for($row=0;$row -lt $fullSky.rows;$row++){for($column=0;$column -lt $indices.Count;$column++){for($k=0;$k -lt 3;$k++){if($selectedSky.values[($row*$indices.Count+$column)*3+$k] -ne $fullSky.values[($row*8760+$indices[$column])*3+$k]){throw 'Prepared sky values differ from exact retained reference columns'}}}}
  $sunByIndex=@{};foreach($match in [regex]::Matches([IO.File]::ReadAllText((Join-Path $directory 'suns.rad')),'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')){$sunByIndex[[int](([int]$match.Groups[1].Value-30)/60)]=$match.Value.Replace("`r`n","`n")}
  $expectedSources=[Text.StringBuilder]::new();$expectedModifiers=[Text.StringBuilder]::new()
  foreach($index in $indices){if(!$sunByIndex.ContainsKey([int]$index)){throw 'Missing reference source'};if([IO.File]::ReadAllText((Join-Path $SourceDirectory "hour-$index-sun.rad")) -cne $sunByIndex[[int]$index]){throw 'Prepared individual solar source differs'};$null=$expectedSources.Append($sunByIndex[[int]$index]).Append("`n");$null=$expectedModifiers.Append("solar$($index*60+30)`n")}
  if([IO.File]::ReadAllText((Join-Path $SourceDirectory 'selected-suns.rad')) -cne $expectedSources.ToString() -or [IO.File]::ReadAllText((Join-Path $SourceDirectory 'selected-modifiers.txt')) -cne $expectedModifiers.ToString()){throw 'Prepared solar source sequence differs'}
  $verifiedSource=$true
 }
 foreach($check in $report.checks){$index=[int]$check.interval_index;if($available.ContainsKey($index)){throw 'Duplicate completed reference interval'};Assert-MatchedRaw $directory $check;$available[$index]=@{directory=$directory;check=$check}}
}
foreach($transport in @(@($CoefficientDirectory,'coefficient'),@($SolarDirectory,'solar'))){Assert-MatchedTransport $transport[0] $transport[1]}
$missing=@($indices|Where-Object{!$available.ContainsKey([int]$_)})
if($missing.Count -and !$AllowPartial){throw "Required references are not complete: $($missing -join ',')"}
foreach($name in @('scene.rad','sensors.pts')){
 $hash=Get-MatchedHash (Join-Path $SourceDirectory $name)
 foreach($directory in @($CoefficientDirectory,$SolarDirectory)){if((Get-MatchedHash (Join-Path $directory $name)) -cne $hash){throw "Transport input mismatch: $name"}}
 Copy-Item -LiteralPath (Join-Path $SourceDirectory $name) -Destination (Join-Path $OutputDirectory $name)
}
foreach($name in @('receiver.rad','selected-skies.mtx')){Copy-Item -LiteralPath (Join-Path $SourceDirectory $name) -Destination (Join-Path $OutputDirectory $name)}
Copy-Item -LiteralPath (Join-Path $CoefficientDirectory 'coefficients.mtx') -Destination (Join-Path $OutputDirectory 'coefficients.mtx')
foreach($name in @('solar-rgb.mtx','solar-centroid-direct.mtx')){Copy-Item -LiteralPath (Join-Path $SolarDirectory $name) -Destination (Join-Path $OutputDirectory $name)}
$solarDeclaration=Get-Content -LiteralPath (Join-Path $SolarDirectory 'solar-declaration.json') -Raw|ConvertFrom-Json
if(($solarDeclaration.interval_indices -join ',') -ne ($indices -join ',')){throw 'Solar column order differs'}
foreach($field in @('scene_sha256','sensor_sha256')){if($solarDeclaration.$field -cne $prior.$field){throw 'Solar declaration input identity differs'}}
if($solarDeclaration.sources_sha256 -cne (Get-MatchedHash (Join-Path $SourceDirectory 'selected-suns.rad')) -or (Get-MatchedHash (Join-Path $SolarDirectory 'selected-skies.mtx')) -cne (Get-MatchedHash (Join-Path $SourceDirectory 'selected-skies.mtx'))){throw 'Solar cache sky/source identity differs'}
if((Get-MatchedHash (Join-Path $CoefficientDirectory 'receiver.rad')) -cne (Get-MatchedHash (Join-Path $SourceDirectory 'receiver.rad'))){throw 'Coefficient receiver differs'}
$declaration=[ordered]@{scope='Explicit-source plus finite-disc investigation; production and full-year convergence remain outstanding';declared_utc=[DateTime]::UtcNow.ToString('o');source=$prior;coefficients=(Get-Content -LiteralPath (Join-Path $CoefficientDirectory 'declaration.json') -Raw|ConvertFrom-Json);solar=$solarDeclaration;reference_identity_sha256=$referenceIdentity;finite_kernel_sha256=$kernelHash;harness_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'harness-source.ps1'));coefficients_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'coefficients.mtx'));solar_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'solar-rgb.mtx'));centroid_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'solar-centroid-direct.mtx'));declared_interval_indices=$indices;missing_interval_indices=$missing;finite_samples=@(1024,4096);centroid_agreement_tolerance_lux=.0001;tolerance='max(5.0 lx, 5% of reference) for every sensor';reference_policy='Identical generated sky plus continuous source, high-quality full trace minus ab0 centroid direct plus all-ray4096 finite transmission. Candidate diffuse coefficient plus explicit full-solar rcontrib minus ab0 explicit solar plus1024 finite transmission.'}
$declaration|ConvertTo-Json -Depth 18|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'dctimestep' @('-oa','coefficients.mtx','selected-skies.mtx') 'diffuse-rgb.mtx'
$diffuse=Read-Matrix 'diffuse-rgb.mtx';$solar=Read-Matrix 'solar-rgb.mtx';$direct=Read-Matrix 'solar-centroid-direct.mtx';$count=$diffuse.rows
if($solar.rows -ne $count -or $direct.rows -ne $count -or $diffuse.columns -ne $indices.Count -or $solar.columns -ne $indices.Count -or $direct.columns -ne $indices.Count){throw 'Transport shape differs'}
$sensors=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'sensors.pts'))
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'transmission.oct'
$options=@('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$checks=[Collections.Generic.List[object]]::new()
for($column=0;$column -lt $indices.Count;$column++){
 $index=[int]$indices[$column];if(!$available.ContainsKey($index)){continue};$name="hour-$index";$cache=$available[$index]
 foreach($suffix in @('reference.rgb','direct.rgb','finite.rgb')){Copy-Item -LiteralPath (Join-Path $cache.directory "$name-$suffix") -Destination (Join-Path $OutputDirectory "$name-$suffix")}
 $sun=[IO.File]::ReadAllText((Join-Path $SourceDirectory "$name-sun.rad"))
 Invoke-Engine 'rtrace' ($options+@('transmission.oct')) "$name-candidate-finite.rgb" ([GreenLensExplicit1024.Direct]::Rays($sensors,$sun))
 $candidateFinite=[GreenLensExplicit1024.Direct]::Finite($sensors,$sun,(Join-Path $OutputDirectory "$name-candidate-finite.rgb"))
 $referenceFinite=[GreenLensExplicit4096.Direct]::Finite($sensors,$sun,(Join-Path $OutputDirectory "$name-finite.rgb"))
 $referenceFull=[GreenLensExplicit4096.Direct]::Rgb((Join-Path $OutputDirectory "$name-reference.rgb"),$count)
 $referenceDirect=[GreenLensExplicit4096.Direct]::Rgb((Join-Path $OutputDirectory "$name-direct.rgb"),$count)
 $values=[Collections.Generic.List[object]]::new();$failures=0;$maxRatio=0.0;$maxDirectDifference=0.0
 for($sensor=0;$sensor -lt $count;$sensor++){
  $offset=($sensor*$indices.Count+$column)*3;$rgb=[double[]]::new(3);$reference=[double[]]::new(3)
  for($k=0;$k -lt 3;$k++){$rgb[$k]=$diffuse.values[$offset+$k]+$solar.values[$offset+$k]-$direct.values[$offset+$k]+$candidateFinite[$sensor][$k];$reference[$k]=$referenceFull[$sensor][$k]-$referenceDirect[$sensor][$k]+$referenceFinite[$sensor][$k]}
  $candidateLux=[GreenLensExplicit4096.Direct]::Lux($rgb);$referenceLux=[GreenLensExplicit4096.Direct]::Lux($reference)
  $centroidDifference=[Math]::Abs([GreenLensExplicit4096.Direct]::Lux($direct.values[$offset..($offset+2)])-[GreenLensExplicit4096.Direct]::Lux($referenceDirect[$sensor]));$maxDirectDifference=[Math]::Max($maxDirectDifference,$centroidDifference)
  $error=[Math]::Abs($candidateLux-$referenceLux);$allowed=[Math]::Max(5.0,.05*$referenceLux);$passed=$error -le $allowed;if(!$passed){$failures++};$maxRatio=[Math]::Max($maxRatio,$error/$allowed)
  $values.Add(@{sensor_index=$sensor;candidate_lux=$candidateLux;reference_lux=$referenceLux;absolute_error_lux=$error;allowed_error_lux=$allowed;passed=$passed;matrix_vs_trace_centroid_difference_lux=$centroidDifference})
 }
 $checks.Add(@{interval_index=$index;pass=($failures -eq 0 -and $maxDirectDifference -le .0001);failure_count=$failures;maximum_error_over_tolerance=$maxRatio;maximum_centroid_difference_lux=$maxDirectDifference;sensors=$values;reference_sha256=$cache.check.reference_sha256;direct_sha256=$cache.check.direct_sha256;finite_sha256=$cache.check.finite_sha256;candidate_finite_sha256=(Get-MatchedHash (Join-Path $OutputDirectory "$name-candidate-finite.rgb"))})
 Write-Host "Explicit corrected interval ${index}: $failures failures; error/tolerance $maxRatio; centroid difference $maxDirectDifference lx"
}
@{declaration=$declaration;checks=$checks;completed=($missing.Count -eq 0);pass=($missing.Count -eq 0 -and @($checks|Where-Object{!$_.pass}).Count -eq 0)}|ConvertTo-Json -Depth 22|Set-Content -LiteralPath (Join-Path $OutputDirectory 'explicit-matched.json') -Encoding utf8
if(@($checks|Where-Object{!$_.pass}).Count){throw 'Explicit matched investigation failed; all values retained.'}
