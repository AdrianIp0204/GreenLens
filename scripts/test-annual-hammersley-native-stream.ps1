#Requires -Version 7.2
param([string]$OutputDirectory='')
$ErrorActionPreference='Stop'
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/Hammersley native stream '+[Guid]::NewGuid().ToString('N')+' '+[char]0x7a7a+[char]0x9593)}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Choose a new retained directory'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$testRoot=$OutputDirectory;$EngineRoot=Join-Path $testRoot 'unused-engine'
. "$PSScriptRoot/annual-paired-process.ps1"
Add-Type -LiteralPath "$PSScriptRoot/annual-paired-primary-stream.cs"
Add-Type -TypeDefinition @'
using System; using System.IO; using System.Threading; using System.Threading.Tasks;
public sealed class FailingPairedProducer {
 public Task GenerateAsync(Stream output,CancellationToken cancel) {
  return Task.Run(async()=> { await output.WriteAsync(new byte[]{49,32,50,10},cancel); throw new InvalidDataException("Declared synthetic generator failure"); },cancel);
 }
}
'@
$fixture=Join-Path $testRoot 'child.ps1'
@'
param([string]$Mode)
[IO.File]::WriteAllText((Join-Path (Get-Location) 'child.pid'),"$PID")
if($Mode -eq 'echo'){$inputStream=[Console]::OpenStandardInput();$outputStream=[Console]::OpenStandardOutput();$inputStream.CopyTo($outputStream,65536);$outputStream.Flush();exit 0}
if($Mode -eq 'sleep'){Start-Sleep -Seconds 30;exit 0}
throw 'Unknown fixture mode'
'@|Set-Content -LiteralPath $fixture -Encoding utf8NoBOM
$exe=(Get-Command pwsh).Source
$producer=[GreenLensPairedStream.HammersleyProducer]::new("1 2 0.8 0 0 1`n3 4 0.9 0 0 1`n",16384)
$checks=[Collections.Generic.List[object]]::new()
$OutputDirectory=Join-Path $testRoot 'complete';New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$records=[Collections.Generic.List[object]]::new()
$receipt=Invoke-PairedProcess -Executable $exe -Name fixture -Arguments @('-NoProfile','-File',$fixture,'echo') -Output 'native-rays.pts' -InputProducer $producer
$path=Join-Path $OutputDirectory 'native-rays.pts';$actualHash=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()
if($receipt.Rays -ne 32768 -or $receipt.Bytes -ne (Get-Item -LiteralPath $path).Length -or $receipt.Sha256 -cne $actualHash -or !$records[0].completed -or !$records[0].job_empty){throw 'Generated native ASCII stream differs from receipt or incomplete job'}
$checks.Add(@{name='complete generated native ASCII bytes';passed=$true;receipt=$receipt;process=$records[0]})
foreach($mode in @('timeout','generator-failure')){
 $OutputDirectory=Join-Path $testRoot $mode;New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
 $records=[Collections.Generic.List[object]]::new();$rejected=$false
 $inputProducer=if($mode -eq 'timeout'){$producer}else{[FailingPairedProducer]::new()}
 $childMode=if($mode -eq 'timeout'){'sleep'}else{'echo'}
 $watch=[Diagnostics.Stopwatch]::StartNew()
 try{$null=Invoke-PairedProcess -Executable $exe -Name fixture -Arguments @('-NoProfile','-File',$fixture,$childMode) -Output 'partial.pts' -InputProducer $inputProducer -TimeoutSeconds 4}catch{$rejected=$true}
 if(!$rejected -or $records.Count -ne 1 -or $records[0].completed -or !$records[0].job_empty -or !$records[0].stream_tasks_completed -or $watch.Elapsed.TotalSeconds -gt 10){throw 'Failed generated stream was not rejected and terminated within bound'}
 if($null -ne $records[0].input_receipt -or $null -ne $records[0].input_bytes){throw 'A failed producer advertised a complete byte receipt'}
 $pidPath=Join-Path $OutputDirectory 'child.pid';if(Test-Path -LiteralPath $pidPath){if(Get-Process -Id ([int][IO.File]::ReadAllText($pidPath)) -ErrorAction SilentlyContinue){throw 'Owned generated-stream child survived'}}
 $checks.Add(@{name=$mode;passed=$true;process=$records[0];observed_seconds=$watch.Elapsed.TotalSeconds})
}
@{schema_version='greenlens.hammersley-native-stream-regression.v1';passed=$true;completed_utc=[DateTime]::UtcNow.ToString('o');checks=$checks.ToArray();producer_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-primary-stream.cs").Hash.ToLowerInvariant();supervisor_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-process.ps1").Hash.ToLowerInvariant();native_process_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-native-process.cs").Hash.ToLowerInvariant();test_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();scope='Synthetic native stream/producer lifecycle; no Radiance numerical acceptance'}|ConvertTo-Json -Depth 14|Set-Content -LiteralPath (Join-Path $testRoot 'summary.json') -Encoding utf8NoBOM
Write-Host "Three generated native stream checks passed; retained $testRoot"
