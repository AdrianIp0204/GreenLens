#Requires -Version 7.5
param(
 [ValidateSet('A','B')][string]$Alternative,
 [int]$IntervalIndex,
 [string]$InputArchive,
 [string]$OutputDirectory,
 [string]$SourceRoot=(Get-Location).Path,
 [switch]$PrepareOnly,
 [switch]$FunctionsOnly
)
$ErrorActionPreference='Stop'
function Get-HostedReferenceCases {
 @(
  foreach($i in @(1908,1912,4116,4120,6320,6324,6328,8508)){[ordered]@{alternative='A';interval_index=$i}}
  foreach($i in @(1908,1912,4116,4120,6320,6324,6328,8504,8508)){[ordered]@{alternative='B';interval_index=$i}}
 )
}
function Assert-HostedReferenceCase([string]$Design,[int]$Index) {
 if(@(Get-HostedReferenceCases|Where-Object{$_.alternative -ceq $Design -and $_.interval_index -eq $Index}).Count -ne 1){throw 'Only the sealed seventeen missing baseline tuples are permitted.'}
}
function Assert-HostedReferencePlan($Plan) {
 if($Plan.schema_version -cne 'greenlens.hosted-reference-plan.v1' -or $Plan.maximum_parallel_jobs -ne 4 -or $Plan.engine_workers_per_job -ne 1 -or $Plan.process_seconds -ne 7200 -or $Plan.job_minutes -ne 300){throw 'Hosted plan limits differ.'}
 $actual=@($Plan.cases|ForEach-Object{"$($_.alternative):$($_.interval_index)"})
 $expected=@(Get-HostedReferenceCases|ForEach-Object{"$($_.alternative):$($_.interval_index)"})
 if(($actual -join ',') -cne ($expected -join ',')){throw 'Hosted case plan differs, duplicates or omits a tuple.'}
 if(($Plan.excluded_qualified -join ',') -cne 'A:1904,A:4112,A:8504,A:8512,B:1904,B:4112,B:8512'){throw 'Qualified-case exclusions differ.'}
}
function Expand-HostedReferenceInputs([string]$Archive,[string]$Destination,[string]$Design,$Pinned) {
 if(Test-Path -LiteralPath $Destination){throw 'Fresh input destination required.'}
 $file=Get-Item -LiteralPath $Archive
 if($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $file.Length -ne $Pinned.bytes -or (Get-FileHash -LiteralPath $Archive).Hash.ToLowerInvariant() -cne $Pinned.sha256){throw 'Input archive size or digest differs.'}
 $expectedNames=@('original-job/annual-scene.rad','original-job/annual-sensors.pts','original-job/annual-sky.mtx','original-job/annual-suns.rad','original-job/annual-weather.epw','original-job/annual-recipe.json','original-job/input.json','original-job/study.json','original-job/weather/original.epw','original-job/weather/normalized.epw','original-job/weather/asset.json','notices/GreenLens-LICENSE.txt','notices/PVGIS-ERA5-NOTICE.txt','notices/Radiance-LICENSE.txt')
 $zip=[IO.Compression.ZipFile]::OpenRead($Archive)
 try {
  if($zip.Entries.Count -ne 15){throw 'Unexpected archive entry count.'}
  $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
  foreach($entry in $zip.Entries){
   if(!$seen.Add($entry.FullName) -or $entry.FullName -notmatch '^[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)*$' -or $entry.FullName -match '(^|/)\.\.(/|$)' -or !$entry.Name -or (($entry.ExternalAttributes -shr 16) -band 0xF000) -eq 0xA000 -or $entry.Length -gt 67108864){throw 'Unsafe, duplicate, link or oversized member.'}
  }
  if((@($seen|Sort-Object) -join ',') -cne (@(($expectedNames+@('input-manifest.json'))|Sort-Object) -join ',')){throw 'Input member allowlist differs.'}
  $manifestEntry=$zip.GetEntry('input-manifest.json')
  if($manifestEntry.Length -gt 1048576){throw 'Manifest cap exceeded.'}
  $stream=$manifestEntry.Open();$memory=[IO.MemoryStream]::new()
  try{$stream.CopyTo($memory);$bytes=$memory.ToArray()}finally{$stream.Dispose();$memory.Dispose()}
  if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant() -cne $Pinned.manifest_sha256){throw 'Input manifest digest differs.'}
  $manifest=[Text.Encoding]::UTF8.GetString($bytes)|ConvertFrom-Json -Depth 40 -DateKind String
  if($manifest.schema_version -cne $Pinned.manifest_schema -or $manifest.alternative -cne $Design -or $manifest.required_input_count -ne 11 -or $manifest.members.Count -ne 14){throw 'Manifest scope differs.'}
  if($Design -ceq 'A'){
   if($manifest.schema_version -cne 'greenlens.hosted-reference-inputs.v1' -or $manifest.interval_index -ne 8504){throw 'Pinned original A data-bundle provenance differs.'}
  }elseif($Design -ceq 'B'){
   if($manifest.schema_version -cne 'greenlens.hosted-reference-inputs.v2' -or ($manifest.allowed_interval_indices -join ',') -cne '1904,1908,1912,4112,4116,4120,6320,6324,6328,8504,8508,8512'){throw 'Pinned B data-bundle scope differs.'}
  }else{throw 'Unsupported alternative.'}
  $sourceNames=@($manifest.source_sha256.PSObject.Properties.Name|Sort-Object)
  if($sourceNames.Count -ne 9 -or ($sourceNames -join ',') -cne (@($Pinned.source_sha256.PSObject.Properties.Name|Sort-Object) -join ',')){throw 'Source pin member set differs.'}
  foreach($name in $sourceNames){if($manifest.source_sha256.$name -cne $Pinned.source_sha256.$name){throw 'Source pin differs.'}}
  $memberSeen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
  $total=0L
  foreach($member in $manifest.members){
   if(!$memberSeen.Add($member.path) -or $member.path -cnotin $expectedNames){throw 'Duplicate or unknown manifest member.'}
   $entry=$zip.GetEntry($member.path)
   if($entry.Length -ne $member.bytes -or $member.sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Member metadata differs.'}
   $total+=$entry.Length
   $stream=$entry.Open()
   try{$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()}finally{$stream.Dispose()}
   if($hash -cne $member.sha256){throw 'Member content digest differs.'}
  }
  if($total -gt 70000000){throw 'Input total size exceeds fixed bound.'}
  # All input bytes and paths are validated before creating any destination.
  $null=New-Item -ItemType Directory -Path $Destination
  foreach($member in $manifest.members){
   $dest=Join-Path $Destination $member.path
   $null=New-Item -ItemType Directory -Path (Split-Path -Parent $dest) -Force
   $stream=$zip.GetEntry($member.path).Open();$output=[IO.File]::Open($dest,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write)
   try{$stream.CopyTo($output)}finally{$output.Dispose();$stream.Dispose()}
   if((Get-FileHash -LiteralPath $dest).Hash.ToLowerInvariant() -cne $member.sha256){throw 'Extraction digest differs.'}
  }
  [IO.File]::WriteAllBytes((Join-Path $Destination 'input-manifest.json'),$bytes)
  return $manifest
 }finally{$zip.Dispose()}
}
if($FunctionsOnly){return}
Assert-HostedReferenceCase $Alternative $IntervalIndex
$plan=Get-Content -LiteralPath "$PSScriptRoot/hosted-reference-plan.json" -Raw|ConvertFrom-Json -Depth 40 -DateKind String
Assert-HostedReferencePlan $plan
$pinned=$plan.archives.$Alternative
if(!$InputArchive -or !$OutputDirectory -or (Test-Path -LiteralPath $OutputDirectory)){throw 'Input archive and fresh output directory required.'}
. "$PSScriptRoot/prepare-reference-sources.ps1"
$manifest=Expand-HostedReferenceInputs $InputArchive (Join-Path $OutputDirectory 'inputs') $Alternative $pinned
$preparation=Prepare-ReferenceSources $SourceRoot $manifest.source_sha256
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'trusted-source-preparation.json'),($preparation|ConvertTo-Json -Depth 15)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
$receipt=[ordered]@{schema_version='greenlens.hosted-reference-case.v1';started_utc=[DateTime]::UtcNow.ToString('o');alternative=$Alternative;interval_index=$IntervalIndex;archive_sha256=$pinned.sha256;manifest_sha256=$pinned.manifest_sha256;plan_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/hosted-reference-plan.json").Hash.ToLowerInvariant();source_sha256=$manifest.source_sha256;engine_workers=1;process_timeout_seconds=7200;job_envelope_minutes=300;prepare_only=[bool]$PrepareOnly;completed=$false;baseline_case_qualified=$false;release_acceptance_complete=$false}
try{
 & (Join-Path $SourceRoot 'scripts/annual-controlled-reference.ps1') -OriginalJobDirectory (Join-Path $OutputDirectory 'inputs/original-job') -OutputDirectory (Join-Path $OutputDirectory 'reference') -Alternative $Alternative -IntervalIndices @($IntervalIndex) -PrepareOnly:$PrepareOnly
 if(!$PrepareOnly){
  $report=Get-Content -LiteralPath (Join-Path $OutputDirectory 'reference/controlled-reference.json') -Raw|ConvertFrom-Json -Depth 60 -DateKind String
  if(!$report.completed -or @($report.checks).Count -ne 1 -or $report.checks[0].interval_index -ne $IntervalIndex){throw 'Single-case completed result shape differs.'}
  $receipt.completed=$true;$receipt.baseline_case_qualified=[bool]$report.checks[0].qualified
  if(!$receipt.baseline_case_qualified){throw 'Baseline reference did not meet the unchanged qualification criteria.'}
 }
}finally{
 $receipt['finished_utc']=[DateTime]::UtcNow.ToString('o')
 [IO.File]::WriteAllText((Join-Path $OutputDirectory 'case.json'),($receipt|ConvertTo-Json -Depth 20)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
}
$global:LASTEXITCODE=0
