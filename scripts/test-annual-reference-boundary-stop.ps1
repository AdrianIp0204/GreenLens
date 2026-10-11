# Native process-lifecycle regression using explicitly synthetic sleeping
# children. This does not execute Radiance or qualify a scientific reference.
$ErrorActionPreference='Stop'
$root=Join-Path $env:LOCALAPPDATA ("GreenLens/verification/reference scheduling regression "+[guid]::NewGuid().ToString('N')+' 空間')
$null=New-Item -ItemType Directory -Path $root
$fixture=Join-Path $root 'annual-controlled-reference.ps1'
@'
param([string]$OutputDirectory,[int]$InitialCases=3,[switch]$WrongPrefix)
$info=[Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path)
$info.UseShellExecute=$false;$info.CreateNoWindow=$true
$info.ArgumentList.Add('-NoProfile');$info.ArgumentList.Add('-Command');$info.ArgumentList.Add('Start-Sleep -Seconds 90')
$child=[Diagnostics.Process]::Start($info)
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'synthetic-child.pid'),[string]$child.Id)
$prefix=@(8512,8504,8508);if($WrongPrefix){$prefix=@(8512,8508,8504)}
[ordered]@{declaration=@{schema_version='greenlens.controlled-reference.v1';alternative='A';executed_interval_indices=@(8512,8504,8508,1904)};checks=@($prefix[0..($InitialCases-1)]|ForEach-Object{@{interval_index=$_}})}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $OutputDirectory 'controlled-reference.json')
Start-Sleep -Seconds 90
'@|Set-Content -LiteralPath $fixture -Encoding utf8
$start=[Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path)
$start.UseShellExecute=$false;$start.CreateNoWindow=$true
foreach($argument in @('-NoProfile','-File',$fixture,'-OutputDirectory',$root)){$start.ArgumentList.Add($argument)}
$worker=[Diagnostics.Process]::Start($start)
try{
 $deadline=[DateTime]::UtcNow.AddSeconds(60)
 while(!(Test-Path -LiteralPath (Join-Path $root 'controlled-reference.json'))){if([DateTime]::UtcNow -gt $deadline){throw 'Synthetic child startup timed out.'};Start-Sleep -Milliseconds 100}
 $command=(Get-CimInstance Win32_Process -Filter "ProcessId=$($worker.Id)").CommandLine
 $hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($command))).ToLowerInvariant()
 $options=@{WorkerProcessId=$worker.Id;ExpectedStartTimeUtc=$worker.StartTime.ToUniversalTime().ToString('o');ExpectedCommandSha256=$hash;OutputDirectory=$root;Alternative='A'}
 & "$PSScriptRoot/annual-reference-boundary-stop.ps1" @options -CheckOnly
 $bad=@{}+$options;$bad.ExpectedCommandSha256='0'*64;$rejected=$false
 try{& "$PSScriptRoot/annual-reference-boundary-stop.ps1" @bad -CheckOnly}catch{$rejected=$_.Exception.Message -like '*identity differs*'}
 if(!$rejected -or $worker.HasExited){throw 'Wrong command identity did not safely reject.'}
 $bad=@{}+$options;$bad.ExpectedStartTimeUtc='2000-01-01T00:00:00Z';$rejected=$false
 try{& "$PSScriptRoot/annual-reference-boundary-stop.ps1" @bad -CheckOnly}catch{$rejected=$_.Exception.Message -like '*PID was reused*'}
 if(!$rejected -or $worker.HasExited){throw 'Wrong start time did not safely reject.'}
 $childId=[int](Get-Content -LiteralPath (Join-Path $root 'synthetic-child.pid'))
 & "$PSScriptRoot/annual-reference-boundary-stop.ps1" @options
 $worker.WaitForExit()
 $record=Get-Content -LiteralPath (Join-Path $root 'scheduling-interruption.json') -Raw|ConvertFrom-Json
 if(!$record.terminated -or ($record.completed_interval_indices -join ',') -ne '8512,8504,8508' -or (Get-Process -Id $childId -ErrorAction SilentlyContinue)){throw 'The owned synthetic process tree or scheduling evidence is incomplete.'}
 $snapshotPath=Join-Path $root $record.completed_report_snapshot
 $snapshot=Get-Content -LiteralPath $snapshotPath -Raw|ConvertFrom-Json
 if((Get-FileHash -LiteralPath $snapshotPath).Hash.ToLowerInvariant() -ne $record.completed_report_sha256 -or ($snapshot.checks.interval_index -join ',') -ne ($record.completed_interval_indices -join ',')){throw 'The interruption hash and intervals do not bind the same retained snapshot.'}
 Write-Host 'Four synthetic watcher checks passed, including actual owned-tree termination; no engine or scientific acceptance.'
 Write-Host "Retained synthetic evidence: $root"
}finally{if(!$worker.HasExited){$worker.Kill($true);$worker.WaitForExit()};$worker.Dispose()}

function Start-BoundaryFixture([string]$Name,[int]$InitialCases,[bool]$WrongPrefix=$false){
 $directory=Join-Path $root $Name;New-Item -ItemType Directory -Path $directory|Out-Null
 $info=[Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path);$info.UseShellExecute=$false;$info.CreateNoWindow=$true
 foreach($argument in @('-NoProfile','-File',$fixture,'-OutputDirectory',$directory,'-InitialCases',"$InitialCases")){$info.ArgumentList.Add($argument)}
 if($WrongPrefix){$info.ArgumentList.Add('-WrongPrefix')}
 $process=[Diagnostics.Process]::Start($info)
 $deadline=[DateTime]::UtcNow.AddSeconds(60)
 while(!(Test-Path -LiteralPath (Join-Path $directory 'controlled-reference.json'))){if([DateTime]::UtcNow -gt $deadline){$process.Kill($true);throw 'Two-case fixture startup timed out.'};Start-Sleep -Milliseconds 100}
 $command=(Get-CimInstance Win32_Process -Filter "ProcessId=$($process.Id)").CommandLine
 $options=@{WorkerProcessId=$process.Id;ExpectedStartTimeUtc=$process.StartTime.ToUniversalTime().ToString('o');ExpectedCommandSha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($command))).ToLowerInvariant();OutputDirectory=$directory;Alternative='A';StopAfterCases=2}
 return @{process=$process;options=$options;directory=$directory;child=[int](Get-Content -LiteralPath (Join-Path $directory 'synthetic-child.pid'))}
}
$early=Start-BoundaryFixture 'two-case-early' 1;$watcher=$null
try{
 $info=[Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path);$info.UseShellExecute=$false;$info.CreateNoWindow=$true
 foreach($argument in @('-NoProfile','-File',(Join-Path $PSScriptRoot 'annual-reference-boundary-stop.ps1'))){$info.ArgumentList.Add($argument)}
 foreach($entry in $early.options.GetEnumerator()){$info.ArgumentList.Add('-'+$entry.Key);$info.ArgumentList.Add([string]$entry.Value)}
 $watcher=[Diagnostics.Process]::Start($info)
 Start-Sleep -Seconds 3
 if($watcher.HasExited -or $early.process.HasExited -or (Test-Path -LiteralPath (Join-Path $early.directory 'scheduling-interruption.json'))){throw 'Two-case watcher terminated before its declared boundary.'}
 $report=[ordered]@{declaration=@{schema_version='greenlens.controlled-reference.v1';alternative='A';executed_interval_indices=@(8512,8504,8508,1904)};checks=@(@{interval_index=8512},@{interval_index=8504})}
 $report|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $early.directory 'controlled-reference.json')
 if(!$watcher.WaitForExit(60000) -or $watcher.ExitCode -ne 0){throw 'Two-case watcher did not complete at its declared boundary.'}
 if(!$early.process.WaitForExit(5000) -or (Get-Process -Id $early.child -ErrorAction SilentlyContinue)){throw 'Two-case fixture tree survived the boundary stop.'}
 $record=Get-Content -LiteralPath (Join-Path $early.directory 'scheduling-interruption.json') -Raw|ConvertFrom-Json
 $snapshot=Join-Path $early.directory $record.completed_report_snapshot
 if(!$record.terminated -or $record.stop_after_cases -ne 2 -or ($record.completed_interval_indices -join ',') -cne '8512,8504' -or (Get-FileHash -LiteralPath $snapshot).Hash.ToLowerInvariant() -cne $record.completed_report_sha256 -or $record.next_case_policy -notmatch 'partial/cancelled') {throw 'Two-case completed snapshot or partial-next-case policy differs.'}
}finally{if($watcher){if(!$watcher.HasExited){$watcher.Kill($true);$watcher.WaitForExit()};$watcher.Dispose()};if(!$early.process.HasExited){$early.process.Kill($true);$early.process.WaitForExit()};$early.process.Dispose()}

foreach($case in @(@{name='two-case-late';count=3;wrong=$false;error='boundary already passed'},@{name='two-case-wrong-prefix';count=2;wrong=$true;error='completed interval sequence'})){
 $fixtureCase=Start-BoundaryFixture $case.name $case.count $case.wrong
 try{
  $message=$null
  $fixtureOptions=$fixtureCase.options
  try{& "$PSScriptRoot/annual-reference-boundary-stop.ps1" @fixtureOptions}catch{$message=$_.Exception.Message}
  if(!$message -or $message -notmatch $case.error -or $fixtureCase.process.HasExited -or !(Get-Process -Id $fixtureCase.child -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath (Join-Path $fixtureCase.directory 'scheduling-interruption.json'))){throw "Unsafe rejection for $($case.name): $message"}
 }finally{if(!$fixtureCase.process.HasExited){$fixtureCase.process.Kill($true);$fixtureCase.process.WaitForExit()};$fixtureCase.process.Dispose()}
}
Write-Host 'Eight synthetic watcher checks passed: default three-case guards, early two-case waiting, exact two-case snapshot/tree stop, late and wrong-prefix rejection. No Radiance ran.'
