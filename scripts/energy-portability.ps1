# Full compact-evidence copy/report/export with original weather/run scratch hidden.
# Own verification inputs only; all renames are restored in finally.
[CmdletBinding(DefaultParameterSetName='Run')]
param(
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Cli,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Project,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$OutputDirectory,
 [Parameter(ParameterSetName='Run')][switch]$Replay,
 [Parameter(ParameterSetName='Run')][string]$BaselineRun,
 [Parameter(ParameterSetName='Run')][string]$ProposedRun,
 [Parameter(Mandatory,ParameterSetName='Functions')][switch]$FunctionsOnly
)
function Restore-EnergyVerificationPaths($Moves){
 $errors=[Collections.Generic.List[string]]::new()
 for($index=$Moves.Count-1;$index -ge 0;$index--){
  $move=$Moves[$index]
  try {
   if(Test-Path -LiteralPath $move.original){throw 'Original path was recreated externally; hidden evidence preserved'}
   Move-Item -LiteralPath $move.hidden -Destination $move.original -ErrorAction Stop
  }catch{$errors.Add("$($move.original): $($_.Exception.Message)")}
 }
 if($errors.Count){throw ('Cold verification restore conflicts: '+($errors -join '; '))}
}
function Assert-EnergyPortableMember([string]$Name){
 if([string]::IsNullOrEmpty($Name) -or $Name.Contains('\') -or $Name.Contains(':') -or $Name -match '[\x00-\x1f\x7f<>"|?*]'){throw 'Unsafe portable evidence member'}
 foreach($part in $Name.Split('/')){
  if(!$part -or $part -in @('.','..') -or $part.EndsWith('.') -or $part.EndsWith(' ') -or ($part.Split('.')[0] -match '^(CON|PRN|AUX|NUL|CONIN\$|CONOUT\$|COM[1-9]|LPT[1-9])$')){throw 'Unsafe portable evidence member'}
 }
}
function Get-EnergyPortableSnapshot([string]$Root){
 $snapshot=[ordered]@{}
 $manifest=Join-Path $Root 'project.json'
 if(!(Test-Path -LiteralPath $manifest -PathType Leaf)){throw 'Portable project manifest is missing'}
 $snapshot['project.json']=(Get-FileHash -LiteralPath $manifest).Hash.ToLowerInvariant()
 foreach($directory in @('runs','weather','energy-assessments','references','attachments')){
  $path=Join-Path $Root $directory
  if(Test-Path -LiteralPath $path){foreach($file in Get-ChildItem -LiteralPath $path -Recurse -File){
   $name=[IO.Path]::GetRelativePath($Root,$file.FullName).Replace('\','/')
   Assert-EnergyPortableMember $name
   $snapshot[$name]=(Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant()
  }}
 }
 return $snapshot
}
function Assert-EnergyPortableSnapshot([string]$Root,[Collections.IDictionary]$Expected){
 $actual=Get-EnergyPortableSnapshot $Root
 if($actual.Count -ne $Expected.Count){throw 'Immutable portable evidence member set changed'}
 foreach($name in $Expected.Keys){
  if(!$actual.Contains($name) -or $actual[$name] -cne $Expected[$name]){throw "Immutable portable evidence changed: $name"}
 }
}
function Get-EnergyReferenceMembers([string]$Root,[Collections.IDictionary]$Document){
 # The shared CLI has already admitted these contracts. This independent
 # inventory checks exact normalized bytes, not geometry or scientific validity.
 $members=[ordered]@{}
 if(!$Document.Contains('architecture') -or $null -eq $Document.architecture){return $members}
 foreach($asset in $Document.architecture.assets){
  $prefix=[string]$asset.path
  Assert-EnergyPortableMember $prefix
  if($prefix -cne "references/$($asset.id)"){throw 'Reference path differs from its asset identity'}
  $manifestName="$prefix/reference.json"; $manifestPath=Join-Path $Root $manifestName
  if((Get-Item -LiteralPath $manifestPath).Length -gt 16MB){throw 'Reference manifest exceeds the 16 MiB bound'}
  $manifestHash=(Get-FileHash -LiteralPath $manifestPath).Hash.ToLowerInvariant()
  if($manifestHash -cne $asset.manifest_sha256){throw 'Reference manifest differs from project binding'}
  $manifest=Get-Content -LiteralPath $manifestPath -Raw|ConvertFrom-Json -AsHashtable
  if($manifest.schema_version -cne 'greenlens.reference-model.v1' -or $manifest.revision_sha256 -cne $asset.revision_sha256 -or $manifest.geometry_sha256 -cne $asset.geometry_sha256){throw 'Reference revision or geometry binding differs'}
  foreach($name in @('reference.json','checksums.json')+@($manifest.chunks|ForEach-Object { $_.path })){
   Assert-EnergyPortableMember $name
   $relative="$prefix/$name"
   if($members.Contains($relative)){throw 'Duplicate required reference member'}
   $path=Join-Path $Root $relative; $file=Get-Item -LiteralPath $path
   if($file.Length -gt 64MB){throw 'Reference evidence exceeds the shared per-entry bound'}
   $members[$relative]=[ordered]@{bytes=[long]$file.Length;sha256=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()}
  }
  foreach($chunk in $manifest.chunks){
   $member=$members["$prefix/$($chunk.path)"]
   if($member.bytes -ne $chunk.byte_length -or $member.sha256 -cne $chunk.sha256){throw 'Normalized reference chunk differs from its manifest'}
  }
 }
 return $members
}
function Assert-EnergyArchiveSizes([long[]]$Sizes){
 if($Sizes.Count -gt 5000){throw 'Archive exceeds the shared 5,000-entry bound'}
 [long]$total=0
 foreach($size in $Sizes){
  if($size -lt 0 -or $size -gt 64MB){throw 'Archive exceeds the shared 64 MiB per-entry bound'}
  $total+=$size
  if($total -gt 256MB){throw 'Archive exceeds the shared 256 MiB uncompressed bound'}
 }
 return $total
}
function Assert-EnergyReferenceArchive([string]$Package,[Collections.IDictionary]$Expected){
 $archive=[IO.Compression.ZipFile]::OpenRead($Package)
 try {
  $total=Assert-EnergyArchiveSizes @($archive.Entries|ForEach-Object { [long]$_.Length })
  $names=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
  $references=[ordered]@{}
  foreach($entry in $archive.Entries){
   Assert-EnergyPortableMember $entry.FullName
   if(!$names.Add($entry.FullName)){throw 'Archive contains duplicate/case-colliding paths'}
   if(($entry.ExternalAttributes -shr 16 -band 0xF000) -eq 0xA000){throw 'Archive contains a symbolic link'}
   if($entry.FullName -match '\.(skp|glb)$'){throw 'Original architectural source or conversion was included unexpectedly'}
   if($entry.FullName.StartsWith('references/',[StringComparison]::OrdinalIgnoreCase)){
    if(!(@($Expected.Keys) -ccontains $entry.FullName)){throw 'Archive contains an undeclared normalized reference member'}
    $bound=$Expected[$entry.FullName]
    if($entry.Length -ne $bound.bytes){throw 'Archive reference length differs from retained bytes'}
    $stream=$entry.Open(); $hasher=[Security.Cryptography.SHA256]::Create()
    try {
     # Hash actual decompressed bytes with the declared/shared limit enforced.
     $buffer=[byte[]]::new(65536); [long]$read=0
     while(($count=$stream.Read($buffer,0,$buffer.Length)) -gt 0){
      $read+=$count
      if($read -gt $bound.bytes -or $read -gt 64MB){throw 'Archive reference expands beyond its declared bound'}
      $null=$hasher.TransformBlock($buffer,0,$count,$buffer,0)
     }
     $null=$hasher.TransformFinalBlock([byte[]]::new(0),0,0)
     $hash=[Convert]::ToHexString($hasher.Hash).ToLowerInvariant()
     if($read -ne $bound.bytes -or $hash -cne $bound.sha256){throw 'Archive reference bytes differ from the retained asset'}
     $references[$entry.FullName]=[ordered]@{bytes=$read;sha256=$hash}
    } finally {$hasher.Dispose();$stream.Dispose()}
   }
  }
  if($references.Count -ne $Expected.Count){throw 'Archive omitted a required normalized reference member'}
  return [ordered]@{schema='greenlens.normalized-reference-archive-check.v1';entries=$archive.Entries.Count;uncompressed_bytes=$total;normalized_members=$references;original_source_and_conversion_excluded=$true;scope='Exact normalized member/byte equality and shared structural bounds; complete package integrity is checked separately by the CLI.'}
 } finally {$archive.Dispose()}
}
if($FunctionsOnly){return}
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
$base=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
$Project=[IO.Path]::GetFullPath($Project); $OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
if(!$Project.StartsWith((Join-Path $base 'verification')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Cold verification source must be an owned GreenLens verification project'}
if(!$OutputDirectory.StartsWith((Join-Path $base 'verification')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Cold verification outputs belong under GreenLens verification'}
if($OutputDirectory.StartsWith($Project+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $Project.StartsWith($OutputDirectory+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Cold source and output must be independent directories'}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Evidence output must be new'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$frozenCli=Join-Path $OutputDirectory 'greenlens.exe'; Copy-Item -LiteralPath $Cli -Destination $frozenCli
$cliHash=(Get-FileHash -LiteralPath $frozenCli).Hash.ToLowerInvariant()
$copy=Join-Path $OutputDirectory 'cold copy 空間'
$commands=[Collections.Generic.List[object]]::new()
function Invoke-Cli([string]$Name,[string[]]$Arguments){
 if((Get-FileHash -LiteralPath $frozenCli).Hash.ToLowerInvariant() -cne $cliHash){throw 'Frozen CLI changed'}
 $watch=[Diagnostics.Stopwatch]::StartNew()
 & $frozenCli @Arguments 1> (Join-Path $OutputDirectory "$Name.json") 2> (Join-Path $OutputDirectory "$Name.stderr.txt")
 $code=$LASTEXITCODE; $watch.Stop()
 $commands.Add([ordered]@{name=$Name;exit_code=$code;wall_seconds=$watch.Elapsed.TotalSeconds})
 ConvertTo-Json -InputObject @($commands.ToArray()) -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'commands.json') -Encoding utf8NoBOM
 if($code -ne 0){throw "Cold verification $Name failed; retained diagnostics are authoritative"}
}
Invoke-Cli 'copy' @('copy',$Project,$copy)
$doc=Get-Content -LiteralPath (Join-Path $copy 'project.json') -Raw|ConvertFrom-Json -AsHashtable
$runs=@($doc.energy.runs)
if([string]::IsNullOrEmpty($BaselineRun) -ne [string]::IsNullOrEmpty($ProposedRun)){throw 'Supply both explicit comparison run IDs or neither'}
if($BaselineRun){
 if($runs.Count -gt 4 -or @($runs|Where-Object { $_.id -ceq $BaselineRun -and $_.alternative_id -ceq 'A' }).Count -ne 1 -or @($runs|Where-Object { $_.id -ceq $ProposedRun -and $_.alternative_id -ceq 'B' }).Count -ne 1){throw 'Select one retained A/B pair within the four-run workbench verification bound'}
 $a=$BaselineRun; $b=$ProposedRun
}else{
 if($runs.Count -ne 2 -or @($runs|Where-Object alternative_id -eq 'A').Count -ne 1 -or @($runs|Where-Object alternative_id -eq 'B').Count -ne 1){throw 'Multiple retained runs require explicit BaselineRun and ProposedRun; no favorable selection is inferred'}
 $a=($runs|Where-Object alternative_id -eq A).id; $b=($runs|Where-Object alternative_id -eq B).id
}
$snapshot=Get-EnergyPortableSnapshot $copy
$referenceMembers=Get-EnergyReferenceMembers $copy $doc
$moves=[Collections.Generic.List[object]]::new();$oldEngine=$env:GREENLENS_ENERGYPLUS_DIR
try {
 # Only known UUID-named run scratch owned by this project's retained runs.
 foreach($target in @($Project)+@($runs|ForEach-Object { $id=[guid]::Parse($_.id).ToString(); Join-Path $base "jobs/$id" })){
  $resolved=[IO.Path]::GetFullPath($target)
  if(!$resolved.StartsWith($base+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Unsafe cold verification path'}
  if(Test-Path -LiteralPath $resolved){
   $hidden=$resolved+'.unavailable-'+[guid]::NewGuid().ToString('N')
   if((Split-Path -Parent $resolved) -cne (Split-Path -Parent $hidden) -or (Test-Path -LiteralPath $hidden)){throw 'Unsafe hidden destination'}
   Move-Item -LiteralPath $resolved -Destination $hidden
   $moves.Add(@{original=$resolved;hidden=$hidden})
  }
 }
 $env:GREENLENS_ENERGYPLUS_DIR=Join-Path $OutputDirectory 'intentionally-unavailable-engine'
 & $frozenCli energy doctor 1> (Join-Path $OutputDirectory 'missing-engine.json') 2> (Join-Path $OutputDirectory 'missing-engine.stderr.txt')
 if($LASTEXITCODE -eq 0){throw 'Engine was not made unavailable to this verification process'}
 Invoke-Cli 'inspect-A' @('energy','inspect',$copy,$a)
 Invoke-Cli 'inspect-B' @('energy','inspect',$copy,$b)
 if($referenceMembers.Count){
  Invoke-Cli 'reference-inspect' @('reference','inspect',$copy)
  Invoke-Cli 'reference-scope-context' @('reference','scope-context',$copy)
  if($doc.architecture.Contains('scope') -and $null -ne $doc.architecture.scope){
   $scopePath=Join-Path $OutputDirectory 'scope-snapshot.json'
   $doc.architecture.scope|ConvertTo-Json -Depth 100|Set-Content -LiteralPath $scopePath -Encoding utf8NoBOM
   Invoke-Cli 'reference-scope-assess' @('reference','scope-assess',$copy,$scopePath)
  }
 }
 Invoke-Cli 'report' @('energy','report',$copy,$a,$b,(Join-Path $OutputDirectory 'reports'))
 $zip=Join-Path $OutputDirectory 'energy-evidence.zip'
 Invoke-Cli 'export' @('energy','export',$copy,$a,$b,$zip)
 Invoke-Cli 'package-inspect' @('energy','package-inspect',$zip)
 Assert-EnergyPortableSnapshot $copy $snapshot
 $archiveCheck=Assert-EnergyReferenceArchive $zip $referenceMembers
 $archiveCheck|ConvertTo-Json -Depth 20|Set-Content -LiteralPath (Join-Path $OutputDirectory 'reference-archive.json') -Encoding utf8NoBOM
 $cold=[ordered]@{schema='greenlens.energy-cold-portability.v1';cli_sha256=$cliHash;original_project_unavailable=$true;original_job_scratch_unavailable=$true;engine_discovery_failed=$true;retained_files=$snapshot;normalized_reference_members=$referenceMembers;reference_archive_check='reference-archive.json';report_and_export_completed=$true;package_bytes=(Get-Item -LiteralPath $zip).Length;package_sha256=(Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant();scientific_qualification_asserted=$false}
 $cold|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'cold-evidence.json') -Encoding utf8NoBOM
} finally {
 $env:GREENLENS_ENERGYPLUS_DIR=$oldEngine
 Restore-EnergyVerificationPaths $moves
}
if($Replay){Invoke-Cli 'replay' @('energy','replay',(Join-Path $OutputDirectory 'energy-evidence.zip'))}
Write-Output $OutputDirectory
