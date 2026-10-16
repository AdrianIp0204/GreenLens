# Original synthetic metadata only; never creates accepted engine evidence.
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'verify-study-issues.ps1') -FunctionsOnly
$checks=0
function Require-Rejection([scriptblock]$Action,[string]$Description) {
 $rejected=$false
 try{& $Action}catch{$rejected=$true}
 if(!$rejected){throw "Expected rejection: $Description"}
 $script:checks++
}
$before=@{zones=@(1..4|ForEach-Object {@{id="synthetic-zone-$_";cooling_system=@{gross_rated_cop=3.6;gross_rated_capacity_w=5000}}});title='Original synthetic collector fixture'}
$after=Copy-IssueJson $before
foreach($zone in $after.zones){$zone.cooling_system.gross_rated_cop=3.8}
Assert-IssueCopEdit $before $after;$checks++
$after.zones[0].cooling_system.gross_rated_capacity_w=6000
Require-Rejection {Assert-IssueCopEdit $before $after} 'undeclared capacity change'
$after=Copy-IssueJson $before
Require-Rejection {Assert-IssueCopEdit $before $after} 'unchanged proposed COP'
$plan=@{rows=@(
 @{id='original-synthetic-row-1';key='opaque-row-key';kind='input';change='changed';requires_review=$true;before=@{applied_value=3.6};after=@{applied_value=3.8;field_path='/cooling_system/gross_rated_cop';design_id='B'}},
 @{id='original-synthetic-row-2';key='original-source';kind='assumption';change='changed';requires_review=$true},
 @{id='original-synthetic-row-3';key='unchanged-object';kind='mapping';change='unchanged';requires_review=$false}
)}
$decisions=New-IssueReviewDecisions $plan
if($decisions.Count -ne 2 -or $decisions[0].resolution -cne 'reviewed' -or $decisions[1].resolution -cne 'unresolved'){throw 'Explicit input review/source uncertainty policy failed'};$checks++
$plan.rows[0].after.field_path='/cooling_system/gross_rated_capacity_w'
Require-Rejection {New-IssueReviewDecisions $plan} 'undeclared input review'
$plan.rows[0].after.field_path='/cooling_system/gross_rated_cop';$plan.rows[2].change='changed'
Require-Rejection {New-IssueReviewDecisions $plan} 'geometry change in COP-only protocol'
$malformed=@{rows=@(@{kind='input';change='changed';requires_review=$true})}
Require-Rejection {New-IssueReviewDecisions $malformed} 'malformed review row'
Assert-IssueExecutionBudgets 600 600 2GB 2GB 16 1 1 300;$checks++
Require-Rejection {Assert-IssueExecutionBudgets 600.01 600 1 1 1 1 1 1} 'command deadline'
Require-Rejection {Assert-IssueExecutionBudgets 301 600 1 1 1 1 1 300.01} 'engine deadline independently shorter than CLI deadline'
Require-Rejection {Assert-IssueExecutionBudgets 1 600 (2GB+1) 1 1 1 1 1} 'memory guard'
Require-Rejection {Assert-IssueExecutionBudgets 1 600 1 (2GB+1) 1 1 1 1} 'scratch guard'
Require-Rejection {Assert-IssueExecutionBudgets 1 600 1 1 17 1 1 1} 'process guard'
Require-Rejection {Assert-IssueExecutionBudgets 1 600 1 1 1 1 0 1} 'engine during report regeneration'
Require-Rejection {Assert-IssueExecutionBudgets 1 900 1 1 1 3 2 1} 'undeclared third replay'
Require-Rejection {Assert-IssueExecutionBudgets 1 1200 1 1 1 0 0 0} 'silently enlarged deadline'
if(!(Test-IssueJsonEqual @{nested=@(1,2);name='A'} @{name='A';nested=@(1.0,2)})){throw 'Order-independent model comparison failed'};$checks++
if(Test-IssueJsonEqual @{name='A'} @{name='a'}){throw 'Case-sensitive stable ID comparison failed'};$checks++
$stream=[IO.MemoryStream]::new([byte[]](1,2,3));$copy=[IO.MemoryStream]::new()
try {
 $task=$stream.CopyToAsync($copy)
 $emitted=@(Wait-IssueStreamTask $task)
 if($emitted.Count -ne 0 -or $copy.Length -ne 3){throw 'Asynchronous stream wait polluted the response or lost bytes'};$checks++
 $fault=[Threading.Tasks.Task]::FromException([IO.IOException]::new('Original synthetic stream failure'))
 Require-Rejection {Wait-IssueStreamTask $fault} 'stream-copy fault propagation'
 $response=@(& { Wait-IssueStreamTask $task; @{issue=@{id='synthetic-result-only'}} })
 if($response.Count -ne 1 -or $response[0].issue.id -cne 'synthetic-result-only'){throw 'CLI response is no longer exactly one JSON object'};$checks++
} finally {$stream.Dispose();$copy.Dispose()}
$errors=$null;$tokens=$null
[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'verify-study-issues.ps1'),[ref]$tokens,[ref]$errors)|Out-Null
if($errors.Count){throw ($errors.Message -join '; ')};$checks++
Write-Output "Passed $checks pure collector checks. No CLI, simulation, renderer, project edit or native interaction was executed."
