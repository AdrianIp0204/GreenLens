#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. Shared observer path, not an application
# worker or sandbox. Callers own process creation and domain evidence checks.
function Initialize-ObservedOutputCapture {
 Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Threading.Tasks;
namespace GreenLensObservedJob {
 public sealed class Capture {
  readonly byte[] bytes=new byte[1048576]; readonly object gate=new object();
  int retained; long total; bool overflow;
  public long Total { get { lock(gate) return total; } }
  public bool Overflow { get { lock(gate) return overflow; } }
  public byte[] Bytes { get { lock(gate) { var result=new byte[retained]; Array.Copy(bytes,result,retained); return result; } } }
  public async Task Drain(Stream source) {
   var buffer=new byte[65536]; int count;
   while((count=await source.ReadAsync(buffer,0,buffer.Length).ConfigureAwait(false))!=0) {
    lock(gate) { total=checked(total+count); int keep=Math.Min(count,bytes.Length-retained);
     if(keep>0) { Array.Copy(buffer,0,bytes,retained,keep); retained+=keep; }
     if(keep<count) overflow=true;
    }
   }
  }
 }
}
'@
}

function New-ObservedJobSession($Process,$Record,[scriptblock]$Policy,$Stdout,$Stderr,$OutTask,$ErrTask) {
 if(!$Process -or !$Record -or !$Policy -or !$Stdout -or !$Stderr -or !$OutTask -or !$ErrTask){throw 'Complete held-process, policy and bounded-drain state is required.'}
 return [pscustomobject]@{Process=$Process;Record=$Record;Policy=$Policy;Stdout=$Stdout;Stderr=$Stderr;OutTask=$OutTask;ErrTask=$ErrTask;Seen=@{};PolicyCounts=@{};SampleCount=0;Failed=$false;Finalized=$false;CleanupPassed=$false;MaximumIdentities=4096}
}

function Set-ObservedJobFailure($Session,[string]$Stage,$ErrorRecord,$Member=$null) {
 $Session.Failed=$true;$Session.Record.completed=$false
 $detail=[ordered]@{stage=$Stage;message=$ErrorRecord.Exception.Message;exception_type=$ErrorRecord.Exception.GetType().FullName;category=$ErrorRecord.CategoryInfo.Category.ToString();script_stack_trace=$ErrorRecord.ScriptStackTrace;position=$ErrorRecord.InvocationInfo.PositionMessage}
 if($Member){$detail.member=$Member}
 if(!$Session.Record.failure){$Session.Record.failure=$detail.message;$Session.Record.failure_detail=$detail}
 $Session.Record.observation_failure=$detail
}

function Get-ObservedImageHash([string]$Path) {
 if([string]::IsNullOrWhiteSpace($Path)){throw 'Observed executable identity is unavailable; configured paths cannot substitute for it.'}
 $stream=[IO.FileStream]::new($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
 try{[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()}finally{$stream.Dispose()}
}

# Public to the integration harness so invalid held bindings can exercise this
# exact production sequence without replacing any native identity API.
function Get-ObservedHeldMember($Session,[Diagnostics.Process]$Process,[int]$ExpectedPid,[long]$ExpectedStartedUtcTicks) {
 $row=$null
 try{
  if($Session.Failed -or $Session.Finalized){throw 'A failed/finalized observation cannot admit another identity.'}
  if(!$Process){throw 'Observed process object is unavailable.'}
  $held=$Process.SafeHandle
  $Session.Process.RequireMember($held)
  [GreenLensObservation.HeldProcessImage]::ValidateBinding($held,$ExpectedPid,$ExpectedStartedUtcTicks)
  $key=[string]$ExpectedPid+':'+$ExpectedStartedUtcTicks
  if($Session.Seen.ContainsKey($key)){return $Session.Seen[$key]}
  if($Session.Seen.Count -ge $Session.MaximumIdentities){throw 'Bounded observed identity capacity exceeded.'}
  $image=[GreenLensObservation.HeldProcessImage]::Read($held,$ExpectedPid,$ExpectedStartedUtcTicks)
  $row=[ordered]@{pid=$ExpectedPid;name=$image.Name;image_path=$image.ImagePath;started_utc=([DateTime]::new($ExpectedStartedUtcTicks,[DateTimeKind]::Utc)).ToString('o');first_observed_utc=[DateTime]::UtcNow.ToString('o');image_identity_method='held-handle-query-full-process-image-name.v1';image_sha256=(Get-ObservedImageHash $image.ImagePath);policy=$null}
 }catch{Set-ObservedJobFailure $Session 'identity' $_ @{pid=$ExpectedPid;expected_started_ticks=$ExpectedStartedUtcTicks};$Session.Record.process_identity_failure=$Session.Record.observation_failure;throw}
 # Policy is outside the inspection catch. Failed rows are retained as failure
 # diagnostics, never inserted into the trusted identity cache.
 try{
  $classification=& $Session.Policy $row
  if($classification -isnot [string] -or [string]::IsNullOrWhiteSpace($classification)){throw 'Observed executable policy did not return one explicit classification.'}
  $row.policy=$classification;$Session.Seen[$key]=$row
  if(!$Session.PolicyCounts.ContainsKey($classification)){$Session.PolicyCounts[$classification]=0}
  $Session.PolicyCounts[$classification]++
  return $row
 }catch{Set-ObservedJobFailure $Session 'policy' $_ $row;throw}
}

function Get-ObservedJobMember($Session,[int]$ProcessId) {
 $p=$null
 try{
  $p=[Diagnostics.Process]::GetProcessById($ProcessId)
  $started=$p.StartTime.ToUniversalTime()
  $row=Get-ObservedHeldMember $Session $p $ProcessId $started.Ticks
  $p.Refresh()
  return [ordered]@{pid=$ProcessId;name=$row.name;private_bytes=[long]$p.PrivateMemorySize64}
 }catch{
  if(!$Session.Failed){Set-ObservedJobFailure $Session 'identity-resource' $_ @{pid=$ProcessId};$Session.Record.process_identity_failure=$Session.Record.observation_failure}
  throw
 }finally{if($p){$p.Dispose()}}
}

function Assert-ObservedElapsed($Session,[double]$ElapsedMilliseconds,[double]$DeadlineMilliseconds,[string]$Stage) {
 try{
  if(![double]::IsFinite($ElapsedMilliseconds) -or $ElapsedMilliseconds -lt 0 -or ![double]::IsFinite($DeadlineMilliseconds) -or $DeadlineMilliseconds -le 0 -or $ElapsedMilliseconds -gt $DeadlineMilliseconds){throw "Observed $Stage exceeded its fixed deadline or supplied invalid timing."}
 }catch{Set-ObservedJobFailure $Session 'deadline' $_;throw}
}

function Get-ObservedJobSample($Session,[double]$ElapsedMilliseconds,[double]$DeadlineMilliseconds,[int]$MaximumSamples) {
 try{
  if($Session.Failed -or $Session.Finalized){throw 'A failed/finalized observation cannot produce accepted samples.'}
  Assert-ObservedElapsed $Session $ElapsedMilliseconds $DeadlineMilliseconds 'sample'
  if($MaximumSamples -lt 1 -or $Session.SampleCount -ge $MaximumSamples){throw 'Observation sample capacity exceeded.'}
  if($Session.Stdout.Overflow -or $Session.Stderr.Overflow){throw 'A process stream exceeded its fixed1 MiB bound.'}
  if($Session.OutTask.IsFaulted -or $Session.ErrTask.IsFaulted -or $Session.OutTask.IsCanceled -or $Session.ErrTask.IsCanceled){throw 'A bounded process-output drain failed.'}
  $proc=$Session.Process;$limits=@($proc.CommittedMemoryLimitSnapshot)
  if($limits.Count -ne 4 -or ($limits[0] -band 0x2208) -ne 0x2208 -or $limits[1] -ne 16 -or $limits[2] -ne 1GB){throw 'Held Job kernel limits differ from fixed1 GiB/16-process supervision.'}
  $Session.Record.raw_peak_max_bytes=[Math]::Max([long]$Session.Record.raw_peak_max_bytes,[long]$limits[3])
  if($limits[3] -gt 2GB){throw 'Raw kernel Job peak exceeded the unchanged2 GiB acceptance ceiling.'}
  $members=@($proc.MemberIds);$active=$proc.ActiveProcesses;$working=$proc.WorkingSetBytes
  if($active -gt 16 -or $members.Count -gt 16){throw 'Owned Job process count exceeded16.'}
  $observed=[Collections.Generic.List[object]]::new();[long]$private=0
  foreach($member in $members){$row=Get-ObservedJobMember $Session ([int]$member);$private+=$row.private_bytes;$observed.Add($row)}
  $Session.Record.sampled_private_max_bytes=[Math]::Max([long]$Session.Record.sampled_private_max_bytes,$private)
  $Session.Record.member_count_max=[Math]::Max([int]$Session.Record.member_count_max,[int]$active)
  $Session.SampleCount++
  return [ordered]@{elapsed_ms=$ElapsedMilliseconds;active_processes=$active;member_ids=$members;members=$observed.ToArray();kernel_limit_bytes=$limits[2];raw_peak_bytes=$limits[3];sampled_sum_private_bytes=$private;sampled_sum_working_set_bytes=$working}
 }catch{if(!$Session.Failed){Set-ObservedJobFailure $Session 'sample' $_};throw}
}

function Close-ObservedJobSession($Session) {
 if(!$Session -or $Session.Finalized){return}
 try{
  $proc=$Session.Process
  if(!$proc.JobEmpty){$Session.Record.forced_owned_job_kill=$true;$proc.Kill();$kill=[Diagnostics.Stopwatch]::StartNew();while(!$proc.JobEmpty -and $kill.ElapsedMilliseconds -le 5000){Start-Sleep -Milliseconds 25}}
  $Session.Record.cleanup_job_empty=$proc.JobEmpty
  $Session.Record.final_raw_peak_bytes=$proc.PeakJobCommittedBytes
  $Session.Record.raw_peak_max_bytes=[Math]::Max([long]$Session.Record.raw_peak_max_bytes,[long]$Session.Record.final_raw_peak_bytes)
  if(!$Session.Record.cleanup_job_empty -or $Session.Record.raw_peak_max_bytes -gt 2GB){throw 'Owned tree remained live or final raw kernel peak exceeded2 GiB.'}
  foreach($task in @($Session.OutTask,$Session.ErrTask)){if(!$task.Wait(5000)){throw 'Bounded process output did not reach EOF.'};$null=$task.GetAwaiter().GetResult()}
  if($Session.Stdout.Overflow -or $Session.Stderr.Overflow){throw 'A completed process stream exceeded its fixed1 MiB bound.'}
  $Session.CleanupPassed=$true
 }catch{Set-ObservedJobFailure $Session 'cleanup' $_;$Session.Record.cleanup_error=$_.Exception.Message}
 finally{
  try{$Session.Process.Dispose()}catch{Set-ObservedJobFailure $Session 'cleanup-disposal' $_;$Session.Record.cleanup_error=$_.Exception.Message;$Session.CleanupPassed=$false}
  $Session.Finalized=$true
 }
}

function Save-ObservedJobOutput($Session,[string]$Directory) {
 try{
  [IO.File]::WriteAllBytes((Join-Path $Directory 'stdout.bin'),$Session.Stdout.Bytes)
  [IO.File]::WriteAllBytes((Join-Path $Directory 'stderr.bin'),$Session.Stderr.Bytes)
 }catch{Set-ObservedJobFailure $Session 'output-publication' $_}
 $Session.Record.stdout_total_bytes=$Session.Stdout.Total;$Session.Record.stderr_total_bytes=$Session.Stderr.Total
 $Session.Record.stdout_overflow=$Session.Stdout.Overflow;$Session.Record.stderr_overflow=$Session.Stderr.Overflow
}

function Assert-ObservedJobSuccess($Session) {
 if(!$Session -or $Session.Failed -or !$Session.Finalized -or !$Session.CleanupPassed -or !$Session.Record.cleanup_job_empty -or $Session.SampleCount -lt 1 -or $Session.Seen.Count -lt 1){throw 'Identity, sample, policy and owned cleanup checks must all pass before success publication.'}
 if($Session.Record.failure -or $Session.Record.energyplus_observed -or $Session.Stdout.Overflow -or $Session.Stderr.Overflow -or $Session.Record.raw_peak_max_bytes -gt 2GB){throw 'Failed observation cannot publish success.'}
}

function Write-ObservedJobReceipt($Session,[string]$Path,$Record,[string]$FailureDirectory,[string]$SummaryPath='',$Summary=$null) {
 $temporary=$Path+'.writing';$summaryTemporary=$SummaryPath+'.writing';$summaryPublished=$false;$receiptPublished=$false
 try{
  if($Record.completed){Assert-ObservedJobSuccess $Session}
  elseif($SummaryPath){throw 'An incomplete observation cannot publish a successful summary.'}
  if(Test-Path -LiteralPath $Path){throw 'Observer receipt already exists; no overwrite or resume.'}
  $text=ConvertTo-Json -InputObject $Record -Depth 24
  if([Text.Encoding]::UTF8.GetByteCount($text) -gt 64MB){throw 'Observer receipt exceeds its bounded64 MiB capacity.'}
  $file=[IO.FileStream]::new($temporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
  try{$bytes=[Text.UTF8Encoding]::new($false).GetBytes($text);$file.Write($bytes);$file.Flush($true)}finally{$file.Dispose()}
  if($SummaryPath){
   if(!$Summary -or !$Summary.completed -or (Test-Path -LiteralPath $SummaryPath)){throw 'Invalid or existing observer success summary.'}
   $Summary.receipt_sha256=Get-ObservedImageHash $temporary
   $summaryText=ConvertTo-Json -InputObject $Summary -Depth 12
   if([Text.Encoding]::UTF8.GetByteCount($summaryText) -gt 64KB){throw 'Observer summary exceeds its bounded capacity.'}
   $file=[IO.FileStream]::new($summaryTemporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
   try{$bytes=[Text.UTF8Encoding]::new($false).GetBytes($summaryText);$file.Write($bytes);$file.Flush($true)}finally{$file.Dispose()}
   [IO.File]::Move($summaryTemporary,$SummaryPath);$summaryPublished=$true
  }
  # The receipt is the final commit point. A summary alone is never acceptance.
  [IO.File]::Move($temporary,$Path);$receiptPublished=$true
 }catch{
  $Record.completed=$false
  if($Session){Set-ObservedJobFailure $Session 'receipt-publication' $_;Close-ObservedJobSession $Session}
  $publicationError=$_.Exception.Message;$rollbackError=$null
  if($summaryPublished -and !$receiptPublished){try{[IO.File]::Delete($SummaryPath)}catch{$rollbackError=$_.Exception.Message}}
  $diagnostic=[ordered]@{schema_version='greenlens.observer-publication-failure.v1';completed=$false;failure=$publicationError;record=$Record;temporary_receipt_retained=(Test-Path -LiteralPath $temporary);success_receipt_published=$receiptPublished;summary_rollback_error=$rollbackError;acceptance_rule='Only the final receipt is a commit point; a summary or .writing file without it is not acceptance.'}
  try{
   $fallback=Join-Path $FailureDirectory 'observer-publication-failure.json'
   $text=ConvertTo-Json -InputObject $diagnostic -Depth 24
   if([Text.Encoding]::UTF8.GetByteCount($text) -gt 64MB){$text='{"completed":false,"failure":"Observer publication failed; detailed receipt exceeded bound."}'}
   $file=[IO.FileStream]::new($fallback,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
   try{$bytes=[Text.UTF8Encoding]::new($false).GetBytes($text);$file.Write($bytes);$file.Flush($true)}finally{$file.Dispose()}
  }catch{[Console]::Error.WriteLine('Observer receipt publication failed; no success accepted. Bounded failure receipt also unavailable: '+$_.Exception.Message.Substring(0,[Math]::Min(2048,$_.Exception.Message.Length)))}
  throw
 }
}
