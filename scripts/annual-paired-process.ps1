# Research-only supervisor. The sealed reference helper and live snapshots stay unchanged.
# All child processes enter a kill-on-close Job Object atomically at CreateProcess.
if(!('GreenLensPairedProcess.NativeProcess' -as [type])){Add-Type -LiteralPath "$PSScriptRoot/annual-paired-native-process.cs"}
function Invoke-Engine([string]$Name, [string[]]$Arguments, [string]$Output, [string]$InputText = '') {
 Invoke-PairedProcess -Executable (Join-Path $EngineRoot "bin/$Name.exe") -Name $Name -Arguments $Arguments -Output $Output -InputText $InputText
}
function Invoke-PairedProcess {
 param([string]$Executable,[string]$Name,[string[]]$Arguments,[string]$Output,[string]$InputText='',[string]$InputFile='',[object]$InputProducer=$null,
  [double]$TimeoutSeconds=7200,[long]$MemoryLimitBytes=2147483648,[long]$ScratchLimitBytes=2147483648,[long]$FileLimitBytes=536870912)
 if(@(@([bool]$InputFile,[bool]$InputText,($null -ne $InputProducer))|Where-Object{$_}).Count -gt 1){throw 'InputFile, InputText and InputProducer are mutually exclusive'}
 if(![double]::IsFinite($TimeoutSeconds) -or $TimeoutSeconds -le 0 -or $MemoryLimitBytes -le 0 -or $ScratchLimitBytes -le 0 -or $FileLimitBytes -le 0){throw 'Positive finite supervision limits required'}
 $process=$null;$runner=[Diagnostics.Process]::GetCurrentProcess();$stream=$null;$inputStream=$null;$errorReader=$null
 $cancellation=[Threading.CancellationTokenSource]::new();$inputTask=$null;$outputTask=$null;$errorTask=$null
 $started=[DateTime]::UtcNow.ToString('o');$timer=[Diagnostics.Stopwatch]::StartNew()
 $peak=0L;$combinedPeak=0L;$scratchPeak=0L;$reason=$null;$lastDiskCheck=-5.;$inputLength=0L;$receipt=$null;$stderr='';$inputClosed=$false;$drainsCompleted=$false
 try{
  $ownedRoot=[IO.Path]::GetFullPath($OutputDirectory).TrimEnd([IO.Path]::DirectorySeparatorChar)+[IO.Path]::DirectorySeparatorChar
  $outputPath=[IO.Path]::GetFullPath($(if([IO.Path]::IsPathRooted($Output)){$Output}else{Join-Path $OutputDirectory $Output}))
  $inputPath=if($InputFile){[IO.Path]::GetFullPath($(if([IO.Path]::IsPathRooted($InputFile)){$InputFile}else{Join-Path $OutputDirectory $InputFile}))}else{$null}
  foreach($ownedPath in @($outputPath,$inputPath)){
   if(!$ownedPath){continue}
   if(!$ownedPath.StartsWith($ownedRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Input and output files must be beneath the owned output directory'}
   if(!([IO.Path]::GetDirectoryName($ownedPath)+[IO.Path]::DirectorySeparatorChar).Equals($ownedRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Research input/output must be immediate files of the owned directory'}
   $ancestor=$ownedPath
   while($ancestor -and $ancestor.Length -ge $ownedRoot.Length-1){
    if(([IO.File]::Exists($ancestor) -or [IO.Directory]::Exists($ancestor)) -and (([IO.File]::GetAttributes($ancestor) -band [IO.FileAttributes]::ReparsePoint) -ne 0)){throw 'Owned input/output paths must not traverse reparse points'}
    $ancestor=[IO.Path]::GetDirectoryName($ancestor)
   }
  }
  if($inputPath -and $inputPath.Equals($outputPath,[StringComparison]::OrdinalIgnoreCase)){throw 'Input and output files must differ'}
  if($InputFile){
   $inputStream=[IO.FileStream]::new($inputPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read,65536,[IO.FileOptions]::Asynchronous -bor [IO.FileOptions]::SequentialScan)
   $inputLength=$inputStream.Length
   if($inputLength -gt $FileLimitBytes){throw 'InputFile exceeds declared single-file bytes'}
  }
  # Owned file-open failures happen before native launch.
  $stream=[IO.File]::Create($outputPath)
  $process=[GreenLensPairedProcess.NativeProcess]::Start($Executable,$Arguments,$OutputDirectory,('.;'+(Join-Path $EngineRoot 'lib')))
  $outputTask=$process.Output.CopyToAsync($stream,65536,$cancellation.Token)
  $errorReader=[IO.StreamReader]::new($process.Error,[Text.UTF8Encoding]::new($false),$true,4096,$true)
  $errorTask=$errorReader.ReadToEndAsync()
  if($InputProducer){$inputTask=$InputProducer.GenerateAsync($process.Input,$cancellation.Token)}
  elseif($inputStream){$inputTask=$inputStream.CopyToAsync($process.Input,65536,$cancellation.Token)}
  else{$inputBytes=[Text.UTF8Encoding]::new($false).GetBytes($InputText);$inputLength=$inputBytes.Length;$inputTask=$process.Input.WriteAsync($inputBytes,0,$inputBytes.Length,$cancellation.Token)}
  # A parent exit alone is not completion: descendants can retain all three pipes.
  # The one deadline includes producer, parent, job descendants and both drains.
  while($true){
   if(!$inputClosed -and $inputTask.IsCompleted){
    try{$receipt=$inputTask.GetAwaiter().GetResult();$process.Input.Dispose();$inputClosed=$true;if($InputProducer){$inputLength=$receipt.Bytes}}
    catch{$reason='engine exited before accepting complete input'}
   }
   if($outputTask.IsFaulted -or $outputTask.IsCanceled){$reason='engine output stream failed'}
   if($errorTask.IsFaulted -or $errorTask.IsCanceled){$reason='engine error stream failed'}
   $jobBytes=$process.WorkingSetBytes;$peak=[Math]::Max($peak,$process.PeakWorkingSetBytes)
   $runner.Refresh();$combined=$jobBytes+$runner.WorkingSet64;$combinedPeak=[Math]::Max($combinedPeak,$combined)
   if($timer.Elapsed.TotalSeconds -gt $TimeoutSeconds){$reason='process and stream deadline exceeded declared seconds'}
   elseif($combined -gt $MemoryLimitBytes){$reason='observed runner-plus-job working set exceeded declared bytes'}
   if($timer.Elapsed.TotalSeconds-$lastDiskCheck -ge 5){
    $lastDiskCheck=$timer.Elapsed.TotalSeconds;$size=0L
    foreach($file in [IO.Directory]::EnumerateFiles($OutputDirectory)){$length=if([IO.Path]::GetFullPath($file).Equals($outputPath,[StringComparison]::OrdinalIgnoreCase)){$stream.Length}else{[GreenLensPairedProcess.NativeProcess]::FileSize($file)};$size+=$length;if($length -gt $FileLimitBytes){$reason='generated file exceeded declared bytes'}}
    $scratchPeak=[Math]::Max($scratchPeak,$size);if($size -gt $ScratchLimitBytes){$reason='paired scratch exceeded declared bytes'}
   }
   if($reason){break}
   if($process.HasExited -and $process.JobEmpty -and $inputClosed -and $outputTask.IsCompleted -and $errorTask.IsCompleted){$drainsCompleted=$true;break}
   Start-Sleep -Milliseconds 50
  }
  if($reason){
   $process.Kill();$cancellation.Cancel()
   # Cleanup has its own fixed bound. Never synchronously await an unfinished task.
   $cleanup=[Diagnostics.Stopwatch]::StartNew()
   while($cleanup.Elapsed.TotalSeconds -lt 5 -and (!$process.HasExited -or !$process.JobEmpty -or !$inputTask.IsCompleted -or !$outputTask.IsCompleted -or !$errorTask.IsCompleted)){Start-Sleep -Milliseconds 25}
   $drainsCompleted=$inputTask.IsCompleted -and $outputTask.IsCompleted -and $errorTask.IsCompleted
  }
  if($errorTask.Status -eq [Threading.Tasks.TaskStatus]::RanToCompletion){$stderr=$errorTask.GetAwaiter().GetResult()}
  if($drainsCompleted){try{$stream.Flush()}catch{if(!$reason){$reason='output flush failed'}}}
  $timer.Stop();$runner.Refresh();$combinedPeak=[Math]::Max($combinedPeak,$runner.WorkingSet64)
  $size=0L;foreach($file in [IO.Directory]::EnumerateFiles($OutputDirectory)){$length=if([IO.Path]::GetFullPath($file).Equals($outputPath,[StringComparison]::OrdinalIgnoreCase)){$stream.Length}else{[GreenLensPairedProcess.NativeProcess]::FileSize($file)};$size+=$length;if($length -gt $FileLimitBytes){$reason='generated file exceeded declared bytes'}}
  $scratchPeak=[Math]::Max($scratchPeak,$size)
  if($size -gt $ScratchLimitBytes){$reason='paired scratch exceeded declared bytes'}
  if($combinedPeak -gt $MemoryLimitBytes){$reason='observed runner-plus-job working set exceeded declared bytes'}
  if(!$process.HasExited -or !$process.JobEmpty -or !$drainsCompleted){$reason='owned job or stream cleanup did not finish within the declared cleanup bound'}
  $exitCode=if($process.HasExited){$process.ExitCode}else{-1}
  $record=[ordered]@{executable="$Name.exe";arguments=$Arguments;exit_code=$exitCode;stderr=$stderr;started_utc=$started;finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$timer.Elapsed.TotalSeconds;sampled_peak_working_set_bytes=$(if($peak){$peak}else{$null});sampled_runner_plus_engine_peak_bytes=$(if($combinedPeak){$combinedPeak}else{$null});sampled_scratch_peak_bytes=$scratchPeak;input_bytes=$(if($InputProducer -and !$receipt){$null}else{$inputLength});input_transport=$(if($InputProducer){'typed-continuous-generated-stream-65536'}elseif($InputFile){'owned-file-stream-65536'}else{'small-text'});input_receipt=$receipt;output_bytes=$stream.Length;terminated_tree_reason=$reason;job_assignment='CreateProcess STARTUPINFOEX JOB_LIST';job_empty=$process.JobEmpty;stream_tasks_completed=$drainsCompleted;limits=@{timeout_seconds=$TimeoutSeconds;cleanup_seconds=5;memory_bytes=$MemoryLimitBytes;scratch_bytes=$ScratchLimitBytes;file_bytes=$FileLimitBytes};completed=(!$reason -and $exitCode -eq 0)}
  $records.Add($record);$records|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'processes.json') -Encoding utf8
  Write-Host "$Name exit$exitCode, $([Math]::Round($timer.Elapsed.TotalSeconds,3)) seconds, $($stream.Length) bytes"
  if($reason -or $exitCode -ne 0){throw "Required paired process failed: $reason $stderr"}
  if($InputProducer){return $receipt}
 }finally{
  $cancellation.Cancel()
  if($process){$process.Dispose()}
  if($inputStream){$inputStream.Dispose()};if($stream){$stream.Dispose()};if($errorReader){$errorReader.Dispose()}
  $cancellation.Dispose();$runner.Dispose()
 }
}
