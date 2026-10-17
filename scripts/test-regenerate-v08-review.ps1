#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. Pure helper/source regressions only.
# Never starts the CLI, a native helper, Typst, an engine, or an installed app.
$ErrorActionPreference='Stop'
$scriptPath=Join-Path $PSScriptRoot 'regenerate-v08-review-once.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($scriptPath,[ref]$tokens,[ref]$errors)
if($errors.Count){throw ($errors|Out-String)}
. $scriptPath -FunctionsOnly
$passed=0
function Check([bool]$Condition,[string]$Name){if(!$Condition){throw "Failed: $Name"};$script:passed++}
function Reject([scriptblock]$Action,[string]$Name){$rejected=$false;try{& $Action|Out-Null}catch{$rejected=$true};Check $rejected $Name}
foreach($emptyPath in @($null,'',' ')){$message=$null;try{File-Hash $emptyPath|Out-Null}catch{$message=$_.Exception.Message};Check ($message -ceq 'Cannot verify bytes without an observed nonempty file path.') 'Missing observed path fails explicitly before file hashing'}
foreach($seconds in @(0,599.999,600)){Assert-RegenerationDeadline $seconds;Check $true "Accepted within deadline: $seconds"}
foreach($seconds in @(600.000001,601,[double]::NaN,[double]::PositiveInfinity,-1)){Reject {Assert-RegenerationDeadline $seconds} "Rejected invalid/late completion: $seconds"}

# Exercise the actual success block, not a duplicated predicate. A final polling
# iteration may begin before600s, then finish its process inspection after600s.
$capture=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -ceq '$record.command_seconds'},$true))
$accept=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -ceq '$commandPassed' -and $n.Right.Extent.Text -ceq '$true'},$true))
$guards=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.CommandAst] -and $n.GetCommandName() -ceq 'Assert-RegenerationDeadline'},$true))
Check ($capture.Count -eq 1 -and $accept.Count -eq 1 -and $guards.Count -eq 2) 'Both command and whole-verification deadline checks are present'
Check ($capture[0].Extent.EndOffset -lt $guards[0].Extent.StartOffset -and $guards[0].Extent.EndOffset -lt $accept[0].Extent.StartOffset) 'Fresh elapsed capture precedes guard; guard precedes success'
$source=[IO.File]::ReadAllText($scriptPath)
$fresh=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.CommandAst] -and $a.GetCommandName() -ceq 'Assert-FreshObserverHost'},$true))
Check ($fresh.Count -eq 1 -and $fresh[0].Extent.StartOffset -gt $source.IndexOf('if($FunctionsOnly){return}') -and $fresh[0].Extent.EndOffset -lt $source.IndexOf('$cache=')) 'Fresh-host guard follows FunctionsOnly and precedes run admission'
Add-Type -TypeDefinition 'namespace GreenLensColdRegeneration { public sealed class Capture {} }'
$message=$null;try{. ([scriptblock]::Create($fresh[0].Extent.Text))}catch{$message=$_.Exception.Message}
Check ($message -like 'A fresh PowerShell process is required;*GreenLensColdRegeneration.Capture*') 'Actual cold Run guard rejects preloaded helper rather than claiming current file identity'
Check (!$source.Contains('.MainModule') -and $source.Contains('[GreenLensObservation.HeldProcessImage]::Read($held,[int]$member,$started.Ticks)')) 'Cold observer uses bound kernel image rather than nullable module enumeration'
$unknown=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.AssignmentStatementAst] -and $a.Left.Extent.Text -ceq '$record.process_identity_failure'},$true))
Check ($unknown.Count -eq 1 -and $unknown[0].Parent.Extent.Text.Contains('unidentified membership cannot pass')) 'Unknown cold member is recorded and fails closed'
Check ($unknown[0].Extent.Text.Contains('exception_type=') -and $unknown[0].Extent.Text.Contains('category=') -and $unknown[0].Extent.Text.Contains('script_stack_trace=')) 'Cold unknown-member record retains category/type/stack'
$boundImage=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $a.Member.Value -ceq 'ValidateBinding'},$true))
Check ($boundImage.Count -eq 1 -and $source.IndexOf('if($seen.ContainsKey($key))') -gt $boundImage[0].Extent.EndOffset) 'Cold identity cache lookup follows held PID/creation-time check'
$membership=@($ast.FindAll({param($a) $a -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $a.Member.Value -ceq 'RequireMember'},$true))
Check ($membership.Count -eq 1 -and $membership[0].Extent.EndOffset -lt $boundImage[0].Extent.StartOffset) 'Cold exact Job membership precedes identity and cache reuse'
$successBlock=[scriptblock]::Create($source.Substring($capture[0].Extent.StartOffset,$accept[0].Extent.EndOffset-$capture[0].Extent.StartOffset))
$record=[ordered]@{};$timer=[pscustomobject]@{Elapsed=[TimeSpan]::FromTicks(6000000001L)};$commandPassed=$false;$rejected=$false
try{. $successBlock}catch{$rejected=$true}
Check ($rejected -and !$commandPassed -and $record.command_seconds -gt 600) 'Late final polling iteration preserves failure and measured duration'
$record=[ordered]@{};$timer=[pscustomobject]@{Elapsed=[TimeSpan]::FromSeconds(600)};$commandPassed=$false
. $successBlock
Check ($commandPassed -and $record.command_seconds -eq 600) 'Exact deadline remains accepted without weakening its boundary'
$finalCapture=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -ceq '$record.verification_seconds'},$true))
$completed=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -ceq '$record.completed' -and $n.Right.Extent.Text -ceq '$true'},$true))
$productPassed=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -ceq '$record.products_identical' -and $n.Right.Extent.Text -ceq '$true'},$true))
Check ($finalCapture.Count -eq 1 -and $completed.Count -eq 1 -and $productPassed.Count -eq 1 -and $finalCapture[0].Extent.EndOffset -lt $guards[1].Extent.StartOffset -and $guards[1].Extent.EndOffset -lt $productPassed[0].Extent.StartOffset -and $productPassed[0].Extent.EndOffset -lt $completed[0].Extent.StartOffset) 'Whole-operation guard precedes product and completed success flags'
$finalBlock=[scriptblock]::Create($source.Substring($finalCapture[0].Extent.StartOffset,$completed[0].Extent.EndOffset-$finalCapture[0].Extent.StartOffset))
$record=[ordered]@{command_seconds=599;products_identical=$false;completed=$false};$timer=[pscustomobject]@{Elapsed=[TimeSpan]::FromTicks(6000000001L)};$rejected=$false
try{. $finalBlock}catch{$rejected=$true}
Check ($rejected -and !$record.products_identical -and !$record.completed -and $record.command_seconds -eq 599 -and $record.verification_seconds -gt 600) 'Verification crossing deadline cannot publish success despite timely CLI exit'
$record=[ordered]@{command_seconds=599;products_identical=$false;completed=$false};$timer=[pscustomobject]@{Elapsed=[TimeSpan]::FromSeconds(600)}
. $finalBlock
Check ($record.products_identical -and $record.completed -and $record.verification_seconds -eq 600) 'Whole verification at exact deadline passes'
$starts=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $n.Member.Value -ceq 'StartBounded'},$true))
Check ($starts.Count -eq 1) 'The supervisor still contains exactly one native launch site'
[ordered]@{passed=$passed;failed=0;parser_errors=0;native_or_renderer_executions=0}|ConvertTo-Json
