# Stop only an explicitly identified owned reference worker after its declared
# winter-case boundary. This scheduling record never marks a numeric pass.
param(
 [Parameter(Mandatory)][int]$WorkerProcessId,
 [Parameter(Mandatory)][string]$ExpectedStartTimeUtc,
 [Parameter(Mandatory)][string]$ExpectedCommandSha256,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [Parameter(Mandatory)][ValidateSet('A','B')][string]$Alternative,
 [ValidateSet(2,3)][int]$StopAfterCases=3,
 [switch]$CheckOnly
)
$ErrorActionPreference='Stop'
$watcherSourceHash=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
$expectedStart=[DateTime]::Parse($ExpectedStartTimeUtc,[Globalization.CultureInfo]::InvariantCulture,[Globalization.DateTimeStyles]::RoundtripKind).ToUniversalTime()
$recordPath=Join-Path $OutputDirectory 'scheduling-interruption.json'
if(Test-Path -LiteralPath $recordPath){throw 'A scheduling record already exists.'}
function Test-CompletedBoundary($Report) {
 if($Report.declaration.alternative -cne $Alternative -or $Report.declaration.schema_version -cne 'greenlens.controlled-reference.v1'){throw 'Unexpected retained worker declaration.'}
 $winterPrefix=@(8512,8504,8508)
 $declared=@($Report.declaration.executed_interval_indices)
 if($declared.Count -lt 3 -or ($declared[0..2] -join ',') -cne ($winterPrefix -join ',')){throw 'Unexpected declared winter sequence; refusing termination.'}
 $completed=@($Report.checks.interval_index)
 if($completed.Count -gt $StopAfterCases){throw 'Requested scheduling boundary already passed; refusing late termination.'}
 if($completed.Count -and (($completed -join ',') -cne ($winterPrefix[0..($completed.Count-1)] -join ','))){throw 'Unexpected completed interval sequence; refusing termination.'}
 return $completed.Count -eq $StopAfterCases
}
function Owned-Worker {
 $process=Get-Process -Id $WorkerProcessId -ErrorAction SilentlyContinue
 if(!$process){return $null}
 try{
  $null=$process.Handle # Keep the verified process handle, not just a reusable PID.
  if($process.StartTime.ToUniversalTime().Ticks -ne $expectedStart.Ticks){throw 'Worker PID was reused; refusing termination.'}
  $info=Get-CimInstance Win32_Process -Filter "ProcessId=$WorkerProcessId"
  $hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($info.CommandLine))).ToLowerInvariant()
  if($hash -cne $ExpectedCommandSha256 -or !$info.CommandLine.Contains('annual-controlled-reference.ps1') -or !$info.CommandLine.Contains($OutputDirectory)){throw 'Worker command/input-output identity differs; refusing termination.'}
 }catch{$process.Dispose();throw}
 return $process
}
$initialWorker=Owned-Worker
if(!$initialWorker){throw 'Owned worker is unavailable.'};$initialWorker.Dispose()
if($CheckOnly){Write-Host 'Exact worker identity verified; no waiting or termination performed.';return}
while($true){
 $worker=Owned-Worker
 if(!$worker){throw 'Owned worker exited before the scheduling boundary; inspect its retained process evidence.'}
 try{
 $report=$null
 try{
  $reportBytes=[IO.File]::ReadAllBytes((Join-Path $OutputDirectory 'controlled-reference.json'))
  $report=[Text.Encoding]::UTF8.GetString($reportBytes).TrimStart([char]0xfeff)|ConvertFrom-Json -ErrorAction Stop
 }catch{$worker.Dispose();Start-Sleep -Seconds 1;continue}
 $completed=@($report.checks.interval_index)
 if(!(Test-CompletedBoundary $report)){$worker.Dispose();Start-Sleep -Seconds 1;continue}
 # Recheck immediately before acting. The next case may have begun preparation
 # after its predecessor was sealed; that partial case remains uncompleted.
 $worker.Dispose();$worker=Owned-Worker
 if(!$worker){throw 'Worker exited at the scheduling boundary; no termination performed.'}
 $all=@(Get-CimInstance Win32_Process);$tree=@($WorkerProcessId)
 do{$children=@($all|Where-Object{$_.ParentProcessId -in $tree -and $_.ProcessId -notin $tree});$tree+=@($children.ProcessId)}while($children.Count)
 $observed=@($all|Where-Object{$_.ProcessId -in $tree}|Select-Object ProcessId,ParentProcessId,CreationDate,Name)
 $snapshotName='scheduling-completed-report.json';$snapshotPath=Join-Path $OutputDirectory $snapshotName
 if(Test-Path -LiteralPath $snapshotPath){throw 'A prior scheduling snapshot already exists.'}
 [IO.File]::WriteAllBytes($snapshotPath,$reportBytes)
 $reportHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($reportBytes)).ToLowerInvariant()
 $record=[ordered]@{schema_version='greenlens.reference-scheduling-interruption.v1';requested_utc=[DateTime]::UtcNow.ToString('o');reason="Redistribute remaining fixed cases after retaining $StopAfterCases complete winter cases";numeric_outcome='No new pass or failure is inferred by this scheduling action';alternative=$Alternative;stop_after_cases=$StopAfterCases;watcher_source_sha256=$watcherSourceHash;worker_pid=$WorkerProcessId;worker_start_utc=$ExpectedStartTimeUtc;worker_command_sha256=$ExpectedCommandSha256;completed_interval_indices=$completed;completed_report_snapshot=$snapshotName;completed_report_sha256=$reportHash;observed_tree=$observed;next_case_preparation_may_be_incomplete=$true;next_case_policy='Any next-case preparation or process that raced with this finalized-report boundary remains partial/cancelled, never unexecuted, completed or qualified; rerun that entire missing case in fresh scratch';concurrency_context='At most eight independent one-worker heavy engine processes; nonisolated timings';terminated=$false}
 $record|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $recordPath -Encoding utf8
 $worker.Kill($true);$worker.WaitForExit()
 $worker.Dispose()
 $record.terminated=$true;$record.finished_utc=[DateTime]::UtcNow.ToString('o')
 $record|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $recordPath -Encoding utf8
 Write-Host "Stopped owned $Alternative reference worker after retaining intervals $($completed -join ',')."
 break
 }finally{if($worker){$worker.Dispose()}}
}
