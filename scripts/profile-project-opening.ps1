#Requires -Version 7.2
<#
Read-only opening profile; no builds, engines, cache eviction or UI automation.
Exactly three fresh CLI processes are planned against one new, hash-checked copy.
Raw paths, project/run IDs and progress are private: output is restricted to local
GreenLens storage and must not be copied wholesale into public evidence.

Example (use an already built, identified executable):
  ./scripts/profile-project-opening.ps1 -Executable $cli -Project $project `
    -OutputDirectory (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/open-profile-01')
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateRange(1,64)][int]$ExpectedEnergyRuns = 4
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (!$IsWindows -or !$env:LOCALAPPDATA) { throw 'This collector requires native Windows and LOCALAPPDATA.' }

$limits = [ordered]@{
    planned_open_processes=3; first_progress_ms=250; first_copy_open_ms=30000; warm_open_ms=20000
    hard_process_ms=120000; termination_join_ms=5000; sample_interval_ms=20
    declared_cooperative_cancel_ms=1000; adapter_metadata_limit_bytes=512MB
    collector_process_working_set_guard_bytes=2GB; max_stream_bytes=16MB
    max_progress_line_chars=1MB; max_progress_events=4096
    max_project_files=5000; max_project_directories=5000; max_project_file_bytes=64MB; max_project_bytes=256MB
    copy_deadline_ms=120000; max_collector_output_bytes=512MB
}
function Resolve-PlainPath([string]$Value) {
    $full = [IO.Path]::GetFullPath($Value)
    $cursor = $full
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $entry = Get-Item -LiteralPath $cursor -Force
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse-point path is unsupported: $cursor" }
        }
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
    if ($full -eq [IO.Path]::GetPathRoot($full)) { return $full }
    return $full.TrimEnd([IO.Path]::DirectorySeparatorChar)
}
function Is-InDirectory([string]$Path,[string]$Directory) {
    $prefix=$Directory.TrimEnd([IO.Path]::DirectorySeparatorChar)+[IO.Path]::DirectorySeparatorChar
    return $Path.Equals($Directory,[StringComparison]::OrdinalIgnoreCase) -or $Path.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)
}
function Write-PrivateJson([string]$Name,$Value) {
    $Value | ConvertTo-Json -Depth 60 | Set-Content -LiteralPath (Join-Path $outputRoot $Name) -Encoding utf8NoBOM
}
function Get-BoundedInventory([string]$Root) {
    $clock=[Diagnostics.Stopwatch]::StartNew();$queue=[Collections.Generic.Queue[string]]::new();$queue.Enqueue($Root)
    $files=[Collections.Generic.List[object]]::new();$total=0L;$directories=1
    while($queue.Count) {
        if($clock.ElapsedMilliseconds -gt $limits.copy_deadline_ms){throw 'Project inventory exceeded its fixed 120-second budget.'}
        foreach($entry in [IO.DirectoryInfo]::new($queue.Dequeue()).EnumerateFileSystemInfos()) {
            if($clock.ElapsedMilliseconds -gt $limits.copy_deadline_ms){throw 'Project inventory exceeded its fixed 120-second budget.'}
            if($entry.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Project contains an unsupported reparse point: $($entry.FullName)"}
            if($entry.Attributes -band [IO.FileAttributes]::Directory){$directories++;if($directories -gt $limits.max_project_directories){throw 'Project exceeds the collector directory bound.'};$queue.Enqueue($entry.FullName);continue}
            if($files.Count -ge $limits.max_project_files -or $entry.Length -gt $limits.max_project_file_bytes){throw 'Project exceeds the declared collector file/count bound.'}
            $total+=$entry.Length;if($total -gt $limits.max_project_bytes){throw 'Project exceeds the declared 256 MiB collector input bound.'}
            $relative=[IO.Path]::GetRelativePath($Root,$entry.FullName)
            $files.Add([ordered]@{relative_path=$relative.Replace('\','/');bytes=$entry.Length;sha256=(Get-FileHash -LiteralPath $entry.FullName -Algorithm SHA256).Hash.ToLowerInvariant()})
        }
    }
    return @($files | Sort-Object { $_.relative_path })
}
function Receive-Progress([string]$Line,[double]$ArrivalMs,$Events) {
    if(!$Line.Trim()){return}
    if($Line.Length -gt $limits.max_progress_line_chars){throw 'Progress line exceeded its declared bound.'}
    if($Events.Count -ge $limits.max_progress_events){throw 'Progress count exceeded its declared bound.'}
    try{$value=ConvertFrom-Json -InputObject $Line -AsHashtable -Depth 40}catch{throw 'stderr contained a non-JSON progress line; raw stderr is retained.'}
    if($value.schema_version -cne 'greenlens.project-open-progress.v1' -or $value.stage -isnot [string] -or $value.numeric_passes -isnot [Collections.IDictionary]){throw 'Unexpected progress contract; raw stderr is retained.'}
    foreach($field in @('runs_completed','runs_total','bytes_read','files_verified','elapsed_ms')) {
        if(!$value.Contains($field) -or $value[$field] -isnot [ValueType] -or ![double]::IsFinite([double]$value[$field]) -or [double]$value[$field] -lt 0 -or [Math]::Floor([double]$value[$field]) -ne [double]$value[$field]){throw "Invalid progress counter: $field"}
    }
    $Events.Add([ordered]@{collector_arrival_ms=$ArrivalMs;event=$value})
}
function Invoke-Opening([int]$Index,[string]$CopiedProject) {
    $label='open-{0:D2}' -f $Index
    $stdoutPath=Join-Path $outputRoot "$label.stdout.json";$stderrPath=Join-Path $outputRoot "$label.stderr.jsonl"
    $info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=$cliPath;$info.WorkingDirectory=$outputRoot
    $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
    $info.ArgumentList.Add('open');$info.ArgumentList.Add($CopiedProject)
    $process=[Diagnostics.Process]::new();$process.StartInfo=$info
    $stdout=[IO.File]::Create($stdoutPath);$stderr=[IO.File]::Create($stderrPath)
    $outBuffer=[byte[]]::new(65536);$errBuffer=[byte[]]::new(65536);$chars=[char[]]::new(65536)
    $decoder=[Text.UTF8Encoding]::new($false,$true).GetDecoder();$pendingLine=''
    $events=[Collections.Generic.List[object]]::new();$reasons=[Collections.Generic.List[string]]::new()
    $timer=[Diagnostics.Stopwatch]::StartNew();$startedUtc=[DateTime]::UtcNow.ToString('o')
    $peak=0L;$sampledPeak=0L;$samples=0;$started=$false;$terminated=$false;$exitCode=$null;$receipt=$null;$receiptElapsed=$null
    $outTask=$null;$errTask=$null;$outDone=$false;$errDone=$false;$processExitedAt=$null
    try {
        if(!$process.Start()){throw 'Opening process could not start.'};$started=$true
        $outTask=$process.StandardOutput.BaseStream.ReadAsync($outBuffer,0,$outBuffer.Length)
        $errTask=$process.StandardError.BaseStream.ReadAsync($errBuffer,0,$errBuffer.Length)
        while(!$process.HasExited -or !$outDone -or !$errDone) {
            if($timer.ElapsedMilliseconds -ge $limits.hard_process_ms){throw 'Hard 120-second process budget exceeded.'}
            if(!$process.HasExited) {
                $process.Refresh();$samples++;$sampledPeak=[Math]::Max($sampledPeak,$process.WorkingSet64);$peak=[Math]::Max($peak,$process.PeakWorkingSet64)
                if($process.WorkingSet64 -gt $limits.collector_process_working_set_guard_bytes){throw 'Collector 2 GiB process working-set guard exceeded.'}
            } elseif($null -eq $processExitedAt){$processExitedAt=$timer.ElapsedMilliseconds}
            if($null -ne $processExitedAt -and $timer.ElapsedMilliseconds-$processExitedAt -gt $limits.termination_join_ms){throw 'Output streams did not close within the five-second drain budget.'}
            if(!$outDone -and $outTask.IsCompleted) {
                $n=$outTask.GetAwaiter().GetResult()
                if($n -eq 0){$outDone=$true}else{
                    if($stdout.Length+$n -gt $limits.max_stream_bytes){throw 'stdout exceeded its 16 MiB bound.'}
                    $stdout.Write($outBuffer,0,$n);$outTask=$process.StandardOutput.BaseStream.ReadAsync($outBuffer,0,$outBuffer.Length)
                }
            }
            if(!$errDone -and $errTask.IsCompleted) {
                $n=$errTask.GetAwaiter().GetResult()
                if($n -eq 0){$errDone=$true}else{
                    if($stderr.Length+$n -gt $limits.max_stream_bytes){throw 'stderr exceeded its 16 MiB bound.'}
                    $stderr.Write($errBuffer,0,$n)
                    $count=$decoder.GetChars($errBuffer,0,$n,$chars,0,$false);$pendingLine += [string]::new($chars,0,$count)
                    while(($newline=$pendingLine.IndexOf("`n")) -ge 0){$line=$pendingLine.Substring(0,$newline).TrimEnd("`r");$pendingLine=$pendingLine.Substring($newline+1);Receive-Progress $line $timer.Elapsed.TotalMilliseconds $events}
                    if($pendingLine.Length -gt $limits.max_progress_line_chars){throw 'Unterminated progress line exceeded its bound.'}
                    $errTask=$process.StandardError.BaseStream.ReadAsync($errBuffer,0,$errBuffer.Length)
                }
            }
            if(!$process.HasExited -or !$outDone -or !$errDone){Start-Sleep -Milliseconds $limits.sample_interval_ms}
        }
        $count=$decoder.GetChars([byte[]]::new(0),0,0,$chars,0,$true)
        $pendingLine += [string]::new($chars,0,$count)
        if($pendingLine.Trim()){Receive-Progress $pendingLine $timer.Elapsed.TotalMilliseconds $events}
        $process.WaitForExit();$exitCode=$process.ExitCode
        if($exitCode -ne 0){$reasons.Add("Native exit $exitCode; no completed-open acceptance.")}
    } catch {
        $reasons.Add($_.Exception.Message)
        if($started -and !$process.HasExited){
            $terminated=$true
            try{$process.Kill($true);if(!$process.WaitForExit($limits.termination_join_ms)){$reasons.Add('Process tree termination did not join within five seconds.')}}catch{$reasons.Add('Process tree termination failed: '+$_.Exception.Message)}
        }
        if($started -and $process.HasExited){$exitCode=$process.ExitCode}
    } finally {
        if($started -and !$process.HasExited){
            $terminated=$true
            try{$process.Kill($true);if(!$process.WaitForExit($limits.termination_join_ms)){$reasons.Add('Final process tree termination did not join within five seconds.')}}catch{$reasons.Add('Final process tree termination failed: '+$_.Exception.Message)}
        }
        $timer.Stop();$stdout.Dispose();$stderr.Dispose();$process.Dispose()
    }
    try {
        $receipt=Get-Content -LiteralPath $stdoutPath -Raw -Encoding utf8 | ConvertFrom-Json -AsHashtable -Depth 50
        if($receipt.schema_version -cne 'greenlens.project-verification.v1' -or $receipt.progress.schema_version -cne 'greenlens.project-open-progress.v1' -or $receipt.progress.stage -cne 'completed'){throw 'Missing recognized completed verification receipt.'}
        if($receipt.content_snapshot_sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Receipt has no valid content-snapshot identity.'}
        if($receipt.progress.numeric_passes -isnot [Collections.IDictionary]){throw 'Receipt numeric-pass counters are missing.'}
        $receiptElapsed=$receipt.progress.elapsed_ms
        if($receipt.manifest_sha256 -cne $manifestHash){throw 'Verification receipt binds a different manifest.'}
        if($receipt.progress.runs_total -ne $expectedTotal -or $receipt.progress.runs_completed -ne $expectedTotal){throw 'Receipt has an unexpected completed/total run count.'}
        if((($receipt.progress.numeric_passes.Keys | Sort-Object) -join ',') -cne (($expectedIds | Sort-Object) -join ',')){throw 'Numeric-pass run identities differ from the admitted energy summaries.'}
        foreach($id in $expectedIds){if($receipt.progress.numeric_passes[$id] -ne 1){throw 'Every expected energy run must have exactly one numeric pass.'}}
        if(!$events.Count -or $events[$events.Count-1].event.stage -cne 'completed'){throw 'No terminal completed progress event was observed.'}
    }catch{$reasons.Add($_.Exception.Message)}
    $firstProgress=if($events.Count){$events[0].collector_arrival_ms}else{$null}
    $target=if($Index -eq 1){$limits.first_copy_open_ms}else{$limits.warm_open_ms}
    $completed=$reasons.Count -eq 0 -and !$terminated -and $exitCode -eq 0
    $record=[ordered]@{
        ordinal=$Index;started_utc=$startedUtc;finished_utc=[DateTime]::UtcNow.ToString('o');arguments=@('open',$CopiedProject)
        context=$(if($Index -eq 1){'Fresh process, newly copied and hash-checked project; copying/hashing can warm OS cache. No cache eviction.'}else{'Fresh process, same project copy; warmed filesystem cache expected. No in-process snapshot reuse across opens.'})
        wall_ms=$timer.Elapsed.TotalMilliseconds;receipt_verification_ms=$receiptElapsed
        first_progress_arrival_ms=$firstProgress;first_progress_is_cli_arrival_not_visible_desktop=$true
        working_set_samples=$samples;sampled_peak_working_set_bytes=$(if($samples){$sampledPeak}else{$null});observed_os_peak_working_set_bytes=$(if($samples){$peak}else{$null})
        working_set_scope='CLI process only; no engine is launched by open. Working set is not verified metadata allocation.'
        exit_code=$exitCode;terminated_tree_by_collector=$terminated;failure_reasons=@($reasons);completed=$completed
        checks=[ordered]@{completed_receipt_and_exact_numeric_passes=$completed;wall_target_ms=$target;wall_target_pass=($completed -and $timer.Elapsed.TotalMilliseconds -le $target);first_progress_target_ms=$limits.first_progress_ms;first_progress_target_pass=($null -ne $firstProgress -and $firstProgress -le $limits.first_progress_ms)}
        stdout=[ordered]@{name=[IO.Path]::GetFileName($stdoutPath);bytes=(Get-Item -LiteralPath $stdoutPath).Length;sha256=(Get-FileHash -LiteralPath $stdoutPath -Algorithm SHA256).Hash.ToLowerInvariant()}
        stderr=[ordered]@{name=[IO.Path]::GetFileName($stderrPath);bytes=(Get-Item -LiteralPath $stderrPath).Length;sha256=(Get-FileHash -LiteralPath $stderrPath -Algorithm SHA256).Hash.ToLowerInvariant()}
        receipt=$receipt;progress_events=@($events)
    }
    Write-PrivateJson "$label.measurement.json" $record
    return $record
}

$cliPath=Resolve-PlainPath $Executable;$sourceRoot=Resolve-PlainPath $Project;$outputRoot=Resolve-PlainPath $OutputDirectory
$localRoot=Resolve-PlainPath (Join-Path $env:LOCALAPPDATA 'GreenLens')
# The authorized checkout may have OneDrive-managed ancestor reparse points.
# It is only an exclusion path here, never a copy target or evidence input.
$checkoutRoot=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if(!(Test-Path -LiteralPath $cliPath -PathType Leaf) -or [IO.Path]::GetExtension($cliPath) -ine '.exe'){throw 'Executable must identify an existing native Windows CLI executable.'}
if(!(Test-Path -LiteralPath $sourceRoot -PathType Container)){throw 'Project must identify an existing portable project directory.'}
if(!(Test-Path -LiteralPath (Join-Path $sourceRoot 'project.json') -PathType Leaf)){throw 'Portable project.json is absent.'}
if(!(Is-InDirectory $outputRoot $localRoot) -or (Is-InDirectory $outputRoot $checkoutRoot) -or (Is-InDirectory $outputRoot $sourceRoot) -or (Is-InDirectory $sourceRoot $outputRoot)){throw 'Output must be a new directory under LOCALAPPDATA/GreenLens, outside source and project, with no overlapping paths.'}
if(Test-Path -LiteralPath $outputRoot){throw 'Output already exists; choose a new directory. Earlier results are never overwritten.'}
$null=[IO.Directory]::CreateDirectory($outputRoot)
$cliHash=(Get-FileHash -LiteralPath $cliPath -Algorithm SHA256).Hash.ToLowerInvariant()
$records=[Collections.Generic.List[object]]::new();$fatal=$null;$copyRoot=Join-Path $outputRoot 'project copy 空間'
$declaration=[ordered]@{
    schema_version='greenlens.opening-profile-declaration.v1';created_utc=[DateTime]::UtcNow.ToString('o');private_raw_record=$true
    executable_path=$cliPath;executable_sha256=$cliHash;source_project_path=$sourceRoot;copied_project_path=$copyRoot
    collector_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant();expected_energy_runs=$ExpectedEnergyRuns;limits=$limits
    process_plan=@('first-copy fresh-process open','warm fresh-process open','warm fresh-process open')
    stop_rule='Stop remaining opens after failed process, invalid receipt, output/resource limit, or changed executable. Fixed warm repetitions continue after soft timing failure; no tuning/retries.'
    cancellation=[ordered]@{status='supported_by_current_cli_not_exercised_here';attempted=$false;reason='The current open command installs a standard Ctrl-C handler. This collector does not inject console-control events into redirected child processes. Collector deadline tree-kill is containment, not cooperative cancellation evidence.';remaining='Observe controlled CLI or actual desktop cooperative cancellation, and separately preserve the previous desktop document/pending evidence; budget 1000 ms.'}
    metadata_budget_measurement='The service enforces 512 MiB metadata; its receipt does not expose allocated metadata bytes. Process working set cannot establish that separate quantity.'
    cache_disclosure='No OS cache eviction. Copy/hash work touches source and destination bytes before the first measured process; first-copy is not a hardware-cold cache claim.'
    filesystem_deadline_disclosure='Inventory/copy/hash deadlines are checked between bounded files and directory entries. An individual synchronous operating-system file operation cannot be preempted by this script; a blocked filesystem is not a measured successful run.'
    environment=[ordered]@{os=[Environment]::OSVersion.VersionString;powershell=$PSVersionTable.PSVersion.ToString();logical_processors=[Environment]::ProcessorCount;cli_file_version=[Diagnostics.FileVersionInfo]::GetVersionInfo($cliPath).FileVersion}
}
Write-PrivateJson 'declaration.json' $declaration
try {
    $before=@(Get-BoundedInventory $sourceRoot);Write-PrivateJson 'source-files.json' $before
    $manifest=Join-Path $sourceRoot 'project.json'
    if(!(Test-Path -LiteralPath $manifest -PathType Leaf)){throw 'Portable project.json is absent.'}
    $projectData=Get-Content -LiteralPath $manifest -Raw -Encoding utf8 | ConvertFrom-Json -AsHashtable -Depth 60
    if(!$projectData.Contains('energy') -or !$projectData.energy -or !$projectData.energy.Contains('runs')){throw 'Expected a project with retained energy runs.'}
    $expectedIds=@($projectData.energy.runs | ForEach-Object {$_.id})
    if($expectedIds.Count -ne $ExpectedEnergyRuns -or @($expectedIds | Sort-Object -Unique).Count -ne $ExpectedEnergyRuns){throw 'Expected retained energy run count/unique identities do not match. No open was launched.'}
    $expectedTotal=$expectedIds.Count+@($projectData.runs).Count
    if($projectData.Contains('climate') -and $projectData.climate){$expectedTotal+=@($projectData.climate.runs).Count}
    $manifestHash=(Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant()
    $copyClock=[Diagnostics.Stopwatch]::StartNew();$null=[IO.Directory]::CreateDirectory($copyRoot)
    foreach($file in $before) {
        if($copyClock.ElapsedMilliseconds -gt $limits.copy_deadline_ms){throw 'Project copy exceeded its fixed 120-second budget.'}
        $from=Join-Path $sourceRoot $file.relative_path;$to=Join-Path $copyRoot $file.relative_path
        $null=[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($to));[IO.File]::Copy($from,$to,$false)
    }
    $copied=@(Get-BoundedInventory $copyRoot)
    if($copyClock.ElapsedMilliseconds -gt $limits.copy_deadline_ms){throw 'Copy and destination hashing exceeded the fixed 120-second budget.'}
    if((ConvertTo-Json -InputObject $before -Depth 5 -Compress) -cne (ConvertTo-Json -InputObject $copied -Depth 5 -Compress)){throw 'Copied project inventory differs from original; no open was launched.'}
    Write-PrivateJson 'copy.json' ([ordered]@{copy_and_hash_ms=$copyClock.Elapsed.TotalMilliseconds;file_count=$copied.Count;bytes=($copied | ForEach-Object { $_.bytes } | Measure-Object -Sum).Sum;exact_source_inventory_match=$true})
    foreach($index in 1..3) {
        if((Get-FileHash -LiteralPath $cliPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $cliHash){throw 'Executable changed during profiling; remaining cases are not run.'}
        $record=Invoke-Opening $index $copyRoot;$records.Add($record)
        if(!$record.completed){throw 'Opening failed; remaining unchanged process attempts are not run.'}
    }
    $after=@(Get-BoundedInventory $copyRoot)
    if((ConvertTo-Json -InputObject $copied -Depth 5 -Compress) -cne (ConvertTo-Json -InputObject $after -Depth 5 -Compress)){throw 'Copied project bytes changed during opening.'}
    if(@($records | ForEach-Object {$_.receipt.content_snapshot_sha256} | Sort-Object -Unique).Count -ne 1){throw 'The three verified content snapshot identities differ.'}
    if((Get-FileHash -LiteralPath $cliPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $cliHash){throw 'Executable changed before final acceptance.'}
} catch {$fatal=$_.Exception.Message}
$retainedBytes=(Get-ChildItem -LiteralPath $outputRoot -File -Recurse | Measure-Object Length -Sum).Sum
if($retainedBytes -gt $limits.max_collector_output_bytes){$fatal='Collector retained output exceeded its declared 512 MiB bound.'}
$summary=[ordered]@{
    schema_version='greenlens.opening-profile.v1';finished_utc=[DateTime]::UtcNow.ToString('o');private_raw_record=$true
    executable_sha256=$cliHash;collector_sha256=$declaration.collector_sha256;limits=$limits
    recorded_open_processes=$records.Count;completed_open_processes=@($records | Where-Object { $_.completed }).Count;planned_open_processes=3;unexecuted_open_processes=3-$records.Count
    fatal_error=$fatal;retained_output_bytes_before_summary=$retainedBytes;measurements=@($records)
    opening_targets_pass=(!$fatal -and $records.Count -eq 3 -and @($records | Where-Object {!$_.checks.wall_target_pass -or !$_.checks.first_progress_target_pass}).Count -eq 0)
    cooperative_cancellation=$declaration.cancellation;complete_opening_acceptance=$false
    limits_of_evidence='CLI progress arrival is not visible desktop progress. Cancellation and metadata-allocation observations are unavailable here. No engine, GPU, installed workflow, physical accuracy, cache eviction or publication is established.'
}
Write-PrivateJson 'summary.json' $summary
Write-Host "Recorded $($records.Count) of three planned opens. Output is private local evidence. Cooperative cancellation is not exercised by this collector."
if($fatal){throw $fatal}
if(!$summary.opening_targets_pass){throw 'One or more declared timing/progress targets failed; measured outcomes are retained without retries.'}
