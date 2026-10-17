#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. Harmless owned PowerShell children;
# no GreenLens, renderer, engine, report, project or native-observation retry.
[CmdletBinding()]
param([string]$OutputDirectory=(Join-Path $env:LOCALAPPDATA ('GreenLens\verification\observed-image-'+[Guid]::NewGuid().ToString('N')+' 空間')))
$ErrorActionPreference='Stop'
$source=Join-Path $PSScriptRoot 'observed-process-image.cs'
$output=[IO.Path]::GetFullPath($OutputDirectory)
$cache=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
if(!$output.StartsWith($cache+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $output)){throw 'Use a new external synthetic receipt directory.'}
New-Item -ItemType Directory -Path $output|Out-Null
$checks=[Collections.Generic.List[object]]::new();$failure=$null;$child=$null;$owned=$null;$ownedProcess=$null;$foreign=$null;$launched=0;$receiptIdentity=$null;$afterExitQuery=$null
function Check([bool]$Condition,[string]$Label){if(!$Condition){throw ('Failed: '+$Label)};$checks.Add(@{name=$Label;passed=$true})}
function Reject([scriptblock]$Action,[string]$Label,[string]$Expected){$message=$null;try{& $Action|Out-Null}catch{$message=$_.Exception.Message};Check ($null -ne $message -and $message.Contains($Expected)) $Label}
try{
 Add-Type -LiteralPath $source
 Add-Type -LiteralPath (Join-Path $PSScriptRoot 'annual-paired-native-process.cs')
 $invalid=[Microsoft.Win32.SafeHandles.SafeProcessHandle]::new([IntPtr]::Zero,$false)
 try{Reject {[GreenLensObservation.HeldProcessImage]::Read($invalid,123,1)} 'Invalid handle fails explicitly before native lookup' 'handle is unavailable'}finally{$invalid.Dispose()}
 Reject {[GreenLensObservation.HeldProcessImage]::Read($null,123,1)} 'Null handle cannot inherit supplied identity' 'handle is unavailable'
 $info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=[Environment]::ProcessPath;$info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.WorkingDirectory=$output
 $info.RedirectStandardInput=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 foreach($argument in @('-NoProfile','-NonInteractive','-Command',"[Console]::WriteLine('synthetic-ready'); [Console]::ReadLine() | Out-Null; exit 0")){$info.ArgumentList.Add($argument)}
 $child=[Diagnostics.Process]::Start($info);$launched++;$held=$child.SafeHandle;$started=$child.StartTime.ToUniversalTime();$ready=$child.StandardOutput.ReadLineAsync()
 Check ($ready.Wait(10000) -and $ready.Result -ceq 'synthetic-ready') 'Owned synthetic child reaches bounded ready state'
 $identity=[GreenLensObservation.HeldProcessImage]::Read($held,$child.Id,$started.Ticks)
 Check ($identity.ProcessId -eq $child.Id -and $identity.StartedUtcTicks -eq $started.Ticks) 'Kernel image binds exact PID and process creation time'
 Check ($identity.ImagePath.Equals($info.FileName,[StringComparison]::OrdinalIgnoreCase) -and $identity.Name -ceq [IO.Path]::GetFileNameWithoutExtension($info.FileName)) 'Kernel-derived path/name match independently selected child executable'
 $identityHash=(Get-FileHash -LiteralPath $identity.ImagePath -Algorithm SHA256).Hash.ToLowerInvariant()
 Check ($identityHash -ceq (Get-FileHash -LiteralPath $info.FileName -Algorithm SHA256).Hash.ToLowerInvariant()) 'Observed actual executable bytes match selected synthetic executable'
 Reject {[GreenLensObservation.HeldProcessImage]::Read($held,($child.Id+1),$started.Ticks)} 'Different expected PID cannot reuse held identity' 'binding changed'
 Reject {[GreenLensObservation.HeldProcessImage]::Read($held,$child.Id,($started.Ticks+1))} 'Reused-PID creation-time mismatch fails closed' 'binding changed'
 $child.StandardInput.WriteLine('exit');$child.StandardInput.Close();Check ($child.WaitForExit(10000) -and $child.ExitCode -eq 0) 'Owned child exits normally within fixed deadline'
 [GreenLensObservation.HeldProcessImage]::ValidateBinding($held,$child.Id,$started.Ticks)
 Check $true 'Held PID/creation binding remains exact after exit; only already observed identity may be reused'
 # Windows can reject a new image query once a process has exited, even with
 # its handle retained. Accept no replacement path in that case. A previously
 # verified row is reusable only after the separate held binding check above.
 try{$after=[GreenLensObservation.HeldProcessImage]::Read($held,$child.Id,$started.Ticks);$afterExitQuery=@{verified=$true;error=$null};Check ($after.ImagePath -ceq $identity.ImagePath -and $after.StartedUtcTicks -eq $identity.StartedUtcTicks) 'Post-exit image query, when available, remains exact'}
 catch{$afterExitQuery=@{verified=$false;error=$_.Exception.Message};Check ($_.Exception.Message.Contains('Observed executable image query failed; identity remains unverified.')) 'Unavailable post-exit image remains explicit and unverified'}
 $receiptIdentity=@{pid=$child.Id;started_utc=$started.ToString('o');executable_sha256=$identityHash;executable_name=$identity.Name;image_query='QueryFullProcessImageNameW through held handle'}
 $child.Dispose();$child=$null
 Reject {[GreenLensObservation.HeldProcessImage]::Read($held,$receiptIdentity.pid,$started.Ticks)} 'Closed handle cannot query a newly reused PID' 'handle is unavailable'
 # A PID snapshot can be recycled before it is reopened. A valid new process
 # creation time alone is insufficient: the held handle must belong to this Job.
 $owned=[GreenLensPairedProcess.NativeProcess]::StartBounded([Environment]::ProcessPath,@('-NoProfile','-NonInteractive','-Command',"[Console]::WriteLine('owned-ready'); [Console]::ReadLine() | Out-Null; exit 0"),$output,'.',1GB,2);$launched++
 $ownedProcess=[Diagnostics.Process]::GetProcessById($owned.Id);$ownedHandle=$ownedProcess.SafeHandle
 $reader=[IO.StreamReader]::new($owned.Output,[Text.Encoding]::UTF8,$true,1024,$true);$ready=$reader.ReadLineAsync()
 Check ($ready.Wait(10000) -and $ready.Result -ceq 'owned-ready') 'Bounded owned Job child reaches ready state'
 $owned.RequireMember($ownedHandle);Check $true 'Exact held owned process passes Job membership'
 $foreign=[Diagnostics.Process]::GetCurrentProcess();$foreignHandle=$foreign.SafeHandle;$foreignStarted=$foreign.StartTime.ToUniversalTime()
 $foreignIdentity=[GreenLensObservation.HeldProcessImage]::Read($foreignHandle,$foreign.Id,$foreignStarted.Ticks)
 Check ($foreignIdentity.ProcessId -eq $foreign.Id -and $foreignIdentity.StartedUtcTicks -eq $foreignStarted.Ticks) 'Foreign live process has a valid PID/time/image identity'
 Reject {$owned.RequireMember($foreignHandle)} 'Valid foreign PID/time cannot enter owned identity cache' 'outside the owned Job'
 $writer=[IO.StreamWriter]::new($owned.Input,[Text.UTF8Encoding]::new($false),1024,$true);$writer.WriteLine('exit');$writer.Flush();$writer.Dispose();$owned.Input.Close()
 Check ($owned.WaitForExit(10000) -and $owned.ExitCode -eq 0) 'Owned Job child exits normally within fixed deadline'
 $wait=[Diagnostics.Stopwatch]::StartNew();while(!$owned.JobEmpty -and $wait.ElapsedMilliseconds -lt 5000){Start-Sleep -Milliseconds 20};Check $owned.JobEmpty 'Owned test Job becomes empty'
 $reader.Dispose();$ownedProcess.Dispose();$ownedProcess=$null;$foreign.Dispose();$foreign=$null;$owned.Dispose();$owned=$null
}catch{$failure=$_.Exception.Message}
finally{
 if($child){try{if(!$child.HasExited){$child.Kill($true);$null=$child.WaitForExit(5000)}}finally{$child.Dispose()}}
 if($owned){try{if(!$owned.JobEmpty){$owned.Kill();$null=$owned.WaitForExit(5000)}}finally{$owned.Dispose()}};if($ownedProcess){$ownedProcess.Dispose()};if($foreign){$foreign.Dispose()}
 $receipt=[ordered]@{schema_version='greenlens.observed-process-image-test.v1';helper_sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant();native_helper_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'annual-paired-native-process.cs') -Algorithm SHA256).Hash.ToLowerInvariant();test_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant();powershell=$PSVersionTable.PSVersion.ToString();owned_synthetic_child_launches=$launched;app_renderer_engine_launches=0;identity=$receiptIdentity;after_exit_image_query=$afterExitQuery;passed_count=$checks.Count;failure=$failure;checks=$checks.ToArray();completed_utc=[DateTime]::UtcNow.ToString('o')}
 $receipt|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $output 'checks.json') -Encoding utf8NoBOM
}
if($failure){throw "Synthetic image checks failed: $failure; receipt: $output"}
Write-Host "$($checks.Count) held-image checks passed; receipt: $output"
