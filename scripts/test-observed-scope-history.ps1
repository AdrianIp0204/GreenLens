#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. Synthetic storage-envelope fixtures
# only; these are not accepted scope revisions or scientific engine evidence.
[CmdletBinding()]
param([string]$OutputDirectory=(Join-Path $env:LOCALAPPDATA ('GreenLens\verification\scope-observer-source-'+[Guid]::NewGuid().ToString('N'))))
$ErrorActionPreference='Stop'
$helper=Join-Path $PSScriptRoot 'observed-scope-history.ps1'
. $helper
$output=[IO.Path]::GetFullPath($OutputDirectory)
$cache=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
if(!$output.StartsWith($cache+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $output)){throw 'Use a new source-check directory below LOCALAPPDATA/GreenLens.'}
New-Item -ItemType Directory -Path $output|Out-Null
$checks=[Collections.Generic.List[object]]::new();$failure=$null
function Check([string]$Name,[bool]$Passed){if(!$Passed){throw ('Failed: '+$Name)};$checks.Add(@{name=$Name;passed=$true})}
function Reject([scriptblock]$Action,[string]$Name){$rejected=$false;try{& $Action|Out-Null}catch{$rejected=$true};Check $Name $rejected}
function Clone-Json($Value){return ,(ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject $Value -Depth 100 -Compress) -AsHashtable -DateKind String -Depth 100 -NoEnumerate)}
try{
 $id='c'*64;$other='d'*64;$root=Join-Path $output 'original fixture 空間';$folder=Join-Path $root 'scope-history';New-Item -ItemType Directory -Path $folder -Force|Out-Null
 $relative='scope-history/'+$id+'.json';$path=Join-Path $root $relative
 $revision=@{schema_version='greenlens.scope-revision.v3';revision_sha256=$id;scope=@{title='Original synthetic 假設';source='Envelope fixture; no accepted scientific record'}}
 $bytes=[Text.UTF8Encoding]::new($false).GetBytes((ConvertTo-Json -InputObject $revision -Compress -Depth 10));[IO.File]::WriteAllBytes($path,$bytes)
 $sha=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
 $entry=@{revision_sha256=$id;path=$relative;bytes=[long]$bytes.Length;sha256=$sha}
 $manifest=@{architecture=@{scope_history=@{schema_version='greenlens.scope-history.v2';entries=@($entry);active_revision_sha256=$id}}}
 $before=ConvertTo-Json -InputObject $manifest -Depth 20 -Compress
 $selected=Get-ObservedScopeSelection $root $manifest $id
 Check 'V2 selected bytes, exact hash, non-ASCII path and revision identity' ($selected.member_path -ceq $relative -and $selected.member_sha256 -ceq $sha -and $selected.member_bytes -eq $bytes.Length -and $selected.revision.scope.title -ceq 'Original synthetic 假設')
 Check 'Observation scope does not claim Rust semantic verification' ($selected.verification_scope.Contains('Rust checks') -and $selected.verification_scope.Contains('selected sidecar'))
 $legacy=@{architecture=@{scope_history=@{schema_version='greenlens.scope-history.v1';revisions=@($revision);active_revision_sha256=$id}}}
 $selectedLegacy=Get-ObservedScopeSelection '' $legacy $id
 Check 'Inline v1 remains readable without disk sidecars' ($selectedLegacy.revision.revision_sha256 -ceq $id -and $null -eq $selectedLegacy.member_path)
 Reject {Get-ObservedScopeSelection $root $manifest $other} 'Missing selected identity rejected'
 foreach($badPath in @('../outside.json','scope-history/../outside.json','C:/outside.json',('scope-history\'+$id+'.json'),('scope-history/'+$other+'.json'))){
  $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0].path=$badPath;Reject {Get-ObservedScopeSelection $root $bad $id} ('Exact-path rejection '+$badPath)
 }
 foreach($badSize in @(0,-1,32MB+1,'10',10.5,$true)){$bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0].bytes=$badSize;Reject {Get-ObservedScopeSelection $root $bad $id} ('Invalid size type/bound rejected '+$badSize)}
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0].bytes=$bytes.Length+1;Reject {Get-ObservedScopeSelection $root $bad $id} 'Actual length mismatch rejected'
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0].sha256='f'*64;Reject {Get-ObservedScopeSelection $root $bad $id} 'Actual bytes hash mismatch rejected'
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries+=,(Clone-Json $entry);Reject {Get-ObservedScopeSelection $root $bad $id} 'Duplicate descriptors rejected'
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0].bytes=16MB;$second=Clone-Json $entry;$second.revision_sha256=$other;$second.path='scope-history/'+$other+'.json';$second.bytes=16MB+1;$bad.architecture.scope_history.entries+=,$second;Reject {Get-ObservedScopeSelection $root $bad $id} 'Aggregate 32 MiB bound rejected before opening selected file'
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries=@();Reject {Get-ObservedScopeSelection $root $bad $id} 'Empty descriptor list rejected'
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries=@(1..65|ForEach-Object {$entry});Reject {Get-ObservedScopeSelection $root $bad $id} 'More than 64 descriptors rejected'
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.revisions=@($revision);Reject {Get-ObservedScopeSelection $root $bad $id} 'Mixed v2 inline and external envelope rejected'
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0].unexpected='value';Reject {Get-ObservedScopeSelection $root $bad $id} 'Unexpected descriptor fields rejected'
 foreach($field in @('revision_sha256','sha256','path')){$bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0][$field]=@($bad.architecture.scope_history.entries[0][$field]);Reject {Get-ObservedScopeSelection $root $bad $id} ('Array coercion for descriptor '+$field+' rejected')}
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.active_revision_sha256=$other;Reject {Get-ObservedScopeSelection $root $bad $id} 'Absent active revision rejected'
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.schema_version='unknown';Reject {Get-ObservedScopeSelection $root $bad $id} 'Unknown storage schema rejected'
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0].revision_sha256=$other;$bad.architecture.scope_history.entries[0].path='scope-history/'+$other+'.json';$bad.architecture.scope_history.active_revision_sha256=$other;Reject {Get-ObservedScopeSelection $root $bad $other} 'Missing external sidecar rejected'
 # Consistently resealing storage bytes cannot make the contained identity agree.
 $changed=Clone-Json $revision;$changed.revision_sha256=$other;$changedBytes=[Text.UTF8Encoding]::new($false).GetBytes((ConvertTo-Json -InputObject $changed -Compress -Depth 10));[IO.File]::WriteAllBytes($path,$changedBytes)
 $bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0].bytes=$changedBytes.Length;$bad.architecture.scope_history.entries[0].sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($changedBytes)).ToLowerInvariant();Reject {Get-ObservedScopeSelection $root $bad $id} 'Resealed different revision identity rejected'
 foreach($invalidText in @('{','[]','null',('{"schema_version":"unknown","revision_sha256":"'+$id+'"}'))){
  $invalidBytes=[Text.UTF8Encoding]::new($false).GetBytes($invalidText);[IO.File]::WriteAllBytes($path,$invalidBytes);$bad=Clone-Json $manifest;$bad.architecture.scope_history.entries[0].bytes=$invalidBytes.Length;$bad.architecture.scope_history.entries[0].sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($invalidBytes)).ToLowerInvariant();Reject {Get-ObservedScopeSelection $root $bad $id} ('Resealed malformed/unsupported content rejected '+$invalidText)
 }
 [IO.File]::WriteAllBytes($path,$bytes)
 Check 'Selection does not mutate manifest or retained sidecar' ((ConvertTo-Json -InputObject $manifest -Depth 20 -Compress) -ceq $before -and (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $sha)
 foreach($scriptName in @('supervise-v08-report-review.ps1','regenerate-v08-review-once.ps1')){
  $tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $scriptName),[ref]$tokens,[ref]$errors)
  Check ($scriptName+' parses') ($errors.Count -eq 0)
  $lookups=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -ceq 'Get-ObservedScopeSelection'},$true))
  Check ($scriptName+' explicitly uses shared history selection') ($lookups.Count -eq 1)
  Check ($scriptName+' pins the read-only helper') ($ast.Extent.Text.Contains('scope_history_helper_sha256=(File-Hash $scopeHelper)') -and $ast.Extent.Text.Contains('(File-Hash $scopeHelper) -cne $identities.scope_history_helper_sha256'))
 }
}catch{$failure=$_.Exception.Message}
finally{
 [ordered]@{schema_version='greenlens.scope-history-observer-source-checks.v1';helper_sha256=(Get-FileHash -LiteralPath $helper -Algorithm SHA256).Hash.ToLowerInvariant();test_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant();checks=$checks.ToArray();passed=$checks.Count;failure=$failure;native_or_renderer_executions=0;process_api_calls=0;completed_utc=[DateTime]::UtcNow.ToString('o')}|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $output 'checks.json') -Encoding utf8NoBOM
}
if($failure){throw "Source checks failed: $failure; receipt: $output"}
Write-Host "$($checks.Count) read-only history checks passed; receipt: $output"
