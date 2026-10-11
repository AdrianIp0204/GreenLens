#Requires -Version 7.5
param(
 [Parameter(Mandatory)][string]$InputArchive,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [string]$SourceRoot=(Get-Location).Path,
 [switch]$PrepareOnly
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/prepare-reference-sources.ps1"
$expectedArchive='0b77ddbcf6d1ca03ecece672c2d1d40bdaabab6c53e30b3648bb5e14264f6f83'
$expectedManifest='ad232a37630912863c1d2123275ede5080bccad123d8a5364df95c157d86cb99'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a fresh pilot output directory.'}
$archive=[IO.File]::OpenRead((Resolve-Path -LiteralPath $InputArchive).Path)
try {
 if($archive.Length -ne 8682531){throw 'Input archive size differs from the reviewed bundle.'}
 $hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($archive)).ToLowerInvariant()
 if($hash -cne $expectedArchive){throw 'Input archive digest differs from the reviewed bundle.'}
 $archive.Position=0
 $zip=[IO.Compression.ZipArchive]::new($archive,[IO.Compression.ZipArchiveMode]::Read,$true)
 try {
  if($zip.Entries.Count -ne 15){throw 'Unexpected archive member count.'}
  $names=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
  foreach($entry in $zip.Entries){
   if(!$names.Add($entry.FullName) -or $entry.FullName -notmatch '^[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)*$' -or $entry.FullName -match '(^|/)\.\.(/|$)' -or $entry.Length -gt 67108864){throw 'Unsafe or duplicate archive member.'}
  }
  $manifestEntry=$zip.GetEntry('input-manifest.json')
  if(!$manifestEntry -or $manifestEntry.Length -gt 1048576){throw 'Missing bounded input manifest.'}
  $manifestStream=$manifestEntry.Open();$memory=[IO.MemoryStream]::new()
  try{$manifestStream.CopyTo($memory);$manifestBytes=$memory.ToArray()}finally{$manifestStream.Dispose();$memory.Dispose()}
  if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($manifestBytes)).ToLowerInvariant() -cne $expectedManifest){throw 'Input manifest identity differs.'}
  $manifest=[Text.Encoding]::UTF8.GetString($manifestBytes)|ConvertFrom-Json -DateKind String -Depth 30
  if($manifest.schema_version -cne 'greenlens.hosted-reference-inputs.v1' -or $manifest.alternative -cne 'A' -or $manifest.interval_index -ne 8504 -or $manifest.required_input_count -ne 11 -or $manifest.members.Count -ne 14){throw 'Unexpected pilot scope.'}
  $sourcePreparation=Prepare-ReferenceSources $SourceRoot $manifest.source_sha256
  $total=[long]0
  foreach($member in $manifest.members){
   $entry=$zip.GetEntry($member.path)
   if(!$entry -or $entry.Length -ne $member.bytes){throw 'Retained member size differs.'}
   $total+=$entry.Length
  }
  if($total -gt 70000000){throw 'Uncompressed input bound exceeded.'}
  $null=New-Item -ItemType Directory -Path $OutputDirectory
  [IO.File]::WriteAllText((Join-Path $OutputDirectory 'trusted-source-preparation.json'),($sourcePreparation|ConvertTo-Json -Depth 10)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
  $inputs=Join-Path $OutputDirectory 'inputs'
  $null=New-Item -ItemType Directory -Path $inputs
  foreach($member in $manifest.members){
   $entry=$zip.GetEntry($member.path);$dest=Join-Path $inputs $member.path
   $null=New-Item -ItemType Directory -Path (Split-Path $dest -Parent) -Force
   $entryStream=$entry.Open();$output=[IO.File]::Create($dest)
   try{$entryStream.CopyTo($output)}finally{$output.Dispose();$entryStream.Dispose()}
   if((Get-FileHash -LiteralPath $dest).Hash.ToLowerInvariant() -cne $member.sha256){throw 'Extracted member digest differs.'}
  }
  [IO.File]::WriteAllBytes((Join-Path $inputs 'input-manifest.json'),$manifestBytes)
 }finally{$zip.Dispose()}
}finally{$archive.Dispose()}
$receipt=[ordered]@{schema_version='greenlens.hosted-reference-pilot.v1';started_utc=[DateTime]::UtcNow.ToString('o');alternative='A';interval_index=8504;archive_sha256=$expectedArchive;manifest_sha256=$expectedManifest;engine_workers=1;process_timeout_seconds=7200;job_envelope_minutes=300;source_sha256=$manifest.source_sha256;prepare_only=[bool]$PrepareOnly;completed=$false;release_acceptance_complete=$false}
$receiptPath=Join-Path $OutputDirectory 'pilot.json'
try {
 # Only checked-out trusted code runs. The archive contains data and notices.
 & (Join-Path $SourceRoot 'scripts/annual-controlled-reference.ps1') -OriginalJobDirectory (Join-Path $inputs 'original-job') -OutputDirectory (Join-Path $OutputDirectory 'reference') -Alternative A -IntervalIndices @(8504) -PrepareOnly:$PrepareOnly
 if(!$PrepareOnly){$report=Get-Content -LiteralPath (Join-Path $OutputDirectory 'reference/controlled-reference.json') -Raw|ConvertFrom-Json -DateKind String -Depth 40;if(!$report.completed -or @($report.checks).Count -ne 1 -or $report.checks[0].interval_index -ne 8504){throw 'Single-case completion shape differs.'};$receipt.completed=$true;$receipt['baseline_case_qualified']=$report.checks[0].qualified}
}catch{$receipt['failure']=$_.Exception.Message;throw}
finally{$receipt['finished_utc']=[DateTime]::UtcNow.ToString('o');[IO.File]::WriteAllText($receiptPath,($receipt|ConvertTo-Json -Depth 15)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))}
