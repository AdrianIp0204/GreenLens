#Requires -Version 7.2
# Real native pipes with original synthetic byte/process fixtures. No Radiance,
# annual calculations, production jobs or existing reference workers are touched.
[CmdletBinding()]
param([string]$OutputDirectory='')
$ErrorActionPreference='Stop'
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/paired process regression '+[Guid]::NewGuid().ToString('N')+' 空間')}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Choose a new test directory.'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$testRoot=[IO.Path]::GetFullPath($OutputDirectory)
$fixture=Join-Path $testRoot 'child.ps1'
@'
param([string]$Mode)
$ErrorActionPreference='Stop'
[IO.File]::WriteAllText((Join-Path (Get-Location) "fixture-$Mode.pid"),"$PID")
if($Mode -eq 'echo'){
 $inputStream=[Console]::OpenStandardInput();$outputStream=[Console]::OpenStandardOutput()
 $inputStream.CopyTo($outputStream,65536);$outputStream.Flush();exit 0
}
if($Mode -eq 'nonzero'){exit 7}
if($Mode -eq 'zero-no-input'){exit 0}
if($Mode -eq 'partial-input'){
 $inputStream=[Console]::OpenStandardInput();$bytes=[byte[]]::new(1024)
 $null=$inputStream.Read($bytes,0,$bytes.Length);exit 0
}
if($Mode -eq 'file'){[Console]::Write(('x'*4096));exit 0}
if($Mode -in @('live-stdout','live-scratch')){
 $bytes=[byte[]]::new(131088)
 $writer=if($Mode -eq 'live-stdout'){[Console]::OpenStandardOutput()}else{[IO.FileStream]::new((Join-Path (Get-Location) 'live-open.bin'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::ReadWrite)}
 $writer.Write($bytes,0,$bytes.Length);$writer.Flush()
 [IO.File]::WriteAllText((Join-Path (Get-Location) 'live-write-ready.txt'),"$($bytes.Length)")
 # Keep the handle open beyond the declared limit. The supervisor must detect
 # actual live bytes, not merely reject the file after this process exits.
 Start-Sleep -Seconds 30;$writer.Dispose();exit 0
}
if($Mode -eq 'sleep'){Start-Sleep -Seconds 30;exit 0}
if($Mode -in @('tree','orphan')){
 $info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=(Get-Command pwsh).Source
 $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.WorkingDirectory=(Get-Location).Path
 foreach($arg in @('-NoProfile','-File',$PSCommandPath,'sleep')){$info.ArgumentList.Add($arg)}
 $child=[Diagnostics.Process]::Start($info)
 [IO.File]::WriteAllText((Join-Path (Get-Location) 'grandchild.pid'),"$($child.Id)")
 if($Mode -eq 'orphan'){
  $ready=[Diagnostics.Stopwatch]::StartNew()
  while(!(Test-Path -LiteralPath (Join-Path (Get-Location) 'fixture-sleep.pid'))){if($ready.Elapsed.TotalSeconds -gt 15){throw 'Pipe-holding child did not start'};Start-Sleep -Milliseconds 20}
  $child.Dispose();exit 0
 }
 Start-Sleep -Seconds 30;exit 0
}
throw 'Unknown synthetic process mode.'
'@|Set-Content -LiteralPath $fixture -Encoding utf8NoBOM
$exe=(Get-Command pwsh).Source
$EngineRoot=Join-Path $testRoot 'unused-engine'
$utf8=[Text.UTF8Encoding]::new($false)
$loadedSupervisorHash=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-process.ps1").Hash.ToLowerInvariant()
$loadedNativeHash=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-native-process.cs").Hash.ToLowerInvariant()
. "$PSScriptRoot/annual-paired-process.ps1"
$checkResults=[Collections.Generic.List[object]]::new()

# A non-text payload catches newline/BOM/transcoding damage. Repeated writes keep
# the test's own allocation small, independently of the supervised input reader.
$payload=Join-Path $testRoot 'binary-input.dat'
$stream=[IO.File]::Open($payload,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write)
try{
 $block=[byte[]]::new(65536)
 for($i=0;$i -lt $block.Length;$i++){$block[$i]=[byte](($i*29+17)%256)}
 for($i=0;$i -lt 32;$i++){$stream.Write($block,0,$block.Length)}
}finally{$stream.Dispose()}
$payloadHash=(Get-FileHash -LiteralPath $payload).Hash.ToLowerInvariant()

function Assert-NoFixtureProcesses([string]$Directory){
 foreach($pidFile in Get-ChildItem -LiteralPath $Directory -Filter '*.pid' -File){
  $childId=[int][IO.File]::ReadAllText($pidFile.FullName)
  if(Get-Process -Id $childId -ErrorAction SilentlyContinue){throw 'An owned synthetic fixture process remained alive.'}
 }
}

$OutputDirectory=Join-Path $testRoot 'byte-exact-success'
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$records=[Collections.Generic.List[object]]::new()
$casePayload=Join-Path $OutputDirectory 'input.dat'
Copy-Item -LiteralPath $payload -Destination $casePayload
Invoke-PairedProcess -Executable $exe -Name fixture -Arguments @('-NoProfile','-File',$fixture,'echo') -Output 'raw.bin' -InputFile $casePayload
$record=@(Get-Content -LiteralPath (Join-Path $OutputDirectory 'processes.json') -Raw|ConvertFrom-Json)[-1]
if(!$record.completed -or $record.exit_code -ne 0 -or (Get-Item -LiteralPath (Join-Path $OutputDirectory 'raw.bin')).Length -ne 2MB -or (Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'raw.bin')).Hash.ToLowerInvariant() -ne $payloadHash){throw 'File-streamed native input/output was incomplete or changed bytes.'}
Assert-NoFixtureProcesses $OutputDirectory
$checkResults.Add(@{name='byte-exact 2 MiB native pipe';passed=$true;input_sha256=$payloadHash;output_sha256=$payloadHash;bytes=2MB;process=$record})

$cases=@(
 @{name='nonzero';mode='nonzero';options=@{};input=$payload},
 @{name='zero-incomplete-input';mode='zero-no-input';options=@{};input=$payload},
 @{name='partial-input';mode='partial-input';options=@{};input=$payload},
 @{name='timeout';mode='tree';options=@{TimeoutSeconds=4};input=$payload},
 @{name='memory';mode='sleep';options=@{MemoryLimitBytes=1};input=$null},
 @{name='file';mode='file';options=@{FileLimitBytes=1024};input=$null},
 @{name='scratch';mode='file';options=@{ScratchLimitBytes=1024};input=$null}
 @{name='live-open-stdout-file-cap';mode='live-stdout';options=@{FileLimitBytes=4096;TimeoutSeconds=20};input=$null}
 @{name='live-open-scratch-cap';mode='live-scratch';options=@{ScratchLimitBytes=8192;TimeoutSeconds=20};input=$null}
 @{name='exited-parent-inherited-output';mode='orphan';options=@{TimeoutSeconds=10};input=$null}
 @{name='exited-parent-inherited-stdin-output';mode='orphan';options=@{TimeoutSeconds=10};input=$payload}
)
foreach($case in $cases){
 $OutputDirectory=Join-Path $testRoot $case.name
 New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
 $records=[Collections.Generic.List[object]]::new()
 $options=$case.options;$inputOptions=@{}
 if($case.input){$inputOptions.InputFile=Join-Path $OutputDirectory 'input.dat';Copy-Item -LiteralPath $case.input -Destination $inputOptions.InputFile}
 $rejected=$false
 try{Invoke-PairedProcess -Executable $exe -Name fixture -Arguments @('-NoProfile','-File',$fixture,$case.mode) -Output 'raw.txt' @inputOptions @options}catch{$rejected=$true}
 if(!$rejected){throw "Expected supervised failure: $($case.name)"}
 $record=@(Get-Content -LiteralPath (Join-Path $OutputDirectory 'processes.json') -Raw|ConvertFrom-Json)[-1]
 if($record.completed){throw 'A failed process advertised successful completion.'}
 if($case.name -eq 'timeout' -and !(Test-Path -LiteralPath (Join-Path $OutputDirectory 'grandchild.pid'))){throw 'Tree timeout fixture did not establish its child before cancellation.'}
 if($case.name -like 'live-open-*'){
  if([IO.File]::ReadAllText((Join-Path $OutputDirectory 'live-write-ready.txt')) -ne '131088'){throw 'Live writer did not flush its fixture bytes before detection.'}
  $expectedReason=if($case.mode -eq 'live-stdout'){'generated file exceeded declared bytes'}else{'paired scratch exceeded declared bytes'}
  if($record.terminated_tree_reason -ne $expectedReason -or $record.wall_seconds -ge 15 -or $record.sampled_scratch_peak_bytes -lt 131088 -or $record.exit_code -eq 0){throw 'Live bytes were not detected and killed by the resource cap before timeout or natural exit.'}
 }
 if($case.name -like 'exited-parent-*'){
  if(!(Test-Path -LiteralPath (Join-Path $OutputDirectory 'fixture-sleep.pid')) -or $record.exit_code -ne 0){throw 'Pipe fixture did not establish an exited direct parent with a live child.'}
  if(!$record.job_empty -or !$record.stream_tasks_completed -or $record.wall_seconds -gt 15.5){throw 'Orphaned pipes exceeded deadline/cleanup bound or left unfinished tasks.'}
 }
 Assert-NoFixtureProcesses $OutputDirectory
 $checkResults.Add(@{name=$case.name;passed=$true;process=$record})
 Write-Host "$($case.name): rejected; no completed result or surviving owned process"
}
foreach($name in @('input-open-failure','output-open-failure','input-path-escape','output-path-escape')){
 $OutputDirectory=Join-Path $testRoot $name
 New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
 $records=[Collections.Generic.List[object]]::new()
 $inputPath=Join-Path $OutputDirectory 'input.dat'
 if($name -eq 'input-open-failure'){$inputPath=Join-Path $OutputDirectory 'missing-input.dat'}
 elseif($name -eq 'input-path-escape'){$inputPath=$payload}
 else{Copy-Item -LiteralPath $payload -Destination $inputPath}
 $outputPath=if($name -eq 'output-open-failure'){'missing/raw.txt'}elseif($name -eq 'output-path-escape'){'../escaped-output.txt'}else{'raw.txt'}
 $rejected=$false
 try{Invoke-PairedProcess -Executable $exe -Name fixture -Arguments @('-NoProfile','-File',$fixture,'sleep') -Output $outputPath -InputFile $inputPath}catch{$rejected=$true}
 if(!$rejected -or @(Get-ChildItem -LiteralPath $OutputDirectory -Filter '*.pid' -File).Count){throw 'Invalid input/output path did not fail before starting a child.'}
 $checkResults.Add(@{name=$name;passed=$true;child_started=$false})
}
if(Test-Path -LiteralPath (Join-Path $testRoot 'escaped-output.txt')){throw 'Rejected output escape created a file outside the owned process directory.'}
if((Get-FileHash -LiteralPath $payload).Hash.ToLowerInvariant() -ne $payloadHash){throw 'Input payload changed during streaming.'}
if((Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-process.ps1").Hash.ToLowerInvariant() -cne $loadedSupervisorHash -or (Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-native-process.cs").Hash.ToLowerInvariant() -cne $loadedNativeHash){throw 'Supervision source changed during the fixture run.'}
@{schema_version='greenlens.paired-process-regression.v1';passed=$true;completed_utc=[DateTime]::UtcNow.ToString('o');checks=$checkResults.ToArray();supervisor_sha256=$loadedSupervisorHash;native_process_sha256=$loadedNativeHash;test_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();scope='Native pipe/file/lifecycle/resource-limit regression using original synthetic processes; no Radiance numerical or annual acceptance'}|ConvertTo-Json -Depth 20|Set-Content -LiteralPath (Join-Path $testRoot 'summary.json') -Encoding utf8NoBOM
$global:LASTEXITCODE=0
Write-Host "$($checkResults.Count) paired-process checks passed; retained $testRoot"
