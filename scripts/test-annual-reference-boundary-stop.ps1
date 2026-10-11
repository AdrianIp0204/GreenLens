# Native process-lifecycle regression using explicitly synthetic sleeping
# children. This does not execute Radiance or qualify a scientific reference.
$ErrorActionPreference='Stop'
$root=Join-Path $env:LOCALAPPDATA ("GreenLens/verification/reference scheduling regression "+[guid]::NewGuid().ToString('N')+' 空間')
$null=New-Item -ItemType Directory -Path $root
$fixture=Join-Path $root 'annual-controlled-reference.ps1'
@'
param([string]$OutputDirectory)
$info=[Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path)
$info.UseShellExecute=$false;$info.CreateNoWindow=$true
$info.ArgumentList.Add('-NoProfile');$info.ArgumentList.Add('-Command');$info.ArgumentList.Add('Start-Sleep -Seconds 90')
$child=[Diagnostics.Process]::Start($info)
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'synthetic-child.pid'),[string]$child.Id)
[ordered]@{declaration=@{schema_version='greenlens.controlled-reference.v1';alternative='A'};checks=@(@{interval_index=8512},@{interval_index=8504},@{interval_index=8508})}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $OutputDirectory 'controlled-reference.json')
Start-Sleep -Seconds 90
'@|Set-Content -LiteralPath $fixture -Encoding utf8
$start=[Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path)
$start.UseShellExecute=$false;$start.CreateNoWindow=$true
foreach($argument in @('-NoProfile','-File',$fixture,'-OutputDirectory',$root)){$start.ArgumentList.Add($argument)}
$worker=[Diagnostics.Process]::Start($start)
try{
 $deadline=[DateTime]::UtcNow.AddSeconds(15)
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
