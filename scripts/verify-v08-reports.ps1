# Finite original synthetic presentation protocol; no simulation commands.
# Run with native PowerShell 7. All data/receipts remain in the supplied external
# LOCALAPPDATA directory. A failed attempt is retained and never auto-retried.
# The explicit renderer-path repair is a separately authorized engineering
# attempt for a proven same-file Windows alias, not a reset of the first render
# or a scientific/profile retry. It is allowed once, with both receipts kept.
[CmdletBinding(DefaultParameterSetName='Run')]
param(
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Cli,
 [Parameter(Mandatory,ParameterSetName='Run')][ValidatePattern('^[a-fA-F0-9]{40}$')][string]$CliSourceCommit,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Project,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$BaselineRun,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$ProposedRun,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$TypstDirectory,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$OutputDirectory,
 [Parameter(ParameterSetName='Run')][string]$LegacyProject,
 [Parameter(ParameterSetName='Run')][ValidateCount(0,2)][ValidatePattern('^[a-f0-9]{64}$')][string[]]$LegacyIssueIds=@(),
 # A collector-only repair may resume the verified pre-issuance checkpoint.
 # This never resets an issue/regeneration allowance or retries a failed render.
 [Parameter(ParameterSetName='Run')][switch]$ResumeVerifiedPreparation,
 [Parameter(ParameterSetName='Run')][switch]$ResumeVerifiedReview,
 [Parameter(ParameterSetName='Run')][switch]$ResumeVerifiedDecisions,
 [Parameter(ParameterSetName='Run')][switch]$ResumeVerifiedRendererPath,
 [Parameter(ParameterSetName='Run')][ValidatePattern('^[a-f0-9-]{36}$')][string]$AbortedRendererStageId,
 [Parameter(Mandatory,ParameterSetName='Functions')][switch]$FunctionsOnly
)

# Reuse the existing inspected JSON, hash, snapshot and finite-budget helpers.
# Its FunctionsOnly branch returns before any process or filesystem mutation.
# Isolate its parameter bindings: ordinary dot-sourcing would overwrite this
# collector's Cli/Project/FunctionsOnly variables with the helper's defaults.
$helperModule=New-Module -Name GreenLensV08ReportCollectorHelpers -ArgumentList (Join-Path $PSScriptRoot 'verify-study-issues.ps1') -ScriptBlock {
 param($helperPath)
 . $helperPath -FunctionsOnly
 Export-ModuleMember -Function Read-IssueJson,Write-IssueJson,Get-IssueHash,Assert-IssueChild,Get-IssueTreeBytes,Get-IssueSnapshot,Assert-IssueSnapshot,Test-IssueJsonEqual,Copy-IssueJson,Assert-IssueExecutionBudgets,Wait-IssueStreamTask
}
Import-Module -ModuleInfo $helperModule -Scope Local

function ConvertTo-ReportExecutablePath([string]$Path) {
 if([string]::IsNullOrWhiteSpace($Path)){return $null}
 if($Path.StartsWith('\\?\',[StringComparison]::Ordinal)){
  if($Path.StartsWith('\\?\UNC\',[StringComparison]::Ordinal)){$Path='\\'+$Path.Substring(8)}
  elseif($Path -cmatch '^\\\\\?\\[A-Za-z]:\\'){$Path=$Path.Substring(4)}
  else{return $null}
  # Extended paths do not have ordinary Win32 dot/trailing-space semantics.
  # Admit only a plain DOS/UNC alias, never reinterpret a special component.
  foreach($part in $Path.Split('\')){if($part -in @('.','..') -or $part.EndsWith('.') -or $part.EndsWith(' ')){return $null}}
 }
 if($Path.StartsWith('\\.\',[StringComparison]::Ordinal)){return $null}
 try{if([IO.Path]::IsPathFullyQualified($Path)){return [IO.Path]::GetFullPath($Path)}}catch{return $null}
 return $null
}
function Test-ReportChildAllowed([string]$Name,[string]$ExecutablePath,[string]$SystemConhostPath,[string]$DeclaredTypstPath,[bool]$AllowTypst) {
 # Pure admission of an actual CIM path, not a process-name wildcard. Missing
 # permission/path information is an explicit failure rather than a bypass.
 try{
  $actual=ConvertTo-ReportExecutablePath $ExecutablePath
  if($null -eq $actual){return $false}
  if($Name -ieq 'conhost.exe'){
   $expected=ConvertTo-ReportExecutablePath $SystemConhostPath
   return $null -ne $expected -and $actual.Equals($expected,[StringComparison]::OrdinalIgnoreCase)
  }
  if($AllowTypst -and $Name -ieq 'typst.exe'){
   $expected=ConvertTo-ReportExecutablePath $DeclaredTypstPath
   return $null -ne $expected -and $actual.Equals($expected,[StringComparison]::OrdinalIgnoreCase)
  }
 }catch{return $false}
 return $false
}

function Get-ReportChildDecision([string]$Name,[string]$CimPath,[string]$ProbeState,[string]$ProbePath,[string]$SystemConhostPath,[string]$DeclaredTypstPath,[bool]$AllowTypst) {
 # An exited child with an unobserved path is not retrospectively identified.
 # Only the known console/allowed renderer names may take that narrow branch;
 # an observed engine or arbitrary executable remains a failed protocol.
 if(![string]::IsNullOrWhiteSpace($CimPath)){
  return @{allowed=(Test-ReportChildAllowed $Name $CimPath $SystemConhostPath $DeclaredTypstPath $AllowTypst);identity_status='cim_path_observed';path=$CimPath}
 }
 $knownName=$Name -ieq 'conhost.exe' -or ($AllowTypst -and $Name -ieq 'typst.exe')
 if($knownName -and $ProbeState -ceq 'exited'){
  return @{allowed=$true;identity_status='proven_exited_path_unobserved';path=$null}
 }
 if($knownName -and $ProbeState -ceq 'live'){
  return @{allowed=(Test-ReportChildAllowed $Name $ProbePath $SystemConhostPath $DeclaredTypstPath $AllowTypst);identity_status='live_handle_path_observed';path=$ProbePath}
 }
 return @{allowed=$false;identity_status='identity_unavailable_rejected';path=$null}
}

function Get-ReportChildHandleIdentity([int]$ProcessId,[DateTime]$ExpectedCreation) {
 if(-not ('GreenLensReportChildProbeV1' -as [type])){
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public sealed class GreenLensReportChildIdentityV1 {
 public string State = "unknown";
 public string ImagePath;
 public int Error;
 public long CreationUtcTicks;
}

public static class GreenLensReportChildProbeV1 {
 [StructLayout(LayoutKind.Sequential)] private struct FileTime { public uint Low; public uint High; }
 [DllImport("kernel32.dll", SetLastError=true)] private static extern IntPtr OpenProcess(uint access, bool inherit, int id);
 [DllImport("kernel32.dll", SetLastError=true)] private static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll", SetLastError=true)] private static extern uint WaitForSingleObject(IntPtr h, uint ms);
 [DllImport("kernel32.dll", SetLastError=true)] private static extern bool GetProcessTimes(IntPtr h, out FileTime creation, out FileTime exit, out FileTime kernel, out FileTime user);
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode, ExactSpelling=true, SetLastError=true)] private static extern bool QueryFullProcessImageNameW(IntPtr h, uint flags, StringBuilder name, ref uint count);
 public static GreenLensReportChildIdentityV1 Read(int id, long expectedTicks) {
  var result = new GreenLensReportChildIdentityV1();
  var handle = OpenProcess(0x00100000 | 0x1000, false, id); // SYNCHRONIZE | QUERY_LIMITED_INFORMATION
  if (handle == IntPtr.Zero) { result.Error=Marshal.GetLastWin32Error(); if(result.Error==87) result.State="exited"; return result; }
  try {
   if(WaitForSingleObject(handle,0)==0) { result.State="exited"; return result; }
   FileTime creation, exit, kernel, user;
   if(!GetProcessTimes(handle,out creation,out exit,out kernel,out user)) { result.Error=Marshal.GetLastWin32Error(); return result; }
   long ft=((long)creation.High<<32)|creation.Low;
   result.CreationUtcTicks=DateTime.FromFileTimeUtc(ft).Ticks;
   // CIM timestamps have microsecond precision. A recycled ID is not an
   // identified child and is rejected instead of authorizing its pathname.
   if(result.CreationUtcTicks/10 != expectedTicks/10) { result.State="reused_or_changed_identity"; return result; }
   var text=new StringBuilder(32768); uint length=32768;
   if(QueryFullProcessImageNameW(handle,0,text,ref length)) { result.State="live"; result.ImagePath=text.ToString(); return result; }
   result.Error=Marshal.GetLastWin32Error();
   if(WaitForSingleObject(handle,0)==0) result.State="exited";
   return result;
  } finally { CloseHandle(handle); }
 }
}
'@
 }
 return [GreenLensReportChildProbeV1]::Read($ProcessId,$ExpectedCreation.ToUniversalTime().Ticks)
}

function Open-ReportPinnedFile([string]$Path) {
 if(-not ('GreenLensReportPinnedFileV1' -as [type])){
  Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using Microsoft.Win32.SafeHandles;
public sealed class GreenLensReportPinnedFileV1 : IDisposable {
 [StructLayout(LayoutKind.Sequential)] private struct Info { public uint Attributes,CreationLow,CreationHigh,AccessLow,AccessHigh,WriteLow,WriteHigh,Volume,SizeHigh,SizeLow,Links,IndexHigh,IndexLow; }
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,ExactSpelling=true,SetLastError=true)] private static extern uint GetFinalPathNameByHandleW(SafeFileHandle h,StringBuilder path,uint count,uint flags);
 [DllImport("kernel32.dll",SetLastError=true)] private static extern bool GetFileInformationByHandle(SafeFileHandle h,out Info info);
 private FileStream file;
 public string RequestedPath,FinalPath,VolumeSerial,FileId,Sha256;
 public long Bytes;
 public GreenLensReportPinnedFileV1(string path) {
  file=new FileStream(path,FileMode.Open,FileAccess.Read,FileShare.Read);
  try {
   RequestedPath=path;var name=new StringBuilder(32768);uint n=GetFinalPathNameByHandleW(file.SafeFileHandle,name,32768,0);
   if(n==0||n>=32768)throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
   Info info;if(!GetFileInformationByHandle(file.SafeFileHandle,out info))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
   FinalPath=name.ToString();VolumeSerial=info.Volume.ToString("x8");FileId=info.IndexHigh.ToString("x8")+info.IndexLow.ToString("x8");Bytes=file.Length;
   Sha256=Convert.ToHexString(SHA256.HashData(file)).ToLowerInvariant();
  } catch { file.Dispose();throw; }
 }
 public void Dispose() { file.Dispose(); }
}
'@
 }
 return [GreenLensReportPinnedFileV1]::new($Path)
}
function Get-ReportPinnedIdentity($Lease) {
 $final=ConvertTo-ReportExecutablePath $Lease.FinalPath
 if($null -eq $final){throw 'Pinned image has no supported plain DOS/UNC physical pathname'}
 return @{requested_path=$Lease.RequestedPath;physical_path=$final;volume_serial=$Lease.VolumeSerial;file_id=$Lease.FileId;bytes=$Lease.Bytes;sha256=$Lease.Sha256}
}
function Test-ReportSamePinnedFile($A,$B,[string]$ExpectedHash) {
 return $null -ne $A -and $null -ne $B -and $ExpectedHash -cmatch '^[a-f0-9]{64}$' -and $A.sha256 -ceq $ExpectedHash -and $B.sha256 -ceq $ExpectedHash -and $A.volume_serial -ceq $B.volume_serial -and $A.file_id -ceq $B.file_id -and $A.bytes -eq $B.bytes -and $A.bytes -gt 0 -and [IO.Path]::IsPathFullyQualified($A.physical_path) -and $A.physical_path.Equals($B.physical_path,[StringComparison]::OrdinalIgnoreCase)
}
function Get-ReportPinnedChildDecision([string]$Name,[string]$CimPath,[string]$ProbeState,[string]$ProbePath,[string]$SystemConhostPath,[string]$DeclaredTypstPath,[bool]$AllowTypst,$PinnedIdentity) {
 $decision=Get-ReportChildDecision $Name $CimPath $ProbeState $ProbePath $SystemConhostPath $DeclaredTypstPath $AllowTypst
 if(!$decision.allowed -and $AllowTypst -and $Name -ieq 'typst.exe' -and $null -ne $PinnedIdentity){
  $actual=if(![string]::IsNullOrWhiteSpace($CimPath)){$CimPath}elseif($ProbeState -ceq 'live'){$ProbePath}else{$null}
  if(Test-ReportChildAllowed $Name $actual $SystemConhostPath $PinnedIdentity.physical_path $true){return @{allowed=$true;identity_status='held_handle_verified_pinned_physical_alias';path=$actual}}
 }
 return $decision
}
function Assert-ReportPreparationResume($State,$Declaration,$Current,$Copied,$ExpectedEnergy) {
 if($State.completed -or $State.new_issue_attempt_started -or $State.legacy_regeneration_attempts.Count){throw 'Resume cannot reset any report/regeneration attempt'}
 $expected=@('complete-save-as','verify-run-A','verify-run-B','verify-compatible-pair','adopt-old-scope')
 if(!(Test-IssueJsonEqual @($State.commands|ForEach-Object {$_.name}) $expected)){throw 'Only the declared pre-issuance adoption checkpoint can resume'}
 if($Declaration.new_issue_attempts -ne 1 -or $Declaration.legacy_regeneration_attempts_per_selected_issue -ne 1 -or $Declaration.maximum_energyplus_processes -ne 0 -or $Declaration.maximum_seconds_per_command -ne 600){throw 'Resume cannot change finite protocol allowances'}
 if(!(Test-IssueJsonEqual $Current.energy $ExpectedEnergy)){throw 'Accepted energy inputs or retained run identities changed'}
 $actual=Copy-IssueJson $Current;$expectedCopy=Copy-IssueJson $Copied
 if(!$actual.architecture.Contains('scope_history') -or $null -eq $actual.architecture.scope_history -or $actual.architecture.scope_history.revisions.Count -ne 1){throw 'Resume requires exactly the accepted initial legacy scope'}
 $history=$actual.architecture.scope_history
 if($null -ne $history.revisions[0].parent_revision_sha256 -or $history.active_revision_sha256 -cne $history.revisions[0].revision_sha256 -or !(Test-IssueJsonEqual $history.revisions[0].scope $actual.architecture.scope)){throw 'Unexpected scope-history change at resume'}
 $actual.architecture.Remove('scope_history')|Out-Null;$expectedCopy.architecture.Remove('scope_history')|Out-Null
 if(!(Test-IssueJsonEqual $actual $expectedCopy)){throw 'Working copy changed beyond explicit legacy-scope adoption'}
}

function Get-ReportScopeWireForm($Scope) {
 $wire=Copy-IssueJson $Scope
 # ArchitecturalScope.report_views alone has serde(default,
 # skip_serializing_if = Vec::is_empty). Do not drop other fields or nulls.
 if($wire.Contains('report_views') -and $wire.report_views -is [array] -and $wire.report_views.Count -eq 0){$wire.Remove('report_views')|Out-Null}
 return $wire
}
function Assert-ReportFixtureReview($Plan,$Scope,[string]$Parent) {
 if($Plan.schema_version -cne 'greenlens.scope-carry-forward.v2' -or $Scope.schema_version -cne 'greenlens.architectural-scope.v2'){throw 'Explicit fixture review requires the declared v2 contracts'}
 if($Parent -cnotmatch '^[a-f0-9]{64}$' -or $Plan.parent_revision_sha256 -cne $Parent -or $Plan.target_context_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $Plan.plan_sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'Review parent or identity differs from the declared context'}
 # The mechanical proposal is intentionally retained separately. Only the
 # explicit candidate is the submitted fixture; never silently substitute it.
 if(!$Plan.Contains('candidate_scope') -or $null -eq $Plan.candidate_scope -or !(Test-IssueJsonEqual (Get-ReportScopeWireForm $Plan.candidate_scope) (Get-ReportScopeWireForm $Scope))){throw 'Rust reviewed a different explicit candidate; stop rather than accepting it silently'}
 if($null -eq $Plan.proposed_scope -or $Plan.proposed_scope.schema_version -cne 'greenlens.architectural-scope.v2'){throw 'Review omitted its separately preserved mechanical proposal'}
 if(!$Plan.rows.Count){throw 'The declared reference replacement requires explicit review rows'}
 foreach($row in $Plan.rows){if($row.kind -eq 'input' -and $row.change -ne 'unchanged'){throw 'This report fixture must not change any accepted scientific input'}}
}

function Assert-ReportReviewResume($State,$Declaration,$Current,$Copied,$Imported,$Manifest,$Context,$Scope,$Plan,$ExpectedEnergy,[string]$FixtureHash) {
 if($State.completed -or $State.new_issue_attempt_started -or $State.legacy_regeneration_attempts.Count){throw 'Review resume cannot reset any report/regeneration attempt'}
 $expected=@('complete-save-as','verify-run-A','verify-run-B','verify-compatible-pair','adopt-old-scope','resume-verified-open','reference-capacity','import-original-fixture','inspect-original-reference','scope-context','assess-fixture-scope','review-fixture-scope')
 if(!(Test-IssueJsonEqual @($State.commands|ForEach-Object {$_.name}) $expected)){throw 'Only the completed preparation-to-review checkpoint can resume'}
 if($Declaration.new_issue_attempts -ne 1 -or $Declaration.legacy_regeneration_attempts_per_selected_issue -ne 1 -or $Declaration.maximum_energyplus_processes -ne 0 -or $Declaration.maximum_seconds_per_command -ne 600){throw 'Review resume cannot change finite allowances'}
 if(!(Test-IssueJsonEqual $Current.energy $ExpectedEnergy) -or !(Test-IssueJsonEqual $Current.architecture $Imported)){throw 'Working energy or imported reference state changed after preparation'}
 $expectedDocument=Copy-IssueJson $Copied;$expectedDocument.architecture=Copy-IssueJson $Imported
 if(!(Test-IssueJsonEqual $Current $expectedDocument)){throw 'Working document changed beyond the declared imported reference'}
 $history=$Current.architecture.scope_history
 if($null -eq $history -or $history.revisions.Count -ne 1 -or $null -ne $history.revisions[0].parent_revision_sha256 -or $history.active_revision_sha256 -cne $history.revisions[0].revision_sha256 -or !(Test-IssueJsonEqual $history.revisions[0].scope $Copied.architecture.scope) -or !(Test-IssueJsonEqual $Current.architecture.scope $Copied.architecture.scope)){throw 'Review resume requires the unchanged accepted initial parent scope'}
 $oldAssets=@($Imported.assets|Where-Object id -CNE $Imported.active_asset_id)
 $active=@($Imported.assets|Where-Object id -CEQ $Imported.active_asset_id)
 if($active.Count -ne 1 -or !(Test-IssueJsonEqual $oldAssets @($Copied.architecture.assets))){throw 'Reference import changed old assets or added undeclared assets'}
 if($Manifest.source_sha256 -cne $FixtureHash -or $Manifest.occurrences.Count -ne 81 -or $active[0].revision_sha256 -cne $Manifest.revision_sha256 -or $active[0].geometry_sha256 -cne $Manifest.geometry_sha256){throw 'Imported active reference does not bind the declared original fixture'}
 $expectedScope=New-ReportFixtureScope $Manifest $Context $Current
 if(!(Test-IssueJsonEqual $Scope $expectedScope)){throw 'Retained fixture candidate differs from its declared generator and context'}
 Assert-ReportFixtureReview $Plan $Scope $history.active_revision_sha256
}

function New-ReportFixtureDecisions($Plan,$Scope) {
 Assert-ReportFixtureReview $Plan $Scope $Plan.parent_revision_sha256
 $decisions=@()
 foreach($row in $Plan.rows){
  if(!$row.requires_review){continue}
  if($row.kind -notin @('reference','input','mapping','assumption','reconciliation','report_view','scope')){throw 'Fixture decisions do not recognize this review row kind'}
  $resolution='reviewed';$detail='Explicitly reviewed addition/removal or replacement; no source equivalence or qualification is inferred.'
  if($row.kind -eq 'mapping'){
   $matches=@($Scope.mappings|Where-Object occurrence_id -CEQ $row.key)
   if($matches.Count -gt 1){throw 'Ambiguous fixture mapping review'}
   if($matches.Count -and $matches[0].status -ceq 'unresolved'){$resolution='unresolved';$detail='This candidate mapping intentionally remains unresolved.'}
  }elseif($row.kind -eq 'assumption'){
   $matches=@($Scope.assumptions|Where-Object id -CEQ $row.key)
   if($matches.Count -gt 1){throw 'Ambiguous fixture assumption review'}
   if($matches.Count -and $matches[0].status -ceq 'missing'){$resolution='unresolved';$detail='This candidate source information intentionally remains missing.'}
   elseif($matches.Count){$detail='The candidate explicitly retains this synthetic or assumed source declaration. Contradictory COP declarations remain conflicting; reviewed does not mean verified, resolved or applied.'}
  }
  $decisions+=@(@{row_id=$row.id;resolution=$resolution;rationale=('Original report fixture: '+$detail+' Preserve prior history and all actual scientific inputs; no physical qualification is asserted.')})
 }
 return ,$decisions
}

function Assert-ReportDecisionsResume($State,$Declaration,$Current,$Copied,$Imported,$Manifest,$Context,$Scope,$Plan,$ExpectedEnergy,[string]$FixtureHash,$ReviewRepair,$FailedReceipt,[string]$FailedError,[string]$ManifestHash) {
 $expected=@('complete-save-as','verify-run-A','verify-run-B','verify-compatible-pair','adopt-old-scope','resume-verified-open','reference-capacity','import-original-fixture','inspect-original-reference','scope-context','assess-fixture-scope','review-fixture-scope','resume-review-fixture-scope','accept-reviewed-fixture-scope')
 if(!(Test-IssueJsonEqual @($State.commands|ForEach-Object {$_.name}) $expected)){throw 'Only the exact failed decision-acceptance checkpoint can resume'}
 $preparation=Copy-IssueJson $State;$preparation.commands=@($preparation.commands|Select-Object -First 12)
 Assert-ReportReviewResume $preparation $Declaration $Current $Copied $Imported $Manifest $Context $Scope $Plan $ExpectedEnergy $FixtureHash
 if($ReviewRepair.schema_version -cne 'greenlens.v08-collector-review-repair.v1' -or $ReviewRepair.working_manifest_sha256 -cne $ManifestHash -or $ReviewRepair.prior_plan_sha256 -cne $Plan.plan_sha256){throw 'Working manifest or reviewed plan changed after the prior repair fence'}
 if($FailedReceipt.exit_code -ne 1 -or $null -ne $FailedReceipt.failure -or !$FailedError.TrimEnd().EndsWith('GreenLens: revision.review: An unresolved decision cannot be presented as a resolved mapping, measurement, view or source declaration',[StringComparison]::Ordinal)){throw 'Only the retained decision classification error is eligible for this repair'}
}

function Assert-ReportRendererResume($State,$Declaration,$Current,$Copied,$Imported,$History,$Scope,$Plan,$ExpectedEnergy,$StageContent,$FailedReceipt,$PinnedIdentity) {
 $expected=@('complete-save-as','verify-run-A','verify-run-B','verify-compatible-pair','adopt-old-scope','resume-verified-open','reference-capacity','import-original-fixture','inspect-original-reference','scope-context','assess-fixture-scope','review-fixture-scope','resume-review-fixture-scope','accept-reviewed-fixture-scope','accept-reviewed-fixture-scope-corrected','issue-capacity','issue-once')
 if(!(Test-IssueJsonEqual @($State.commands|ForEach-Object {$_.name}) $expected) -or $State.completed -or !$State.new_issue_attempt_started -or $State.legacy_regeneration_attempts.Count -or ($State.Contains('renderer_path_repair_attempt_started') -and $State.renderer_path_repair_attempt_started)){throw 'Only the once-aborted renderer-alias checkpoint is eligible; never reset an attempt'}
 if($Declaration.new_issue_attempts -ne 1 -or $Declaration.legacy_regeneration_attempts_per_selected_issue -ne 1 -or $Declaration.maximum_energyplus_processes -ne 0 -or $Declaration.maximum_seconds_per_command -ne 600){throw 'Renderer repair cannot change original protocol allowances'}
 if($Current.Contains('report_issues') -and $null -ne $Current.report_issues){throw 'A retained issue exists; inspect it instead of rendering again'}
 if(!(Test-IssueJsonEqual $Current.energy $ExpectedEnergy) -or !(Test-IssueJsonEqual $Current.architecture.scope_history $History) -or $History.revisions.Count -ne 2){throw 'Accepted energy or scope history differs from completed preparation'}
 $first=$History.revisions[0];$last=$History.revisions[1]
 if(!(Test-IssueJsonEqual $first $Imported.scope_history.revisions[0]) -or $last.parent_revision_sha256 -cne $first.revision_sha256 -or $History.active_revision_sha256 -cne $last.revision_sha256 -or !(Test-IssueJsonEqual (Get-ReportScopeWireForm $last.scope) (Get-ReportScopeWireForm $Scope))){throw 'Accepted report scope or immutable parent differs'}
 Assert-ReportFixtureReview $Plan $Scope $first.revision_sha256
 $expectedDocument=Copy-IssueJson $Copied;$expectedDocument.architecture=Copy-IssueJson $Imported;$expectedDocument.architecture.scope_history=Copy-IssueJson $History;$expectedDocument.architecture.scope=Copy-IssueJson $last.scope
 if(!(Test-IssueJsonEqual $Current $expectedDocument)){throw 'Working document changed beyond the declared accepted fixture scope'}
 if(!(Test-IssueJsonEqual $StageContent.scope_revision $last) -or !$StageContent.include_pdf -or $StageContent.renderer.method -cne 'greenlens.energy-report-issue-renderer/2'){throw 'Aborted products do not bind the exact selected scope and renderer'}
 foreach($pair in @(@('baseline','A'),@('proposed','B'))){
  $case=$StageContent.workbench.base[$pair[0]]
  if($case.run_id -cne $Declaration[$pair[0]].id -or !(Test-IssueJsonEqual $case.model $Current.energy.designs[$pair[1]].model) -or !(Test-IssueJsonEqual $case.study $Current.energy.designs[$pair[1]].study)){throw 'Aborted products differ from the declared genuine pair or accepted inputs'}
 }
 $denied=@($FailedReceipt.observed_children|Where-Object {$_.policy_allowed -eq $false})
 if($null -ne $FailedReceipt.exit_code -or $FailedReceipt.failure -cne 'An undeclared child executable started; only the canonical Windows console host and explicitly allowed pinned Typst path are permitted, never an engine' -or !$FailedReceipt.request.allow_typst -or $denied.Count -ne 1 -or $denied[0].name -ine 'typst.exe' -or !(Test-ReportChildAllowed 'typst.exe' $denied[0].executable_path '' $PinnedIdentity.physical_path $true)){throw 'Aborted command is not the proven pinned renderer-path alias failure'}
}
function Test-ReportPublishedHistoricalStage($Pending,$Legacy,[string[]]$SelectedIds,[string]$CurrentScope) {
 if($null -eq $Legacy -or !$Legacy.Contains('report_issues') -or $null -eq $Legacy.report_issues){return $false}
 if($Pending.schema_version -cne 'greenlens.staged-report-issue.v1' -or $Pending.project_id -cne $Legacy.id -or $Pending.summary.id -cnotin $SelectedIds -or $Pending.summary.scope_revision_sha256 -ceq $CurrentScope){return $false}
 $matches=@($Legacy.report_issues.issues|Where-Object id -CEQ $Pending.summary.id)
 return $matches.Count -eq 1 -and (Test-IssueJsonEqual $Pending.summary $matches[0])
}

function New-ReportFixtureGlb([string]$Path) {
 $nodes=@(0..80|ForEach-Object {[ordered]@{name=('Synthetic component {0:D3} - original synthetic geometry' -f $_);mesh=0;translation=@($_,0,0)}})
 $json=[ordered]@{
  asset=@{version='2.0';generator='GreenLens original bounded report fixture v1'}
  buffers=@(@{byteLength=36});bufferViews=@(@{buffer=0;byteLength=36})
  accessors=@(@{bufferView=0;componentType=5126;count=3;type='VEC3';min=@(0,0,0);max=@(1,1,0)})
  meshes=@(@{primitives=@(@{attributes=@{POSITION=0}})});nodes=$nodes
  scenes=@(@{nodes=@(0..80)});scene=0
 }
 $utf8=[Text.UTF8Encoding]::new($false)
 $text=ConvertTo-Json -InputObject $json -Depth 20 -Compress
 while(($utf8.GetByteCount($text)%4)-ne 0){$text+=' '}
 $data=$utf8.GetBytes($text);$stream=[IO.MemoryStream]::new();$writer=[IO.BinaryWriter]::new($stream)
 try{
  foreach($value in @([uint32]0x46546c67,[uint32]2,[uint32](28+$data.Length+36),[uint32]$data.Length,[uint32]0x4e4f534a)){$writer.Write($value)}
  $writer.Write($data);$writer.Write([uint32]36);$writer.Write([uint32]0x004e4942)
  foreach($value in @(0,0,0,1,0,0,0,1,0)){$writer.Write([single]$value)}
  $writer.Flush();[IO.File]::WriteAllBytes($Path,$stream.ToArray())
 }finally{$writer.Dispose();$stream.Dispose()}
}
function Assert-ReportTreeBounds([string]$Root) {
 [long]$bytes=0;$count=0
 if((Get-Item -LiteralPath $Root).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Protocol root cannot be a reparse point'}
 foreach($entry in Get-ChildItem -LiteralPath $Root -Recurse -Force){
  if($entry.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Protocol input/output cannot contain reparse points'}
  if($entry.PSIsContainer){continue}
  if($entry.Length -gt 64MB){throw 'Project member exceeds the unchanged 64 MiB entry limit'}
  $bytes+=$entry.Length;$count++
  if($count -gt 5000 -or $bytes -gt 256MB){throw 'Complete project exceeds the unchanged archive bounds; no history is pruned'}
 }
 return [ordered]@{entries=$count;bytes=$bytes;maximum_entries=5000;maximum_entry_bytes=64MB;maximum_uncompressed_bytes=256MB}
}
function Get-ReportArtifactSnapshot([string]$Root) {
 $null=Assert-ReportTreeBounds $Root
 $snapshot=[ordered]@{}
 foreach($file in Get-ChildItem -LiteralPath $Root -File -Recurse -Force|Sort-Object FullName){
  $path=[IO.Path]::GetRelativePath($Root,$file.FullName).Replace('\','/');Assert-IssueChild $Root $file.FullName
  $snapshot[$path]=Get-IssueHash $file.FullName
 }
 return $snapshot
}
function Assert-ReportArtifactSnapshot([string]$Root,$Expected) {
 if(!(Test-IssueJsonEqual (Get-ReportArtifactSnapshot $Root) $Expected)){throw 'Retained artifact member names or actual bytes changed'}
}
function Complete-ReportCollection($State,$Result,$FinalChecks,[scriptblock]$PublishResult,[scriptblock]$PublishState) {
 if($null -eq $Result -or !@($FinalChecks).Count -or @($FinalChecks|Where-Object status -CNE 'passed').Count){throw 'Completion requires successful final actual-byte evidence checks'}
 & $PublishResult $Result
 $completed=Copy-IssueJson $State;$completed.completed=$true
 & $PublishState $completed
 return $completed
}
function New-ReportFixtureScope($Manifest,$Context,$Document) {
 if($Manifest.occurrences.Count -ne 81){throw 'The declared fixture requires exactly 81 admitted occurrences'}
 $mappings=[Collections.Generic.List[object]]::new();$assumptions=[Collections.Generic.List[object]]::new()
 for($i=0;$i -lt 81;$i++){
  $id=$Manifest.occurrences[$i].id
  $status=if($i -eq 80){'unresolved'}elseif($i -eq 79){'excluded'}else{'context_only'}
  $mappings.Add([ordered]@{occurrence_id=$id;status=$status;targets=@();rationale="Original explicit report disposition row $i; this triangle is not a thermal zone"})
  $assumptions.Add([ordered]@{id=('source-{0:D3}' -f $i);link=@{kind='reference_occurrence';occurrence_id=$id};field='measured dimension';status=$(if($i -eq 80){'missing'}else{'synthetic'});value=$(if($i -eq 80){$null}else{'1'});units='m';source='Original synthetic geometry';rationale='Readability fixture, not measured building information';affected_scope='synthetic reference'})
 }
 $system=$Document.energy.designs.B.model.zones[0].cooling_system
 if($null -eq $system){throw 'The finite fixture requires a conditioned first proposed zone'}
 foreach($declaration in @(@('conflict-one','3.6'),@('conflict-two','3.7'))){
  $assumptions.Add([ordered]@{id=$declaration[0];link=@{kind='thermal_object';design_id='B';object_id=$system.id};field='/gross_rated_cop';status='assumed';value=$declaration[1];units='1';source='Deliberately contradictory original source declarations';rationale='Must be disclosed after the original first-80 boundary; does not edit the applied COP';affected_scope=$Document.energy.designs.B.model.zones[0].id})
 }
 $hashes=[ordered]@{};foreach($id in @('A','B')){$hashes[$id]=$Context.designs[$id].model_input_sha256}
 return [ordered]@{
  schema_version='greenlens.architectural-scope.v2';title='Original 81-row report completeness fixture'
  purpose='Report completeness, pagination and immutable evidence handling using genuine retained simulations. No architectural equivalence or scientific qualification is inferred.'
  scope_kind='representative_study';reference_revision_sha256=$Manifest.revision_sha256;reference_geometry_sha256=$Manifest.geometry_sha256;reference_options_sha256=$Context.reference_options_sha256
  design_model_sha256=$hashes;mappings=@($mappings.ToArray());assumptions=@($assumptions.ToArray());reconciliations=@()
  included_scope=@('The unchanged accepted A/B thermal models and their selected retained annual results.','Eighty-one original synthetic triangles test report register completeness only.')
  excluded_scope=@('No private architecture, source-equivalence claim or measured physical validation.','Source disposition row 79 is explicitly excluded; row 80 remains unresolved.')
  boundary_consequences='Thermal geometry, boundaries, schedules, weather, system and service assumptions remain exactly those of the retained selected runs. The unrelated synthetic display triangles do not change simulation inclusion.'
  coverage=@{status='unknown';reason='The original report fixture is not a building source; no valid whole-building area denominator exists.'};whole_building_review=$null;report_views=@()
 }
}
function Assert-ReportFixtureProducts($Inspected,[string]$IssueRoot) {
 $c=Read-IssueJson (Join-Path $IssueRoot 'presentation-coverage.json')
 $m=Read-IssueJson (Join-Path $IssueRoot 'scope-mappings.json')
 $a=Read-IssueJson (Join-Path $IssueRoot 'scope-assumptions.json')
 if($Inspected.content.renderer.method -cne 'greenlens.energy-report-issue-renderer/2'){throw 'New issue did not use the separately versioned renderer/2'}
 if(!$Inspected.summary.include_pdf){throw 'Native PDF output was not included'}
 if($m.rows.Count -ne 81 -or $m.rows[80].status -cne 'unresolved' -or $m.rows[79].status -cne 'excluded'){throw 'Lossless mapping artifact lost the declared late exceptions'}
 if($a.rows.Count -ne 83 -or $a.rows[80].status -cne 'missing' -or $a.rows[81].id -cne 'conflict-one' -or $a.rows[82].id -cne 'conflict-two'){throw 'Lossless assumption artifact lost the late missing/conflicting declarations'}
 if($c.mappings.total_rows -ne 81 -or $c.mappings.displayed_rows -ne 80 -or $c.mappings.omitted_rows -ne 1 -or $c.mappings.status_counts.unresolved -ne 1 -or $c.mappings.status_counts.excluded -ne 1 -or $c.mappings.status_counts.'context only' -ne 79){throw 'All-row mapping totals differ from the declared fixture'}
 if($c.mappings.displayed_ids[0] -cne $m.rows[80].occurrence_id -or $c.assumptions.displayed_ids[0] -cne 'source-080' -or $c.assumptions.total_rows -ne 83 -or $c.provenance.status_counts.conflicting -ne 1){throw 'Late review-critical rows are not reflected in bounded presentation'}
 foreach($kind in @('mappings','assumptions','provenance','reconciliations')){
  $t=$c[$kind];$total=0;foreach($n in $t.status_counts.Values){$total+=$n}
  if($total -ne $t.total_rows -or $t.displayed_rows+$t.omitted_rows -ne $t.total_rows -or $t.review_required_displayed+$t.review_required_omitted -ne $t.review_required_total){throw "Incomplete register accounting: $kind"}
 }
 $rows=$Inspected.content.scope_revision.provenance.rows
 $conflicts=@($rows|Where-Object status -CEQ 'conflicting')
 if($conflicts.Count -ne 1 -or $c.provenance.displayed_ids[0] -cne $conflicts[0].id){throw 'Conflicting applied-field provenance is not disclosed first'}
 $appendix=[IO.File]::ReadAllText((Join-Path $IssueRoot 'input-appendix.html'))
 $executive=[IO.File]::ReadAllText((Join-Path $IssueRoot 'executive.html'))
 foreach($text in @('Displayed 80 of 81','1 omitted','unresolved: 1','conflicting: 1','scope-mappings.json','source-080','conflict-one')){if(!$appendix.Contains($text)){throw "HTML appendix lacks explicit complete-register disclosure: $text"}}
 foreach($text in @('Documentation: INCOMPLETE','conflicting','unresolved rows','Comparison eligibility','Unassessed')){if(!$executive.Contains($text)){throw "Executive HTML lacks material disclosure: $text"}}
 if($null -eq $Inspected.summary.sensitivity_session_id -and !$executive.Contains('Exact selected-pair sensitivity: MISSING')){throw 'Absent exact-pair sensitivity is not disclosed'}
 $products=[Collections.Generic.List[object]]::new()
 foreach($p in $Inspected.products.products){
  $file=Join-Path $IssueRoot $p.path;Assert-IssueChild $IssueRoot $file
  if((Get-Item -LiteralPath $file).Length -ne $p.bytes -or (Get-IssueHash $file) -cne $p.sha256){throw "Generated product differs from its immutable manifest: $($p.path)"}
  $products.Add([ordered]@{path=$p.path;bytes=$p.bytes;sha256=$p.sha256})
 }
 return [ordered]@{coverage=$c;products=@($products.ToArray());complete_artifact_counts=@{mappings=81;assumptions=83};html_disclosures_passed=$true;pdf_visual_review='Not performed by this collector; every new page/image requires separate review.'}
}
if($FunctionsOnly){return}
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
if(!$IsWindows -or $PSVersionTable.PSVersion.Major -lt 7){throw 'Use native Windows PowerShell 7'}
if(@($ResumeVerifiedPreparation,$ResumeVerifiedReview,$ResumeVerifiedDecisions,$ResumeVerifiedRendererPath|Where-Object {$_}).Count -gt 1){throw 'Select only one declared collector repair checkpoint'}
if([bool]$AbortedRendererStageId -ne [bool]$ResumeVerifiedRendererPath){throw 'The explicit renderer repair requires exactly one retained aborted stage identity'}
$resume=[bool]($ResumeVerifiedPreparation -or $ResumeVerifiedReview -or $ResumeVerifiedDecisions -or $ResumeVerifiedRendererPath)
$preparedReview=[bool]($ResumeVerifiedReview -or $ResumeVerifiedDecisions)
$localBase=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
foreach($name in @('Cli','Project','TypstDirectory','OutputDirectory')){Set-Variable -Name $name -Value ([IO.Path]::GetFullPath((Get-Variable $name -ValueOnly)))}
Assert-IssueChild $localBase $Project;Assert-IssueChild $localBase $OutputDirectory
if($OutputDirectory -ieq $Project -or $OutputDirectory.StartsWith($Project+'\',[StringComparison]::OrdinalIgnoreCase) -or $Project.StartsWith($OutputDirectory+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Source and collector output must be independent'}
if((Test-Path -LiteralPath $OutputDirectory) -and !$resume){throw 'Use a new output directory or an explicit declared repair checkpoint; no automatic render retry is permitted.'}
if($resume -and !(Test-Path -LiteralPath $OutputDirectory)){throw 'Resume requires the retained original collector directory'}
if([bool]$LegacyProject -ne ($LegacyIssueIds.Count -gt 0)){throw 'Supply both LegacyProject and one or two explicit LegacyIssueIds, or neither'}
if(@($LegacyIssueIds|Select-Object -Unique).Count -ne $LegacyIssueIds.Count){throw 'Duplicate legacy issue IDs are not permitted'}
if($LegacyProject){
 $LegacyProject=[IO.Path]::GetFullPath($LegacyProject);Assert-IssueChild $localBase $LegacyProject
 if($LegacyProject -ieq $Project){throw 'The pre-issue source and legacy issued source are distinct declared documents'}
 if($OutputDirectory -ieq $LegacyProject -or $OutputDirectory.StartsWith($LegacyProject+'\',[StringComparison]::OrdinalIgnoreCase) -or $LegacyProject.StartsWith($OutputDirectory+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Legacy source and collector output must be independent'}
 $null=Assert-ReportTreeBounds $LegacyProject
}
$source=Read-IssueJson (Join-Path $Project 'project.json')
if($source.Contains('report_issues') -and $null -ne $source.report_issues -and $source.report_issues.issues.Count){throw 'The primary source must be pre-issue; never prune an issued document for this fixture'}
foreach($id in @('A','B')){if(!$source.energy.designs.Contains($id)){throw 'Declared A/B accepted designs are required'}}
$selected=@();foreach($pair in @(@($BaselineRun,'A'),@($ProposedRun,'B'))){$run=@($source.energy.runs|Where-Object id -CEQ $pair[0]);if($run.Count -ne 1 -or $run[0].alternative_id -cne $pair[1]){throw 'Select one genuine retained baseline A and proposed B'};$selected+=@($run[0])}
$initialBounds=Assert-ReportTreeBounds $Project
$original=Get-IssueSnapshot $Project;$legacyOriginal=if($LegacyProject){Get-IssueSnapshot $LegacyProject}else{$null}
$logs=Join-Path $OutputDirectory 'commands';New-Item -ItemType Directory -Path $logs -Force|Out-Null
$cliHash=Get-IssueHash $Cli;$collectorHash=Get-IssueHash $PSCommandPath;$typstHash=Get-IssueHash (Join-Path $TypstDirectory 'typst.exe')
$systemConhost=[IO.Path]::GetFullPath((Join-Path ([Environment]::SystemDirectory) 'conhost.exe'))
$declaredTypst=[IO.Path]::GetFullPath((Join-Path $TypstDirectory 'typst.exe'))
$identityLease=Open-ReportPinnedFile $declaredTypst
try{$pinnedTypstIdentity=Get-ReportPinnedIdentity $identityLease}finally{$identityLease.Dispose()}
if($pinnedTypstIdentity.sha256 -cne $typstHash){throw 'Held renderer bytes differ from the declared digest'}
$frozenCli=Join-Path $OutputDirectory ('greenlens-'+$cliHash+'.exe')
if(!$resume){Copy-Item -LiteralPath $Cli -Destination $frozenCli}
if((Get-IssueHash $frozenCli) -cne $cliHash){throw 'Frozen CLI identity differs'}
$work=Join-Path $OutputDirectory 'report fixture study';$fixture=Join-Path $OutputDirectory 'original-81-occurrences.glb'
if(!$resume){New-ReportFixtureGlb $fixture}
$declaration=[ordered]@{
 schema_version='greenlens.v08-report-fixture-protocol.v1';declared_utc=[DateTime]::UtcNow.ToString('o');cli_source_commit_declared=$CliSourceCommit.ToLowerInvariant();cli_sha256=$cliHash;collector_sha256=$collectorHash;typst_sha256=$typstHash
 source_manifest_sha256=$original['project.json'];baseline=$selected[0];proposed=$selected[1];source_project=$Project;legacy_project=$LegacyProject;legacy_issue_ids=$LegacyIssueIds
 fixture_sha256=Get-IssueHash $fixture;fixture_bytes=(Get-Item -LiteralPath $fixture).Length;expected=@{occurrences=81;mappings=81;assumptions=83;unresolved=1;excluded=1;conflicting_applied_fields=1}
 new_issue_attempts=1;legacy_regeneration_attempts_per_selected_issue=1;maximum_seconds_per_command=600;maximum_process_tree_working_set_bytes=2GB;maximum_new_scratch_bytes=2GB;maximum_processes=16;maximum_energyplus_processes=0;maximum_cli_log_bytes=64MB
 allowed_console_host=@{path=$systemConhost;sha256=Get-IssueHash $systemConhost;policy='CIM executable path must equal the canonical Windows system-directory conhost.exe; its resources remain counted.'}
 pinned_renderer_file=$pinnedTypstIdentity
 source_bounds=$initialBounds;scope='Synthetic report completeness/readability and existing evidence integration only. No new simulation, scientific qualification, architectural equivalence, installed interaction or visual PDF acceptance.'
}
$statePath=Join-Path $OutputDirectory 'collector-state.json'
if($resume){
 $prior=Read-IssueJson (Join-Path $OutputDirectory 'declared-protocol.json');$state=Read-IssueJson $statePath
 foreach($field in @('cli_source_commit_declared','cli_sha256','typst_sha256','source_manifest_sha256','source_project','legacy_project','legacy_issue_ids','baseline','proposed','fixture_sha256','fixture_bytes','expected')){
  if(!(Test-IssueJsonEqual $prior[$field] $declaration[$field])){throw "Resume changed a declared input or tool: $field"}
 }
 $repairFile=if($ResumeVerifiedRendererPath){'collector-renderer-path-repair.json'}elseif($ResumeVerifiedDecisions){'collector-decisions-repair.json'}elseif($ResumeVerifiedReview){'collector-review-repair.json'}else{'collector-preparation-repair.json'}
 if(Test-Path -LiteralPath (Join-Path $OutputDirectory $repairFile)){throw 'This collector-only checkpoint repair was already attempted; preserve evidence and stop'}
 foreach($command in $state.commands){
  $receiptPath=Join-Path $logs ($command.name+'/receipt.json');Assert-IssueChild $logs $receiptPath
  if((Get-IssueHash $receiptPath) -cne $command.receipt_sha256){throw 'Original command receipt was modified'}
  $receipt=Read-IssueJson $receiptPath
  if($command.name -ne 'adopt-old-scope' -and !(($ResumeVerifiedDecisions -or $ResumeVerifiedRendererPath) -and $command.name -eq 'accept-reviewed-fixture-scope') -and !($ResumeVerifiedRendererPath -and $command.name -eq 'issue-once') -and ($null -ne $receipt.failure -or $receipt.exit_code -ne 0)){throw 'Preparation contains an earlier failed command'}
  if($command.name -eq 'adopt-old-scope'){
   if($null -ne $receipt.exit_code -or $receipt.failure -cne 'An undeclared child executable started; only the canonical Windows console host and explicitly allowed pinned Typst path are permitted, never an engine' -or $receipt.observed_children.Count -ne 1 -or $receipt.observed_children[0].name -ine 'conhost.exe' -or $null -ne $receipt.observed_children[0].executable_path){throw 'Only the retained null-path console exit-race failure is eligible for this preparation repair'}
  }
 }
 $retainedOriginal=Read-IssueJson (Join-Path $OutputDirectory 'original-member-hashes.json');Assert-IssueSnapshot $Project $retainedOriginal
 if($LegacyProject){Assert-IssueSnapshot $LegacyProject (Read-IssueJson (Join-Path $OutputDirectory 'legacy-original-member-hashes.json'))}
 $resumeDocument=Read-IssueJson (Join-Path $work 'project.json')
 $copied=Read-IssueJson (Join-Path $logs 'complete-save-as/stdout.json')
 if($ResumeVerifiedRendererPath){
  $imported=Read-IssueJson (Join-Path $logs 'import-original-fixture/stdout.json')
  $history=Read-IssueJson (Join-Path $logs 'accept-reviewed-fixture-scope-corrected/stdout.json')
  $scopePath=Join-Path $OutputDirectory 'report-fixture-scope.json';$scope=Read-IssueJson $scopePath
  $plan=Read-IssueJson (Join-Path $logs 'resume-review-fixture-scope/stdout.json')
  $failedReceipt=Read-IssueJson (Join-Path $logs 'issue-once/receipt.json')
  $stageBase=Join-Path $localBase 'report-issue-staging';$abortedStage=Join-Path $stageBase $AbortedRendererStageId
  Assert-IssueChild $stageBase $abortedStage;$null=Assert-ReportTreeBounds $abortedStage
  if((Test-Path -LiteralPath (Join-Path $work 'report-issues')) -or (Test-Path -LiteralPath (Join-Path $abortedStage 'staged-issue.json')) -or (Test-Path -LiteralPath (Join-Path $abortedStage 'local-renderer-receipt.json'))){throw 'Published or completed staged products exist; inspect/recover them instead of rendering again'}
  $pendingInspected=@();$stageFolders=@(Get-ChildItem -LiteralPath $stageBase -Directory)
  $legacyDocument=if($LegacyProject){Read-IssueJson (Join-Path $LegacyProject 'project.json')}else{$null}
  if($stageFolders.Count -gt 128){throw 'Bounded pending-stage inspection exceeded 128 directories; stop for explicit review'}
  foreach($folder in $stageFolders){
   if($folder.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Pending-stage directory cannot be a reparse point'}
   $marker=Join-Path $folder.FullName 'staged-issue.json'
   if(Test-Path -LiteralPath $marker){
    $item=Get-Item -LiteralPath $marker;if($item.Length -gt 1MB -or $item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Invalid pending-stage receipt'}
    $pending=Read-IssueJson $marker
    $matchesProject=$pending.project_id -ceq $resumeDocument.id;$classification='other_project'
    if($matchesProject){
     if(!(Test-ReportPublishedHistoricalStage $pending $legacyDocument $LegacyIssueIds $history.active_revision_sha256)){throw 'A completed issue for this project is pending recovery; never render over it'}
     $summary=$pending.summary;$published=Join-Path $LegacyProject $summary.path;Assert-IssueChild $LegacyProject $published
     foreach($pair in @(@('issue-content.json','content_file_sha256'),@('issue-products.json','products_file_sha256'),@('issue-runtime.json','receipt_file_sha256'))){if((Get-IssueHash (Join-Path $published $pair[0])) -cne $summary[$pair[1]]){throw 'Already-published historical issue file identity changed'}}
     $products=Read-IssueJson (Join-Path $published 'issue-products.json');$seal=Read-IssueJson (Join-Path $published 'checksums.json')
     if($products.products.Count -gt 252 -or $products.content_sha256 -cne $summary.content_sha256 -or $products.products_sha256 -cne $summary.products_sha256){throw 'Already-published historical product binding differs'}
     $expectedSeal=@{'issue-products.json'=$summary.products_file_sha256;'issue-runtime.json'=$summary.receipt_file_sha256}
     foreach($product in $products.products){
      $file=Join-Path $published $product.path;Assert-IssueChild $published $file
      if((Get-IssueHash $file) -cne $product.sha256 -or (Get-Item -LiteralPath $file).Length -ne $product.bytes){throw 'Already-published historical product bytes differ'}
      $expectedSeal[$product.path]=$product.sha256
     }
     if($seal.schema_version -cne '0.1.0' -or !(Test-IssueJsonEqual $seal.sha256 $expectedSeal)){throw 'Already-published historical seal differs'}
     $classification='already_published_selected_legacy_issue_exact_summary_and_member_hashes'
    }
    $pendingInspected+=@(@{stage_id=$folder.Name;receipt_sha256=Get-IssueHash $marker;matches_current_project=$matchesProject;classification=$classification;issue_id=$pending.summary.id})
   }
  }
  $stageContent=Read-IssueJson (Join-Path $abortedStage 'products/issue-content.json')
  Assert-ReportRendererResume $state $prior $resumeDocument $copied $imported $history $scope $plan $source.energy $stageContent $failedReceipt $pinnedTypstIdentity
  $expectedArgs=@('report-issue','create',$work,'--a',$BaselineRun,'--b',$ProposedRun,'--scope',$history.active_revision_sha256,'--typst-dir',$TypstDirectory)
  if(!(Test-IssueJsonEqual $failedReceipt.request.arguments $expectedArgs)){throw 'Aborted renderer request differs from this exact pair/scope/tool configuration'}
  $logicalLease=Open-ReportPinnedFile $declaredTypst;$physicalLease=$null
  try{
   $logicalIdentity=Get-ReportPinnedIdentity $logicalLease
   $physicalLease=Open-ReportPinnedFile $pinnedTypstIdentity.physical_path;$physicalIdentity=Get-ReportPinnedIdentity $physicalLease
   if(!(Test-ReportSamePinnedFile $logicalIdentity $physicalIdentity $prior.typst_sha256)){throw 'Logical and observed physical renderer are not the identical pinned file'}
  }finally{if($physicalLease){$physicalLease.Dispose()};$logicalLease.Dispose()}
  $abortedStageSnapshot=Get-ReportArtifactSnapshot $abortedStage
  $repair=@{schema_version='greenlens.v08-collector-renderer-path-repair.v1';authorization='One explicit engineering rendering attempt for the proven same-file Windows package alias. The original aborted render remains counted; no scientific/profile change or automatic retry.';maximum_corrected_render_attempts=1;original_issue_attempt_remains_started=$true;new_simulations_permitted=0;legacy_attempts_per_issue_unchanged=1;failed_issue_receipt_sha256=Get-IssueHash (Join-Path $logs 'issue-once/receipt.json');logical_renderer=$logicalIdentity;physical_renderer=$physicalIdentity;aborted_stage_id=$AbortedRendererStageId;aborted_stage_members=$abortedStageSnapshot;pending_stages_inspected=$pendingInspected;selected_scope=$history.active_revision_sha256;scope_file_sha256=Get-IssueHash $scopePath;accepted_history_stdout_sha256=Get-IssueHash (Join-Path $logs 'accept-reviewed-fixture-scope-corrected/stdout.json');policy='Retain all partial products unchanged. No copy/import/plan/open prepass. Corrected create performs ordinary full verification and rendering with unchanged original inputs and limits.'}
 }elseif($preparedReview){
  if(!(Test-Path -LiteralPath (Join-Path $OutputDirectory 'collector-preparation-repair.json'))){throw 'Review checkpoint lacks its retained earlier preparation repair'}
  $imported=Read-IssueJson (Join-Path $logs 'import-original-fixture/stdout.json')
  $manifest=Read-IssueJson (Join-Path $logs 'inspect-original-reference/stdout.json')
  $context=Read-IssueJson (Join-Path $logs 'scope-context/stdout.json')
  $scopePath=Join-Path $OutputDirectory 'report-fixture-scope.json';$scope=Read-IssueJson $scopePath
  $previousPlan=Read-IssueJson (Join-Path $logs 'review-fixture-scope/stdout.json')
  if($ResumeVerifiedDecisions){
   $reviewRepair=Read-IssueJson (Join-Path $OutputDirectory 'collector-review-repair.json')
   $freshPlan=Read-IssueJson (Join-Path $logs 'resume-review-fixture-scope/stdout.json')
   if(!(Test-IssueJsonEqual $freshPlan $previousPlan)){throw 'Fresh retained review differs from its prior exact plan'}
   $failedReceipt=Read-IssueJson (Join-Path $logs 'accept-reviewed-fixture-scope/receipt.json')
   $failedError=[IO.File]::ReadAllText((Join-Path $logs 'accept-reviewed-fixture-scope/stderr.txt'))
   Assert-ReportDecisionsResume $state $prior $resumeDocument $copied $imported $manifest $context $scope $freshPlan $source.energy $prior.fixture_sha256 $reviewRepair $failedReceipt $failedError (Get-IssueHash (Join-Path $work 'project.json'))
   $plan=$freshPlan
  }else{Assert-ReportReviewResume $state $prior $resumeDocument $copied $imported $manifest $context $scope $previousPlan $source.energy $prior.fixture_sha256}
  $asset=@($imported.assets|Where-Object id -CEQ $imported.active_asset_id)[0]
  $manifestPath=Join-Path $work ($asset.path+'/reference.json');Assert-IssueChild $work $manifestPath
  if((Get-IssueHash $manifestPath) -cne $asset.manifest_sha256 -or !(Test-IssueJsonEqual (Read-IssueJson $manifestPath) $manifest)){throw 'Retained imported manifest bytes or decoded identity changed'}
  $parent=$previousPlan.parent_revision_sha256
  if($ResumeVerifiedDecisions){
   $repair=@{schema_version='greenlens.v08-collector-decisions-repair.v1';policy='Only candidate Missing assumptions and Unresolved mappings receive an unresolved decision. Other explicit declarations are reviewed, not physically qualified. One corrected acceptance re-verifies actual bytes and recomputes the exact plan. No repeated preparation or reset render allowance.';prior_plan_sha256=$plan.plan_sha256;prior_review_repair_sha256=Get-IssueHash (Join-Path $OutputDirectory 'collector-review-repair.json');failed_accept_receipt_sha256=Get-IssueHash (Join-Path $logs 'accept-reviewed-fixture-scope/receipt.json');failed_accept_stderr_sha256=Get-IssueHash (Join-Path $logs 'accept-reviewed-fixture-scope/stderr.txt');prior_decisions_sha256=Get-IssueHash (Join-Path $OutputDirectory 'review-decisions.json');candidate_file_sha256=Get-IssueHash $scopePath;imported_manifest_sha256=$asset.manifest_sha256}
  }else{$repair=@{schema_version='greenlens.v08-collector-review-repair.v1';policy='Exactly one fresh explicit-candidate scope review re-verifies actual project bytes and identities. No repeated copy/import/inspection, no reset render allowance. The previous mechanical proposal and candidate remain separate retained records.';prior_plan_sha256=$previousPlan.plan_sha256;prior_review_stdout_sha256=Get-IssueHash (Join-Path $logs 'review-fixture-scope/stdout.json');prior_preparation_repair_sha256=Get-IssueHash (Join-Path $OutputDirectory 'collector-preparation-repair.json');candidate_file_sha256=Get-IssueHash $scopePath;imported_manifest_sha256=$asset.manifest_sha256}}
 }else{
  Assert-ReportPreparationResume $state $prior $resumeDocument $copied $source.energy
  $repair=@{schema_version='greenlens.v08-collector-preparation-repair.v1';policy='One fresh Rust open verifies actual bytes and adopted history; no copy or successful A/B inspection is repeated. Previous failures remain. No issue or regeneration attempt is reset.'}
 }
 $repair.recorded_utc=[DateTime]::UtcNow.ToString('o');$repair.prior_declaration_sha256=Get-IssueHash (Join-Path $OutputDirectory 'declared-protocol.json');$repair.prior_state_sha256=Get-IssueHash $statePath;$repair.collector_sha256=$collectorHash;$repair.cli_sha256=$cliHash;$repair.working_manifest_sha256=Get-IssueHash (Join-Path $work 'project.json')
 $repair.child_identity_policy='Canonical CIM/process-handle path or exact pinned physical renderer alias proven by held-handle file ID and hash. Proven exited known child may be logged as path unobserved; a live unknown identity remains rejected.'
 Write-IssueJson (Join-Path $OutputDirectory $repairFile) $repair
}else{
 Write-IssueJson (Join-Path $OutputDirectory 'declared-protocol.json') $declaration
 Write-IssueJson (Join-Path $OutputDirectory 'original-member-hashes.json') $original
 if($LegacyProject){Write-IssueJson (Join-Path $OutputDirectory 'legacy-original-member-hashes.json') $legacyOriginal}
 $state=[ordered]@{new_issue_attempt_started=$false;legacy_regeneration_attempts=@();completed=$false;commands=@()};Write-IssueJson $statePath $state
}
function Get-ReportScratch {
 $set=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
 foreach($name in @('jobs','report-jobs','report-issue-staging')){$path=Join-Path $localBase $name;if(Test-Path -LiteralPath $path){foreach($dir in Get-ChildItem -LiteralPath $path -Directory){$null=$set.Add($dir.FullName)}}}
 return ,$set
}
function Invoke-ReportCli([string]$Name,[string[]]$Arguments,[switch]$AllowTypst) {
 $verb=$Arguments[0];$sub=if($Arguments.Count -gt 1){$Arguments[1]}else{''}
 if(!(($verb -in @('copy','capacity','open')) -or ($verb -eq 'energy' -and $sub -in @('inspect','compare')) -or ($verb -eq 'reference' -and $sub -in @('import','inspect','scope-context','scope-apply','scope-assess','scope-review','scope-accept')) -or ($verb -eq 'report-issue' -and $sub -in @('create','inspect','regenerate')))){throw 'CLI operation is outside this no-engine protocol'}
 if($AllowTypst -and !($verb -eq 'report-issue' -and $sub -in @('create','regenerate'))){throw 'Only explicit report generation may start Typst'}
 if((Get-IssueHash $frozenCli) -cne $cliHash -or (Get-IssueHash (Join-Path $TypstDirectory 'typst.exe')) -cne $typstHash){throw 'Declared tool bytes changed'}
 $attempt=Join-Path $logs $Name;if(Test-Path -LiteralPath $attempt){throw 'No command attempt is silently repeated'};New-Item -ItemType Directory -Path $attempt|Out-Null
 $request=@{arguments=$Arguments;deadline_seconds=600;allowed_engines=0;allow_typst=[bool]$AllowTypst;pinned_renderer_file=$(if($AllowTypst){$pinnedTypstIdentity}else{$null});started_utc=[DateTime]::UtcNow.ToString('o')};Write-IssueJson (Join-Path $attempt 'request.json') $request
 $start=[Diagnostics.ProcessStartInfo]::new($frozenCli);$start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.WorkingDirectory=$OutputDirectory;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
 foreach($arg in $Arguments){$start.ArgumentList.Add($arg)}
 $start.Environment['GREENLENS_ENERGYPLUS_DIR']=Join-Path $OutputDirectory 'engine-not-available'
 $stdout=[IO.File]::Create((Join-Path $attempt 'stdout.json'));$stderr=[IO.File]::Create((Join-Path $attempt 'stderr.txt'));$p=[Diagnostics.Process]::new();$p.StartInfo=$start
 $known=[Collections.Generic.HashSet[int]]::new();$births=@{};$children=@{};$scratchBefore=Get-ReportScratch;$watch=[Diagnostics.Stopwatch]::StartNew();[long]$peakMemory=0;[long]$peakScratch=0;$peakCount=0;$failure=$null;$exitCode=$null;$started=$false;$logicalToolLease=$null;$physicalToolLease=$null
 try{
  if($AllowTypst){
   $logicalToolLease=Open-ReportPinnedFile $declaredTypst;$physicalToolLease=Open-ReportPinnedFile $pinnedTypstIdentity.physical_path
   $logicalProof=Get-ReportPinnedIdentity $logicalToolLease;$physicalProof=Get-ReportPinnedIdentity $physicalToolLease
   if(!(Test-ReportSamePinnedFile $logicalProof $physicalProof $typstHash) -or !(Test-ReportSamePinnedFile $logicalProof $pinnedTypstIdentity $typstHash)){throw 'Pinned renderer identity changed before execution'}
  }
  if(!$p.Start()){throw 'CLI did not start'};$started=$true;$null=$known.Add($p.Id)
  $outTask=$p.StandardOutput.BaseStream.CopyToAsync($stdout);$errTask=$p.StandardError.BaseStream.CopyToAsync($stderr)
  do{
   $table=@(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name,ExecutablePath,CreationDate)
   foreach($row in $table){$key=[string]$row.ProcessId;if($known.Contains([int]$row.ProcessId)){if($births.ContainsKey($key) -and $births[$key] -ne $row.CreationDate){$null=$known.Remove([int]$row.ProcessId)}elseif(!$births.ContainsKey($key)){$births[$key]=$row.CreationDate}}}
   do{$added=$false;foreach($row in $table){if($known.Contains([int]$row.ParentProcessId) -and $known.Add([int]$row.ProcessId)){$births[[string]$row.ProcessId]=$row.CreationDate;$added=$true}}}while($added)
   [long]$memory=0;$alive=0;$engineCount=0;$undeclaredChild=$false
   foreach($row in $table){if($known.Contains([int]$row.ProcessId)){
    try{$child=[Diagnostics.Process]::GetProcessById([int]$row.ProcessId);$memory+=$child.WorkingSet64;$alive++;$child.Dispose()}catch [ArgumentException]{}
    if($row.Name -ieq 'energyplus.exe'){$engineCount++}
    if($row.ProcessId -ne $p.Id){
     $probe=@{State='not_required';ImagePath=$null;Error=0;CreationUtcTicks=0}
     if([string]::IsNullOrWhiteSpace($row.ExecutablePath)){$probe=Get-ReportChildHandleIdentity ([int]$row.ProcessId) $row.CreationDate}
     $decision=Get-ReportPinnedChildDecision $row.Name $row.ExecutablePath $probe.State $probe.ImagePath $systemConhost $declaredTypst ([bool]$AllowTypst) $pinnedTypstIdentity
     $previousObservation=if($children.ContainsKey([string]$row.ProcessId)){$children[[string]$row.ProcessId]}else{$null}
     $children[[string]$row.ProcessId]=@{name=$row.Name;executable_path=$decision.path;policy_allowed=$decision.allowed;identity_status=$decision.identity_status;handle_probe_state=$probe.State;handle_probe_error=$probe.Error;queried_creation_utc_ticks=$probe.CreationUtcTicks;creation_utc=$row.CreationDate.ToUniversalTime().ToString('o');previous_observed_path=$(if($previousObservation){$previousObservation.executable_path}else{$null})}
     if(!$decision.allowed){$undeclaredChild=$true}
    }
   }}
   [long]$scratch=0;foreach($path in (Get-ReportScratch)){if(!$scratchBefore.Contains($path)){$scratch+=Get-IssueTreeBytes $path -Live}}
   $peakMemory=[Math]::Max($peakMemory,$memory);$peakScratch=[Math]::Max($peakScratch,$scratch);$peakCount=[Math]::Max($peakCount,$alive)
   Assert-IssueExecutionBudgets $watch.Elapsed.TotalSeconds 600 $memory $scratch $alive $engineCount 0 0
   if($undeclaredChild){throw 'An undeclared child executable started; only the canonical Windows console host and explicitly allowed pinned Typst path are permitted, never an engine'}
   if($stdout.Length -gt 64MB -or $stderr.Length -gt 64MB){throw 'Collector log exceeded its 64 MiB bound'}
   if(!$p.HasExited){Start-Sleep -Milliseconds 400}
  }while(!$p.HasExited)
  $p.WaitForExit()
  foreach($task in @($outTask,$errTask)){
   $remaining=[int][Math]::Max(0,[Math]::Min(10000,600000-$watch.Elapsed.TotalMilliseconds))
   if(!$task.Wait($remaining)){throw 'CLI stream drain exceeded its remaining command deadline'}
   Wait-IssueStreamTask $task
  }
  if($watch.Elapsed.TotalSeconds -gt 600){throw 'Declared command deadline exceeded during stream drain'}
  $exitCode=$p.ExitCode
 }catch{
  $failure=$_.Exception.Message
  if($started -and !$p.HasExited){$p.Kill($true);$null=$p.WaitForExit(10000)}
  foreach($row in @(Get-CimInstance Win32_Process -Property ProcessId,CreationDate -ErrorAction SilentlyContinue)){if($known.Contains([int]$row.ProcessId) -and $births.ContainsKey([string]$row.ProcessId) -and $births[[string]$row.ProcessId] -eq $row.CreationDate){try{$child=[Diagnostics.Process]::GetProcessById([int]$row.ProcessId);if(!$child.HasExited){$child.Kill($true)};$child.Dispose()}catch [ArgumentException]{}}}
 }finally{
  if($physicalToolLease){$physicalToolLease.Dispose()};if($logicalToolLease){$logicalToolLease.Dispose()}
  $watch.Stop();$stdout.Dispose();$stderr.Dispose();$p.Dispose()
  $receipt=@{request=$request;exit_code=$exitCode;wall_seconds=$watch.Elapsed.TotalSeconds;peak_observed_working_set_bytes=$peakMemory;peak_observed_new_scratch_bytes=$peakScratch;peak_process_count=$peakCount;observed_children=@($children.Values);sampling='Windows process table and working sets sampled approximately every 400 ms; this is not an absolute instrumented peak or native cancellation observation.';failure=$failure;finished_utc=[DateTime]::UtcNow.ToString('o')}
  Write-IssueJson (Join-Path $attempt 'receipt.json') $receipt;$state.commands+=@(@{name=$Name;receipt_sha256=Get-IssueHash (Join-Path $attempt 'receipt.json')});Write-IssueJson $statePath $state
 }
 if($failure -or $exitCode -ne 0){throw "Report collector stopped at $Name; retained receipt contains failure and resources"}
 return Read-IssueJson (Join-Path $attempt 'stdout.json')
}
$mainFailure=$null
$pendingResult=$null
try{
 if(!$ResumeVerifiedRendererPath){
 if($ResumeVerifiedDecisions){
  # Corrected acceptance below performs the single fresh integrity/review
  # traversal. Its retained plan and unchanged manifest were fenced above.
 }elseif($ResumeVerifiedReview){
  $plan=Invoke-ReportCli 'resume-review-fixture-scope' @('reference','scope-review',$work,'--parent',$parent,'--candidate',$scopePath)
  Assert-ReportFixtureReview $plan $scope $parent
  if(!(Test-IssueJsonEqual $plan $previousPlan)){throw 'Fresh review differs from the retained exact candidate, parent, mechanical proposal or target context'}
 }elseif($ResumeVerifiedPreparation){$null=Invoke-ReportCli 'resume-verified-open' @('open',$work)}else{$null=Invoke-ReportCli 'complete-save-as' @('copy',$Project,$work)}
 $retained=Copy-IssueJson $original;$retained.Remove('project.json')|Out-Null;Assert-IssueSnapshot $work $retained -AllowAdditional:$preparedReview
 $doc=Read-IssueJson (Join-Path $work 'project.json')
 if(!$resume){foreach($pair in @(@($BaselineRun,'A'),@($ProposedRun,'B'))){
  $inspected=Invoke-ReportCli ('verify-run-'+$pair[1]) @('energy','inspect',$work,$pair[0]);$run=$inspected.run
  if($run.series.interval_count -ne 52560 -or $run.series.interval_minutes -ne 10 -or $run.provenance.severe_count -ne 0){throw 'Selected genuine run does not meet the declared complete annual shape'}
  if(!(Test-IssueJsonEqual $run.model $doc.energy.designs[$pair[1]].model) -or !(Test-IssueJsonEqual $run.study $doc.energy.designs[$pair[1]].study)){throw 'Accepted inputs differ from selected retained run; do not rebind a historical run'}
 }
  $comparison=Invoke-ReportCli 'verify-compatible-pair' @('energy','compare',$work,$BaselineRun,$ProposedRun)
  Write-IssueJson (Join-Path $OutputDirectory 'canonical-comparison.json') $comparison
 }
 if(!$preparedReview){
 if($doc.Contains('architecture') -and $null -ne $doc.architecture -and $doc.architecture.Contains('scope') -and $null -ne $doc.architecture.scope -and (!$doc.architecture.Contains('scope_history') -or $null -eq $doc.architecture.scope_history)){
  $legacyScope=Join-Path $OutputDirectory 'adopted-legacy-scope.json';Write-IssueJson $legacyScope $doc.architecture.scope
  $null=Invoke-ReportCli 'adopt-old-scope' @('reference','scope-apply',$work,$legacyScope)
 }
 $null=Invoke-ReportCli 'reference-capacity' @('capacity',$work,'--operation','import-reference')
 $null=Invoke-ReportCli 'import-original-fixture' @('reference','import',$work,$fixture)
 $manifest=Invoke-ReportCli 'inspect-original-reference' @('reference','inspect',$work)
 $context=Invoke-ReportCli 'scope-context' @('reference','scope-context',$work)
 $doc=Read-IssueJson (Join-Path $work 'project.json');$scope=New-ReportFixtureScope $manifest $context $doc;$scopePath=Join-Path $OutputDirectory 'report-fixture-scope.json';Write-IssueJson $scopePath $scope
 $null=Invoke-ReportCli 'assess-fixture-scope' @('reference','scope-assess',$work,$scopePath)
 }
 $history=if($doc.architecture.Contains('scope_history')){$doc.architecture.scope_history}else{$null}
 if($null -eq $history){$null=Invoke-ReportCli 'accept-initial-fixture-scope' @('reference','scope-apply',$work,$scopePath)}else{
  $parent=$history.active_revision_sha256
  if(!$preparedReview){$plan=Invoke-ReportCli 'review-fixture-scope' @('reference','scope-review',$work,'--parent',$parent,'--candidate',$scopePath)}
  Assert-ReportFixtureReview $plan $scope $parent
  $decisions=New-ReportFixtureDecisions $plan $scope
  $decisionsFile=if($ResumeVerifiedDecisions){'review-decisions-corrected.json'}else{'review-decisions.json'}
  $decisionsPath=Join-Path $OutputDirectory $decisionsFile;Write-IssueJson $decisionsPath $decisions
  $acceptCommand=if($ResumeVerifiedDecisions){'accept-reviewed-fixture-scope-corrected'}else{'accept-reviewed-fixture-scope'}
  $null=Invoke-ReportCli $acceptCommand @('reference','scope-accept',$work,'--parent',$parent,'--plan',$plan.plan_sha256,'--scope',$scopePath,'--decisions',$decisionsPath)
 }
 }else{
  $retained=Copy-IssueJson $original;$retained.Remove('project.json')|Out-Null;Assert-IssueSnapshot $work $retained -AllowAdditional
 }
 $doc=Read-IssueJson (Join-Path $work 'project.json');$scopeId=$doc.architecture.scope_history.active_revision_sha256
 if(!(Test-IssueJsonEqual $doc.energy $source.energy)){throw 'Report preparation changed retained energy inputs or evidence indexes'}
 if(!$ResumeVerifiedRendererPath){$null=Invoke-ReportCli 'issue-capacity' @('capacity',$work,'--operation','report-issue')}
 if($ResumeVerifiedRendererPath){$state.renderer_path_repair_attempt_started=$true}else{$state.new_issue_attempt_started=$true};Write-IssueJson $statePath $state
 $issueCommand=if($ResumeVerifiedRendererPath){'issue-once-renderer-path-repair'}else{'issue-once'}
 $created=Invoke-ReportCli $issueCommand @('report-issue','create',$work,'--a',$BaselineRun,'--b',$ProposedRun,'--scope',$scopeId,'--typst-dir',$TypstDirectory) -AllowTypst
 $issueId=$created.issue.summary.id;$inspected=Invoke-ReportCli 'inspect-new-issue' @('report-issue','inspect',$work,$issueId)
 if($inspected.summary.baseline.run_id -cne $BaselineRun -or $inspected.summary.proposed.run_id -cne $ProposedRun -or $inspected.summary.scope_revision_sha256 -cne $scopeId){throw 'Issued scope/pair binding differs from the declared protocol'}
 $issueRoot=Join-Path $work $inspected.summary.path;Assert-IssueChild $work $issueRoot
 $checks=Assert-ReportFixtureProducts $inspected $issueRoot;Write-IssueJson (Join-Path $OutputDirectory 'complete-register-and-product-checks.json') $checks
 Assert-IssueSnapshot $work $retained -AllowAdditional
 $finalBounds=Assert-ReportTreeBounds $work
 $legacyResults=@()
 foreach($id in $LegacyIssueIds){
  $old=Invoke-ReportCli ('inspect-legacy-'+$id) @('report-issue','inspect',$LegacyProject,$id)
  if($old.content.renderer.method -cne 'greenlens.energy-report-issue-renderer/1' -or !$old.summary.include_pdf){throw 'Select an actual preserved renderer/1 PDF issue'}
  $state.legacy_regeneration_attempts+=@($id);Write-IssueJson $statePath $state
  $dest=Join-Path $OutputDirectory ('legacy-regenerated-'+$id)
  $null=Invoke-ReportCli ('regenerate-legacy-'+$id) @('report-issue','regenerate',$LegacyProject,$id,$dest,'--typst-dir',$TypstDirectory) -AllowTypst
  foreach($product in $old.products.products){$file=Join-Path $dest $product.path;Assert-IssueChild $dest $file;if((Get-IssueHash $file) -cne $product.sha256 -or (Get-Item -LiteralPath $file).Length -ne $product.bytes){throw 'Legacy regenerated deterministic product differs from retained issue'}}
  $legacyResults+=@(@{id=$id;content_sha256=$old.summary.content_sha256;products_sha256=$old.summary.products_sha256;products=$old.products.products;all_deterministic_product_bytes_equal=$true})
 }
 $pendingResult=@{schema_version='greenlens.v08-report-fixture-result.v1';cli_sha256=$cliHash;cli_source_commit_declared=$CliSourceCommit.ToLowerInvariant();collector_sha256=$collectorHash;fixture_sha256=Get-IssueHash $fixture;scope_revision_sha256=$scopeId;issue=$inspected.summary;products=$checks.products;legacy_regeneration=$legacyResults;final_project_bounds=$finalBounds;renderer_path_engineering_repair_used=[bool]$ResumeVerifiedRendererPath;original_aborted_render_preserved=[bool]$ResumeVerifiedRendererPath;new_simulations_permitted=0;native_interaction_observed=$false;pdf_visual_review='Pending separate page-by-page review';qualification='No physical qualification or private-case acceptance';finished_utc=[DateTime]::UtcNow.ToString('o')}
}catch{$mainFailure=$_;throw}finally{
 $finalChecks=@(@{name='original_source';verify={Assert-IssueSnapshot $Project $original}})
 if($LegacyProject){$finalChecks+=@(@{name='original_legacy_source';verify={Assert-IssueSnapshot $LegacyProject $legacyOriginal}})}
 if($ResumeVerifiedRendererPath){$finalChecks+=@(@{name='aborted_stage';verify={Assert-ReportArtifactSnapshot $abortedStage $abortedStageSnapshot}})}
 $outcomes=@();foreach($check in $finalChecks){try{& $check.verify;$outcomes+=@(@{name=$check.name;status='passed'})}catch{$outcomes+=@(@{name=$check.name;status='failed';error=$_.Exception.Message})}}
 try{Write-IssueJson (Join-Path $OutputDirectory 'collector-final-verification.json') @{schema_version='greenlens.v08-collector-final-verification.v1';checks=$outcomes;primary_failure=$(if($mainFailure){$mainFailure.Exception.Message}else{$null});recorded_utc=[DateTime]::UtcNow.ToString('o')}}catch{Write-Warning ('Could not retain final verification receipt: '+$_.Exception.Message)}
 $failures=@($outcomes|Where-Object status -CEQ 'failed')
 if($failures.Count){$message='Final evidence verification failed: '+(($failures|ForEach-Object {$_.name+': '+$_.error}) -join '; ');if($mainFailure){Write-Warning $message}else{throw $message}}
}
$state=Complete-ReportCollection $state $pendingResult $outcomes {param($value) Write-IssueJson (Join-Path $OutputDirectory 'result.json') $value} {param($value) Write-IssueJson $statePath $value}
Write-Output (ConvertTo-Json -InputObject @{output=$OutputDirectory;completed=$state.completed;issue=$issueId;new_simulations_permitted=0} -Depth 8)
