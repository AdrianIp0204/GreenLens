#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. Shared observer path, not an application
# worker or sandbox. Callers own process creation and domain evidence checks.
function Initialize-ObservedOutputCapture {
 Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;
namespace GreenLensObservedJob {
 // Retention owns a duplicate, never the caller's Process/SafeHandle. A held
 // process object keeps its PID from being recycled until this handle closes:
 // https://devblogs.microsoft.com/oldnewthing/20110107-00/?p=11803
 public sealed class RetainedMember : IDisposable {
  public SafeProcessHandle Handle { get; private set; }
  [StructLayout(LayoutKind.Sequential)]
  struct MemoryCounters {
   public uint Size, PageFaultCount;
   public UIntPtr PeakWorkingSet, WorkingSet, PeakPagedPool, PagedPool;
   public UIntPtr PeakNonPagedPool, NonPagedPool, Pagefile, PeakPagefile, Private;
  }
  [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
  [DllImport("kernel32.dll", SetLastError=true)]
  [return: MarshalAs(UnmanagedType.Bool)]
  static extern bool DuplicateHandle(IntPtr sourceProcess, SafeProcessHandle source,
   IntPtr targetProcess, out SafeProcessHandle target, uint access,
   [MarshalAs(UnmanagedType.Bool)] bool inherit, uint options);
  [DllImport("kernel32.dll", SetLastError=true)]
  static extern uint WaitForSingleObject(SafeProcessHandle process, uint milliseconds);
  [DllImport("psapi.dll", SetLastError=true)]
  [return: MarshalAs(UnmanagedType.Bool)]
  static extern bool GetProcessMemoryInfo(SafeProcessHandle process, ref MemoryCounters counters, uint size);
  public RetainedMember(SafeProcessHandle source) {
   if(source==null || source.IsClosed || source.IsInvalid)
    throw new InvalidOperationException("Cannot retain an unavailable observed process handle.");
   SafeProcessHandle duplicate;
   if(!DuplicateHandle(GetCurrentProcess(),source,GetCurrentProcess(),out duplicate,0,false,2)) {
    int error=Marshal.GetLastWin32Error(); if(duplicate!=null) duplicate.Dispose();
    throw new Win32Exception(error,"Duplicate observed process handle");
   }
   Handle=duplicate;
  }
  public bool HasExited {
   get {
    if(Handle==null || Handle.IsClosed || Handle.IsInvalid)
     throw new InvalidOperationException("Retained observed process handle is unavailable.");
    uint state=WaitForSingleObject(Handle,0);
    if(state==0) return true;
    if(state==258) return false;
    throw new Win32Exception(Marshal.GetLastWin32Error(),"Query retained observed process exit");
   }
  }
  public long? PrivateBytes {
   get {
    if(HasExited) return null;
    var counters=new MemoryCounters(); counters.Size=(uint)Marshal.SizeOf<MemoryCounters>();
    if(IntPtr.Size!=8 || counters.Size!=80)
     throw new PlatformNotSupportedException("Audited x64 process-memory ABI required.");
    if(!GetProcessMemoryInfo(Handle,ref counters,counters.Size)) {
     int error=Marshal.GetLastWin32Error();
     if(HasExited) return null;
     throw new Win32Exception(error,"Read retained observed process private commitment");
    }
    // Do not report a terminated member's stale counters as current or as zero.
    if(HasExited) return null;
    return checked((long)counters.Private.ToUInt64());
   }
  }
  public void Dispose() { if(Handle!=null) Handle.Dispose(); }
 }
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
 return [pscustomobject]@{Process=$Process;Record=$Record;Policy=$Policy;Stdout=$Stdout;Stderr=$Stderr;OutTask=$OutTask;ErrTask=$ErrTask;Seen=@{};HeldByPid=@{};PolicyCounts=@{};SampleCount=0;Failed=$false;Finalized=$false;CleanupPassed=$false;MaximumIdentities=4096}
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
  if($Session.Seen.ContainsKey($key)){
   if(!$Session.HeldByPid.ContainsKey([string]$ExpectedPid) -or $Session.HeldByPid[[string]$ExpectedPid].key -cne $key){throw 'Cached identity has no exact retained process binding.'}
   return $Session.Seen[$key]
  }
  if($Session.Seen.Count -ge $Session.MaximumIdentities){throw 'Bounded observed identity capacity exceeded.'}
  if($Session.HeldByPid.ContainsKey([string]$ExpectedPid)){throw 'Observed PID conflicts with a retained creation-time binding.'}
  $image=[GreenLensObservation.HeldProcessImage]::Read($held,$ExpectedPid,$ExpectedStartedUtcTicks)
  $row=[ordered]@{pid=$ExpectedPid;name=$image.Name;image_path=$image.ImagePath;started_utc=([DateTime]::new($ExpectedStartedUtcTicks,[DateTimeKind]::Utc)).ToString('o');first_observed_utc=[DateTime]::UtcNow.ToString('o');image_identity_method='held-handle-query-full-process-image-name.v1';image_sha256=(Get-ObservedImageHash $image.ImagePath);policy=$null}
 }catch{Set-ObservedJobFailure $Session 'identity' $_ @{pid=$ExpectedPid;expected_started_ticks=$ExpectedStartedUtcTicks};$Session.Record.process_identity_failure=$Session.Record.observation_failure;throw}
 # Policy is outside the inspection catch. Failed rows are retained as failure
 # diagnostics, never inserted into the trusted identity cache.
 try{
  $classification=& $Session.Policy $row
  if($classification -isnot [string] -or [string]::IsNullOrWhiteSpace($classification)){throw 'Observed executable policy did not return one explicit classification.'}
  $row.policy=$classification
 }catch{Set-ObservedJobFailure $Session 'policy' $_ $row;throw}
 $retained=$null
 try{
  $retained=[GreenLensObservedJob.RetainedMember]::new($held)
  $Session.Process.RequireMember($retained.Handle)
  [GreenLensObservation.HeldProcessImage]::ValidateBinding($retained.Handle,$ExpectedPid,$ExpectedStartedUtcTicks)
  $Session.HeldByPid[[string]$ExpectedPid]=[pscustomobject]@{key=$key;pid=$ExpectedPid;started_ticks=$ExpectedStartedUtcTicks;member=$retained}
  $retained=$null
  $Session.Seen[$key]=$row
  if(!$Session.PolicyCounts.ContainsKey($classification)){$Session.PolicyCounts[$classification]=0}
  $Session.PolicyCounts[$classification]++
  return $row
 }catch{Set-ObservedJobFailure $Session 'identity-retention' $_ $row;throw}finally{if($retained){$retained.Dispose()}}
}

function Get-ObservedJobMember($Session,[int]$ProcessId) {
 $p=$null
 try{
  if($Session.Failed -or $Session.Finalized){throw 'A failed/finalized observation cannot inspect another member.'}
  if(!$Session.HeldByPid.ContainsKey([string]$ProcessId)){
   # Unknown/disappeared first observations still fail closed. Never substitute
   # a configured image, name or a stale metadata-only PID entry.
   $p=[Diagnostics.Process]::GetProcessById($ProcessId)
   $started=$p.StartTime.ToUniversalTime()
   $null=Get-ObservedHeldMember $Session $p $ProcessId $started.Ticks
  }
  $entry=$Session.HeldByPid[[string]$ProcessId];$held=$entry.member.Handle
  $Session.Process.RequireMember($held)
  [GreenLensObservation.HeldProcessImage]::ValidateBinding($held,$ProcessId,$entry.started_ticks)
  if($entry.key -cne ([string]$ProcessId+':'+$entry.started_ticks) -or !$Session.Seen.ContainsKey($entry.key)){throw 'Retained member has no exact policy-approved identity.'}
  $row=$Session.Seen[$entry.key];$private=$entry.member.PrivateBytes
  return [ordered]@{pid=$ProcessId;name=$row.name;started_utc=$row.started_utc;state=$(if($null -eq $private){'verified-exited'}else{'verified-live'});private_bytes=$private;private_bytes_observed=($null -ne $private)}
 }catch{
  if(!$Session.Failed){Set-ObservedJobFailure $Session 'identity-resource' $_ @{pid=$ProcessId};$Session.Record.process_identity_failure=$Session.Record.observation_failure}
  throw
 }finally{if($p){$p.Dispose()}}
}

# The collector and handshake regression share this exact post-enumeration path.
# A known member may exit after MemberIds returns: retain its verified identity,
# disclose an unavailable live-memory reading, and keep raw Job peaks unchanged.
function Get-ObservedMemberSnapshot($Session,[uint[]]$MemberIds) {
 if($MemberIds.Count -gt 16 -or @($MemberIds|Select-Object -Unique).Count -ne $MemberIds.Count){throw 'Owned member snapshot exceeds bounds or repeats an identity.'}
 $observed=[Collections.Generic.List[object]]::new();[long]$private=0;[int]$exited=0
 foreach($member in $MemberIds){
  $row=Get-ObservedJobMember $Session ([int]$member)
  if($row.private_bytes_observed){$private+=$row.private_bytes}else{$exited++}
  $observed.Add($row)
 }
 return [ordered]@{members=$observed.ToArray();sampled_sum_private_bytes=$private;sampled_private_complete=($exited -eq 0);known_exited_members=$exited}
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
  $observed=Get-ObservedMemberSnapshot $Session $members
  $Session.Record.sampled_private_max_bytes=[Math]::Max([long]$Session.Record.sampled_private_max_bytes,[long]$observed.sampled_sum_private_bytes)
  $Session.Record.member_count_max=[Math]::Max([int]$Session.Record.member_count_max,[int]$active)
  $Session.SampleCount++
  return [ordered]@{elapsed_ms=$ElapsedMilliseconds;active_processes=$active;member_ids=$members;members=$observed.members;kernel_limit_bytes=$limits[2];raw_peak_bytes=$limits[3];sampled_sum_private_bytes=$observed.sampled_sum_private_bytes;sampled_private_complete=$observed.sampled_private_complete;known_exited_members=$observed.known_exited_members;sampled_sum_working_set_bytes=$working}
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
  $Session.Record.retained_member_handles_total=$Session.HeldByPid.Count
  $Session.Record.retained_member_handles_closed=0
  foreach($entry in @($Session.HeldByPid.Values)){
   try{
    $entry.member.Dispose()
    if(!$entry.member.Handle.IsClosed){throw 'Retained member handle did not close.'}
    $Session.Record.retained_member_handles_closed++
   }catch{Set-ObservedJobFailure $Session 'member-disposal' $_;$Session.Record.cleanup_error=$_.Exception.Message;$Session.CleanupPassed=$false}
  }
  $Session.Record.retained_member_handles_remaining=$Session.Record.retained_member_handles_total-$Session.Record.retained_member_handles_closed
  $Session.HeldByPid.Clear()
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
