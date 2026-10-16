# Original synthetic path metadata; no process, renderer or simulation execution.
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'verify-v08-reports.ps1') -FunctionsOnly
$checks=0
$completionState=@{completed=$false};$published=[Collections.Generic.List[string]]::new()
foreach($outcomes in @(@(@{name='source';status='failed'}),@())){
 $rejected=$false;try{Complete-ReportCollection $completionState @{data='verified fixture'} $outcomes {param($v) $published.Add('result')} {param($v) $published.Add('state')}|Out-Null}catch{$rejected=$true}
 if(!$rejected -or $published.Count -or $completionState.completed){throw 'Invalid evidence was published as complete'};$checks++
}
$rejected=$false;try{Complete-ReportCollection $completionState @{data='verified fixture'} @(@{status='passed'}) {param($v) throw 'injected result write failure'} {param($v) $published.Add('state')}|Out-Null}catch{$rejected=$true}
if(!$rejected -or $published.Count -or $completionState.completed){throw 'Failed result publication marked state complete'};$checks++
$completed=Complete-ReportCollection $completionState @{data='verified fixture'} @(@{status='passed'}) {param($v) $published.Add('result')} {param($v) $published.Add('state')}
if(!$completed.completed -or $completionState.completed -or ($published -join ',') -cne 'result,state'){throw 'Successful evidence publication order or original state changed'};$checks++
$system='C:\Windows\System32\conhost.exe';$typst='C:\Synthetic tools\Typst\typst.exe'
function Require-ChildPolicy([string]$Name,[string]$Path,[bool]$Enabled,[bool]$Expected,[string]$Reason) {
 $actual=Test-ReportChildAllowed $Name $Path $system $typst $Enabled
 if($actual -ne $Expected){throw "Unexpected child executable decision: $Reason"}
 $script:checks++
}
Require-ChildPolicy 'conhost.exe' $system $false $true 'canonical Windows console host during copy'
Require-ChildPolicy 'CONHOST.EXE' 'c:\windows\system32\CONHOST.exe' $true $true 'Windows case-insensitive canonical location'
Require-ChildPolicy 'conhost.exe' 'C:\Synthetic tools\conhost.exe' $true $false 'spoofed console-host name'
Require-ChildPolicy 'conhost.exe' 'C:\Windows\System32-foreign\conhost.exe' $true $false 'lookalike system-directory prefix'
Require-ChildPolicy 'conhost.exe' 'C:\Windows\System32\nested\conhost.exe' $true $false 'nested system-directory executable'
Require-ChildPolicy 'conhost.exe' 'C:\Windows\SysWOW64\conhost.exe' $true $false 'undeclared alternative host path'
Require-ChildPolicy 'conhost.exe' $null $true $false 'missing CIM executable path'
Require-ChildPolicy 'conhost.exe' 'conhost.exe' $true $false 'relative path'
Require-ChildPolicy 'conhost.exe' '\Windows\System32\conhost.exe' $true $false 'drive-relative path'
Require-ChildPolicy 'conhost.exe' "C:\Windows\System32\conhost.exe`0" $true $false 'invalid path'
Require-ChildPolicy 'typst.exe' $typst $true $true 'explicitly allowed exact renderer path'
Require-ChildPolicy 'typst.exe' $typst $false $false 'renderer forbidden outside create/regenerate'
Require-ChildPolicy 'typst.exe' 'C:\Another renderer\typst.exe' $true $false 'unbound renderer path'
Require-ChildPolicy 'typst.exe' $system $true $false 'renderer name does not authorize console-host path'
Require-ChildPolicy 'conhost.exe' $typst $true $false 'console-host name does not authorize renderer path'
Require-ChildPolicy 'energyplus.exe' 'C:\Synthetic engines\energyplus.exe' $true $false 'engine is never admitted'
Require-ChildPolicy 'energyplus.exe' $system $true $false 'engine cannot borrow an allowed pathname'
Require-ChildPolicy 'cmd.exe' 'C:\Windows\System32\cmd.exe' $true $false 'unrelated genuine OS executable'
Require-ChildPolicy 'unknown.exe' $typst $true $false 'unknown child name'
Require-ChildPolicy 'typst.exe' ('\\?\'+$typst) $true $true 'extended DOS form of exact pinned path'
Require-ChildPolicy 'conhost.exe' ('\\?\'+$system) $false $true 'extended DOS form of canonical console host'
Require-ChildPolicy 'typst.exe' '\\?\C:\Other\typst.exe' $true $false 'unrelated extended DOS path'
Require-ChildPolicy 'typst.exe' '\\.\C:\Synthetic tools\Typst\typst.exe' $true $false 'device namespace is not a DOS alias'
Require-ChildPolicy 'typst.exe' '\\?\C:\Synthetic tools\Typst\..\Typst\typst.exe' $true $false 'extended dot semantics must not be reinterpreted'
Require-ChildPolicy 'typst.exe' '\\?\C:\Synthetic tools\Typst.\typst.exe' $true $false 'extended trailing-period semantics must not be reinterpreted'
if((ConvertTo-ReportExecutablePath '\\?\UNC\synthetic-server\share\typst.exe') -cne '\\synthetic-server\share\typst.exe'){throw 'Extended UNC physical alias not normalized exactly'};$checks++
function Require-Observation([string]$Name,[string]$Cim,[string]$State,[string]$Path,[bool]$Enabled,[bool]$Expected,[string]$Status) {
 $decision=Get-ReportChildDecision $Name $Cim $State $Path $system $typst $Enabled
 if($decision.allowed -ne $Expected -or $decision.identity_status -cne $Status){throw "Unexpected fallback decision: $Name / $State"};$script:checks++
}
Require-Observation 'conhost.exe' $null 'exited' $null $false $true 'proven_exited_path_unobserved'
Require-Observation 'conhost.exe' $null 'live' $system $false $true 'live_handle_path_observed'
Require-Observation 'conhost.exe' $null 'live' 'C:\Elsewhere\conhost.exe' $false $false 'live_handle_path_observed'
Require-Observation 'conhost.exe' $null 'unknown' $null $false $false 'identity_unavailable_rejected'
Require-Observation 'conhost.exe' $null 'reused_or_changed_identity' $system $false $false 'identity_unavailable_rejected'
Require-Observation 'energyplus.exe' $null 'exited' $null $true $false 'identity_unavailable_rejected'
Require-Observation 'unknown.exe' $null 'exited' $null $true $false 'identity_unavailable_rejected'
Require-Observation 'typst.exe' $null 'exited' $null $false $false 'identity_unavailable_rejected'
Require-Observation 'typst.exe' $null 'live' $typst $true $true 'live_handle_path_observed'
Require-Observation 'typst.exe' $null 'live' $null $true $false 'live_handle_path_observed'
Require-Observation 'conhost.exe' 'C:\Elsewhere\conhost.exe' 'exited' $null $false $false 'cim_path_observed'
$original=@{energy=@{designs=@{A='accepted';B='accepted'};runs=@('retained-A','retained-B')};architecture=@{scope=@{title='Original synthetic scope'};assets=@('retained-reference')}}
$current=Copy-IssueJson $original
$current.architecture.scope_history=@{active_revision_sha256='synthetic-revision';revisions=@(@{revision_sha256='synthetic-revision';parent_revision_sha256=$null;scope=$original.architecture.scope})}
$state=@{completed=$false;new_issue_attempt_started=$false;legacy_regeneration_attempts=@();commands=@('complete-save-as','verify-run-A','verify-run-B','verify-compatible-pair','adopt-old-scope'|ForEach-Object {@{name=$_}})}
$declaration=@{new_issue_attempts=1;legacy_regeneration_attempts_per_selected_issue=1;maximum_energyplus_processes=0;maximum_seconds_per_command=600}
Assert-ReportPreparationResume $state $declaration $current $original $original.energy;$checks++
function Require-ResumeRejection([scriptblock]$Mutate) {
 $s=Copy-IssueJson $state;$d=Copy-IssueJson $declaration;$c=Copy-IssueJson $current
 & $Mutate $s $d $c
 $rejected=$false;try{Assert-ReportPreparationResume $s $d $c $original $original.energy}catch{$rejected=$true}
 if(!$rejected){throw 'Unsafe preparation resume was admitted'};$script:checks++
}
Require-ResumeRejection {param($s,$d,$c) $s.new_issue_attempt_started=$true}
Require-ResumeRejection {param($s,$d,$c) $s.legacy_regeneration_attempts=@('already-attempted')}
Require-ResumeRejection {param($s,$d,$c) $s.commands+=@(@{name='issue-once'})}
Require-ResumeRejection {param($s,$d,$c) $d.maximum_energyplus_processes=1}
Require-ResumeRejection {param($s,$d,$c) $c.energy.designs.B='modified'}
Require-ResumeRejection {param($s,$d,$c) $c.architecture.assets=@('different-reference')}
Require-ResumeRejection {param($s,$d,$c) $c.architecture.scope_history.active_revision_sha256='different'}
# Contract-shaped v2 review: the mechanical carry-forward proposal is expected
# to differ from the explicit candidate. Neither one may replace the other.
$parentHash='a'*64;$fixtureHash='b'*64;$revisionHash='c'*64;$geometryHash='d'*64
$copied=@{energy=@{designs=@{A=@{model=@{zones=@()}};B=@{model=@{zones=@(@{id='zone-B';cooling_system=@{id='dx-B'}})}}};runs=@('genuine-bound-A','genuine-bound-B')};architecture=@{schema_version='greenlens.architectural-project.v1';scope=@{schema_version='greenlens.architectural-scope.v1';title='Accepted initial scope'};assets=@(@{id='old';path='references/old'});active_asset_id='old'}}
$imported=Copy-IssueJson $copied.architecture
$imported.scope_history=@{schema_version='greenlens.scope-history.v1';active_revision_sha256=$parentHash;revisions=@(@{revision_sha256=$parentHash;parent_revision_sha256=$null;scope=$copied.architecture.scope})}
$imported.assets+=@(@{id='new';path='references/new';revision_sha256=$revisionHash;geometry_sha256=$geometryHash});$imported.active_asset_id='new'
$reviewDocument=Copy-IssueJson $copied;$reviewDocument.architecture=Copy-IssueJson $imported
$manifest=@{source_sha256=$fixtureHash;revision_sha256=$revisionHash;geometry_sha256=$geometryHash;occurrences=@(0..80|ForEach-Object {@{id=('occurrence-'+$_)}})}
$context=@{reference_options_sha256=('e'*64);designs=@{A=@{model_input_sha256=('f'*64)};B=@{model_input_sha256=('1'*64)}}}
$candidate=New-ReportFixtureScope $manifest $context $reviewDocument
$proposal=Copy-IssueJson $candidate;$proposal.title='Mechanical proposal retained independently';$proposal.mappings=@()
$plan=@{schema_version='greenlens.scope-carry-forward.v2';parent_revision_sha256=$parentHash;target_context_sha256=('2'*64);plan_sha256=('3'*64);candidate_scope=$candidate;proposed_scope=$proposal;rows=@(@{id='mapping-review';kind='mapping';change='changed';requires_review=$true})}
Assert-ReportFixtureReview $plan $candidate $parentHash;$checks++
$wirePlan=Copy-IssueJson $plan;$wirePlan.candidate_scope.Remove('report_views')|Out-Null
Assert-ReportFixtureReview $wirePlan $candidate $parentHash;$checks++
if(Test-IssueJsonEqual $plan.proposed_scope $candidate){throw 'Fixture must distinguish proposal from explicit candidate'};$checks++
function Require-ReviewRejection([scriptblock]$Mutate) {
 $p=Copy-IssueJson $plan;& $Mutate $p
 $rejected=$false;try{Assert-ReportFixtureReview $p $candidate $parentHash}catch{$rejected=$true}
 if(!$rejected){throw 'An invalid explicit candidate review was admitted'};$script:checks++
}
Require-ReviewRejection {param($p) $p.schema_version='greenlens.scope-carry-forward.v1'}
Require-ReviewRejection {param($p) $p.candidate_scope=$p.proposed_scope}
Require-ReviewRejection {param($p) $p.candidate_scope=$null}
Require-ReviewRejection {param($p) $p.parent_revision_sha256='4'*64}
Require-ReviewRejection {param($p) $p.target_context_sha256='not-a-hash'}
Require-ReviewRejection {param($p) $p.proposed_scope=$null}
Require-ReviewRejection {param($p) $p.candidate_scope.report_views=$null}
Require-ReviewRejection {param($p) $p.candidate_scope.report_views=@(@{name='unexpected retained view'})}
Require-ReviewRejection {param($p) $p.rows=@(@{kind='input';change='changed'})}
$reviewState=@{completed=$false;new_issue_attempt_started=$false;legacy_regeneration_attempts=@();commands=@('complete-save-as','verify-run-A','verify-run-B','verify-compatible-pair','adopt-old-scope','resume-verified-open','reference-capacity','import-original-fixture','inspect-original-reference','scope-context','assess-fixture-scope','review-fixture-scope'|ForEach-Object {@{name=$_}})}
Assert-ReportReviewResume $reviewState $declaration $reviewDocument $copied $imported $manifest $context $candidate $plan $copied.energy $fixtureHash;$checks++
function Require-ReviewResumeRejection([scriptblock]$Mutate) {
 $s=Copy-IssueJson $reviewState;$d=Copy-IssueJson $declaration;$c=Copy-IssueJson $reviewDocument;$i=Copy-IssueJson $imported;$m=Copy-IssueJson $manifest;$x=Copy-IssueJson $context;$sc=Copy-IssueJson $candidate;$p=Copy-IssueJson $plan
 & $Mutate $s $d $c $i $m $x $sc $p
 $rejected=$false;try{Assert-ReportReviewResume $s $d $c $copied $i $m $x $sc $p $copied.energy $fixtureHash}catch{$rejected=$true}
 if(!$rejected){throw 'An unsafe completed-review checkpoint was admitted'};$script:checks++
}
Require-ReviewResumeRejection {param($s) $s.new_issue_attempt_started=$true}
Require-ReviewResumeRejection {param($s) $s.legacy_regeneration_attempts=@('attempted')}
Require-ReviewResumeRejection {param($s) $s.commands+=@(@{name='accept-reviewed-fixture-scope'})}
Require-ReviewResumeRejection {param($s,$d) $d.maximum_seconds_per_command=601}
Require-ReviewResumeRejection {param($s,$d,$c) $c.energy.runs=@('different')}
Require-ReviewResumeRejection {param($s,$d,$c) $c.architecture.scope_history.active_revision_sha256='4'*64}
Require-ReviewResumeRejection {param($s,$d,$c) $c.unrelated='changed'}
Require-ReviewResumeRejection {param($s,$d,$c,$i) $i.assets[0].path='references/substituted';$c.architecture=Copy-IssueJson $i}
Require-ReviewResumeRejection {param($s,$d,$c,$i,$m) $m.source_sha256='4'*64}
Require-ReviewResumeRejection {param($s,$d,$c,$i,$m) $m.geometry_sha256='4'*64}
Require-ReviewResumeRejection {param($s,$d,$c,$i,$m,$x) $x.designs.B.model_input_sha256='4'*64}
Require-ReviewResumeRejection {param($s,$d,$c,$i,$m,$x,$sc) $sc.assumptions[82].value='silently edited'}
$decisionPlan=Copy-IssueJson $plan
$decisionPlan.rows=@(
 @{id='context';kind='mapping';key='occurrence-0';change='changed';requires_review=$true},
 @{id='excluded';kind='mapping';key='occurrence-79';change='changed';requires_review=$true},
 @{id='unresolved';kind='mapping';key='occurrence-80';change='changed';requires_review=$true},
 @{id='deleted';kind='mapping';key='removed-old';change='deleted';requires_review=$true},
 @{id='synthetic';kind='assumption';key='source-000';change='added';requires_review=$true},
 @{id='missing';kind='assumption';key='source-080';change='added';requires_review=$true},
 @{id='conflict-one';kind='assumption';key='conflict-one';change='added';requires_review=$true},
 @{id='conflict-two';kind='assumption';key='conflict-two';change='added';requires_review=$true},
 @{id='old-view';kind='report_view';key='removed-view';change='deleted';requires_review=$true},
 @{id='reference';kind='reference';key='occurrence-0';change='added';requires_review=$true})
$decisions=New-ReportFixtureDecisions $decisionPlan $candidate
if($decisions.Count -ne 10 -or !(Test-IssueJsonEqual @($decisions|Where-Object resolution -CEQ 'unresolved'|ForEach-Object row_id) @('unresolved','missing'))){throw 'Explicit candidate decisions erased an unresolved/missing state or misclassified a resolved declaration'};$checks++
foreach($id in @('context','excluded','deleted','synthetic','conflict-one','conflict-two','old-view','reference')){
 $d=@($decisions|Where-Object row_id -CEQ $id)
 if($d.Count -ne 1 -or $d[0].resolution -cne 'reviewed'){throw "Explicit review did not preserve the candidate declaration: $id"};$checks++
}
foreach($id in @('conflict-one','conflict-two')){if(!(@($decisions|Where-Object row_id -CEQ $id)[0].rationale.Contains('remain conflicting'))){throw 'Contradictory fixture declaration was portrayed as resolved'};$checks++}
if($candidate.assumptions[81].value -cne '3.6' -or $candidate.assumptions[82].value -cne '3.7'){throw 'Review decisions changed applied candidate source values'};$checks++
$decisionState=Copy-IssueJson $reviewState;$decisionState.commands+=@(@{name='resume-review-fixture-scope'},@{name='accept-reviewed-fixture-scope'})
$manifestHash='4'*64;$reviewRepair=@{schema_version='greenlens.v08-collector-review-repair.v1';working_manifest_sha256=$manifestHash;prior_plan_sha256=$plan.plan_sha256}
$failedReceipt=@{exit_code=1;failure=$null}
$failedError='GreenLens: revision.review: An unresolved decision cannot be presented as a resolved mapping, measurement, view or source declaration'
Assert-ReportDecisionsResume $decisionState $declaration $reviewDocument $copied $imported $manifest $context $candidate $plan $copied.energy $fixtureHash $reviewRepair $failedReceipt $failedError $manifestHash;$checks++
function Require-DecisionsResumeRejection([scriptblock]$Mutate) {
 $s=Copy-IssueJson $decisionState;$r=Copy-IssueJson $reviewRepair;$f=Copy-IssueJson $failedReceipt
 & $Mutate $s $r $f
 $rejected=$false;try{Assert-ReportDecisionsResume $s $declaration $reviewDocument $copied $imported $manifest $context $candidate $plan $copied.energy $fixtureHash $r $f $failedError $manifestHash}catch{$rejected=$true}
 if(!$rejected){throw 'Unsafe acceptance-error repair was admitted'};$script:checks++
}
Require-DecisionsResumeRejection {param($s) $s.new_issue_attempt_started=$true}
Require-DecisionsResumeRejection {param($s) $s.commands+=@(@{name='accept-reviewed-fixture-scope-corrected'})}
Require-DecisionsResumeRejection {param($s,$r) $r.working_manifest_sha256='5'*64}
Require-DecisionsResumeRejection {param($s,$r) $r.prior_plan_sha256='5'*64}
Require-DecisionsResumeRejection {param($s,$r,$f) $f.exit_code=0}
Require-DecisionsResumeRejection {param($s,$r,$f) $f.failure='unrelated child failure'}
$pinned=@{requested_path=$typst;physical_path='C:\Synthetic package\LocalCache\Typst\typst.exe';volume_serial='12345678';file_id='0000000000000001';bytes=1234;sha256=('7'*64)}
$same=Copy-IssueJson $pinned;$same.requested_path=$pinned.physical_path
if(!(Test-ReportSamePinnedFile $pinned $same $pinned.sha256)){throw 'Proven logical/physical same-file identities were rejected'};$checks++
foreach($field in @('file_id','volume_serial','sha256','physical_path','bytes')){
 $different=Copy-IssueJson $same;if($field -eq 'bytes'){$different[$field]=1235}else{$different[$field]='different'}
 if(Test-ReportSamePinnedFile $pinned $different $pinned.sha256){throw "Different pinned file admitted: $field"};$checks++
}
foreach($case in @(
 @('typst.exe',$pinned.physical_path,'not_required',$null,$true,$true),
 @('typst.exe',('\\?\'+$pinned.physical_path),'not_required',$null,$true,$true),
 @('typst.exe',$null,'live',$pinned.physical_path,$true,$true),
 @('typst.exe',$pinned.physical_path,'not_required',$null,$false,$false),
 @('typst.exe','C:\Other package\typst.exe','not_required',$null,$true,$false),
 @('energyplus.exe',$pinned.physical_path,'not_required',$null,$true,$false),
 @('unknown.exe',$pinned.physical_path,'not_required',$null,$true,$false),
 @('typst.exe',$null,'unknown',$null,$true,$false))){
 $decision=Get-ReportPinnedChildDecision $case[0] $case[1] $case[2] $case[3] $system $typst $case[4] $pinned
 if($decision.allowed -ne $case[5]){throw 'Pinned physical alias weakened child admission'};$checks++
}
$rendererState=Copy-IssueJson $decisionState;$rendererState.commands+=@(@{name='accept-reviewed-fixture-scope-corrected'},@{name='issue-capacity'},@{name='issue-once'});$rendererState.new_issue_attempt_started=$true
$rendererDeclaration=Copy-IssueJson $declaration;$rendererDeclaration.baseline=@{id='genuine-A'};$rendererDeclaration.proposed=@{id='genuine-B'}
$rendererCopied=Copy-IssueJson $copied;$rendererCopied.energy.designs.A.study=@{profile='unchanged-10min'};$rendererCopied.energy.designs.B.study=@{profile='unchanged-10min'}
$accepted=@{revision_sha256=('6'*64);parent_revision_sha256=$parentHash;scope=(Get-ReportScopeWireForm $candidate)}
$acceptedHistory=@{schema_version='greenlens.scope-history.v1';active_revision_sha256=$accepted.revision_sha256;revisions=@($imported.scope_history.revisions[0],$accepted)}
$rendererDocument=Copy-IssueJson $rendererCopied;$rendererDocument.architecture=Copy-IssueJson $imported;$rendererDocument.architecture.scope_history=$acceptedHistory;$rendererDocument.architecture.scope=$accepted.scope
$stageContent=@{scope_revision=$accepted;include_pdf=$true;renderer=@{method='greenlens.energy-report-issue-renderer/2'};workbench=@{base=@{baseline=@{run_id='genuine-A';model=$rendererCopied.energy.designs.A.model;study=$rendererCopied.energy.designs.A.study};proposed=@{run_id='genuine-B';model=$rendererCopied.energy.designs.B.model;study=$rendererCopied.energy.designs.B.study}}}}
$abortedReceipt=@{exit_code=$null;failure='An undeclared child executable started; only the canonical Windows console host and explicitly allowed pinned Typst path are permitted, never an engine';request=@{allow_typst=$true};observed_children=@(@{name='typst.exe';policy_allowed=$false;executable_path=$pinned.physical_path})}
Assert-ReportRendererResume $rendererState $rendererDeclaration $rendererDocument $rendererCopied $imported $acceptedHistory $candidate $plan $rendererCopied.energy $stageContent $abortedReceipt $pinned;$checks++
function Require-RendererResumeRejection([scriptblock]$Mutate) {
 $s=Copy-IssueJson $rendererState;$c=Copy-IssueJson $rendererDocument;$d=Copy-IssueJson $stageContent;$f=Copy-IssueJson $abortedReceipt
 & $Mutate $s $c $d $f
 $rejected=$false;try{Assert-ReportRendererResume $s $rendererDeclaration $c $rendererCopied $imported $acceptedHistory $candidate $plan $rendererCopied.energy $d $f $pinned}catch{$rejected=$true}
 if(!$rejected){throw 'Unsafe renderer engineering repair admitted'};$script:checks++
}
Require-RendererResumeRejection {param($s) $s.new_issue_attempt_started=$false}
Require-RendererResumeRejection {param($s) $s.renderer_path_repair_attempt_started=$true}
Require-RendererResumeRejection {param($s) $s.legacy_regeneration_attempts=@('used')}
Require-RendererResumeRejection {param($s,$c) $c.report_issues=@{issues=@()}}
Require-RendererResumeRejection {param($s,$c) $c.energy.designs.B.study.profile='tuned'}
Require-RendererResumeRejection {param($s,$c,$d) $d.workbench.base.proposed.run_id='different'}
Require-RendererResumeRejection {param($s,$c,$d) $d.scope_revision.revision_sha256='5'*64}
Require-RendererResumeRejection {param($s,$c,$d,$f) $f.exit_code=0}
Require-RendererResumeRejection {param($s,$c,$d,$f) $f.observed_children[0].executable_path='C:\Other\typst.exe'}
$publishedSummary=@{id='original-issue';scope_revision_sha256='old-scope';content_sha256=('8'*64)}
$legacy=@{id='persistent-document';report_issues=@{issues=@($publishedSummary)}}
$pending=@{schema_version='greenlens.staged-report-issue.v1';project_id='persistent-document';summary=$publishedSummary}
if(!(Test-ReportPublishedHistoricalStage $pending $legacy @('original-issue') 'current-scope')){throw 'Already-published exact selected history residue was misclassified'};$checks++
foreach($mutation in @(
 {param($p) $p.summary.content_sha256='9'*64},
 {param($p) $p.summary.scope_revision_sha256='current-scope'},
 {param($p) $p.summary.id='other-pending-issue'},
 {param($p) $p.project_id='different-document'})){
 $p=Copy-IssueJson $pending;& $mutation $p
 if(Test-ReportPublishedHistoricalStage $p $legacy @('original-issue') 'current-scope'){throw 'Unrecognized/conflicting pending stage was dismissed'};$checks++
}
# One bounded read of this already-running test process exercises the actual
# Windows image-path API and creation binding. It launches no child process.
if($IsWindows){
 $self=[Diagnostics.Process]::GetCurrentProcess()
 try{
  $identity=Get-ReportChildHandleIdentity $self.Id $self.StartTime
  if($identity.State -cne 'live' -or ![IO.Path]::IsPathFullyQualified($identity.ImagePath)){throw 'Own-process Windows image-path query failed'};$checks++
  $mismatch=Get-ReportChildHandleIdentity $self.Id ($self.StartTime.AddSeconds(-1))
  if($mismatch.State -cne 'reused_or_changed_identity'){throw 'Creation-time mismatch was not rejected'};$checks++
 }finally{$self.Dispose()}
 $lease=Open-ReportPinnedFile $PSCommandPath;$physicalLease=$null
 try{
  $identity=Get-ReportPinnedIdentity $lease;$physicalLease=Open-ReportPinnedFile $identity.physical_path
  if(!(Test-ReportSamePinnedFile $identity (Get-ReportPinnedIdentity $physicalLease) (Get-IssueHash $PSCommandPath))){throw 'Same-handle source identity does not match its physical-path reopen'};$checks++
 }finally{if($physicalLease){$physicalLease.Dispose()};$lease.Dispose()}
 # A renderer stage is not a project: snapshot every bounded artifact and do
 # not invent project.json or ignore products outside project subdirectories.
 $stageRoot=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA ('GreenLens\verification\collector-artifact-test-'+[Guid]::NewGuid().ToString('N'))))
 $allowedRoot=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens\verification'))
 Assert-IssueChild $allowedRoot $stageRoot
 try{
  New-Item -ItemType Directory -Path (Join-Path $stageRoot 'products') -Force|Out-Null
  [IO.File]::WriteAllText((Join-Path $stageRoot 'products\fixture.txt'),'original synthetic retained product')
  [IO.File]::WriteAllText((Join-Path $stageRoot 'receipt.json'),'{}')
  $snapshot=Get-ReportArtifactSnapshot $stageRoot
  if($snapshot.Count -ne 2 -or $snapshot.Contains('project.json') -or !$snapshot.Contains('products/fixture.txt')){throw 'Artifact snapshot assumed a project or omitted products'};$checks++
  Assert-ReportArtifactSnapshot $stageRoot $snapshot;$checks++
  [IO.File]::WriteAllText((Join-Path $stageRoot 'products\fixture.txt'),'tampered original fixture')
  $rejected=$false;try{Assert-ReportArtifactSnapshot $stageRoot $snapshot}catch{$rejected=$true}
  if(!$rejected){throw 'Changed artifact bytes escaped final verification'};$checks++
 }finally{
  # Both paths are fixed absolute directories under this test-owned prefix.
  Assert-IssueChild $allowedRoot ([IO.Path]::GetFullPath($stageRoot))
  if(Test-Path -LiteralPath $stageRoot){Remove-Item -LiteralPath $stageRoot -Recurse -Force}
 }
}
foreach($script in @('verify-v08-reports.ps1','test-v08-report-collector.ps1')){
 $tokens=$null;$errors=$null
 [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $script),[ref]$tokens,[ref]$errors)|Out-Null
 if($errors.Count){throw ($errors.Message -join '; ')};$checks++
}
Write-Output "Passed $checks collector path/resume/syntax and own-process identity checks. No CLI, renderer, engine, project edits, child process or application interaction executed."
