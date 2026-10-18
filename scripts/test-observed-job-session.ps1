#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. Handshake-controlled synthetic children
# only. Uses the same shared functions as both production collectors.
[CmdletBinding()]
param([string]$OutputDirectory=(Join-Path $env:LOCALAPPDATA ('GreenLens/verification/observed-session-'+[Guid]::NewGuid().ToString('N')+' 空間')))
$ErrorActionPreference='Stop'
$output=[IO.Path]::GetFullPath($OutputDirectory)
$cache=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
if(!$output.StartsWith($cache+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $output)){throw 'Use a new external synthetic receipt directory.'}
New-Item -ItemType Directory -Path $output|Out-Null
$helper=Join-Path $PSScriptRoot 'observed-job-session.ps1'
. $helper
Add-Type -LiteralPath (Join-Path $PSScriptRoot 'annual-paired-native-process.cs')
Add-Type -LiteralPath (Join-Path $PSScriptRoot 'observed-process-image.cs')
Initialize-ObservedOutputCapture
$exe=[Environment]::ProcessPath;$exeHash=Get-ObservedImageHash $exe
$conhost=Join-Path ([Environment]::GetFolderPath('System')) 'conhost.exe';$conhostHash=Get-ObservedImageHash $conhost
$checks=[Collections.Generic.List[object]]::new();$cases=[Collections.Generic.List[object]]::new();$sessions=[Collections.Generic.List[object]]::new();$failure=$null;$launched=0;$timer=[Diagnostics.Stopwatch]::StartNew()
$self=[Diagnostics.Process]::GetCurrentProcess();$selfPid=$self.Id;$selfStarted=$self.StartTime.ToUniversalTime().Ticks
$protocol=[ordered]@{schema_version='greenlens.observed-job-integration.v2';synthetic_only=$true;declared_utc=[DateTime]::UtcNow.ToString('o');maximum_owned_children=12;total_deadline_seconds=180;handshake_deadline_seconds=15;kernel_guard_bytes=1GB;active_process_limit=16;output_stream_cap_bytes=1MB;maximum_retained_member_handles=4096;app_renderer_engine_executions=0;cases=@('live-cache-snapshot-known-exit','disappeared-before-identity','pid-mismatch','creation-time-mismatch','foreign-job','unavailable-held-handle','policy-rejection-owned-descendants','receipt-write-failure','deadline-and-final-completion','bounded-output-overflow','cleanup-drain-failure');helpers=@{session=(Get-FileHash $helper).Hash.ToLowerInvariant();image=(Get-FileHash (Join-Path $PSScriptRoot 'observed-process-image.cs')).Hash.ToLowerInvariant();native=(Get-FileHash (Join-Path $PSScriptRoot 'annual-paired-native-process.cs')).Hash.ToLowerInvariant();test=(Get-FileHash $PSCommandPath).Hash.ToLowerInvariant()}}
$protocol|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $output 'protocol.json') -Encoding utf8NoBOM
$fixture=Join-Path $output 'synthetic-child.ps1'
@'
param([string]$Mode,[string]$Directory)
$ErrorActionPreference='Stop'
function Ready([string]$Name){$p=[Diagnostics.Process]::GetCurrentProcess();$v=@{pid=$PID;started_ticks=$p.StartTime.ToUniversalTime().Ticks}|ConvertTo-Json;$tmp=Join-Path $Directory ($Name+'.tmp');[IO.File]::WriteAllText($tmp,$v);[IO.File]::Move($tmp,(Join-Path $Directory ($Name+'.json')));$p.Dispose()}
if($Mode -eq 'leaf'){Ready 'leaf';while(!(Test-Path -LiteralPath (Join-Path $Directory 'leaf-exit'))){Start-Sleep -Milliseconds 20};exit 0}
if($Mode -eq 'tree'){$info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=[Environment]::ProcessPath;$info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.WorkingDirectory=$Directory;foreach($arg in @('-NoProfile','-NonInteractive','-File',$PSCommandPath,'leaf',$Directory)){$info.ArgumentList.Add($arg)};$leaf=[Diagnostics.Process]::Start($info);$leaf.Dispose()}
if($Mode -eq 'overflow'){$bytes=[byte[]]::new(2MB);[Array]::Fill[byte]($bytes,65);$stream=[Console]::OpenStandardOutput();$stream.Write($bytes);$stream.Flush()}
Ready 'ready';[Console]::Error.WriteLine('original-synthetic-diagnostic');[Console]::ReadLine()|Out-Null;exit 0
'@|Set-Content -LiteralPath $fixture -Encoding utf8NoBOM
function Check([bool]$Condition,[string]$Name){if(!$Condition){throw "Failed integration check: $Name"};$checks.Add(@{name=$Name;passed=$true})}
function Reject([scriptblock]$Action,[string]$Name,[string]$Pattern='*'){$errorText=$null;try{& $Action|Out-Null}catch{$errorText=$_.Exception.Message};Check ($null -ne $errorText -and $errorText -like $Pattern) $Name}
function Check-Deadline {if($timer.ElapsedMilliseconds -gt 180000){throw 'Synthetic integration harness exceeded declared180-second total deadline.'}}
function Await-File([string]$Path){$clock=[Diagnostics.Stopwatch]::StartNew();while(!(Test-Path -LiteralPath $Path)){Check-Deadline;if($clock.ElapsedMilliseconds -gt 15000){throw 'Synthetic handshake exceeded15 seconds.'};Start-Sleep -Milliseconds 20};Get-Content -LiteralPath $Path -Raw|ConvertFrom-Json -AsHashtable}
function New-Case([string]$Name,[string]$Mode='wait',[scriptblock]$Policy=$null){
 Check-Deadline;$script:launched+=if($Mode -ceq 'tree'){2}else{1};if($script:launched -gt 12){throw 'Synthetic child launch capacity exceeded.'}
 $dir=Join-Path $output $Name;New-Item -ItemType Directory -Path $dir|Out-Null
 $record=[ordered]@{completed=$false;failure=$null;cleanup_job_empty=$false;raw_peak_max_bytes=0;sampled_private_max_bytes=0;member_count_max=0;energyplus_observed=$false;synthetic_only=$true}
 if(!$Policy){$Policy={param($row) if($row.image_sha256 -ceq $exeHash -and $row.name -ceq [IO.Path]::GetFileNameWithoutExtension($exe)){return 'synthetic_pwsh'};if($row.image_sha256 -ceq $conhostHash -and $row.image_path.Equals($conhost,[StringComparison]::OrdinalIgnoreCase)){return 'system_console_host'};throw 'Unexpected synthetic test process identity.'}}
 $proc=[GreenLensPairedProcess.NativeProcess]::StartBounded($exe,@('-NoProfile','-NonInteractive','-File',$fixture,$Mode,$dir),$dir,'.',1GB,16)
 $stdout=[GreenLensObservedJob.Capture]::new();$stderr=[GreenLensObservedJob.Capture]::new();$outTask=$stdout.Drain($proc.Output);$errTask=$stderr.Drain($proc.Error)
 $session=New-ObservedJobSession $proc $record $Policy $stdout $stderr $outTask $errTask
 $sessions.Add($session);$ready=Await-File (Join-Path $dir 'ready.json')
 if($Mode -ceq 'tree'){$leaf=Await-File (Join-Path $dir 'leaf.json')}else{$leaf=$null}
 $case=[pscustomobject]@{name=$Name;directory=$dir;session=$session;ready=$ready;leaf=$leaf};$cases.Add($case);return $case
}
function Finish-Case($Case,[bool]$Success=$false){
 $s=$Case.session;$held=@($s.HeldByPid.Values);Close-ObservedJobSession $s;Save-ObservedJobOutput $s $Case.directory
 $s.Record.members=@($s.Seen.Values);$s.Record.completed=$Success
 if($Success){Assert-ObservedJobSuccess $s}
 Write-ObservedJobReceipt $s (Join-Path $Case.directory 'receipt.json') $s.Record $Case.directory
 Check $s.Record.cleanup_job_empty ($Case.name+': owned tree empty after cleanup')
 Check ($s.HeldByPid.Count -eq 0 -and $s.Record.retained_member_handles_remaining -eq 0 -and $s.Record.retained_member_handles_closed -eq $s.Record.retained_member_handles_total -and @($held|Where-Object {!$_.member.Handle.IsClosed}).Count -eq 0) ($Case.name+': retained handles closed on success or failure')
 Check (!$self.HasExited -and $self.Id -eq $selfPid -and $self.StartTime.ToUniversalTime().Ticks -eq $selfStarted) ($Case.name+': unrelated observer process was not terminated')
 if(!$Success){Check (!(Get-Content -LiteralPath (Join-Path $Case.directory 'receipt.json') -Raw|ConvertFrom-Json).completed) ($Case.name+': failure cannot publish successful receipt')}
}
function Release-Case($Case){$writer=[IO.StreamWriter]::new($Case.session.Process.Input,[Text.UTF8Encoding]::new($false),1024,$true);$writer.WriteLine('controlled-exit');$writer.Flush();$writer.Dispose();$Case.session.Process.Input.Close();Check ($Case.session.Process.WaitForExit(15000) -and $Case.session.Process.ExitCode -eq 0) ($Case.name+': controlled normal exit');$clock=[Diagnostics.Stopwatch]::StartNew();while(!$Case.session.Process.JobEmpty){if($clock.ElapsedMilliseconds -gt 5000){throw 'Controlled tree failed to become empty'};Start-Sleep -Milliseconds 20}}
try{
 $case=New-Case 'live-cache-snapshot-known-exit';$s=$case.session
 $first=Get-ObservedJobSample $s 0 15000 100
 Check ($s.Seen.Count -ge 1 -and $first.members.Count -ge 1) 'First production sample observes and admits actual child identity'
 $beforeCount=$s.Seen.Count;$policyCount=$s.PolicyCounts.synthetic_pwsh
 $second=Get-ObservedJobSample $s 1 15000 100
 Check ($s.Seen.Count -eq $beforeCount -and $s.PolicyCounts.synthetic_pwsh -eq $policyCount) 'Repeated production sample reuses policy-approved identity after held membership/time checks'
 Check (@($s.Seen.Values|Where-Object {$_.image_sha256 -ceq $exeHash -and $_.pid -eq $case.ready.pid}).Count -eq 1) 'Observed image hash comes from live executable and binds handshake identity'
 Check ($s.HeldByPid.Count -eq $s.Seen.Count -and $s.HeldByPid.Count -le $s.MaximumIdentities) 'Every approved identity owns one bounded duplicate handle separately from receipt rows'
 $snapshot=@($s.Process.MemberIds)
 Check ($snapshot -contains [uint]$case.ready.pid) 'Controlled live child appears in actual held-Job PID snapshot'
 # Handshake stops the already observed child between the production PID
 # enumeration and the shared per-member read. No timing sleep or API mock.
 Release-Case $case
 Reject {[Diagnostics.Process]::GetProcessById([int]$case.ready.pid)} 'The RC1 managed PID-reopen path cannot find the controlled exited child' '*not running*'
 $afterExit=Get-ObservedMemberSnapshot $s $snapshot
 $exitRow=@($afterExit.members|Where-Object {$_.pid -eq $case.ready.pid})
 Check ($exitRow.Count -eq 1 -and $exitRow[0].state -ceq 'verified-exited' -and $null -eq $exitRow[0].private_bytes -and !$exitRow[0].private_bytes_observed) 'Known exit retains exact identity and reports unavailable private memory rather than invented zero'
 Check (!$s.Failed -and !$afterExit.sampled_private_complete -and $afterExit.known_exited_members -ge 1 -and $s.PolicyCounts.synthetic_pwsh -eq $policyCount) 'Post-enumeration known exit remains accepted without reclassifying policy or hiding incomplete sample'
 $s.Record.controlled_post_enumeration_exit=$afterExit
 Get-ObservedJobSample $s 2 15000 100|Out-Null
 Assert-ObservedElapsed $s 15000 15000 'final completion'
 Finish-Case $case $true
 Check (!(Test-Path (Join-Path $case.directory 'receipt.json.writing'))) 'Successful final receipt is atomic with no unpublished temporary'
 $receiptText=Get-Content -LiteralPath (Join-Path $case.directory 'receipt.json') -Raw
 Check (!$receiptText.Contains('SafeProcessHandle') -and !$receiptText.Contains('HeldByPid') -and !$receiptText.Contains('DangerousGetHandle')) 'Receipt serializes identity and lifecycle evidence without handles or live process objects'

 $case=New-Case 'disappeared-before-identity';Release-Case $case
 Reject {Get-ObservedJobMember $case.session ([int]$case.ready.pid)} 'Disappearance before first identification fails closed'
 Check ($case.session.Seen.Count -eq 0 -and $case.session.Record.process_identity_failure) 'Unidentified vanished child never enters trusted cache'
 Finish-Case $case

 foreach($kind in @('pid-mismatch','creation-time-mismatch','foreign-job','unavailable-held-handle')){
  $case=New-Case $kind;$s=$case.session;$p=[Diagnostics.Process]::GetProcessById([int]$case.ready.pid)
  try{
   # Two cases begin with a real approved cache entry to exercise the new
   # retained path; PID mismatch and foreign membership still cover first use.
   $expectedSeen=0
   if($kind -in @('creation-time-mismatch','unavailable-held-handle')){
    Get-ObservedJobSample $s 0 15000 100|Out-Null;$expectedSeen=$s.Seen.Count
    Check ($expectedSeen -ge 1) ($kind+': establish real approved retained identity before fault')
   }
   switch($kind){
    'pid-mismatch'{Reject {Get-ObservedHeldMember $s $p ($p.Id+1) ([long]$case.ready.started_ticks)} 'PID mismatch rejects whole shared identity/policy path' '*binding changed*'}
    'creation-time-mismatch'{$s.HeldByPid[[string]$p.Id].started_ticks++;Reject {Get-ObservedJobMember $s $p.Id} 'Creation-time mismatch cannot reuse retained identity' '*binding changed*'}
    'foreign-job'{Reject {Get-ObservedHeldMember $s $self $selfPid $selfStarted} 'Foreign held identity fails exact owned Job membership' '*outside the owned Job*'}
    'unavailable-held-handle'{$s.HeldByPid[[string]$p.Id].member.Dispose();Reject {Get-ObservedJobMember $s $p.Id} 'Unavailable retained handle is not replaced by cached executable metadata' '*handle*'}
   }
   Check ($s.Seen.Count -eq $expectedSeen -and $s.Failed) ($kind+': policy/cache cannot accept an additional unresolved identity')
  }finally{$p.Dispose()}
  Finish-Case $case
 }

 $case=New-Case 'policy-rejection-owned-descendants' 'tree' {param($row) throw 'Declared synthetic policy rejection'};$s=$case.session
 Check ($s.Process.ActiveProcesses -ge 2) 'Synthetic parent and descendant are both owned before policy rejection'
 Reject {Get-ObservedJobSample $s 0 15000 100} 'Policy rejection remains failure after actual identity inspection' '*Declared synthetic policy rejection*'
 Check ($s.Seen.Count -eq 0 -and $s.Record.observation_failure.stage -ceq 'policy') 'Rejected row is diagnostic only and absent from identity cache'
 Finish-Case $case
 Check $s.Record.forced_owned_job_kill 'Policy failure cleans entire owned tree'

 $case=New-Case 'receipt-write-failure';$s=$case.session;Get-ObservedJobSample $s 0 15000 100|Out-Null;Release-Case $case
 Close-ObservedJobSession $s;Save-ObservedJobOutput $s $case.directory;$s.Record.completed=$true
 $bad=Join-Path $case.directory 'receipt.json';New-Item -ItemType Directory -Path ($bad+'.writing')|Out-Null
 Reject {Write-ObservedJobReceipt $s $bad $s.Record $case.directory (Join-Path $case.directory 'result.json') @{completed=$true}} 'Receipt failure cannot be reported as success'
 Check ($s.Failed -and !$s.Record.completed -and !(Test-Path -LiteralPath $bad) -and !(Test-Path -LiteralPath (Join-Path $case.directory 'result.json'))) 'Actual receipt-write failure invalidates success and emits no success receipt or summary'
 $diagnostic=Get-Content -LiteralPath (Join-Path $case.directory 'observer-publication-failure.json') -Raw|ConvertFrom-Json
 Check (!$diagnostic.completed -and !$diagnostic.success_receipt_published -and $s.Record.cleanup_job_empty) 'Receipt failure retains explicit diagnostics after owned cleanup'

 $case=New-Case 'deadline-and-final-completion';$s=$case.session;Get-ObservedJobSample $s 0 15000 100|Out-Null
 Reject {Get-ObservedJobSample $s 15000.001 15000 100} 'Live sample deadline rejection is exact and does not extend budget' '*deadline*'
 Reject {Assert-ObservedElapsed $s 600000.001 600000 'final completion'} 'Late final completion cannot publish success after a timely sample' '*deadline*'
 Finish-Case $case
 Reject {Assert-ObservedJobSuccess $s} 'Cleaned deadline failure remains barred from success'

 $case=New-Case 'bounded-output-overflow' 'overflow';$s=$case.session
 Reject {Get-ObservedJobSample $s 0 15000 100} 'Actual child output overflow rejects accepted sampling' '*1 MiB*'
 Finish-Case $case
 Check ($s.Stdout.Total -eq 2MB -and $s.Stdout.Bytes.Length -eq 1MB -and $s.Stdout.Overflow) 'Actual child output drains to EOF while retained allocation stays at1 MiB'
 Check ((Get-Item -LiteralPath (Join-Path $case.directory 'stdout.bin')).Length -eq 1MB) 'Overflow diagnostics retain exactly bounded original bytes'

 $case=New-Case 'cleanup-drain-failure';$s=$case.session;Get-ObservedJobSample $s 0 15000 100|Out-Null;Release-Case $case
 # Explicit synthetic drain fault, after real process observation/exit. It does
 # not replace an image API or pretend that a real child produced this fault.
 $realTask=$s.OutTask;$null=$realTask.GetAwaiter().GetResult()
 $s.OutTask=[Threading.Tasks.Task]::FromException([IO.IOException]::new('Declared synthetic EOF drain failure'))
 Finish-Case $case
 Check ($s.Failed -and !$s.CleanupPassed -and $s.Record.cleanup_error.Contains('Declared synthetic EOF drain failure')) 'Cleanup failure prevents successful final receipt even when Job is empty'
 Check-Deadline;Check ($launched -eq 12) 'Exactly12 harmless owned children cover declared cases without exceeding budget'
}catch{$failure=$_.Exception.Message}
finally{
 foreach($s in $sessions){if(!$s.Finalized){Close-ObservedJobSession $s}}
 $receipt=[ordered]@{schema_version=$protocol.schema_version;synthetic_only=$true;helpers=$protocol.helpers;protocol_sha256=(Get-FileHash (Join-Path $output 'protocol.json')).Hash.ToLowerInvariant();passed_count=$checks.Count;failure=$failure;owned_synthetic_children=$launched;app_renderer_engine_executions=0;elapsed_seconds=$timer.Elapsed.TotalSeconds;all_owned_jobs_cleanup_passed=@($sessions|Where-Object {!$_.Record.cleanup_job_empty}).Count -eq 0;cases=@($cases|ForEach-Object {@{name=$_.name;record=$_.session.Record}});checks=$checks.ToArray();completed_utc=[DateTime]::UtcNow.ToString('o')}
 $receipt|ConvertTo-Json -Depth 24|Set-Content -LiteralPath (Join-Path $output 'checks.json') -Encoding utf8NoBOM
 $self.Dispose()
}
if($failure){throw "Shared observer synthetic integration failed: $failure; receipt: $output"}
Write-Host "$($checks.Count) shared observer integration checks passed; receipt: $output"
