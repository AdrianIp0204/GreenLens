#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. Read-only observation helper. This is
# not the authoritative Rust revision/provenance verifier and launches nothing.
function Get-ObservedScopeSelection([string]$Root,$Manifest,[string]$RevisionSha256){
 if($RevisionSha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'The observed scope requires an exact revision identity.'}
 $history=$Manifest.architecture.scope_history
 if($history -isnot [Collections.IDictionary] -or $history.schema_version -isnot [string]){throw 'The observed project has no supported scope history envelope.'}
 if($history.schema_version -ceq 'greenlens.scope-history.v1'){
  if($history.Contains('entries') -or $history.revisions -isnot [array] -or $history.revisions.Count -lt 1 -or $history.revisions.Count -gt 64){throw 'Invalid inline scope history envelope.'}
  $rows=@($history.revisions|Where-Object {$_.revision_sha256 -ceq $RevisionSha256})
  if($rows.Count -ne 1){throw 'The observed scope must occur exactly once in inline history.'}
  return [ordered]@{revision=$rows[0];storage_schema=$history.schema_version;member_path=$null;member_bytes=$null;member_sha256=$null;verification_scope='Inline manifest selection only; Rust checks revision semantics separately.'}
 }
 if($history.schema_version -cne 'greenlens.scope-history.v2' -or $history.Count -ne 3 -or !$history.Contains('entries') -or !$history.Contains('active_revision_sha256') -or $history.entries -isnot [array] -or $history.entries.Count -lt 1 -or $history.entries.Count -gt 64){throw 'Invalid or unsupported external scope history envelope.'}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal);[long]$total=0;$selected=$null
 foreach($entry in $history.entries){
  if($entry -isnot [Collections.IDictionary] -or $entry.Count -ne 4 -or !$entry.Contains('revision_sha256') -or !$entry.Contains('path') -or !$entry.Contains('bytes') -or !$entry.Contains('sha256') -or $entry.revision_sha256 -isnot [string] -or $entry.sha256 -isnot [string] -or $entry.path -isnot [string] -or $entry.revision_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $entry.sha256 -cnotmatch '^[a-f0-9]{64}$' -or $entry.path -cne ('scope-history/'+$entry.revision_sha256+'.json') -or ($entry.bytes -isnot [int] -and $entry.bytes -isnot [long]) -or $entry.bytes -le 0 -or $entry.bytes -gt 32MB -or !$seen.Add($entry.revision_sha256)){throw 'Invalid, duplicate or oversized external scope descriptor.'}
  $total+=$entry.bytes;if($total -gt 32MB){throw 'Complete external scope history exceeds its 32 MiB bound.'}
  if($entry.revision_sha256 -ceq $RevisionSha256){$selected=$entry}
 }
 if($null -ne $history.active_revision_sha256 -and ($history.active_revision_sha256 -isnot [string] -or !$seen.Contains($history.active_revision_sha256))){throw 'Active scope is absent from the external history inventory.'}
 if($null -eq $selected){throw 'The observed scope is absent from external history.'}
 $rootPath=[IO.Path]::GetFullPath($Root).TrimEnd('\','/');$cursor=$rootPath
 while($cursor){$item=Get-Item -LiteralPath $cursor -Force;if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Reparse points are outside scope-history observation.'};$parent=[IO.Directory]::GetParent($cursor);if($null -eq $parent){break};$cursor=$parent.FullName}
 $path=$rootPath
 foreach($segment in $selected.path.Split('/')){$path=Join-Path $path $segment;$item=Get-Item -LiteralPath $path -Force;if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Reparse points are outside scope-history observation.'}}
 if($item.PSIsContainer -or $item.Length -ne $selected.bytes){throw 'Observed scope-history length differs from its descriptor.'}
 # Hash and decode the bytes from one held handle. Share deletion so this
 # read-only observer does not prevent the application's atomic publication.
 $stream=[IO.FileStream]::new($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
 try{
  if($stream.Length -ne $selected.bytes){throw 'Scope-history length changed before observation.'}
  $bytes=[byte[]]::new([int]$selected.bytes);$stream.ReadExactly($bytes,0,$bytes.Length)
  if($stream.ReadByte() -ne -1){throw 'Scope-history length changed during observation.'}
  $digest=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
  if($digest -cne $selected.sha256){throw 'Observed scope-history bytes differ from the manifest digest.'}
  $text=[Text.UTF8Encoding]::new($false,$true).GetString($bytes)
  $revision=ConvertFrom-Json -InputObject $text -AsHashtable -DateKind String -Depth 100 -NoEnumerate
 }finally{$stream.Dispose()}
 if($revision -isnot [Collections.IDictionary] -or $revision.revision_sha256 -isnot [string] -or $revision.schema_version -isnot [string] -or $revision.revision_sha256 -cne $RevisionSha256 -or $revision.schema_version -cnotin @('greenlens.scope-revision.v1','greenlens.scope-revision.v2','greenlens.scope-revision.v3')){throw 'Observed scope-history content does not identify the declared revision.'}
 return [ordered]@{revision=$revision;storage_schema=$history.schema_version;member_path=$selected.path;member_bytes=$selected.bytes;member_sha256=$digest;verification_scope='Exact selected sidecar bytes and bounded descriptor inventory only; Rust checks historical identities, provenance and cross-bindings separately.'}
}
