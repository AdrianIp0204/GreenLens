#Requires -Version 7.2
# Entirely synthetic process/evidence fixtures. The historical harness is copied
# as pinned bytes, never executed. No Radiance or existing reference run is used.
[CmdletBinding()]
param([string]$OutputDirectory='', [string]$OriginalHarnessPath='')
$ErrorActionPreference='Stop'
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/reference recovery regression '+[Guid]::NewGuid().ToString('N')+' 空間')}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Choose a new synthetic test directory.'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$testRoot=[IO.Path]::GetFullPath($OutputDirectory)
$utf8=[Text.UTF8Encoding]::new($false)
$oldHarnessSha='18646902250606280f96eff3325ff47f669772bb1f9f1f7154401f9429ec017f'
$oldCommit='8e7fe4bc95c07856f89ba008fdd4d6cc751b1615'

if($OriginalHarnessPath){
 $oldHarnessBytes=[IO.File]::ReadAllBytes([IO.Path]::GetFullPath($OriginalHarnessPath))
}else{
 # Git's raw stdout avoids PowerShell newline/BOM conversion. A shallow clone
 # without this historical object must fetch it or supply exact retained bytes.
 $info=[Diagnostics.ProcessStartInfo]::new()
 $info.FileName=(Get-Command git -ErrorAction Stop).Source
 $info.UseShellExecute=$false;$info.CreateNoWindow=$true
 $info.WorkingDirectory=(Resolve-Path "$PSScriptRoot/..").Path
 $info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 $info.ArgumentList.Add('show');$info.ArgumentList.Add("${oldCommit}:scripts/annual-controlled-reference.ps1")
 $process=[Diagnostics.Process]::Start($info)
 $memory=[IO.MemoryStream]::new()
 try{
  $stderr=$process.StandardError.ReadToEndAsync()
  $process.StandardOutput.BaseStream.CopyTo($memory)
  $process.WaitForExit()
  if($process.ExitCode -ne 0){throw 'Pinned historical Git object unavailable; supply -OriginalHarnessPath with the exact retained old harness.'}
  $oldHarnessBytes=$memory.ToArray()
 }finally{$memory.Dispose();$process.Dispose()}
}
$actualOldSha=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($oldHarnessBytes)).ToLowerInvariant()
if($actualOldSha -cne $oldHarnessSha){throw 'Historical fixture harness bytes differ from the explicitly recoverable version.'}

Add-Type -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs"
. "$PSScriptRoot/annual-reference-recovery.ps1"
$EngineRoot=Join-Path $testRoot 'NO-ENGINE-EXISTS'
$script:freshInvocations=0
function Invoke-ReferenceProcess {
 param($Executable,$Name,$Arguments,$Output,$InputText)
 $script:freshInvocations++
 throw 'Synthetic sentinel: fresh native engine execution was requested.'
}
function Assert([bool]$Condition,[string]$Message){if(!$Condition){throw $Message}}
function Write-Json([string]$Path,$Value){[IO.File]::WriteAllText($Path,(ConvertTo-Json -InputObject $Value -Depth 100),$utf8)}
function Hash([string]$Path){return (Get-FileHash -LiteralPath $Path).Hash.ToLowerInvariant()}
function Read-Json([string]$Path){return Read-ReferenceRecoveryJson $Path}
$checks=[Collections.Generic.List[object]]::new()

$template=Join-Path $testRoot 'synthetic-original-template'
New-Item -ItemType Directory -Path $template|Out-Null
[IO.File]::WriteAllBytes((Join-Path $template 'harness-source.ps1'),$oldHarnessBytes)
$sensorText="0 0 0.8 0 0 1`n"*24
$primaryText=$sensorText*256
$inputHashes=[ordered]@{}
foreach($name in @('scene.rad','sensors.pts','identical-sky.mtx','suns.rad','weather.epw','source-recipe.json','model.json','study.json','weather-asset.json')){
 $content=if($name -eq 'sensors.pts'){$sensorText}else{"SYNTHETIC TEST ONLY: $name`n"}
 [IO.File]::WriteAllText((Join-Path $template $name),$content,$utf8)
 $inputHashes[$name]=Hash (Join-Path $template $name)
}
$full=@('-I+','-h','-faa','-n','1','-ab','9','-ad','8192','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-c','1','-V+')
$direct=@('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$calls=@(
 @{name='oconv';arguments=@('-f','scene.rad','receiver.rad');output='transmission.oct';input=''},
 @{name='oconv';arguments=@('-f','scene.rad','hour-8512-scaled.rad');output='hour-8512.oct';input=''},
 @{name='oconv';arguments=@('-f','scene.rad','hour-8512-sun.rad');output='hour-8512-sun.oct';input=''},
 @{name='rtrace';arguments=(@('-I+')+$direct+@('hour-8512-sun.oct'));output='hour-8512-direct.rgb';input=$sensorText},
 @{name='rtrace';arguments=($direct+@('transmission.oct'));output='hour-8512-finite.rgb';input='synthetic finite ray input'},
 @{name='rcontrib';arguments=($full+@('-y','6144','-m','sky_glow','-m','solar510750','hour-8512.oct'));output='hour-8512-rounds256.rgb';input=$primaryText}
)
$originalRecords=[Collections.Generic.List[object]]::new()
foreach($call in $calls){
 $text=if($call.output -eq 'hour-8512-rounds256.rgb'){"1 2 3 4 5 6`n"*(24*256)}else{"SYNTHETIC PROCESS OUTPUT: $($call.output)`n"}
 [IO.File]::WriteAllText((Join-Path $template $call.output),$text,$utf8)
 $originalRecords.Add([ordered]@{
  executable="$($call.name).exe";arguments=$call.arguments;completed=$true;exit_code=0;terminated_tree_reason=$null
  started_utc='2026-01-01T00:00:00.0000000Z';finished_utc='2026-01-01T00:00:01.0000000Z';wall_seconds=1.0
  sampled_runner_plus_engine_peak_bytes=1048576;sampled_peak_working_set_bytes=524288;sampled_scratch_peak_bytes=1048576
  output_bytes=([IO.FileInfo](Join-Path $template $call.output)).Length
  limits=@{timeout_seconds=7200;memory_bytes=2147483648;scratch_bytes=2147483648;file_bytes=536870912}
 })
}
[IO.File]::WriteAllText((Join-Path $template 'hour-8512-rounds256.pts'),$primaryText,$utf8)
Write-Json (Join-Path $template 'processes.json') $originalRecords.ToArray()
$originalDeclaration=[ordered]@{
 schema_version='greenlens.controlled-reference.v1';scope='SYNTHETIC recovery regression; no scientific results';declared_utc='2026-01-01T00:00:00.0000000Z'
 alternative='B';declared_interval_indices=@(1904,1908,1912,4112,4116,4120,6320,6324,6328,8504,8508,8512);executed_interval_indices=@(8512,8504,8508)
 sensor_count=24;input_sha256=$inputHashes;engine_sha256=@{'rcontrib.exe'=('1'*64)};source_scale=100
 ambient_divisions=8192;ambient_bounces=9;ray_depth=-16;ambient_weight=1e-9;initial_rungs=@(64,128,256);optional_final_rung=512
 full_arguments=$full;direct_arguments=$direct;finite_samples=4096;harness_sha256=$oldHarnessSha
 process_helper_sha256=(Hash "$PSScriptRoot/annual-reference-process.ps1");aggregation_sha256=(Hash "$PSScriptRoot/annual-reference-aggregation.cs")
 maximum_process_seconds=7200;maximum_scratch_bytes=2147483648;maximum_single_file_bytes=536870912;maximum_runner_plus_engine_working_set_bytes=2147483648
 normalized_model=@{synthetic=$true};weather_binding=@{synthetic=$true};sensitivities=@{fixed_cases=@('B:8512','A:4116')};release_acceptance_complete=$false
}
Write-Json (Join-Path $template 'declaration.json') $originalDeclaration
$templateHashes=@{}
foreach($item in Get-ChildItem -LiteralPath $template -File){$templateHashes[$item.Name]=Hash $item.FullName}

function New-Fixture([string]$Name){
 $caseRoot=Join-Path $testRoot $Name
 $source=Join-Path $caseRoot 'original';$destination=Join-Path $caseRoot 'recovered'
 New-Item -ItemType Directory -Path $source,$destination|Out-Null
 foreach($file in Get-ChildItem -LiteralPath $template -File){Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $source $file.Name)}
 foreach($name in $inputHashes.Keys){Copy-Item -LiteralPath (Join-Path $template $name) -Destination (Join-Path $destination $name)}
 $declaration=Copy-ReferenceJsonObject (Read-Json (Join-Path $source 'declaration.json'))
 $declaration.declared_utc='2026-01-02T00:00:00.0000000+00:00'
 $declaration.harness_sha256=Hash "$PSScriptRoot/annual-controlled-reference.ps1"
 Write-Json (Join-Path $destination 'declaration.json') $declaration
 return @{source=$source;destination=$destination;declaration=$declaration}
}
function Begin-Fixture($Fixture){
 $script:referenceRecovery=$null
 $script:OutputDirectory=$Fixture.destination
 $script:records=[Collections.Generic.List[object]]::new()
 Initialize-ReferenceRecovery $Fixture.source $Fixture.declaration
}
function Reuse-Six {
 foreach($call in $calls){Invoke-RecoverableReferenceEngine $call.name $call.arguments $call.output $call.input}
}
function Reject([string]$Name,[scriptblock]$Mutation,[scriptblock]$Attempt){
 $fixture=New-Fixture $Name
 & $Mutation $fixture
 $failed=$false;$message=''
 try{& $Attempt $fixture}catch{$failed=$true;$message=$_.Exception.Message}
 Assert $failed "Recovery unexpectedly accepted $Name."
 Assert ($script:freshInvocations -eq 0) 'A rejected recovery invoked the fresh-process sentinel.'
 $stateFile=Join-Path $fixture.destination 'recovery.json'
 if(Test-Path -LiteralPath $stateFile){Assert ((Read-Json $stateFile).reused_process_count -lt 6) 'Rejected recovery was advertised as complete.'}
 $checks.Add(@{name=$Name;passed=$true;expected_rejection=$message})
}

$valid=New-Fixture 'exact-six-reuse'
Begin-Fixture $valid
Reuse-Six
$result=Read-Json (Join-Path $valid.destination 'recovery.json')
Assert ($result.status -ceq 'reused_successful_initial_processes' -and $result.reused_process_count -eq 6 -and !$result.release_acceptance_complete) 'Six-process recovery state differs.'
Assert ($records.Count -eq 6 -and $script:freshInvocations -eq 0) 'Recovery reran an engine or lost original process records.'
Assert ((ConvertTo-ReferenceCanonicalJson $records.ToArray()) -ceq (ConvertTo-ReferenceCanonicalJson (Read-Json (Join-Path $valid.source 'processes.json')))) 'Original process identities/timestamps changed.'
foreach($file in Get-ChildItem -LiteralPath $valid.source -File){Assert ((Hash $file.FullName) -ceq $templateHashes[$file.Name]) 'Recovery changed original evidence.'}
foreach($name in @($calls.output)+@('hour-8512-rounds256.pts')){Assert ((Hash (Join-Path $valid.destination $name)) -ceq $templateHashes[$name]) 'A copied member differs.'}
$checks.Add(@{name='exactly-six-reused-without-engine-or-original-mutation';passed=$true;records=6;fresh_process_calls=0})

Reject 'argv-mismatch' {param($f)} {param($f) Begin-Fixture $f;Invoke-RecoverableReferenceEngine 'oconv' @('-f','wrong.rad','receiver.rad') 'transmission.oct'}
Reject 'output-order-mismatch' {param($f)} {param($f) Begin-Fixture $f;Invoke-RecoverableReferenceEngine 'oconv' $calls[0].arguments 'hour-8512.oct'}
Reject 'executable-mismatch' {param($f)} {param($f) Begin-Fixture $f;Invoke-RecoverableReferenceEngine 'rtrace' $calls[0].arguments 'transmission.oct'}
Reject 'output-hash-mismatch' {param($f)} {param($f) Begin-Fixture $f;[IO.File]::AppendAllText((Join-Path $f.destination 'transmission.oct'),'x');Reuse-Six}
Reject 'original-output-size-mismatch' {param($f) $r=@(Read-Json (Join-Path $f.source 'processes.json'));$r[0].output_bytes++;Write-Json (Join-Path $f.source 'processes.json') $r} {param($f) Begin-Fixture $f;Reuse-Six}
Reject 'unique-sensor-input-mismatch' {param($f)} {param($f) Begin-Fixture $f;foreach($c in $calls[0..2]){Invoke-RecoverableReferenceEngine $c.name $c.arguments $c.output $c.input};Invoke-RecoverableReferenceEngine $calls[3].name $calls[3].arguments $calls[3].output ($sensorText+' ')}
Reject 'round-major-input-mismatch' {param($f)} {param($f) Begin-Fixture $f;foreach($c in $calls[0..4]){Invoke-RecoverableReferenceEngine $c.name $c.arguments $c.output $c.input};Invoke-RecoverableReferenceEngine $calls[5].name $calls[5].arguments $calls[5].output ($primaryText+' ')}
Reject 'source-input-hash-mismatch' {param($f) [IO.File]::AppendAllText((Join-Path $f.source 'scene.rad'),'x')} {param($f) Begin-Fixture $f}
Reject 'current-input-hash-mismatch' {param($f) [IO.File]::AppendAllText((Join-Path $f.destination 'scene.rad'),'x')} {param($f) Begin-Fixture $f}
Reject 'old-harness-hash-mismatch' {param($f) [IO.File]::AppendAllText((Join-Path $f.source 'harness-source.ps1'),' ')} {param($f) Begin-Fixture $f}
Reject 'old-declared-harness-mismatch' {param($f) $d=Read-Json (Join-Path $f.source 'declaration.json');$d.harness_sha256='0'*64;Write-Json (Join-Path $f.source 'declaration.json') $d} {param($f) Begin-Fixture $f}
Reject 'scientific-declaration-mismatch' {param($f) $f.declaration.ambient_divisions=8193} {param($f) Begin-Fixture $f}
Reject 'unknown-declaration-field-mismatch' {param($f) $f.declaration['unknown_extra']='must not be ignored'} {param($f) Begin-Fixture $f}
Reject 'executed-interval-order-mismatch' {param($f) $f.declaration.executed_interval_indices=@(8504,8512,8508)} {param($f) Begin-Fixture $f}
Reject 'fewer-than-six-processes' {param($f) $r=@(Read-Json (Join-Path $f.source 'processes.json'));Write-Json (Join-Path $f.source 'processes.json') $r[0..4]} {param($f) Begin-Fixture $f}
Reject 'more-than-six-processes' {param($f) $r=@(Read-Json (Join-Path $f.source 'processes.json'));Write-Json (Join-Path $f.source 'processes.json') ($r+@($r[0]))} {param($f) Begin-Fixture $f}
Reject 'already-reported-case' {param($f) Write-Json (Join-Path $f.source 'controlled-reference.json') @{completed=$false}} {param($f) Begin-Fixture $f}
Reject 'same-source-and-destination' {param($f) $f.destination=$f.source} {param($f) Begin-Fixture $f}

foreach($case in @(
 @{name='unsuccessful-exit';field='exit_code';value=7},
 @{name='incomplete-process';field='completed';value=$false},
 @{name='string-completed';field='completed';value='true'},
 @{name='terminated-process';field='terminated_tree_reason';value='timeout'},
 @{name='negative-wall-time';field='wall_seconds';value=-1},
 @{name='excess-wall-time';field='wall_seconds';value=7201},
 @{name='missing-memory-measurement';field='sampled_runner_plus_engine_peak_bytes';value=$null},
 @{name='string-memory-measurement';field='sampled_runner_plus_engine_peak_bytes';value='1024'},
 @{name='excess-memory';field='sampled_runner_plus_engine_peak_bytes';value=2147483649},
 @{name='negative-scratch';field='sampled_scratch_peak_bytes';value=-1},
 @{name='excess-scratch';field='sampled_scratch_peak_bytes';value=2147483649},
 @{name='negative-child-memory';field='sampled_peak_working_set_bytes';value=-1},
 @{name='non-utc-start';field='started_utc';value='2026-01-01T08:00:00+08:00'},
 @{name='finish-before-start';field='finished_utc';value='2025-12-31T23:59:59Z'}
)){
 Reject $case.name {param($f) $r=@(Read-Json (Join-Path $f.source 'processes.json'));$r[0].($case.field)=$case.value;Write-Json (Join-Path $f.source 'processes.json') $r} {param($f) Begin-Fixture $f;Reuse-Six}
}
foreach($field in @('timeout_seconds','memory_bytes','scratch_bytes','file_bytes')){
 Reject "changed-limit-$field" {param($f) $r=@(Read-Json (Join-Path $f.source 'processes.json'));$r[0].limits.$field++;Write-Json (Join-Path $f.source 'processes.json') $r} {param($f) Begin-Fixture $f;Reuse-Six}
}
Reject 'truncated-original-primary-output' {param($f)
 $p=Join-Path $f.source 'hour-8512-rounds256.rgb';[IO.File]::WriteAllText($p,'1 2 3',$utf8)
 $r=@(Read-Json (Join-Path $f.source 'processes.json'));$r[5].output_bytes=([IO.FileInfo]$p).Length;Write-Json (Join-Path $f.source 'processes.json') $r
} {param($f) Begin-Fixture $f;Reuse-Six}

# Exercise the precise expression in the corrected trusted harness, not a
# hand-copied successful expression. The old expression must reproduce failure.
$harness=[IO.File]::ReadAllText("$PSScriptRoot/annual-controlled-reference.ps1")
$expression=[regex]::Match($harness,'foreach\(\$n in (?<rungs>@\(\(\$rounds/4\),\(\$rounds/2\),\$rounds\))\)').Groups['rungs'].Value
Assert ($expression.Length -gt 0) 'Trusted harness does not contain the reviewed corrected rung expression.'
$oldText=$utf8.GetString($oldHarnessBytes)
Assert ($oldText.Contains('@($rounds/4,$rounds/2,$rounds)')) 'Pinned old harness does not contain the identified reporting defect.'
$oldFailed=$false;$rounds=256
try{$null=@($rounds/4,$rounds/2,$rounds)}catch{$oldFailed=$true}
Assert $oldFailed 'Old PowerShell expression no longer reproduces the declared defect.'
foreach($rounds in @(256,512)){
 $rungs=@(& ([scriptblock]::Create($expression)))
 $expected=if($rounds -eq 256){'64,128,256'}else{'128,256,512'}
 Assert (($rungs -join ',') -ceq $expected) 'Corrected nested-mean rungs differ.'
 $checks.Add(@{name="trusted-harness-rungs-$rounds";passed=$true;rungs=$rungs;old_expression_rejected=$true})
}
Assert ($harness.Contains('Invoke-RecoverableReferenceEngine $Name $Arguments $Output $InputText')) 'Controlled-reference hook does not forward its trusted command arguments.'
Assert ($script:freshInvocations -eq 0) 'A synthetic recovery test attempted a native engine call.'
foreach($file in Get-ChildItem -LiteralPath $template -File){Assert ((Hash $file.FullName) -ceq $templateHashes[$file.Name]) 'Synthetic original template was modified.'}
$summary=[ordered]@{
 schema_version='greenlens.reference-recovery-regression.v1';scope='Synthetic evidence and process records only; pinned old harness copied as bytes, never executed; no Radiance, old workers, or scientific acceptance'
 passed=$true;checks=$checks.ToArray();check_count=$checks.Count;fresh_engine_invocations=$script:freshInvocations;historical_harness_sha256=$oldHarnessSha
 recovery_helper_sha256=(Hash "$PSScriptRoot/annual-reference-recovery.ps1");controlled_harness_sha256=(Hash "$PSScriptRoot/annual-controlled-reference.ps1")
 test_sha256=(Hash $PSCommandPath);completed_utc=[DateTime]::UtcNow.ToString('o');release_acceptance_complete=$false
}
Write-Json (Join-Path $testRoot 'summary.json') $summary
Write-Host "Reference recovery regression passed: $($checks.Count) synthetic checks; zero engine invocations. Retained $testRoot"
