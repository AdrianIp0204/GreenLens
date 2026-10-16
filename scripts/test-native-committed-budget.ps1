#Requires -Version 7.2
# Original GreenLens engineering regression, Apache-2.0.
# Native fixture only; not EnergyPlus/Typst/converter or installed UI acceptance.
# Build owner compiles fixtures/native-committed-budget.cpp x64 /MT externally.
# This script does not compile or retry failures. Previous receipts stay intact.
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$FixtureExecutable,
 [Parameter(Mandatory)][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$ExpectedFixtureSha256,
 [string]$OutputDirectory=''
)
$ErrorActionPreference='Stop'
$cache=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
if(!$OutputDirectory){$OutputDirectory=Join-Path $cache ('verification/committed budget '+[Guid]::NewGuid().ToString('N')+' 空間')}
$root=[IO.Path]::GetFullPath($OutputDirectory)
if(!$root.StartsWith($cache+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Retain receipts under LOCALAPPDATA/GreenLens, outside synced source.'}
if(Test-Path -LiteralPath $root){throw 'Use a new retained test directory; previous failures are immutable.'}
$exe=(Resolve-Path -LiteralPath $FixtureExecutable).Path
$fixtureHash=(Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant()
if($fixtureHash -ne $ExpectedFixtureSha256.ToLowerInvariant()){throw 'Fixture differs from the explicitly declared build.'}
$native=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'annual-paired-native-process.cs')).Path
$source=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'fixtures/native-committed-budget.cpp')).Path
$nativeHash=(Get-FileHash -LiteralPath $native).Hash.ToLowerInvariant()
Add-Type -LiteralPath $native
New-Item -ItemType Directory -Path $root|Out-Null
$results=[Collections.Generic.List[object]]::new()
$protocol=[ordered]@{
 schema_version='greenlens.native-committed-budget-regression.v2';declared_utc=[DateTime]::UtcNow.ToString('o')
 scope='Native synthetic committed-memory/process containment only; not EnergyPlus cancellation, renderer or converter acceptance'
 native_process_sha256=$nativeHash;fixture_source_sha256=(Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant();fixture_executable_sha256=$fixtureHash
 test_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
 deadline_seconds_per_case=20;kill_deadline_seconds=5;poll_milliseconds=20;settled_observation_milliseconds=250
 cases=@(
  @{name='single-near-limit';mode='single';memory_bytes=384MB;processes=4;max_commit_steps=512;step_bytes=1MB},
  @{name='aggregate-denied';mode='aggregate';memory_bytes=384MB;processes=4;parent_bytes=128MB;child_initial_bytes=128MB;child_extra_bytes=160MB;expect_denied=$true},
  @{name='aggregate-positive-control';mode='aggregate';memory_bytes=512MB;processes=4;parent_bytes=128MB;child_initial_bytes=128MB;child_extra_bytes=160MB;expect_denied=$false},
  @{name='process-count';mode='process-cap';memory_bytes=384MB;processes=1}
 )
 criteria='Every observed kernel peak must be positive and <= its predeclared bound, with no overshoot tolerance. No case retries. Native slots bind live job members. Kill must empty the held job within 5 seconds.'
 previous_failure='The preceding PowerShell/CLR fixture exceeded its 384 MiB kernel-peak criterion despite allocation denial. Cause remains unestablished; this protocol does not erase or accept it.'
 references=@('https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_limit_information','https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information','https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffile')
}
$protocol|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $root 'protocol.json') -Encoding utf8NoBOM

function Read-Slot($View,[long]$Offset){
 # Coherent mapped views on the same local file. ReadFile/Get-Content is not
 # guaranteed coherent with mapped writes. The fixture never formats a receipt.
 $state=$View.ReadInt32($Offset+20)
 $slot=[ordered]@{magic=$View.ReadUInt32($Offset);version=$View.ReadUInt32($Offset+4);pid=$View.ReadUInt32($Offset+8);role=$View.ReadUInt32($Offset+12);mode=$View.ReadUInt32($Offset+16);state=$state;failure=$View.ReadUInt32($Offset+24);child_pid=$View.ReadUInt32($Offset+28);committed_bytes=$View.ReadUInt64($Offset+32);attempted_bytes=$View.ReadUInt64($Offset+40);allocation_denied=$View.ReadUInt32($Offset+48);child_created=$View.ReadUInt32($Offset+52);child_create_error=$View.ReadUInt32($Offset+56);child_wait_result=$View.ReadUInt32($Offset+60);allocation_error=$View.ReadUInt32($Offset+64)}
 $slot.stable=($state -eq $View.ReadInt32($Offset+20));return [pscustomobject]$slot
}
function Observe-Members($Proc,$Seen){
 $members=@($Proc.MemberIds);$active=$Proc.ActiveProcesses
 # Existing supervisor retains verified handles, so PID reuse cannot stand in
 # for termination of an observed member. Working set is distinctly labelled.
 $working=$Proc.WorkingSetBytes;$peak=$Proc.PeakJobCommittedBytes
 foreach($member in $members){
  if(!$Seen.ContainsKey([string]$member)){
   $row=[ordered]@{pid=$member;observed_utc=[DateTime]::UtcNow.ToString('o');name=$null;image_path=$null;started_utc=$null;inspection_error=$null}
   try{$p=[Diagnostics.Process]::GetProcessById([int]$member);try{$row.name=$p.ProcessName;$row.started_utc=$p.StartTime.ToUniversalTime().ToString('o');$row.image_path=$p.MainModule.FileName}finally{$p.Dispose()}}catch{$row.inspection_error=$_.Exception.Message}
   $Seen[[string]$member]=$row
  }
 }
 return [pscustomobject]@{active_processes=$active;member_ids=$members;sampled_working_set_bytes=$working;kernel_peak_committed_bytes=$peak}
}

$failure=$null
try{
 foreach($case in $protocol.cases){
  $directory=Join-Path $root $case.name;New-Item -ItemType Directory -Path $directory|Out-Null
  $proc=$null;$stdout=$null;$stderr=$null;$file=$null;$mapping=$null;$view=$null
  $timer=[Diagnostics.Stopwatch]::StartNew();$seen=@{};$samples=[Collections.Generic.List[object]]::new()
  $record=[ordered]@{case=$case.name;passed=$false;memory_limit_bytes=$case.memory_bytes;active_process_limit=$case.processes;failure=$null;members=@();samples=@();parent=$null;child=$null;observed_peak_job_committed_bytes=0;job_empty_after_kill=$false;root_alive_before_kill=$false;kill_seconds=$null}
  try{
   $file=[IO.FileStream]::new((Join-Path $directory 'status.bin'),[IO.FileMode]::CreateNew,[IO.FileAccess]::ReadWrite,[IO.FileShare]::ReadWrite);$file.SetLength(4096)
   $mapping=[IO.MemoryMappedFiles.MemoryMappedFile]::CreateFromFile($file,[NullString]::Value,4096,[IO.MemoryMappedFiles.MemoryMappedFileAccess]::ReadWrite,[IO.HandleInheritability]::None,$true)
   $view=$mapping.CreateViewAccessor(0,4096,[IO.MemoryMappedFiles.MemoryMappedFileAccess]::ReadWrite)
   $proc=[GreenLensPairedProcess.NativeProcess]::StartBounded($exe,@($case.mode),$directory,'.',[long]$case.memory_bytes,[uint32]$case.processes)
   $proc.Input.Close();$stdout=[IO.MemoryStream]::new();$stderr=[IO.MemoryStream]::new()
   $outTask=$proc.Output.CopyToAsync($stdout);$errTask=$proc.Error.CopyToAsync($stderr)
   $settled=$null
   while($true){
    $parent=Read-Slot $view 0;$child=Read-Slot $view 128;$observation=Observe-Members $proc $seen
    $samples.Add([ordered]@{elapsed_ms=$timer.ElapsedMilliseconds;active_processes=$observation.active_processes;member_ids=$observation.member_ids;kernel_peak_committed_bytes=$observation.kernel_peak_committed_bytes;sampled_working_set_bytes=$observation.sampled_working_set_bytes;parent_state=$parent.state;child_state=$child.state})
    $record.observed_peak_job_committed_bytes=[Math]::Max([long]$record.observed_peak_job_committed_bytes,$observation.kernel_peak_committed_bytes);$record.parent=$parent;$record.child=$child
    if($observation.kernel_peak_committed_bytes -gt $case.memory_bytes){throw "Kernel aggregate peak exceeded the fixed $($case.memory_bytes)-byte bound; no tolerance is permitted."}
    if($observation.active_processes -gt $case.processes -or $observation.member_ids.Count -gt $case.processes){throw 'Active-process bound exceeded; actual member names are retained.'}
    if($parent.state -eq 4 -or $child.state -eq 4){throw 'Native fixture reported a setup/commit failure; fixed slots retain its Win32 error.'}
    if($timer.Elapsed.TotalSeconds -gt 20 -or $proc.HasExited){throw 'Fixture did not remain alive through the declared observation window.'}
    $ready=$parent.stable -and $parent.state -eq 3 -and ($case.mode -ne 'aggregate' -or ($child.stable -and $child.state -eq 3))
    if($ready){if(!$settled){$settled=[Diagnostics.Stopwatch]::StartNew()}elseif($settled.ElapsedMilliseconds -ge 250){break}}
    if($samples.Count -ge 1100){throw 'Fixed observation-sample capacity exceeded.'}
    Start-Sleep -Milliseconds 20
   }
   if($parent.magic -ne 0x474c4244 -or $parent.version -ne 1 -or $parent.pid -ne $proc.Id -or $parent.role -ne 0 -or $parent.failure -ne 0){throw 'Parent slot does not bind the launched fixture identity.'}
   if($observation.member_ids -notcontains $parent.pid -or $record.observed_peak_job_committed_bytes -le 0){throw 'Required live root/kernel memory observation is absent.'}
   if(@($seen.Values|Where-Object {!$_.name -or !$_.image_path -or $_.inspection_error}).Count){throw 'A live member name/image identity could not be inspected; retained membership must not be inferred.'}
   if($case.mode -eq 'single'){
    if($parent.mode -ne 1 -or $parent.allocation_denied -ne 1 -or $parent.allocation_error -eq 0 -or $parent.attempted_bytes -ne 1MB -or $parent.committed_bytes -lt 256MB -or $parent.committed_bytes -ge $case.memory_bytes){throw 'Near-limit fixture did not establish bounded first-denial behavior.'}
   }elseif($case.mode -eq 'aggregate'){
    if($parent.mode -ne 2 -or $parent.committed_bytes -ne 128MB -or $parent.child_created -ne 1 -or $parent.child_pid -ne $child.pid -or $parent.pid -eq $child.pid -or $child.magic -ne 0x474c4244 -or $child.version -ne 1 -or $child.role -ne 1 -or $child.mode -ne 3 -or $child.failure -ne 0 -or $observation.member_ids -notcontains $child.pid){throw 'Aggregate fixture did not establish its exact simultaneously alive parent/descendant.'}
    $expectedChild=if($case.expect_denied){128MB}else{288MB}
    if($child.committed_bytes -ne $expectedChild -or $child.attempted_bytes -ne 160MB -or [bool]$child.allocation_denied -ne $case.expect_denied -or ($case.expect_denied -and $child.allocation_error -eq 0)){throw 'Aggregate denial/control differs from the declared identical allocation recipe.'}
   }else{
    if($parent.mode -ne 4 -or $parent.child_created -ne 0 -or $parent.child_create_error -eq 0 -or $parent.child_pid -ne 0 -or $child.state -ne 0){throw 'Process limit did not reject the descendant before its fixture entry.'}
   }
   $record.root_alive_before_kill=!$proc.HasExited;$record.alive_member_ids_before_kill=@($proc.MemberIds)
   $proc.Kill();$stop=[Diagnostics.Stopwatch]::StartNew()
   while(!$proc.JobEmpty){if($stop.Elapsed.TotalSeconds -gt 5){throw 'Held process tree did not become empty within five seconds.'};Start-Sleep -Milliseconds 20}
   $record.kill_seconds=$stop.Elapsed.TotalSeconds;$record.job_empty_after_kill=$proc.JobEmpty;$record.kernel_peak_after_kill_bytes=$proc.PeakJobCommittedBytes
   if($record.kernel_peak_after_kill_bytes -gt $case.memory_bytes){throw 'Post-kill aggregate peak exceeds the fixed bound.'}
   foreach($task in @($outTask,$errTask)){if(!$task.Wait(5000)){throw 'Owned fixture pipe did not reach EOF after job termination.'}}
   if($stdout.Length -ne 0 -or $stderr.Length -ne 0){throw 'Fixture unexpectedly used stdout/stderr; telemetry must be fixed mapped fields.'}
   $record.passed=$true
  }catch{$record.failure=$_.Exception.Message;throw}
  finally{
   if($proc){try{if(!$proc.JobEmpty){$proc.Kill();$stop=[Diagnostics.Stopwatch]::StartNew();while(!$proc.JobEmpty -and $stop.Elapsed.TotalSeconds -le 5){Start-Sleep -Milliseconds 20}};$record.cleanup_job_empty=$proc.JobEmpty;$record.final_kernel_peak_bytes=$proc.PeakJobCommittedBytes;if(!$record.cleanup_job_empty -or $record.final_kernel_peak_bytes -gt $case.memory_bytes){$record.passed=$false;$record.cleanup_error='Cleanup did not establish an empty job and an in-bound kernel peak.'}}catch{$record.passed=$false;$record.cleanup_error=$_.Exception.Message};$proc.Dispose()}
   if($view){$view.Flush();$view.Dispose()};if($mapping){$mapping.Dispose()};if($file){$file.Dispose()}
   if($stdout){[IO.File]::WriteAllBytes((Join-Path $directory 'stdout.bin'),$stdout.ToArray());$stdout.Dispose()};if($stderr){[IO.File]::WriteAllBytes((Join-Path $directory 'stderr.bin'),$stderr.ToArray());$stderr.Dispose()}
   $record.members=@($seen.Values|Sort-Object pid);$record.samples=$samples.ToArray();$record.elapsed_seconds=$timer.Elapsed.TotalSeconds
   $record|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $directory 'observation.json') -Encoding utf8NoBOM
   $results.Add([pscustomobject]$record)
  }
 }
 if((Get-FileHash -LiteralPath $native).Hash.ToLowerInvariant() -ne $nativeHash -or (Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant() -ne $fixtureHash -or (Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant() -ne $protocol.fixture_source_sha256 -or (Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant() -ne $protocol.test_sha256){throw 'Supervisor, fixture or test source changed during the checks.'}
}catch{$failure=$_.Exception.Message}
$summary=[ordered]@{schema_version=$protocol.schema_version;passed=(!$failure -and $results.Count -eq 4 -and !($results.ToArray()|Where-Object {!$_.passed}));failure=$failure;checks=$results.ToArray();native_process_sha256=$nativeHash;fixture_executable_sha256=$fixtureHash;protocol_sha256=(Get-FileHash -LiteralPath (Join-Path $root 'protocol.json')).Hash.ToLowerInvariant();completed_utc=[DateTime]::UtcNow.ToString('o')}
$summary|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $root 'summary.json') -Encoding utf8NoBOM
if(!$summary.passed){throw "Native containment regression failed; retained observations: $root; $failure"}
Write-Host "Four declared native memory/process-containment checks passed: $root"
