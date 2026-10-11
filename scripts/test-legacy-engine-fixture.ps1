#Requires -Version 7.2
[CmdletBinding()]
param([string]$EngineDirectory='', [string]$OutputDirectory='', [string]$StaticEngineDirectory='')
$ErrorActionPreference='Stop'
. "$PSScriptRoot/legacy-engine-fixture.ps1"
if(!$EngineDirectory){$EngineDirectory=Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'}
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/legacy fixture regression 空間 '+[guid]::NewGuid().ToString('N'))}
if(!$StaticEngineDirectory){$StaticEngineDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/engines/verification/radiance-6.0.2-test-'+[guid]::NewGuid().ToString('N'))}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Choose a new test evidence directory.'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$working=Join-Path $OutputDirectory 'Project with spaces 空間'
New-Item -ItemType Directory -Path $working|Out-Null
$checks=[Collections.Generic.List[string]]::new()
$fixture=New-LegacyEngineFixture -Source $EngineDirectory -Destination $StaticEngineDirectory
if(!$fixture.source_and_copy_identical -or $fixture.file_count -lt 6 -or ($fixture.engine_executables -join ',') -ne 'gensky.exe,oconv.exe,rtrace.exe' -or $fixture.annual_executables_present){throw 'Copied engine identity or executable exclusion failed.'}
$checks.Add('Pinned three-executable fixture and every copied library/DLL byte match the source; annual executables absent')
$before=(Get-FileHash -LiteralPath (Join-Path $StaticEngineDirectory 'lib/rayinit.cal')).Hash
$rejected=$false
try{$null=New-LegacyEngineFixture -Source $EngineDirectory -Destination $StaticEngineDirectory}catch{$rejected=$_.Exception.Message -like '*must be new*'}
if(!$rejected -or (Get-FileHash -LiteralPath (Join-Path $StaticEngineDirectory 'lib/rayinit.cal')).Hash -ne $before){throw 'Existing fixture was not preserved.'}
$checks.Add('Existing fixture rejected without overwriting its CAL library')
$unicodeTarget=Join-Path $OutputDirectory 'Rejected Unicode engine 空間'
$rejected=$false
try{$null=New-LegacyEngineFixture -Source $EngineDirectory -Destination $unicodeTarget}catch{$rejected=$_.Exception.Message -like '*ASCII*'}
if(!$rejected -or (Test-Path -LiteralPath $unicodeTarget)){throw 'Unsupported Unicode engine path was not rejected before writes.'}
$checks.Add('Unicode engine-library destination rejected before writes with an actionable override')
$missingTarget=$StaticEngineDirectory+'-missing'
$rejected=$false
try{$null=New-LegacyEngineFixture -Source (Join-Path $OutputDirectory 'Absent engine') -Destination $missingTarget}catch{$rejected=$true}
if(!$rejected -or (Test-Path -LiteralPath $missingTarget)){throw 'Missing engine did not fail before fixture creation.'}
$checks.Add('Unavailable source engine fails instead of skipping')

# Required real native library initialization. This is deliberately inexpensive;
# the following CI step still performs the complete original A/B replay.
$info=[Diagnostics.ProcessStartInfo]::new()
$info.FileName=Join-Path $StaticEngineDirectory 'bin/rtrace.exe'
$info.WorkingDirectory=[IO.Path]::GetFullPath($working)
$info.ArgumentList.Add('-version')
$info.UseShellExecute=$false;$info.CreateNoWindow=$true
$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
$info.Environment['RAYPATH']='.;'+(Join-Path ([IO.Path]::GetFullPath($StaticEngineDirectory)) 'lib')
$process=[Diagnostics.Process]::new();$process.StartInfo=$info
$started=[DateTime]::UtcNow;$timer=[Diagnostics.Stopwatch]::StartNew();$startedProcess=$false
try{
    $startedProcess=$process.Start()
    if(!$startedProcess){throw 'Required native rtrace could not start.'}
    $stdoutTask=$process.StandardOutput.ReadToEndAsync();$stderrTask=$process.StandardError.ReadToEndAsync()
    $timedOut=!$process.WaitForExit(10000)
    if($timedOut){$process.Kill($true);$process.WaitForExit()}
    $stdout=$stdoutTask.GetAwaiter().GetResult();$stderr=$stderrTask.GetAwaiter().GetResult();$timer.Stop()
    $processRecord=@{executable='rtrace.exe';arguments=@('-version');exit_code=$process.ExitCode;stdout=$stdout;stderr=$stderr;timed_out=$timedOut;started_utc=$started.ToString('o');finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$timer.Elapsed.TotalSeconds;working_path_kind='Unicode and spaces';library_path_kind='ASCII';executable_sha256=(Get-FileHash -LiteralPath $info.FileName).Hash.ToLowerInvariant()}
    $processRecord|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'native-loader.json') -Encoding utf8NoBOM
    if($timedOut -or $process.ExitCode -ne 0 -or $stdout.Trim() -ne 'RADIANCE 6.0.2 2026-02-02 LBNL (6.0.c1700d56cc)' -or $stderr){throw 'Pinned native CAL-library initialization failed; native-loader.json is retained.'}
}finally{if($startedProcess -and !$process.HasExited){$process.Kill($true);$process.WaitForExit()};$process.Dispose()}
$checks.Add('Real pinned rtrace initializes rayinit.cal from an ASCII library path inside a Unicode working directory')

# Mutate only the disposable copied fixture after the native check. Preflight must
# reject altered scientific library bytes before creating another engine copy.
[IO.File]::AppendAllText((Join-Path $StaticEngineDirectory 'lib/rayinit.cal'), "`n{test-only corruption}`n")
$corruptTarget=$StaticEngineDirectory+'-corrupt'
$rejected=$false
try{$null=New-LegacyEngineFixture -Source $StaticEngineDirectory -Destination $corruptTarget}catch{$rejected=$_.Exception.Message -like '*rayinit.cal differs*'}
if(!$rejected -or (Test-Path -LiteralPath $corruptTarget)){throw 'Altered critical library bytes were not rejected before copying.'}
$checks.Add('Altered critical CAL library rejected before destination creation')
@{schema_version='greenlens.legacy-engine-fixture-test.v1';passed=$true;completed_utc=[DateTime]::UtcNow.ToString('o');checks=@($checks);engine_fixture=$fixture;native_process=$processRecord;scope='Fixture byte identity and native CAL-loader regression only; complete legacy replay is a separate mandatory CI step';disposable_fixture_corrupted_after_probe=$true}|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8NoBOM
$global:LASTEXITCODE=0
Write-Output $OutputDirectory
