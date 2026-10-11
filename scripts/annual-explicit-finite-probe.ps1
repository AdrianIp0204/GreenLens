#Requires -Version 7.2
# Correct retained explicit-sun transport and matched references using finite
# directional transmission. Trusted local research inputs only; no imported commands.
param(
 [Parameter(Mandatory)][string]$RecombinedDirectory,
 [Parameter(Mandatory)][string]$SolarDirectory,
 [Parameter(Mandatory)][string]$ReferenceDirectory,
 [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new evidence directory.'}
. "$PSScriptRoot/annual-matched-evidence.ps1"
$source=[IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1")
$kernel=[regex]::Match($source,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
foreach($samples in @(1024,4096)){$side=[int][Math]::Sqrt($samples);$code=$kernel.Replace('GreenLensMatched',"GreenLensExplicit$samples") -replace '\b4096\b',"$samples" -replace '\b64\b',"$side";Add-Type -TypeDefinition $code}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
$prior=Get-Content -LiteralPath (Join-Path $RecombinedDirectory 'declaration.json') -Raw|ConvertFrom-Json
$indices=@($prior.original_matched.intervals.index)
foreach($file in @('scene.rad','sensors.pts')){
 if((Get-MatchedHash (Join-Path $RecombinedDirectory $file)) -ne (Get-MatchedHash (Join-Path $ReferenceDirectory $file)) -or (Get-MatchedHash (Join-Path $SolarDirectory $file)) -ne (Get-MatchedHash (Join-Path $ReferenceDirectory $file))){throw 'Geometry or sensor identity mismatch'}
 Copy-Item -LiteralPath (Join-Path $ReferenceDirectory $file) -Destination (Join-Path $OutputDirectory $file)
}
Copy-Item -LiteralPath (Join-Path $RecombinedDirectory 'diffuse-rgb.mtx') -Destination (Join-Path $OutputDirectory 'diffuse-rgb.mtx')
foreach($name in @('solar-rgb.mtx','solar-centroid-direct.mtx')){Copy-Item -LiteralPath (Join-Path $SolarDirectory $name) -Destination (Join-Path $OutputDirectory $name)}
$diffuse=Read-Matrix 'diffuse-rgb.mtx';$solar=Read-Matrix 'solar-rgb.mtx';$direct=Read-Matrix 'solar-centroid-direct.mtx';$count=$diffuse.rows
if($solar.rows -ne $count -or $direct.rows -ne $count -or $diffuse.columns -ne $indices.Count -or $solar.columns -ne $indices.Count -or $direct.columns -ne $indices.Count){throw 'Matrix shape mismatch'}
$sensors=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'sensors.pts'))
$declaration=@{scope='Finite-corrected explicit-sun investigation; final A/B/full-year profile acceptance remains outstanding';declared_utc=[DateTime]::UtcNow.ToString('o');prior=$prior;solar=(Get-Content -LiteralPath (Join-Path $SolarDirectory 'solar-declaration.json') -Raw|ConvertFrom-Json);finite_kernel_sha256=(Get-MatchedKernelHash $source);interval_indices=$indices;direct_samples=@(1024,4096);tolerance='max(5.0 lx, 5% of reference) for every retained sensor/hour';reference_policy='Original identical-sky high-quality full rtrace, minus independently invoked ab0 centroid rtrace, plus all-ray4096 finite transmission. Candidate uses explicit full-solar rcontrib minus explicit ab0 rcontrib plus1024 finite transmission.'}
$declaration|ConvertTo-Json -Depth 18|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Write-Input 'receiver.rad' "void glow unit`n0`n0`n4 1 1 1 0`nunit source dome`n0`n0`n4 0 0 1 360`n"
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'transmission.oct'
$options=@('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$checks=[Collections.Generic.List[object]]::new()
for($column=0;$column -lt $indices.Count;$column++){
 $index=$indices[$column];$name="hour-$index"
 $sky=[IO.File]::ReadAllText((Join-Path $ReferenceDirectory "$name.rad"));$sun=$sky.Substring($sky.IndexOf('void light solar'));Write-Input "$name-sun.rad" $sun
 Invoke-Engine 'oconv' @('-f','scene.rad',"$name-sun.rad") "$name-sun.oct"
 Invoke-Engine 'rtrace' (@('-I+')+$options+@("$name-sun.oct")) "$name-direct.rgb" $sensors
 $traceDirect=[GreenLensExplicit4096.Direct]::Rgb((Join-Path $OutputDirectory "$name-direct.rgb"),$count)
 $finite=@{}
 foreach($samples in @(1024,4096)){$type="GreenLensExplicit$samples.Direct" -as [type];$file="$name-finite$samples.rgb";Invoke-Engine 'rtrace' ($options+@('transmission.oct')) $file ($type::Rays($sensors,$sun));$finite[$samples]=$type::Finite($sensors,$sun,(Join-Path $OutputDirectory $file))}
 Copy-Item -LiteralPath (Join-Path $ReferenceDirectory "$name.rgb") -Destination (Join-Path $OutputDirectory "$name-reference.rgb")
 $fullReference=[GreenLensExplicit4096.Direct]::Rgb((Join-Path $OutputDirectory "$name-reference.rgb"),$count)
 $values=[Collections.Generic.List[object]]::new();$failures=0;$maxRatio=0.0;$maxDirectDifference=0.0
 for($sensor=0;$sensor -lt $count;$sensor++){
  $offset=($sensor*$indices.Count+$column)*3;$rgb=[double[]]::new(3);$reference=[double[]]::new(3)
  for($k=0;$k -lt 3;$k++){$rgb[$k]=$diffuse.values[$offset+$k]+$solar.values[$offset+$k]-$direct.values[$offset+$k]+$finite[1024][$sensor][$k];$reference[$k]=$fullReference[$sensor][$k]-$traceDirect[$sensor][$k]+$finite[4096][$sensor][$k]}
  $candidateLux=[GreenLensExplicit4096.Direct]::Lux($rgb);$referenceLux=[GreenLensExplicit4096.Direct]::Lux($reference)
  $directDifference=[Math]::Abs([GreenLensExplicit4096.Direct]::Lux($direct.values[$offset..($offset+2)])-[GreenLensExplicit4096.Direct]::Lux($traceDirect[$sensor]));$maxDirectDifference=[Math]::Max($maxDirectDifference,$directDifference)
  $error=[Math]::Abs($candidateLux-$referenceLux);$allowed=[Math]::Max(5.0,.05*$referenceLux);$pass=$error -le $allowed
  if(!$pass){$failures++};$maxRatio=[Math]::Max($maxRatio,$error/$allowed)
  $values.Add(@{sensor_index=$sensor;candidate_lux=$candidateLux;reference_lux=$referenceLux;absolute_error_lux=$error;allowed_error_lux=$allowed;pass=$pass;matrix_vs_trace_centroid_difference_lux=$directDifference})
 }
 $checks.Add(@{interval_index=$index;pass=($failures -eq 0 -and $maxDirectDifference -le .0001);failure_count=$failures;maximum_error_over_tolerance=$maxRatio;maximum_centroid_difference_lux=$maxDirectDifference;sensors=$values;reference_sha256=(Get-MatchedHash (Join-Path $OutputDirectory "$name-reference.rgb"))})
 Write-Host "Explicit finite interval ${index}: $failures failures; max error/tolerance $maxRatio; centroid agreement $maxDirectDifference lx"
 @{declaration=$declaration;checks=$checks;completed=($checks.Count -eq $indices.Count);pass=($checks.Count -eq $indices.Count -and @($checks|Where-Object{!$_.pass}).Count -eq 0)}|ConvertTo-Json -Depth 22|Set-Content -LiteralPath (Join-Path $OutputDirectory 'explicit-finite.json') -Encoding utf8
}
if(@($checks|Where-Object{!$_.pass}).Count){throw 'Finite-corrected explicit-sun investigation failed; all outputs retained.'}
