#Requires -Version 7.5
<#
Assemble the fixed four retained production runs for a panel-remount regression.
No engine is invoked. Original projects/run bytes are never modified. The new
manifest is fixture assembly, not an application editing or import endpoint.
The distributed CLI admits the completed fixture and creates/finalizes its
genuine sensitivity declaration through the existing shared Rust services.
#>
[CmdletBinding(DefaultParameterSetName='Run')]
param(
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Cli,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$ProductionRoot,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$OutputDirectory,
 [Parameter(Mandatory,ParameterSetName='Functions')][switch]$FunctionsOnly
)
function Assert-RemountProductionIndex([object[]]$Index,[Collections.IDictionary]$Ten,[Collections.IDictionary]$Five){
 if(($Index.case -join ',') -cne 'A10,B10,A5,B5'){throw 'Require the exact original A10,B10,A5,B5 production index'}
 if(@($Ten.energy.runs).Count -ne 2 -or @($Five.energy.runs).Count -ne 2){throw 'Require exactly two original runs in each production project'}
 $selected=[ordered]@{}; $ids=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($entry in $Index){
  $minutes=if($entry.case -cin @('A10','B10')){10}else{5}
  $alternative=$entry.case.Substring(0,1)
  if($entry.minutes -ne $minutes -or $entry.project -cne "project-$minutes" -or $entry.alternative -cne $alternative -or $entry.id -cnotmatch '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' -or !$ids.Add($entry.id)){throw 'Production index identity, order or interval differs'}
  $document=if($minutes -eq 10){$Ten}else{$Five}
  $runs=@($document.energy.runs|Where-Object { $_.id -ceq $entry.id -and $_.alternative_id -ceq $alternative })
  if($runs.Count -ne 1 -or $runs[0].path -cne "runs/$($entry.id)" -or $runs[0].record_sha256 -cnotmatch '^[0-9a-f]{64}$' -or $runs[0].physical_input_hash -cnotmatch '^[0-9a-f]{64}$'){throw 'Production summary differs from its declared exact run'}
  $selected[$entry.case]=$runs[0]
 }
 return $selected
}
function Assert-RemountReusePlan([Collections.IDictionary]$Plan,[Collections.IDictionary]$Expected){
 if($Plan.schema_version -cne 'greenlens.energy-sensitivity-plan.v1' -or $Plan.execution_authorized -isnot [bool] -or $Plan.execution_authorized -or ($Plan.slots.slot_id -join ',') -cne 'A10,B10,A5,B5'){throw 'Require the shared four-slot non-executing reuse plan'}
 foreach($slot in $Plan.slots){
  $run=$Expected[$slot.slot_id]
  $declared=@($Plan.declaration.slots|Where-Object id -CEQ $slot.slot_id)
  $steps=if($slot.slot_id -cin @('A10','B10')){6}else{12}
  if($null -eq $slot.reuse -or $slot.reuse.run_id -cne $run.id -or $slot.reuse.record_sha256 -cne $run.record_sha256 -or $declared.Count -ne 1 -or $declared[0].physical_input_sha256 -cne $run.physical_input_hash -or $declared[0].zone_timesteps_per_hour -ne $steps){throw 'A declared sensitivity slot is missing or selected different retained evidence/time resolution; no simulation or favorable selection is allowed'}
 }
}
function Assert-RemountRunShape([Collections.IDictionary]$Run,[Collections.IDictionary]$Summary,[int]$Minutes){
 if($Minutes -notin @(10,5)){throw 'Fixture accepts only the fixed ten/five-minute pair'}
 $steps=60/$Minutes;$intervals=8760*$steps
 if($Run.id -cne $Summary.id -or $Run.alternative_id -cne $Summary.alternative_id -or $Run.provenance.physical_input_hash -cne $Summary.physical_input_hash -or $Run.study.zone_timesteps_per_hour -ne $steps -or $Run.series.interval_minutes -ne $Minutes -or $Run.series.interval_count -ne $intervals -or $Run.totals.interval_count -ne $intervals -or $Run.totals.interval_seconds -ne 60*$Minutes){throw 'Retained annual run shape differs from exact 6/12 timesteps and aligned 52,560/105,120 intervals'}
}
function Resolve-RemountPlainPath([string]$Value){
 $full=[IO.Path]::GetFullPath($Value);$cursor=$full
 while($cursor){
  if(Test-Path -LiteralPath $cursor){if((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Fixture paths must not contain reparse points'}}
  $parent=[IO.Path]::GetDirectoryName($cursor);if($parent -eq $cursor){break};$cursor=$parent
 }
 return $full.TrimEnd([IO.Path]::DirectorySeparatorChar)
}
function Get-RemountInventory([string]$Root){
 $Root=Resolve-RemountPlainPath $Root
 $watch=[Diagnostics.Stopwatch]::StartNew();$files=[ordered]@{};$queue=[Collections.Generic.Queue[string]]::new();$queue.Enqueue($Root);[long]$total=0;$directories=1
 while($queue.Count){foreach($entry in [IO.DirectoryInfo]::new($queue.Dequeue()).EnumerateFileSystemInfos()){
  if($watch.ElapsedMilliseconds -gt 120000){throw 'Fixture inventory exceeds its 120-second budget'}
  if($entry.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Fixture inventory contains a reparse point'}
  if($entry.Attributes -band [IO.FileAttributes]::Directory){$directories++;if($directories -gt 5000){throw 'Fixture exceeds directory bound'};$queue.Enqueue($entry.FullName);continue}
  if($files.Count -ge 5000 -or $entry.Length -gt 64MB){throw 'Fixture exceeds file-count or per-file bound'}
  $total+=$entry.Length;if($total -gt 256MB){throw 'Fixture exceeds 256 MiB total evidence bound'}
  $name=[IO.Path]::GetRelativePath($Root,$entry.FullName).Replace('\','/')
  if($name -eq '.greenlens.lock'){continue}
  $files[$name]=[ordered]@{bytes=[long]$entry.Length;sha256=(Get-FileHash -LiteralPath $entry.FullName).Hash.ToLowerInvariant()}
 }}
 return $files
}
function Assert-RemountSameInventory([Collections.IDictionary]$Expected,[Collections.IDictionary]$Actual){
 if($Expected.Count -ne $Actual.Count){throw 'Retained immutable inventory count changed'}
 foreach($name in $Expected.Keys){if(!$Actual.Contains($name) -or $Actual[$name].bytes -ne $Expected[$name].bytes -or $Actual[$name].sha256 -cne $Expected[$name].sha256){throw 'Retained immutable evidence bytes changed'}}
}
function Invoke-FixtureCli([string]$Name,[string[]]$Arguments){
 if($commands.Count -ge 7 -or $overall.Elapsed.TotalSeconds -ge 900){throw 'Fixture preparation exceeds its seven-command/900-second budget'}
 if((Get-ChildItem -LiteralPath $OutputDirectory -Recurse -File|Measure-Object Length -Sum).Sum -gt 1GB){throw 'Fixture retained output exceeds its 1 GiB budget'}
 if((Get-FileHash -LiteralPath $frozenCli).Hash.ToLowerInvariant() -cne $cliHash){throw 'Fixture CLI identity changed'}
 $stdoutPath=Join-Path $OutputDirectory "$Name.json";$stderrPath=Join-Path $OutputDirectory "$Name.stderr.txt"
 $info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=$frozenCli;$info.WorkingDirectory=$OutputDirectory;$info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 foreach($argument in $Arguments){$info.ArgumentList.Add($argument)}
 $process=[Diagnostics.Process]::new();$process.StartInfo=$info;$stdout=[IO.File]::Create($stdoutPath);$stderr=[IO.File]::Create($stderrPath);$watch=[Diagnostics.Stopwatch]::StartNew();$code=$null;$peak=0L;$started=$false;$failure=$null
 try {
  if(!$process.Start()){throw 'Fixture command did not start'};$started=$true
  $outTask=$process.StandardOutput.BaseStream.CopyToAsync($stdout);$errTask=$process.StandardError.BaseStream.CopyToAsync($stderr)
  while(!$process.WaitForExit(25)){
   $process.Refresh();$peak=[Math]::Max($peak,$process.PeakWorkingSet64)
   if($watch.Elapsed.TotalSeconds -gt 300 -or $overall.Elapsed.TotalSeconds -gt 900 -or $process.WorkingSet64 -gt 2GB -or $stdout.Length -gt 32MB -or $stderr.Length -gt 32MB){throw 'Fixture command exceeded its fixed time/memory/output budget'}
  }
  if(![Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]@($outTask,$errTask),5000)){throw 'Fixture command output did not close within five seconds'}
  # PowerShell may expose Task<VoidTaskResult>'s completion value. Suppress
  # both explicitly so the only function result is the parsed child JSON.
  $null=$outTask.GetAwaiter().GetResult();$null=$errTask.GetAwaiter().GetResult();$code=$process.ExitCode
  if($stdout.Length -gt 32MB -or $stderr.Length -gt 32MB){throw 'Fixture output exceeded its fixed limit'}
  if($code -ne 0){throw "Fixture command $Name failed; original diagnostics retained"}
 }catch{$failure=$_.Exception.Message;throw}finally{
  if($started -and !$process.HasExited){$process.Kill($true);if(!$process.WaitForExit(5000)){$failure='Fixture command tree failed to terminate'}}
  $watch.Stop();$stdout.Dispose();$stderr.Dispose();$process.Dispose()
  $commands.Add([ordered]@{name=$Name;arguments=$Arguments;wall_seconds=$watch.Elapsed.TotalSeconds;peak_working_set_bytes=$peak;exit_code=$code;failure=$failure})
  ConvertTo-Json -InputObject @($commands.ToArray()) -Depth 20|Set-Content -LiteralPath (Join-Path $OutputDirectory 'commands.json') -Encoding utf8NoBOM
 }
 return Get-Content -LiteralPath $stdoutPath -Raw|ConvertFrom-Json -AsHashtable -DateKind String -Depth 100
}
if($FunctionsOnly){return}
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$local=Resolve-RemountPlainPath (Join-Path $env:LOCALAPPDATA 'GreenLens/verification')
$ProductionRoot=Resolve-RemountPlainPath $ProductionRoot;$OutputDirectory=Resolve-RemountPlainPath $OutputDirectory;$Cli=Resolve-RemountPlainPath $Cli
foreach($path in @($ProductionRoot,$OutputDirectory)){if(!$path.StartsWith($local+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Fixture inputs/outputs must be under owned GreenLens verification storage'}}
if($OutputDirectory.StartsWith($ProductionRoot+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $ProductionRoot.StartsWith($OutputDirectory+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $OutputDirectory.Equals($ProductionRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Production and fixture output paths must not overlap'}
if(!(Test-Path -LiteralPath $Cli -PathType Leaf) -or (Test-Path -LiteralPath $OutputDirectory)){throw 'Require an existing CLI and fresh fixture output directory'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$frozenCli=Join-Path $OutputDirectory 'greenlens.exe';Copy-Item -LiteralPath $Cli -Destination $frozenCli
$cliHash=(Get-FileHash -LiteralPath $frozenCli).Hash.ToLowerInvariant();$commands=[Collections.Generic.List[object]]::new();$overall=[Diagnostics.Stopwatch]::StartNew()
$ten=Join-Path $ProductionRoot 'project-10';$five=Join-Path $ProductionRoot 'project-5'
$tenBefore=Get-RemountInventory $ten;$fiveBefore=Get-RemountInventory $five
$index=Get-Content -LiteralPath (Join-Path $ProductionRoot 'run-index.json') -Raw|ConvertFrom-Json -AsHashtable -DateKind String
$tenDocument=Get-Content -LiteralPath (Join-Path $ten 'project.json') -Raw|ConvertFrom-Json -AsHashtable -DateKind String -Depth 100
$fiveDocument=Get-Content -LiteralPath (Join-Path $five 'project.json') -Raw|ConvertFrom-Json -AsHashtable -DateKind String -Depth 100
$expected=Assert-RemountProductionIndex $index $tenDocument $fiveDocument
$declaration=[ordered]@{schema='greenlens.energy-remount-fixture-plan.v1';cli_sha256=$cliHash;source_index_sha256=(Get-FileHash -LiteralPath (Join-Path $ProductionRoot 'run-index.json')).Hash.ToLowerInvariant();selected_run_ids=@($index.id);native_simulations=0;commands_max=7;overall_seconds_max=900;per_command_seconds_max=300;source_projects_unchanged=$null;policy='Exact original indexed A10,B10,A5,B5 only. Preserve all run bytes. Rust admits merged fixture and declares/finalizes all four as reuse. Missing slots stop, never launch. Numerical outcomes are not selected or changed.'}
$declaration|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8NoBOM
$merged=Join-Path $OutputDirectory 'project-four';$fineCopy=Join-Path $OutputDirectory 'fine-copy'
try {
 $null=Invoke-FixtureCli 'copy-ten' @('copy',$ten,$merged)
 $null=Invoke-FixtureCli 'copy-five' @('copy',$five,$fineCopy)
 $copyTen=Get-Content -LiteralPath (Join-Path $merged 'project.json') -Raw|ConvertFrom-Json -AsHashtable -DateKind String -Depth 100
 $copyFive=Get-Content -LiteralPath (Join-Path $fineCopy 'project.json') -Raw|ConvertFrom-Json -AsHashtable -DateKind String -Depth 100
 $copyExpected=Assert-RemountProductionIndex $index $copyTen $copyFive
 foreach($entry in $index){
  $sourceRoot=if($entry.minutes -eq 10){$merged}else{$fineCopy}
  $record=Get-Content -LiteralPath (Join-Path $sourceRoot "runs/$($entry.id)/results.json") -Raw|ConvertFrom-Json -AsHashtable -DateKind String -Depth 100
  Assert-RemountRunShape $record $copyExpected[$entry.case] $entry.minutes
 }
 foreach($case in @('A5','B5')){
  $run=$copyExpected[$case];$source=Join-Path $fineCopy $run.path;$target=Join-Path $merged $run.path
  if(Test-Path -LiteralPath $target){throw 'Fine run conflicts with retained ten-minute identity'}
  Copy-Item -LiteralPath $source -Destination $target -Recurse
  $copyTen.energy.runs=@($copyTen.energy.runs)+@($run)
 }
 $null=Get-RemountInventory $merged
 # Only this fresh fixture's manifest is replaced. Original immutable runs and
 # source manifests are never rewritten. Full Rust admission follows.
 $staged=Join-Path $merged '.fixture-project.tmp'
 $copyTen|ConvertTo-Json -Depth 100|Set-Content -LiteralPath $staged -Encoding utf8NoBOM
 [IO.File]::Move($staged,(Join-Path $merged 'project.json'),$true)
 $null=Invoke-FixtureCli 'admit-four' @('open',$merged)
 $plan=Invoke-FixtureCli 'plan' @('energy','sensitivity','plan',$merged)
 Assert-RemountReusePlan $plan $expected
 $retained=Invoke-FixtureCli 'retain-plan' @('energy','sensitivity','plan',$merged,'--retain')
 Assert-RemountReusePlan $retained.plan $expected
 if($null -ne $retained.next_slot_id -or $null -ne $retained.active_slot_id){throw 'Retained fixture unexpectedly requires execution'}
 $final=Invoke-FixtureCli 'finalize' @('energy','sensitivity','finalize',$merged,$retained.reference.id)
 Assert-RemountReusePlan $final.plan $expected
 if(!$final.finalized -or $null -eq $final.result -or @($final.result.assessment.runs).Count -ne 4 -or !$final.result.assessment.all_declared_runs_integrity_checked -or @($final.result.assessment.missing_slots).Count -ne 0 -or @($final.result.assessment.interruptions).Count -ne 0 -or $final.result.assessment.qualification_promoted){throw 'Shared service did not finalize the complete four-run numerical assessment independently of qualification'}
 $null=Invoke-FixtureCli 'final-open' @('open',$merged)
 foreach($entry in $index){
  $source=Join-Path $ProductionRoot "$($entry.project)/runs/$($entry.id)";$target=Join-Path $merged "runs/$($entry.id)"
  Assert-RemountSameInventory (Get-RemountInventory $source) (Get-RemountInventory $target)
 }
 Assert-RemountSameInventory $tenBefore (Get-RemountInventory $ten)
 Assert-RemountSameInventory $fiveBefore (Get-RemountInventory $five)
 if((Get-FileHash -LiteralPath (Join-Path $ProductionRoot 'run-index.json')).Hash.ToLowerInvariant() -cne $declaration.source_index_sha256){throw 'Original production index changed'}
 [ordered]@{schema='greenlens.energy-remount-fixture.v1';completed=$true;cli_sha256=$cliHash;project_relative_path='project-four';selected_run_ids=@($index.id);session_id=$final.reference.id;source_projects_unchanged=$true;retained_run_bytes_unchanged=$true;native_simulations=0;scientific_qualification_asserted=$false;numerical_sensitivity_passed=$final.result.assessment.numerical_sensitivity_passed;wall_seconds=$overall.Elapsed.TotalSeconds}|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'fixture.json') -Encoding utf8NoBOM
}catch{
 [ordered]@{schema='greenlens.energy-remount-fixture.v1';completed=$false;reason=$_.Exception.Message;native_simulations=0;scientific_qualification_asserted=$false}|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'fixture-failure.json') -Encoding utf8NoBOM
 throw
}
Write-Output $merged
