#Requires -Version 7.2
# Actual Windows process/pipe tests with original synthetic fixtures. No Radiance
# execution, scientific result, existing worker, or global setting is involved.
[CmdletBinding()]
param([string]$OutputDirectory='')
$ErrorActionPreference='Stop'
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/paired atomic job regression '+[Guid]::NewGuid().ToString('N')+' 空間')}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained test directory.'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$testRoot=[IO.Path]::GetFullPath($OutputDirectory)
$source=(Resolve-Path "$PSScriptRoot/annual-paired-native-process.cs").Path
$loadedSourceHash=(Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant()
Add-Type -LiteralPath $source
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using Microsoft.Win32.SafeHandles;
public static class SyntheticInheritedHandle {
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetHandleInformation(SafeFileHandle handle,uint mask,uint flags);
 public static void Set(SafeFileHandle handle) { if(!SetHandleInformation(handle,1,1)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); }
}
'@
$fixture=Join-Path $testRoot 'fixture.ps1'
@'
param([string]$Mode,[Parameter(ValueFromRemainingArguments=$true)][string[]]$Tail)
$ErrorActionPreference='Stop'
function Identity([string]$Name){
 $self=[Diagnostics.Process]::GetCurrentProcess()
 $text=@{pid=$PID;start_ticks=$self.StartTime.ToUniversalTime().Ticks}|ConvertTo-Json
 $target=Join-Path (Get-Location) "$Name.json"
 [IO.File]::WriteAllText(($target+'.tmp'),$text)
 [IO.File]::Move(($target+'.tmp'),$target)
}
if($Mode -eq 'arguments'){
 $json=@{arguments=@($Tail);raypath=$env:RAYPATH;directory=(Get-Location).Path}|ConvertTo-Json -Compress -Depth 5
 $bytes=[Text.Encoding]::UTF8.GetBytes($json);$output=[Console]::OpenStandardOutput();$output.Write($bytes,0,$bytes.Length);$output.Flush()
 exit 0
}
if($Mode -eq 'echo'){
 $i=[Console]::OpenStandardInput();$o=[Console]::OpenStandardOutput();$i.CopyTo($o,65536);$o.Flush();exit 0
}
if($Mode -eq 'nonzero'){[Console]::Error.Write('synthetic failure');exit 7}
if($Mode -eq 'quick'){exit 0}
if($Mode -eq 'child'){
 Identity 'child';[Console]::WriteLine('synthetic child inherited stdout');[Console]::Error.WriteLine('synthetic child inherited stderr')
 Start-Sleep -Seconds 60;exit 0
}
if($Mode -in @('parent-exits','parent-waits')){
 Identity 'parent'
 $info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=(Get-Command pwsh).Source
 $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.WorkingDirectory=(Get-Location).Path
 foreach($arg in @('-NoProfile','-File',$PSCommandPath,'child')){$info.ArgumentList.Add($arg)}
 $child=[Diagnostics.Process]::Start($info);$timer=[Diagnostics.Stopwatch]::StartNew()
 while(!(Test-Path -LiteralPath (Join-Path (Get-Location) 'child.json'))){if($timer.Elapsed.TotalSeconds -gt 15){throw 'Child startup deadline'};Start-Sleep -Milliseconds 20}
 if($Mode -eq 'parent-waits'){Start-Sleep -Seconds 60}
 $child.Dispose();exit 0
}
throw 'Unknown synthetic fixture mode.'
'@|Set-Content -LiteralPath $fixture -Encoding utf8NoBOM
$executable=(Get-Command pwsh).Source
$checks=[Collections.Generic.List[object]]::new()
function Assert([bool]$Condition,[string]$Message){if(!$Condition){throw $Message}}
function New-Case([string]$Name){$path=Join-Path $testRoot $Name;New-Item -ItemType Directory -Path $path|Out-Null;return $path}
function Launch([string]$Mode,[string]$Directory,[string[]]$Tail=@()){
 return [GreenLensPairedProcess.NativeProcess]::Start($executable,(@('-NoProfile','-File',$fixture,$Mode)+$Tail),$Directory,'synthetic-child-only;空間')
}
function Drain($Process){
 $out=[IO.MemoryStream]::new();$err=[IO.MemoryStream]::new()
 return @{output=$out;error=$err;outTask=$Process.Output.CopyToAsync($out);errTask=$Process.Error.CopyToAsync($err)}
}
function Wait-Tasks($Tasks,[double]$Seconds=10){
 $timer=[Diagnostics.Stopwatch]::StartNew()
 while(@($Tasks|Where-Object{!$_.IsCompleted}).Count){if($timer.Elapsed.TotalSeconds -gt $Seconds){throw 'Bounded synthetic stream drain expired'};Start-Sleep -Milliseconds 20}
 foreach($task in $Tasks){$null=$task.GetAwaiter().GetResult()}
}
function Await-Child([string]$Directory){
 $timer=[Diagnostics.Stopwatch]::StartNew();$path=Join-Path $Directory 'child.json'
 while(!(Test-Path -LiteralPath $path)){if($timer.Elapsed.TotalSeconds -gt 15){throw 'Synthetic descendant never started'};Start-Sleep -Milliseconds 20}
 $identity=Get-Content -LiteralPath $path -Raw|ConvertFrom-Json
 $held=[Diagnostics.Process]::GetProcessById([int]$identity.pid)
 if($held.StartTime.ToUniversalTime().Ticks -ne [long]$identity.start_ticks){$held.Dispose();throw 'Synthetic child identity changed'}
 $null=$held.SafeHandle
 return $held
}
function Dispose-Drain($Drain){if($Drain){$Drain.output.Dispose();$Drain.error.Dispose()}}
function Await-EmptyJob($Process){
 $timer=[Diagnostics.Stopwatch]::StartNew()
 while(!$Process.JobEmpty){if($timer.Elapsed.TotalSeconds -gt 5){throw 'Job accounting did not settle after process/stream exit'};Start-Sleep -Milliseconds 20}
}

# Every argument is inspected by the newly created executable, including empty
# strings, embedded quotes, Unicode, and backslashes immediately before quotes.
$case=New-Case 'arguments and environment 空間'
$before=[Environment]::GetEnvironmentVariable('RAYPATH')
$arguments=@('','one two','quote"inside','three\\\"slashes','trailing\','空間',('tab'+"`t"+'value'))
$p=Launch 'arguments' $case $arguments;$drain=Drain $p
try{
 $p.Input.Close();Assert ($p.WaitForExit(15000)) 'Argument fixture did not exit'
 Wait-Tasks @($drain.outTask,$drain.errTask)
 $actual=[Text.Encoding]::UTF8.GetString($drain.output.ToArray())|ConvertFrom-Json
 Await-EmptyJob $p
 Assert ((ConvertTo-Json -InputObject @($actual.arguments) -Compress) -ceq (ConvertTo-Json -InputObject $arguments -Compress)) 'Windows argument quoting changed values/order'
 Assert ($actual.raypath -ceq 'synthetic-child-only;空間') 'Child environment override missing'
 Assert ($actual.directory -ceq $case) 'Unicode working directory changed'
 Assert ([Environment]::GetEnvironmentVariable('RAYPATH') -ceq $before) 'Supervisor environment was changed'
 Assert ($p.ExitCode -eq 0 -and $p.JobEmpty) 'Argument job did not fully exit'
 $checks.Add(@{name='argument-array-quoting-Unicode-child-environment';passed=$true;argument_count=$arguments.Count})
}finally{$p.Dispose();Dispose-Drain $drain}

$case=New-Case 'binary echo'
$payload=[byte[]]::new(2MB);for($i=0;$i -lt $payload.Length;$i++){$payload[$i]=[byte](($i*29+17)%256)}
$expected=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($payload)).ToLowerInvariant()
$p=Launch 'echo' $case;$drain=Drain $p
try{
 $write=$p.Input.WriteAsync($payload,0,$payload.Length);Wait-Tasks @($write);$p.Input.Close()
 Assert ($p.WaitForExit(15000)) 'Binary echo did not exit';Wait-Tasks @($drain.outTask,$drain.errTask)
 $bytes=$drain.output.ToArray();$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
 @{expected_bytes=$payload.Length;actual_bytes=$bytes.Length;expected_sha256=$expected;actual_sha256=$hash;exit_code=$p.ExitCode;job_empty=$p.JobEmpty}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $case 'binary-observation.json') -Encoding utf8NoBOM
 Await-EmptyJob $p
 Assert ($bytes.Length -eq $payload.Length -and $hash -ceq $expected -and $p.ExitCode -eq 0 -and $p.JobEmpty) 'Binary stream changed or job remained alive'
 $checks.Add(@{name='binary-2MiB-round-trip';passed=$true;bytes=$bytes.Length;sha256=$hash})
}finally{$p.Dispose();Dispose-Drain $drain}

$case=New-Case 'nonzero exit';$p=Launch 'nonzero' $case;$drain=Drain $p
try{
 $p.Input.Close();Assert ($p.WaitForExit(15000)) 'Nonzero fixture hung';Wait-Tasks @($drain.outTask,$drain.errTask)
 Await-EmptyJob $p
 Assert ($p.ExitCode -eq 7 -and [Text.Encoding]::UTF8.GetString($drain.error.ToArray()) -ceq 'synthetic failure' -and $p.JobEmpty) 'Nonzero exit/stderr did not propagate'
 $checks.Add(@{name='nonzero-exit-and-stderr';passed=$true;exit_code=7})
}finally{$p.Dispose();Dispose-Drain $drain}

foreach($mode in @('kill-after-parent-exit','dispose-after-parent-exit','pending-stdin-after-parent-exit','kill-live-parent-and-child')){
 $case=New-Case $mode;$p=$null;$child=$null;$drain=$null
 try{
  $p=Launch $(if($mode -eq 'kill-live-parent-and-child'){'parent-waits'}else{'parent-exits'}) $case
  $drain=Drain $p
  $write=$null
  if($mode -eq 'pending-stdin-after-parent-exit'){$write=$p.Input.WriteAsync($payload,0,$payload.Length)}else{$p.Input.Close()}
  $child=Await-Child $case
  if($mode -ne 'kill-live-parent-and-child'){Assert ($p.WaitForExit(15000)) 'Synthetic direct parent did not exit'}
  Assert (!$p.JobEmpty -and !$child.HasExited) 'Pipe-holder was not a live member after direct-parent exit'
  $memory=$p.WorkingSetBytes;Assert ($memory -gt 0 -and $p.PeakWorkingSetBytes -gt 0) 'Live job descendant memory was not observed'
  $timer=[Diagnostics.Stopwatch]::StartNew()
  if($mode -eq 'dispose-after-parent-exit'){$p.Dispose();$p=$null}
  else{$p.Kill();Await-EmptyJob $p;Assert $child.HasExited 'Empty job advertised completion before observed child handle signaled'}
  Assert ($child.WaitForExit(5000)) 'Owned descendant survived job termination'
  # Dispose deliberately closes the readers too; their cancellation is allowed.
  if($mode -eq 'dispose-after-parent-exit'){
   $tasks=@($drain.outTask,$drain.errTask)
   while(@($tasks|Where-Object{!$_.IsCompleted}).Count){if($timer.Elapsed.TotalSeconds -gt 5){throw 'Disposed stream tasks did not settle'};Start-Sleep -Milliseconds 20}
  }else{
   Assert ($p.WaitForExit(5000)) 'Owned direct process survived job termination'
   Wait-Tasks @($drain.outTask,$drain.errTask) 5
   Await-EmptyJob $p
   Assert $p.JobEmpty 'Killed job still contains active members'
  }
  if($write){
   $limit=[Diagnostics.Stopwatch]::StartNew();while(!$write.IsCompleted){if($limit.Elapsed.TotalSeconds -gt 5){throw 'Broken-pipe input task did not settle'};Start-Sleep -Milliseconds 20}
   $failed=$false;try{$null=$write.GetAwaiter().GetResult()}catch{$failed=$true}
   Assert $failed 'Blocked input was incorrectly advertised complete after child termination'
  }
  $checks.Add(@{name=$mode;passed=$true;live_job_working_set_bytes=$memory;termination_seconds=$timer.Elapsed.TotalSeconds;child_exited=$true})
 }finally{
  if($p){$p.Dispose()}
  if($child){if(!$child.HasExited){$child.Kill($true);$null=$child.WaitForExit(5000)};$child.Dispose()}
  Dispose-Drain $drain
 }
}

# An unrelated inheritable file handle must not leak into an otherwise live
# child. The explicit HANDLE_LIST contains only the three redirected pipe ends.
$case=New-Case 'explicit handle list';$file=Join-Path $case 'unrelated-exclusive.dat'
$unrelated=[IO.File]::Open($file,[IO.FileMode]::CreateNew,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
[SyntheticInheritedHandle]::Set($unrelated.SafeFileHandle)
$p=$null;$child=$null;$drain=$null
try{
 $p=Launch 'parent-waits' $case;$drain=Drain $p;$p.Input.Close();$child=Await-Child $case
 $unrelated.Dispose();$unrelated=$null
 $reopen=[IO.File]::Open($file,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None);$reopen.Dispose()
 Assert (!$p.HasExited -and !$child.HasExited) 'Handle-isolation fixture was not alive'
 $checks.Add(@{name='only-explicit-standard-handles-inherited';passed=$true})
}finally{if($unrelated){$unrelated.Dispose()};if($p){$p.Dispose()};if($child){Assert ($child.WaitForExit(5000)) 'Handle fixture survived disposal';$child.Dispose()};Dispose-Drain $drain}

foreach($case in @('missing-executable','missing-directory','NUL-argument','negative-wait')){
 $failed=$false;$p=$null;$directory=New-Case $case
 try{
  switch($case){
   'missing-executable' {$p=[GreenLensPairedProcess.NativeProcess]::Start((Join-Path $directory 'absent.exe'),@(),$directory,'.')}
   'missing-directory' {$p=[GreenLensPairedProcess.NativeProcess]::Start($executable,@('-NoProfile'),(Join-Path $directory 'absent'),'.')}
   'NUL-argument' {$p=[GreenLensPairedProcess.NativeProcess]::Start($executable,@("a`0b"),$directory,'.')}
   'negative-wait' {$p=Launch 'quick' $directory;$null=$p.WaitForExit(-1)}
  }
 }catch{$failed=$true}finally{if($p){$p.Dispose()}}
 Assert $failed "Invalid native launch/wait was accepted: $case"
 $checks.Add(@{name=$case;passed=$true})
}
Assert ((Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant() -ceq $loadedSourceHash) 'Native source changed during the fixture run'
@{schema_version='greenlens.paired-native-process-regression.v1';scope='Actual Windows Job Object, argument and binary pipe behavior using original synthetic processes; no Radiance numerical acceptance';passed=$true;check_count=$checks.Count;checks=$checks.ToArray();source_sha256=$loadedSourceHash;test_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();completed_utc=[DateTime]::UtcNow.ToString('o')}|ConvertTo-Json -Depth 20|Set-Content -LiteralPath (Join-Path $testRoot 'summary.json') -Encoding utf8NoBOM
$global:LASTEXITCODE=0
Write-Host "Atomic job/pipe regression passed: $($checks.Count) checks, retained $testRoot"
