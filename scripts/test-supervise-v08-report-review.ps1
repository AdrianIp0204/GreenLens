#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. Source-only checks: no app, renderer,
# simulation, NativeProcess method, or supervisor top-level execution.
[CmdletBinding()]
param([string]$OutputDirectory=(Join-Path $env:LOCALAPPDATA ('GreenLens\verification\report-supervisor-source-'+[Guid]::NewGuid().ToString('N'))))
$ErrorActionPreference='Stop'
$source=Join-Path $PSScriptRoot 'supervise-v08-report-review.ps1'
$output=[IO.Path]::GetFullPath($OutputDirectory)
$cache=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
if(!$output.StartsWith($cache+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $output)){throw 'Use a new source-check receipt directory under LOCALAPPDATA/GreenLens.'}
New-Item -ItemType Directory -Path $output|Out-Null
$checks=[Collections.Generic.List[object]]::new();$failure=$null
function Check([string]$Label,[bool]$Condition){if(!$Condition){throw ('Failed source check: '+$Label)};$checks.Add(@{name=$Label;passed=$true})}
function Must-Reject([scriptblock]$Action,[string]$Label){$rejected=$false;try{& $Action|Out-Null}catch{$rejected=$true};Check $Label $rejected}
function Parse-Original([string]$Json){$parsed=ConvertFrom-Json -InputObject $Json -AsHashtable -DateKind String -Depth 100 -NoEnumerate;return ,$parsed}
function Clone-Original($Value){return ,(Parse-Original (ConvertTo-Json -InputObject $Value -Depth 100 -Compress))}
try{
 $sourceText=[IO.File]::ReadAllText($source)
 $tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($source,[ref]$tokens,[ref]$errors)
 Check 'Supervisor PowerShell AST has zero errors' ($errors.Count -eq 0)
 foreach($f in $ast.FindAll({param($a) $a -is [Management.Automation.Language.FunctionDefinitionAst]},$true)){. ([scriptblock]::Create($f.Extent.Text))}
 . (Join-Path $PSScriptRoot 'observed-scope-history.ps1')
 $embedded=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.CommandAst] -and $a.GetCommandName() -eq 'Add-Type' -and $a.Extent.Text.Contains('namespace GreenLensReportObservation')},$true))
 if($embedded.Count -ne 1){throw 'Expected one bounded-output helper.'}
 Add-Type -TypeDefinition $embedded[0].CommandElements[-1].Value
 Add-Type -LiteralPath (Join-Path $PSScriptRoot 'annual-paired-native-process.cs')
 Add-Type -LiteralPath (Join-Path $PSScriptRoot 'observed-process-image.cs')
 Check 'Embedded/helper C# compiles; held-limit API exists (not invoked)' ($null -ne [GreenLensPairedProcess.NativeProcess].GetProperty('CommittedMemoryLimitSnapshot'))
 Check 'Held-image API compiles (not invoked)' ($null -ne [GreenLensObservation.HeldProcessImage].GetMethod('Read'))
 $fresh=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.CommandAst] -and $a.GetCommandName() -ceq 'Assert-FreshObserverHost'},$true))
 Check 'Fresh-host guard exists before input admission' ($fresh.Count -eq 1 -and $fresh[0].Extent.StartOffset -lt $sourceText.IndexOf('$scopeHelper='))
 $message=$null;try{. ([scriptblock]::Create($fresh[0].Extent.Text))}catch{$message=$_.Exception.Message}
 Check 'Actual run guard rejects already loaded helper assembly before launch' ($message -like 'A fresh PowerShell process is required;*')
 foreach($length in @(0,100,1MB,(1MB+1),3MB)){
  $data=[byte[]]::new($length);[Array]::Fill[byte]($data,65);$stream=[IO.MemoryStream]::new($data,$false);$capture=[GreenLensReportObservation.Capture]::new()
  try{$null=$capture.Drain($stream).GetAwaiter().GetResult()}finally{$stream.Dispose()}
  $retained=$capture.Bytes;$want=[Math]::Min(1MB,$length)
  Check ('Bounded original in-memory stream '+$length) ($capture.Total -eq $length -and $retained.Length -eq $want -and $capture.Overflow -eq ($length -gt 1MB) -and ($want -eq 0 -or ($retained[0] -eq 65 -and $retained[-1] -eq 65)))
 }
 $shapeCases=@('null','[]','[null]','[[]]','[[null]]','[1]','[[1]]','[1,2]','[[1,2],[],null]','{}','{"a":[]}','{"a":[null,[],[1],{"b":[],"c":null}],"z":null}','false','0','1','"1"','""')
 $identities=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($json in $shapeCases){
  $parsed=Parse-Original $json;$normalized=ConvertTo-Json -InputObject (Stable-Value -Value $parsed) -Compress -Depth 100
  Check ('Canonical JSON preserves exact shape '+$json) ($normalized -ceq $json)
  $hash=Json-Identity $parsed;Check ('Distinct identity for shape '+$json) $identities.Add($hash)
  $expected=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($json))).ToLowerInvariant()
  Check ('Identity hashes actual canonical JSON '+$json) ($hash -ceq $expected)
  $path=Join-Path $output ('shape-'+$identities.Count+'.json');[IO.File]::WriteAllText($path,$json,[Text.UTF8Encoding]::new($false))
  $read=Read-Json $path 1KB;Check ('Bounded reader preserves root shape '+$json) ((Json-Identity $read) -ceq $hash)
 }
 Check 'Dictionary ordering is deterministic inside nested arrays' ((Json-Identity (Parse-Original '{"z":null,"a":[{"b":2,"a":1}]}')) -ceq (Json-Identity (Parse-Original '{"a":[{"a":1,"b":2}],"z":null}')))
 Check 'Array order remains significant' ((Json-Identity (Parse-Original '[1,2]')) -cne (Json-Identity (Parse-Original '[2,1]')))
 Check 'Null child position remains significant' ((Json-Identity (Parse-Original '[null,1]')) -cne (Json-Identity (Parse-Original '[1,null]')))
 $fixture=Join-Path $output 'original-fixture.json';[IO.File]::WriteAllText($fixture,'{"at":"2026-10-16T00:00:00Z","value":1}',[Text.UTF8Encoding]::new($false))
 Check 'Bounded reader preserves timestamps as strings' ((Read-Json $fixture 1KB).at -is [string])
 Must-Reject {Read-Json $fixture 4} 'Bounded reader rejects oversized input'
 foreach($emptyPath in @($null,'',' ')){
  $message=$null;try{File-Hash $emptyPath|Out-Null}catch{$message=$_.Exception.Message};Check 'Empty observed path fails before FileStream construction' ($message -ceq 'Cannot verify bytes without an observed nonempty file path.')
  $message=$null;try{Assert-MemberPolicy @{name='typst';image_path=$emptyPath;has_exited=$true} ('0'*64)|Out-Null}catch{$message=$_.Exception.Message};Check 'Missing renderer identity is explicit even after exit' ($message -ceq 'Observed renderer executable identity is unavailable; no renderer verification is accepted.')
 }
 foreach($memberName in @('energyplus','EnergyPlus','expandobjects','readvarseso')){Must-Reject {Assert-MemberPolicy @{name=$memberName;has_exited=$true} ('0'*64)} ('Forbidden member remains rejected after exit: '+$memberName)}
 Must-Reject {Assert-MemberPolicy @{name='typst';image_path=$fixture;has_exited=$true} ('0'*64)} 'Mismatched renderer bytes remain rejected after exit'
 Check 'Exact fixture bytes accepted only as identity check; ordinary process not renderer' ((Assert-MemberPolicy @{name='typst';image_path=$fixture} (File-Hash $fixture)) -and !(Assert-MemberPolicy @{name='greenlens';image_path=$fixture} (File-Hash $fixture)))
 $policy=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.CommandAst] -and $a.GetCommandName() -eq 'Assert-MemberPolicy'},$true));$ancestor=$policy[0].Parent
 while($ancestor -and $ancestor -isnot [Management.Automation.Language.TryStatementAst]){$ancestor=$ancestor.Parent}
 Check 'Policy assertion remains outside member-inspection race catch' ($ancestor -and $ancestor.Extent.Text.Contains('::StartBounded('))
 $sourceText=[IO.File]::ReadAllText($source)
 Check 'No nullable module enumeration or expected-path substitution' (!$sourceText.Contains('.MainModule') -and $sourceText.Contains('[GreenLensObservation.HeldProcessImage]::Read($held,[int]$member,$started.Ticks)'))
 $unknown=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.AssignmentStatementAst] -and $a.Left.Extent.Text -ceq '$record.process_identity_failure'},$true))
 Check 'Unknown membership is recorded and fails instead of silently skipped' ($unknown.Count -eq 1 -and $unknown[0].Parent.Extent.Text.Contains('unidentified membership cannot pass'))
 Check 'Unknown-member record retains original exception category and stack' ($unknown[0].Extent.Text.Contains('exception_type=') -and $unknown[0].Extent.Text.Contains('category=') -and $unknown[0].Extent.Text.Contains('script_stack_trace='))
 $boundImage=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $a.Member.Value -ceq 'ValidateBinding'},$true))
 Check 'Cache lookup follows held PID/creation-time check' ($boundImage.Count -eq 1 -and $sourceText.IndexOf('if($seen.ContainsKey($key))') -gt $boundImage[0].Extent.EndOffset)
 $membership=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $a.Member.Value -ceq 'RequireMember'},$true))
 Check 'Exact held Job membership precedes identity and cache lookup' ($membership.Count -eq 1 -and $membership[0].Extent.EndOffset -lt $boundImage[0].Extent.StartOffset)
 $scope1='c'*64;$scope2='d'*64
 $manifest=@{id='original-synthetic-project';runs=@();climate=$null;energy=@{runs=@(@{id='original-baseline';alternative_id='A';record_sha256=('a'*64);path='energy-runs/original-baseline'},@{id='original-proposed';alternative_id='B';record_sha256=('b'*64);path='energy-runs/original-proposed'})};architecture=@{scope_history=@{schema_version='greenlens.scope-history.v1';active_revision_sha256=$scope2;revisions=@(@{schema_version='greenlens.scope-revision.v3';revision_sha256=$scope1},@{schema_version='greenlens.scope-revision.v3';revision_sha256=$scope2})}};report_issues=@{issues=@(@{id=('e'*64);products_sha256=('f'*64);products_file_sha256=('1'*64);baseline=@{run_id='original-baseline';record_sha256=('a'*64);design_id='A'};proposed=@{run_id='original-proposed';record_sha256=('b'*64);design_id='B'}})}}
 $manifestBefore=Json-Identity $manifest;$copy=Clone-Original $manifest
 Assert-NoNewRuns $manifest $copy;Check 'Empty legacy/static runs and null climate survive startup inventory check' $true
 $singleton=Clone-Original $manifest;$singleton.energy.runs=@($singleton.energy.runs[0]);Assert-NoNewRuns $singleton (Clone-Original $singleton);Check 'Singleton energy inventory retains array shape' $true
 $empty=Clone-Original $manifest;$empty.energy.runs=@();Assert-NoNewRuns $empty (Clone-Original $empty);Check 'Empty energy inventory compares successfully' $true
 $nullRuns=Clone-Original $manifest;$nullRuns.runs=$null;Assert-NoNewRuns $nullRuns (Clone-Original $nullRuns);Check 'Null legacy inventory compares successfully without becoming empty' $true
 Must-Reject {Assert-NoNewRuns $manifest $nullRuns} 'Null versus empty legacy run inventory rejected'
 $bad=Clone-Original $manifest;$bad.energy.runs=$bad.energy.runs[0];Must-Reject {Assert-NoNewRuns $singleton $bad} 'Singleton array versus scalar run inventory rejected'
 $bad=Clone-Original $manifest;$bad.energy.runs=@($bad.energy.runs[1],$bad.energy.runs[0]);Must-Reject {Assert-NoNewRuns $manifest $bad} 'Reordered retained run inventory rejected'
 $bad=Clone-Original $manifest;$bad.energy.runs[0].record_sha256='2'*64;Must-Reject {Assert-NoNewRuns $manifest $bad} 'Resealed changed record under same run ID rejected'
 $bad=Clone-Original $manifest;$bad.energy.runs+=,@{id='new';alternative_id='A';record_sha256=('3'*64)};Must-Reject {Assert-NoNewRuns $manifest $bad} 'Additional retained run rejected'
 $bad=Clone-Original $manifest;$bad.energy.runs=@($bad.energy.runs[0]);Must-Reject {Assert-NoNewRuns $manifest $bad} 'Deleted retained run rejected'
 Assert-OldIssues $manifest (Clone-Original $manifest);Check 'One retained issue remains inspectable' $true
 $bad=Clone-Original $manifest;$bad.report_issues.issues[0].products_sha256='4'*64;$bad.report_issues.issues[0].products_file_sha256='5'*64;Must-Reject {Assert-OldIssues $manifest $bad} 'Resealed product summary under same issue ID rejected'
 $bad=Clone-Original $manifest;$bad.report_issues.issues[0].baseline.record_sha256='6'*64;Must-Reject {Assert-OldIssues $manifest $bad} 'Changed historical pair record rejected'
 $arm=@{project_id=$manifest.id;baseline_run_id='original-baseline';proposed_run_id='original-proposed';scope_revision_sha256=$scope1};$selection=Assert-ArmSelection $arm $manifest $manifest.id
 Check 'Historical scope and exact retained pair accepted' ($selection.scope_revision_sha256 -ceq $scope1 -and $selection.baseline.record_sha256 -ceq ('a'*64) -and $selection.proposed.design_id -ceq 'B')
 foreach($field in @('baseline_run_id','proposed_run_id','scope_revision_sha256')){$bad=Clone-Original $arm;$bad[$field]=if($field -eq 'scope_revision_sha256'){'7'*64}else{'absent'};Must-Reject {Assert-ArmSelection $bad $manifest $manifest.id} ('Missing saved arm binding rejected: '+$field)}
 $bad=Clone-Original $manifest;$bad.energy.runs+=,(Clone-Original $bad.energy.runs[0]);Must-Reject {Assert-ArmSelection $arm $bad $manifest.id} 'Duplicate armed run rejected'
 $bad=Clone-Original $manifest;$bad.architecture.scope_history.revisions+=,(Clone-Original $bad.architecture.scope_history.revisions[0]);Must-Reject {Assert-ArmSelection $arm $bad $manifest.id} 'Duplicate armed scope rejected'
 $bad=Clone-Original $manifest;$bad.id='other';Must-Reject {Assert-ArmSelection $arm $bad $manifest.id} 'Wrong saved project rejected'
 $bad=Clone-Original $arm;$bad.project_id='other';Must-Reject {Assert-ArmSelection $bad $manifest $manifest.id} 'Wrong arm project rejected'
 Check 'Inspection helpers do not mutate the original synthetic manifest' ((Json-Identity $manifest) -ceq $manifestBefore)
 $assignments=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.AssignmentStatementAst]},$true))
 $hashAssign=@($assignments|Where-Object {$_.Left.Extent.Text -ceq '$lastManifestHash' -and $_.Right.Extent.Text -ceq '$finalMembers.files[''project.json''].sha256'})
 Check 'Final manifest identity comes from final member snapshot' ($hashAssign.Count -eq 1)
 $armCheck=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.CommandAst] -and $a.GetCommandName() -eq 'Assert-ArmSelection'},$true))
 $stageAssign=@($assignments|Where-Object {$_.Left.Extent.Text -ceq '$record.stage' -and $_.Right.Extent.Text -ceq "'armed-report'"})
 Check 'Saved arm binding check precedes armed stage' ($armCheck.Count -eq 1 -and $stageAssign.Count -eq 1 -and $armCheck[0].Extent.StartOffset -lt $stageAssign[0].Extent.StartOffset)
}catch{$failure=$_.Exception.Message}
finally{
 $receipt=[ordered]@{schema_version='greenlens.supervisor-source-checks.v2';source_sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant();test_source_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant();powershell=$PSVersionTable.PSVersion.ToString();supervisor_executed=$false;app_or_renderer_launched=$false;native_api_calls=0;completed_utc=[DateTime]::UtcNow.ToString('o');passed_count=$checks.Count;failure=$failure;checks=$checks.ToArray()}
 ConvertTo-Json -InputObject $receipt -Depth 12|Set-Content -LiteralPath (Join-Path $output 'checks.json') -Encoding utf8NoBOM
}
if($failure){throw "Source checks failed; receipt: $output; $failure"}
Write-Host "$($checks.Count) source-only checks passed; receipt: $output"
