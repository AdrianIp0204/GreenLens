#Requires -Version 7.2
# Scaled-cap process supervision tests. These are lifecycle tests, not physics.
$ErrorActionPreference='Stop'
$parent=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/reference process regression '+[Guid]::NewGuid().ToString('N')+' 空間')
New-Item -ItemType Directory -Path $parent|Out-Null
$fixture=Join-Path $parent 'child.ps1'
@'
param([string]$Mode)
[IO.File]::WriteAllText((Join-Path (Get-Location) 'child.pid'),"$PID")
if($Mode -eq 'nonzero'){exit 7}
if($Mode -eq 'zero-no-input'){exit 0}
if($Mode -eq 'file'){[Console]::Write(('x'*4096));exit 0}
if($Mode -eq 'sleep'){Start-Sleep -Seconds 30;exit 0}
throw 'Unknown regression fixture'
'@|Set-Content -LiteralPath $fixture -Encoding utf8
$exe=(Get-Command pwsh).Source
$cases=@(
 @{name='nonzero';mode='nonzero';options=@{};inputText=('0'*2000000)},
 @{name='zero-incomplete-input';mode='zero-no-input';options=@{};inputText=('0'*2000000)},
 @{name='timeout';mode='sleep';options=@{TimeoutSeconds=.25};inputText=('0'*2000000)},
 @{name='memory';mode='sleep';options=@{MemoryLimitBytes=1};inputText=''},
 @{name='file';mode='file';options=@{FileLimitBytes=1024};inputText=''},
 @{name='scratch';mode='file';options=@{ScratchLimitBytes=1024};inputText=''}
)
foreach($case in $cases){
 $directory=Join-Path $parent $case.name
 . "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $directory -FunctionsOnly
 . "$PSScriptRoot/annual-reference-process.ps1"
 $rejected=$false;$options=$case.options
 try{Invoke-ReferenceProcess -Executable $exe -Name fixture -Arguments @('-NoProfile','-File',$fixture,$case.mode) -Output 'raw.txt' -InputText $case.inputText @options}catch{$rejected=$true}
 if(!$rejected){throw "Expected process failure: $($case.name)"}
 $record=@(Get-Content -LiteralPath (Join-Path $directory 'processes.json') -Raw|ConvertFrom-Json)[-1]
 if($record.completed){throw 'Failure advertised successful completion.'}
 $pidFile=Join-Path $directory 'child.pid';if(Test-Path -LiteralPath $pidFile){$childId=[int](Get-Content -LiteralPath $pidFile);if(Get-Process -Id $childId -ErrorAction SilentlyContinue){throw 'Supervised child remained alive.'}}
 Write-Host "$($case.name): correctly rejected; no partial success"
}
$directory=Join-Path $parent 'output-open-failure'
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $directory -FunctionsOnly
. "$PSScriptRoot/annual-reference-process.ps1"
$rejected=$false
try{Invoke-ReferenceProcess -Executable $exe -Name fixture -Arguments @('-NoProfile','-File',$fixture,'sleep') -Output 'missing/raw.txt'}catch{$rejected=$true}
if(!$rejected -or (Test-Path -LiteralPath (Join-Path $directory 'child.pid'))){throw 'Output-open failure started a child.'}
Write-Host "Seven bounded lifecycle regressions passed; retained $parent"
