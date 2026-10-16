#Requires -Version 7.2
# Original GreenLens diagnostic, Apache-2.0. One declared experiment, no retry.
# This does not repair/reclassify the failed 384 MiB containment regressions.
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$FixtureExecutable,
 [Parameter(Mandatory)][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$ExpectedFixtureSha256,
 [string]$OutputDirectory=''
)
$ErrorActionPreference='Stop'
$cache=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
if(!$OutputDirectory){$OutputDirectory=Join-Path $cache ('verification/accounting discriminator '+[Guid]::NewGuid().ToString('N')+' 空間')}
$root=[IO.Path]::GetFullPath($OutputDirectory)
if(!$root.StartsWith($cache+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Diagnostic receipts must be external under LOCALAPPDATA/GreenLens.'}
if(Test-Path -LiteralPath $root){throw 'Use a new diagnostic directory; retain previous failures unchanged.'}
$exe=(Resolve-Path -LiteralPath $FixtureExecutable).Path
$native=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'annual-paired-native-process.cs')).Path
$source=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'fixtures/native-committed-budget.cpp')).Path
$hashes=[ordered]@{fixture=(Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant();native=(Get-FileHash -LiteralPath $native).Hash.ToLowerInvariant();source=(Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant();diagnostic=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()}
if($hashes.fixture -ne $ExpectedFixtureSha256.ToLowerInvariant()){throw 'Fixture executable differs from the explicitly declared build.'}
Add-Type -LiteralPath $native
New-Item -ItemType Directory -Path $root|Out-Null
$protocol=[ordered]@{
 schema_version='greenlens.native-committed-accounting-diagnostic.v1';declared_utc=[DateTime]::UtcNow.ToString('o');identities=$hashes
 purpose='Distinguish a rejected commitment request and retained private commit from the raw kernel PeakJobMemoryUsed; not a containment qualification or a converter/report execution'
 prior_outcomes='Both CLR and original native near-limit fixtures exceeded the strict384 MiB raw-peak criterion. They remain failed. This is a newly instrumented discriminator, not an unchanged rerun.'
 hypotheses=@('A rejected request may be reflected in raw peak accounting despite leaving its pages reserved and retaining no extra process private commit.','A limit/ABI mismatch, accepted allocation, or concurrent member commitment could instead explain the discrepancy; all must be observed separately.')
 fixed_recipe=[ordered]@{mode='accounting-discriminator';kernel_limit_bytes=384MB;active_process_limit=4;initial_successful_commit_bytes=256MB;single_additional_request_bytes=160MB;reserved_range_bytes=512MB;deadline_seconds=20;kill_deadline_seconds=5;poll_milliseconds=20;stable_stage_milliseconds=250;maximum_samples=1100;attempts=1}
 expected_observations='Held-job and native query-back limits agree with384 MiB/cap4. Initial256 MiB succeeds. Exactly one160 MiB request fails; the requested range remains MEM_RESERVE. Record native own current/peak private commit before/after, each live member private commit and raw job peak. No read or write to the denied range.'
 falsification_and_stop='Stop on ABI/limit mismatch, missing identity, native failure, early exit, process overflow, accepted oversized request, non-reserved rejected range, or deadline. Raw peak above384 MiB remains a failed peak criterion; do not subtract a request, add tolerance, retry or infer hidden transient commitment from samples. A nonincreasing retained commit plus peak rise is only evidence consistent with attempted/rolled-back accounting.'
 references=@('https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_limit_information','https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information','https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex','https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualquery')
}
$protocol|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $root 'protocol.json') -Encoding utf8NoBOM

function Read-Probe($View){
 $stage=$View.ReadInt32(268)
 $snapshots=for($i=0;$i -lt 3;$i++){
  $o=304+56*$i
  [pscustomobject]@{flags=$View.ReadUInt32($o);active_limit=$View.ReadUInt32($o+4);job_limit_bytes=$View.ReadUInt64($o+8);raw_peak_job_bytes=$View.ReadUInt64($o+16);own_private_bytes=$View.ReadUInt64($o+24);own_peak_private_bytes=$View.ReadUInt64($o+32);queried_region_bytes=$View.ReadUInt64($o+40);region_state=$View.ReadUInt32($o+48);query_error=$View.ReadUInt32($o+52)}
 }
 [pscustomobject]@{magic=$View.ReadUInt32(256);version=$View.ReadUInt32(260);command=$View.ReadInt32(264);stage=$stage;region_base=$View.ReadUInt64(272);initial_bytes=$View.ReadUInt64(280);attempt_bytes=$View.ReadUInt64(288);attempt_address=$View.ReadUInt64(296);snapshots=@($snapshots);attempt_succeeded=$View.ReadUInt32(472);attempt_error=$View.ReadUInt32(476);failure=$View.ReadUInt32(480);stable=($stage -eq $View.ReadInt32(268))}
}
function Read-HeldObservation($Proc,$Seen){
 $limits=@($Proc.CommittedMemoryLimitSnapshot)
 if($limits.Count -ne 4 -or ($limits[0] -band 0x2208) -ne 0x2208 -or $limits[1] -ne 4 -or $limits[2] -ne 384MB){throw 'Held job query-back flags, process limit or memory limit differs from the fixed protocol.'}
 $members=@($Proc.MemberIds);$active=$Proc.ActiveProcesses
 $working=$Proc.WorkingSetBytes # Retains verified member handles for later PID-safe empty check.
 $current=[Collections.Generic.List[object]]::new();[long]$sum=0
 foreach($member in $members){
  $p=[Diagnostics.Process]::GetProcessById([int]$member)
  try{
   $p.Refresh();$row=[ordered]@{pid=$member;name=$p.ProcessName;image_path=$p.MainModule.FileName;started_utc=$p.StartTime.ToUniversalTime().ToString('o');private_bytes=[long]$p.PrivateMemorySize64}
   if(!$Seen.ContainsKey([string]$member)){$Seen[[string]$member]=$row}
   $sum+=$row.private_bytes;$current.Add([pscustomobject]$row)
  }finally{$p.Dispose()}
 }
 [pscustomobject]@{flags=$limits[0];process_limit=$limits[1];job_limit_bytes=$limits[2];raw_peak_job_bytes=$limits[3];active_processes=$active;member_ids=$members;members=$current.ToArray();sampled_sum_private_bytes=$sum;sampled_working_set_bytes=$working}
}

$proc=$null;$file=$null;$mapping=$null;$view=$null;$stdout=$null;$stderr=$null;$outTask=$null;$errTask=$null
$seen=@{};$samples=[Collections.Generic.List[object]]::new();$stages=[ordered]@{};$timer=[Diagnostics.Stopwatch]::StartNew();$settledStage=0;$settledAt=0L
$record=[ordered]@{schema_version=$protocol.schema_version;diagnostic_completed=$false;containment_qualified=$false;strict_raw_peak_criterion_passed=$null;failure=$null;identities=$hashes;stages=$stages;samples=@();members=@();raw_peak_max_bytes=0;cleanup_job_empty=$false}
try{
 $file=[IO.FileStream]::new((Join-Path $root 'status.bin'),[IO.FileMode]::CreateNew,[IO.FileAccess]::ReadWrite,[IO.FileShare]::ReadWrite);$file.SetLength(4096)
 $mapping=[IO.MemoryMappedFiles.MemoryMappedFile]::CreateFromFile($file,[NullString]::Value,4096,[IO.MemoryMappedFiles.MemoryMappedFileAccess]::ReadWrite,[IO.HandleInheritability]::None,$true)
 $view=$mapping.CreateViewAccessor(0,4096,[IO.MemoryMappedFiles.MemoryMappedFileAccess]::ReadWrite)
 $proc=[GreenLensPairedProcess.NativeProcess]::StartBounded($exe,@('accounting-discriminator'),$root,'.',384MB,4)
 $proc.Input.Close();$stdout=[IO.MemoryStream]::new();$stderr=[IO.MemoryStream]::new();$outTask=$proc.Output.CopyToAsync($stdout);$errTask=$proc.Error.CopyToAsync($stderr)
 while($true){
  $probe=Read-Probe $view;$held=Read-HeldObservation $proc $seen;$state=$view.ReadInt32(20)
  $record.raw_peak_max_bytes=[Math]::Max([long]$record.raw_peak_max_bytes,[long]$held.raw_peak_job_bytes)
  $samples.Add([ordered]@{elapsed_ms=$timer.ElapsedMilliseconds;native_stage=$probe.stage;native_state=$state;held=$held})
  if($timer.Elapsed.TotalSeconds -gt 20 -or $samples.Count -gt 1100){throw 'Fixed diagnostic time/sample capacity exceeded.'}
  if($proc.HasExited -or $state -eq 4 -or $held.active_processes -gt 4 -or $held.member_ids.Count -gt 4){throw 'Fixture exited, failed, or exceeded its process bound.'}
  if($probe.stage -gt 0 -and $probe.stable){
   $record.probe=$probe
   if($probe.magic -ne 0x474c4244 -or $probe.version -ne 1 -or $view.ReadUInt32(8) -ne $proc.Id -or $view.ReadUInt32(16) -ne 6 -or $held.member_ids -notcontains $proc.Id -or $probe.initial_bytes -ne 256MB -or $probe.attempt_bytes -ne 160MB -or $probe.attempt_address -ne ($probe.region_base+256MB)){throw 'Native probe identity or fixed recipe mismatch.'}
   if($probe.stage -notin @(1,2,3)){throw 'Unexpected native diagnostic stage.'}
   $snap=$probe.snapshots[$probe.stage-1]
   if($snap.query_error -ne 0 -or ($snap.flags -band 0x2208) -ne 0x2208 -or $snap.active_limit -ne 4 -or $snap.job_limit_bytes -ne 384MB){throw 'Native nearest-job accounting does not match the held job limits.'}
   if($probe.stage -eq 3 -and ($probe.attempt_succeeded -ne 0 -or $snap.region_state -ne 0x2000)){throw 'Oversized request was accepted or the rejected range is not reserved; terminate the diagnostic immediately.'}
   if($probe.stage -ne $settledStage){$settledStage=$probe.stage;$settledAt=$timer.ElapsedMilliseconds}
   if(($timer.ElapsedMilliseconds-$settledAt) -ge 250 -and !$stages.Contains([string]$probe.stage)){
    $stages[[string]$probe.stage]=[ordered]@{elapsed_ms=$timer.ElapsedMilliseconds;native=$snap;held=$held}
    if($snap.region_state -ne 0x2000 -or $snap.queried_region_bytes -lt 160MB){throw 'The separately inspected request region is not fully reserved.'}
    if($probe.stage -eq 1){$view.Write(264,[int]1)}
    elseif($probe.stage -eq 2){if($view.ReadUInt64(32) -ne 256MB){throw 'Initial successful commitment is not exactly256 MiB.'};$view.Write(264,[int]2)}
    else{
     if($probe.attempt_succeeded -ne 0 -or $probe.attempt_error -eq 0 -or $state -ne 3 -or $view.ReadUInt64(32) -ne 256MB -or $view.ReadUInt32(48) -ne 1){throw 'Single oversized request did not fail without increasing accepted commitment.'}
     $record.probe=$probe
     break
    }
   }
  }
  Start-Sleep -Milliseconds 20
 }
 $before=$probe.snapshots[1];$after=$probe.snapshots[2]
 $record.retained_native_private_delta_bytes=[long]$after.own_private_bytes-[long]$before.own_private_bytes
 $record.raw_peak_delta_bytes=[long]$after.raw_peak_job_bytes-[long]$before.raw_peak_job_bytes
 $beforeRoot=@($stages['2'].held.members|Where-Object {$_.pid -eq $proc.Id})
 $afterRoot=@($stages['3'].held.members|Where-Object {$_.pid -eq $proc.Id})
 if($beforeRoot.Count -ne 1 -or $afterRoot.Count -ne 1){throw 'Staged member observations do not bind exactly one native root.'}
 $beforeOther=[long]$stages['2'].held.sampled_sum_private_bytes-[long]$beforeRoot[0].private_bytes
 $afterOther=[long]$stages['3'].held.sampled_sum_private_bytes-[long]$afterRoot[0].private_bytes
 $record.sampled_other_member_private_delta_bytes=$afterOther-$beforeOther
 $record.interpretation=if($record.raw_peak_delta_bytes -gt 0 -and $record.retained_native_private_delta_bytes -le 0){'Rejected region remained reserved and native retained private commit did not increase while raw peak rose. Consistent with attempted/rolled-back accounting; not proof that no transient charge occurred. Inspect other-member delta separately.'}else{'Instrumented outcome does not isolate attempted/rolled-back peak accounting. Do not repeat or infer a repair; retain exact measurements for a technical decision.'}
 $record.root_alive_before_kill=!$proc.HasExited;$record.alive_member_ids_before_kill=@($proc.MemberIds)
 $proc.Kill();$kill=[Diagnostics.Stopwatch]::StartNew()
 while(!$proc.JobEmpty){if($kill.Elapsed.TotalSeconds -gt 5){throw 'Held job did not become empty within five seconds.'};Start-Sleep -Milliseconds 20}
 $record.kill_seconds=$kill.Elapsed.TotalSeconds
 foreach($task in @($outTask,$errTask)){if(!$task.Wait(5000)){throw 'Fixture output pipes did not reach EOF after job termination.'}}
 if($stdout.Length -ne 0 -or $stderr.Length -ne 0){throw 'Native fixture unexpectedly wrote output under diagnostic pressure.'}
 foreach($binding in @(@($exe,'fixture'),@($native,'native'),@($source,'source'),@($PSCommandPath,'diagnostic'))){if((Get-FileHash -LiteralPath $binding[0]).Hash.ToLowerInvariant() -ne $hashes[$binding[1]]){throw 'Pinned diagnostic inputs changed during execution.'}}
 $record.diagnostic_completed=$true
}catch{$record.failure=$_.Exception.Message}
finally{
 if($proc){try{if(!$proc.JobEmpty){$proc.Kill();$kill=[Diagnostics.Stopwatch]::StartNew();while(!$proc.JobEmpty -and $kill.Elapsed.TotalSeconds -le 5){Start-Sleep -Milliseconds 20}};$record.cleanup_job_empty=$proc.JobEmpty;$record.final_raw_peak_bytes=$proc.PeakJobCommittedBytes;$record.raw_peak_max_bytes=[Math]::Max([long]$record.raw_peak_max_bytes,[long]$record.final_raw_peak_bytes);if(!$record.cleanup_job_empty){$record.diagnostic_completed=$false;$record.cleanup_error='Owned job remained nonempty.'}}catch{$record.diagnostic_completed=$false;$record.cleanup_error=$_.Exception.Message};$proc.Dispose()}
 if($view){$view.Flush();$view.Dispose()};if($mapping){$mapping.Dispose()};if($file){$file.Dispose()}
 if($stdout){[IO.File]::WriteAllBytes((Join-Path $root 'stdout.bin'),$stdout.ToArray());$stdout.Dispose()};if($stderr){[IO.File]::WriteAllBytes((Join-Path $root 'stderr.bin'),$stderr.ToArray());$stderr.Dispose()}
 $record.strict_raw_peak_criterion_passed=($record.raw_peak_max_bytes -gt 0 -and $record.raw_peak_max_bytes -le 384MB)
 $record.members=@($seen.Values|Sort-Object pid);$record.samples=$samples.ToArray();$record.elapsed_seconds=$timer.Elapsed.TotalSeconds;$record.completed_utc=[DateTime]::UtcNow.ToString('o');$record.protocol_sha256=(Get-FileHash -LiteralPath (Join-Path $root 'protocol.json')).Hash.ToLowerInvariant()
 $record|ConvertTo-Json -Depth 14|Set-Content -LiteralPath (Join-Path $root 'observation.json') -Encoding utf8NoBOM
}
if(!$record.diagnostic_completed){throw "Accounting discriminator failed; preserve receipt: $root; $($record.failure)"}
Write-Host "Accounting diagnostic completed, containment remains unqualified. Raw384 MiB peak criterion: $($record.strict_raw_peak_criterion_passed). Receipt: $root"
