# Bounded native engine supervisor for the reference-only investigation.
# Dot-sourced after annual-feasibility -FunctionsOnly; overrides that local helper.
function Invoke-Engine([string]$Name, [string[]]$Arguments, [string]$Output, [string]$InputText = '') {
 Invoke-ReferenceProcess -Executable (Join-Path $EngineRoot "bin/$Name.exe") -Name $Name -Arguments $Arguments -Output $Output -InputText $InputText
}
function Invoke-ReferenceProcess {
 param([string]$Executable,[string]$Name,[string[]]$Arguments,[string]$Output,[string]$InputText='',
  [double]$TimeoutSeconds=7200,[long]$MemoryLimitBytes=2147483648,[long]$ScratchLimitBytes=2147483648,[long]$FileLimitBytes=536870912)
 $info=[Diagnostics.ProcessStartInfo]::new()
 $info.FileName=$Executable;$info.WorkingDirectory=$OutputDirectory
 $info.UseShellExecute=$false;$info.CreateNoWindow=$true
 $info.RedirectStandardInput=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 foreach($argument in $Arguments){$info.ArgumentList.Add($argument)}
 $info.Environment['RAYPATH']='.;'+(Join-Path $EngineRoot 'lib')
 $process=[Diagnostics.Process]::new();$process.StartInfo=$info
 $started=[DateTime]::UtcNow.ToString('o');$timer=[Diagnostics.Stopwatch]::StartNew()
 $stream=[IO.File]::Create((Join-Path $OutputDirectory $Output));$peak=0L;$combinedPeak=0L;$scratchPeak=0L;$reason=$null;$lastDiskCheck=-5.;$runner=[Diagnostics.Process]::GetCurrentProcess();$didStart=$false
 try{
  if(!$process.Start()){throw "Cannot start $Name"};$didStart=$true
  $outputTask=$process.StandardOutput.BaseStream.CopyToAsync($stream);$errorTask=$process.StandardError.ReadToEndAsync()
  $inputBytes=$utf8.GetBytes($InputText);$inputTask=$process.StandardInput.BaseStream.WriteAsync($inputBytes,0,$inputBytes.Length);$inputClosed=$false
  while(!$process.WaitForExit(100)){
   if(!$inputClosed -and $inputTask.IsCompleted){try{$null=$inputTask.GetAwaiter().GetResult();$process.StandardInput.Close();$inputClosed=$true}catch{$reason='engine input stream failed'}}
   $process.Refresh();$peak=[Math]::Max($peak,$process.PeakWorkingSet64)
   $runner.Refresh();$combined=$process.WorkingSet64+$runner.WorkingSet64;$combinedPeak=[Math]::Max($combinedPeak,$combined)
   if($timer.Elapsed.TotalSeconds -gt $TimeoutSeconds){$reason='process timeout exceeded declared seconds'}
   elseif($combined -gt $MemoryLimitBytes){$reason='observed runner-plus-engine working set exceeded declared bytes'}
   if($timer.Elapsed.TotalSeconds-$lastDiskCheck -ge 5){
    $lastDiskCheck=$timer.Elapsed.TotalSeconds;$size=0L
    foreach($file in [IO.Directory]::EnumerateFiles($OutputDirectory)){$length=([IO.FileInfo]$file).Length;$size+=$length;if($length -gt $FileLimitBytes){$reason='generated file exceeded declared bytes'}}
    $scratchPeak=[Math]::Max($scratchPeak,$size);if($size -gt $ScratchLimitBytes){$reason='reference scratch exceeded declared bytes'}
   }
   if($reason){$process.Kill($true);$process.WaitForExit();break}
  }
  try{$null=$inputTask.GetAwaiter().GetResult()}catch{if(!$reason){$reason='engine exited before accepting complete input'}}
  $null=$outputTask.GetAwaiter().GetResult();$stderr=$errorTask.GetAwaiter().GetResult();$timer.Stop()
  $stream.Flush();$runner.Refresh();$combinedPeak=[Math]::Max($combinedPeak,$runner.WorkingSet64)
  $size=0L;foreach($file in [IO.Directory]::EnumerateFiles($OutputDirectory)){$length=([IO.FileInfo]$file).Length;$size+=$length;if($length -gt $FileLimitBytes){$reason='generated file exceeded declared bytes'}}
  $scratchPeak=[Math]::Max($scratchPeak,$size)
  if($size -gt $ScratchLimitBytes){$reason='reference scratch exceeded declared bytes'}
  if($combinedPeak -gt $MemoryLimitBytes){$reason='observed runner-plus-engine working set exceeded declared bytes'}
  if($timer.Elapsed.TotalSeconds -gt $TimeoutSeconds -and !$reason){$reason='process timeout exceeded declared seconds'}
  $record=[ordered]@{executable="$Name.exe";arguments=$Arguments;exit_code=$process.ExitCode;stderr=$stderr;started_utc=$started;finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$timer.Elapsed.TotalSeconds;sampled_peak_working_set_bytes=$(if($peak){$peak}else{$null});sampled_runner_plus_engine_peak_bytes=$(if($combinedPeak){$combinedPeak}else{$null});sampled_scratch_peak_bytes=$scratchPeak;output_bytes=$stream.Length;terminated_tree_reason=$reason;limits=@{timeout_seconds=$TimeoutSeconds;memory_bytes=$MemoryLimitBytes;scratch_bytes=$ScratchLimitBytes;file_bytes=$FileLimitBytes};completed=(!$reason -and $process.ExitCode -eq 0)}
  $records.Add($record);$records|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'processes.json') -Encoding utf8
  Write-Host "$Name exit$($process.ExitCode), $([Math]::Round($timer.Elapsed.TotalSeconds,3)) seconds, $($stream.Length) bytes"
  if($reason -or $process.ExitCode -ne 0){throw "Required reference process failed: $reason $stderr"}
 }finally{if($didStart -and !$process.HasExited){$process.Kill($true);$process.WaitForExit()};$stream.Dispose();$process.Dispose();$runner.Dispose()}
}
