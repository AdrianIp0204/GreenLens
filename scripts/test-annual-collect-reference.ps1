#Requires -Version 7.2
# Synthetic evidence-handling regressions only; no engine/physics acceptance.
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-collect-reference.ps1" -FunctionsOnly
Initialize-ReferenceCollector
$script:passed=0
function Assert-True([bool]$Condition,[string]$Message){if(!$Condition){throw $Message};$script:passed++}
function Reject([scriptblock]$Action,[string]$Pattern){
 $failure=$null;try{& $Action|Out-Null}catch{$failure=$_.Exception.Message}
 if(!$failure -or $failure -notmatch $Pattern){throw "Expected failure /$Pattern/, observed: $failure"};$script:passed++
}
function Clone($Value){
 $options=@{InputObject=(ConvertTo-Json -InputObject $Value -Depth 100);Depth=100}
 if((Get-Command ConvertFrom-Json).Parameters.ContainsKey('DateKind')){$options.DateKind='String'}
 ConvertFrom-Json @options
}
$cacheRoot=Join-Path $env:LOCALAPPDATA 'GreenLens/verification'
$testRoot=Join-Path $cacheRoot ("reference-collector-synthetic-"+[Guid]::NewGuid())
New-Item -ItemType Directory -Path $testRoot|Out-Null
function Write-TestText([string]$Name,[string]$Text){[IO.File]::WriteAllText((Join-Path $testRoot $Name),$Text,[Text.UTF8Encoding]::new($false))}
$sensorText=((0..23|ForEach-Object{"$_ 0 0.8 0 0 1"}) -join "`n")+"`n"
$zero=[double[][]]@(0..23|ForEach-Object{,[double[]]@(0,0,0)})
function New-Attempt([int]$Rounds,[switch]$Drift){
 $raw=[double[]]::new(24*$Rounds*6)
 for($r=0;$r -lt $Rounds;$r++){for($s=0;$s -lt 24;$s++){for($c=0;$c -lt 3;$c++){
  $raw[($r*24+$s)*6+$c]=100+$(if($Drift -and $r -ge 128){100}else{0})
  $raw[($r*24+$s)*6+3+$c]=50
 }}}
 $name="hour-1904-rounds$Rounds"
 Write-TestText "$name.rgb" (($raw|ForEach-Object{$_.ToString('R',[Globalization.CultureInfo]::InvariantCulture)}) -join ' ')
 Write-TestText "$name.pts" ($sensorText*$Rounds)
 $rungs=@(foreach($n in @(($Rounds/4),($Rounds/2),$Rounds)){[GreenLensReference.Aggregate]::Calculate($raw,24,0,[int]$n,100,$zero,$zero)})
 $first=[GreenLensReference.Aggregate]::Compare($rungs[0],$rungs[1]);$second=[GreenLensReference.Aggregate]::Compare($rungs[1],$rungs[2])
 $block1=[GreenLensReference.Aggregate]::Calculate($raw,24,0,($Rounds/2),100,$zero,$zero)
 $block2=[GreenLensReference.Aggregate]::Calculate($raw,24,($Rounds/2),($Rounds/2),100,$zero,$zero)
 $blocks=[GreenLensReference.Aggregate]::CompareWithScale($block1,$block2,$rungs[2])
 return [ordered]@{rounds=$Rounds;rungs=$rungs;first_transition=$first;second_transition=$second;block1=$block1;block2=$block2;block_comparison=$blocks;qualified=(@(@($first)+@($second)+@($blocks)|Where-Object{!$_.passed}).Count -eq 0);raw_sha256=(Reference-Hash $testRoot "$name.rgb");primary_sha256=(Reference-Hash $testRoot "$name.pts")}
}
$attempt=New-Attempt 256
$actual=Get-ReferenceAttempt $testRoot 1904 (Clone $attempt) $zero $zero $sensorText @()
Assert-True ($actual.record.qualified -and $actual.mean.sensors.Count -eq 24) 'Valid raw repeat evidence rejected.'
Assert-True ([Math]::Abs($actual.mean.sensors[0].lux-268.5) -lt 1e-10) 'Simple independently hand-computed neutral lux differs.'
Assert-True ($actual.mean.sensors[23].sensor_id -ceq 's000024' -and $actual.record.block_comparison.Count -eq 24) 'Sensor order/disjoint coverage differs.'
Assert-True (($attempt.rungs.rounds -join ',') -ceq '64,128,256') 'Parenthesized repeat-rung calculation differs.'
$changed=Clone $attempt;$changed.rungs[2].sensors[0].lux+=1
Reject {Get-ReferenceAttempt $testRoot 1904 $changed $zero $zero $sensorText @()} 'Stored rungs'
$changed=Clone $attempt;$changed.qualified=$false
Reject {Get-ReferenceAttempt $testRoot 1904 $changed $zero $zero $sensorText @()} 'Stored qualified'
$changed=Clone $attempt;$changed.raw_sha256='a'*64
Reject {Get-ReferenceAttempt $testRoot 1904 $changed $zero $zero $sensorText @()} 'bytes changed'
$text=[IO.File]::ReadAllText((Join-Path $testRoot 'hour-1904-rounds256.rgb'))
Write-TestText 'hour-1904-rounds256.rgb' '100 100 100'
$changed=Clone $attempt;$changed.raw_sha256=Reference-Hash $testRoot 'hour-1904-rounds256.rgb'
Reject {Get-ReferenceAttempt $testRoot 1904 $changed $zero $zero $sensorText @()} 'Truncated'
Write-TestText 'hour-1904-rounds256.rgb' ($text+' NaN')
$changed.raw_sha256=Reference-Hash $testRoot 'hour-1904-rounds256.rgb'
Reject {Get-ReferenceAttempt $testRoot 1904 $changed $zero $zero $sensorText @()} 'Excess'
Write-TestText 'hour-1904-rounds256.rgb' $text
Write-TestText 'hour-1904-rounds256.pts' (($sensorText.Trim() -split '\n'|Select-Object -Skip 1) -join "`n")
$changed=Clone $attempt;$changed.primary_sha256=Reference-Hash $testRoot 'hour-1904-rounds256.pts'
Reject {Get-ReferenceAttempt $testRoot 1904 $changed $zero $zero $sensorText @()} 'round-major'
Write-TestText 'hour-1904-rounds256.pts' ($sensorText*256)
$drift=New-Attempt 256 -Drift
$failed=Get-ReferenceAttempt $testRoot 1904 (Clone $drift) $zero $zero $sensorText @()
Assert-True (!$failed.record.qualified -and @($failed.record.block_comparison|Where-Object{!$_.passed}).Count -eq 24) 'Failed repeat/disjoint observations were hidden.'
$attempt=New-Attempt 256;$base=Get-ReferenceAttempt $testRoot 1904 (Clone $attempt) $zero $zero $sensorText @()
$escalated=New-Attempt 512
$final=Get-ReferenceAttempt $testRoot 1904 (Clone $escalated) $zero $zero $sensorText $base.raw
Assert-True ($final.mean.rounds -eq 512 -and $final.mean.first_round -eq 0 -and $final.mean.sensors[0].lux -eq $base.mean.sensors[0].lux) '512 prefix was counted twice.'
Reject {Get-ReferenceAttempt $testRoot 1904 $escalated $zero $zero $sensorText @()} 'lacks its retained256'
$badPrefix=[double[]]$base.raw.Clone();$badPrefix[0]+=1
Reject {Get-ReferenceAttempt $testRoot 1904 $escalated $zero $zero $sensorText $badPrefix} 'deterministic prefix'

Write-TestText 'process.out' 'verified output'
$process=[ordered]@{executable='rtrace.exe';arguments=@('-h','scene.oct');exit_code=0;completed=$true;terminated_tree_reason=$null;limits=@{timeout_seconds=7200.0;memory_bytes=2147483648;scratch_bytes=2147483648;file_bytes=536870912};started_utc='2026-10-11T00:00:00Z';finished_utc='2026-10-11T00:00:01Z';wall_seconds=1;sampled_runner_plus_engine_peak_bytes=100;sampled_scratch_peak_bytes=100;output_bytes=15}
Assert-ReferenceProcess $process rtrace @('-h','scene.oct') $testRoot 'process.out';$script:passed++
foreach($field in @('completed','exit_code','output_bytes','terminated_tree_reason')){
 $changed=Clone $process
 switch($field){completed{$changed.completed=$false} exit_code{$changed.exit_code=1} output_bytes{$changed.output_bytes=14} terminated_tree_reason{$changed.terminated_tree_reason='cancelled'}}
 Reject {Assert-ReferenceProcess $changed rtrace @('-h','scene.oct') $testRoot 'process.out'} 'did not complete|output size'
}
$changed=Clone $process;$changed.completed='true'
Reject {Assert-ReferenceProcess $changed rtrace @('-h','scene.oct') $testRoot 'process.out'} 'did not complete'
$changed=Clone $process;$changed.arguments=@('scene.oct','-h')
Reject {Assert-ReferenceProcess $changed rtrace @('-h','scene.oct') $testRoot 'process.out'} 'arguments'
$changed=Clone $process;$changed.limits.memory_bytes=2147483649
Reject {Assert-ReferenceProcess $changed rtrace @('-h','scene.oct') $testRoot 'process.out'} 'resource limits'
foreach($field in @('wall_seconds','sampled_runner_plus_engine_peak_bytes','sampled_scratch_peak_bytes')){
 foreach($invalid in @($null,-1,'0')){
  $changed=Clone $process;$changed.$field=$invalid
  Reject {Assert-ReferenceProcess $changed rtrace @('-h','scene.oct') $testRoot 'process.out'} 'resource measurement'
 }
}
Reject {Read-ReferenceFile $testRoot '../process.out'} 'Unsafe'
Reject {Read-ReferenceFile $testRoot 'process.out' 3} 'bounded regular'
Write-TestText 'identical-sky.mtx' "NROWS=2147483647`nNCOLS=8760`nNCOMP=3`nFORMAT=ascii`n`n"
Reject {Read-ReferenceSky $testRoot} 'bounded578x8760'
Write-TestText 'identical-sky.mtx' "NROWS=578`nNROWS=578`nNCOLS=8760`nNCOMP=3`nFORMAT=ascii`n`n"
Reject {Read-ReferenceSky $testRoot} 'Duplicate'

# Actual upstream-style CRLF source, generated through the harness LF policy.
# This exercises the full hourly source/sky validator without allocating8760columns.
$hourlyIndex=1904;$hourlyName="hour-$hourlyIndex"
$hourlySun="void light solar114270`n0`n0`n3`n1 2 3`nsolar114270 source sun114270`n0`n0`n4`n0 0 1 0.533`n"
$hourlySky=@{values=@{}};for($patch=0;$patch -lt 578;$patch++){for($channel=0;$channel -lt 3;$channel++){$hourlySky.values[($patch*8760+$hourlyIndex)*3+$channel]=1.0}}
Write-TestText "$hourlyName-sun.rad" $hourlySun
Write-TestText "$hourlyName.dat" ("1`n0 577 578`n"+("1`n"*578))
$hourlyScaled="void brightdata distribution`n4 noop $hourlyName.dat matched-basis.cal rbin`n0`n0`ndistribution glow sky_glow`n0`n0`n4 100 100 100 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"+$hourlySun.Replace("1 2 3`n","100 200 300`n")
Write-TestText "$hourlyName-scaled.rad" $hourlyScaled
$hourlyCheck=@{sky_scene_sha256=(Reference-Hash $testRoot "$hourlyName-scaled.rad")}
foreach($originalSun in @($hourlySun,$hourlySun.Replace("`n","`r`n"))){
 $before=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($originalSun)))
 $verified=Assert-ReferenceHourlyInputs $testRoot $hourlyIndex $hourlyCheck $hourlySky $originalSun
 Assert-True ($verified.sun -ceq $hourlySun) 'CRLF source did not follow exact generated LF policy.'
 Assert-True ($before -ceq [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($originalSun)))) 'Original source text changed during verification.'
}
Write-TestText "$hourlyName-sun.rad" $hourlySun.Replace("1 2 3`n","1 2 4`n")
Reject {Assert-ReferenceHourlyInputs $testRoot $hourlyIndex $hourlyCheck $hourlySky $hourlySun.Replace("`n","`r`n")} 'Hourly sun differs'
Write-TestText "$hourlyName-sun.rad" $hourlySun
Reject {Assert-ReferenceHourlyInputs $testRoot $hourlyIndex $hourlyCheck $hourlySky $hourlySun.Replace("`n","`r")} 'Hourly sun differs'

$bins=@(Compare-ReferenceScaleBins $base.mean $base.mean)
Assert-True ($bins.Count -eq 144 -and @($bins|Where-Object{!$_.passed}).Count -eq 0) 'Scale comparison missed a sensor/bin/channel.'
$changed=Clone $base.mean;$changed.sensors[23].full_solar_rgb[2]+=0.01
Assert-True (@(Compare-ReferenceScaleBins $changed $base.mean|Where-Object{!$_.passed}).Count -eq 1) 'Final RGB bin change was not rejected.'

function Test-SyntheticRecovery {
 # Only the old immutable harness-byte proof is stubbed: this synthetic helper
 # tests lineage checks, not authenticity of a real historical engine process.
 $savedHashCheck=${function:Assert-ReferenceHash}
 function Assert-ReferenceHash([string]$Root,[string]$Name,[string]$Expected){
  if($Name -ceq 'recovery-original-harness.ps1' -and $Expected -ceq ('18646902250606280f96eff3325ff47f669772bb1f9f1f7154401f9429ec017f')){return}
  & $savedHashCheck $Root $Name $Expected
 }
 function SaveJson([string]$Name,$Value){Write-TestText $Name (ConvertTo-Json -InputObject $Value -Depth 100)}
 $oldDecl=@{harness_sha256='18646902250606280f96eff3325ff47f669772bb1f9f1f7154401f9429ec017f';declared_utc='2026-10-11T00:00:00Z';executed_interval_indices=@(1904,1908);scientific=@{samples=8192;new_unknown_setting='retained'}}
 $newDecl=Clone $oldDecl;$newDecl.harness_sha256='b'*64;$newDecl.declared_utc='2026-10-11T01:00:00Z'
 SaveJson 'recovery-original-declaration.json' $oldDecl;SaveJson 'declaration.json' $newDecl
 Write-TestText 'recovery-original-harness.ps1' 'MOCK ONLY; does not authenticate historical source'
 Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-recovery.ps1" -Destination (Join-Path $testRoot 'recovery-helper.ps1')
 Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-identity.ps1" -Destination (Join-Path $testRoot 'recovery-identity-helper.ps1')
 $names=@('transmission.oct','hour-1904.oct','hour-1904-sun.oct','hour-1904-direct.rgb','hour-1904-finite.rgb','hour-1904-rounds256.rgb')
 $commands=@(
  @{exe='oconv.exe';args=@('-f','scene.rad','receiver.rad')},
  @{exe='oconv.exe';args=@('-f','scene.rad','hour-1904-scaled.rad')},
  @{exe='oconv.exe';args=@('-f','scene.rad','hour-1904-sun.rad')},
  @{exe='rtrace.exe';args=(@('-I+')+(Get-ReferenceDirectOptions)+@('hour-1904-sun.oct'))},
  @{exe='rtrace.exe';args=((Get-ReferenceDirectOptions)+@('transmission.oct'))},
  @{exe='rcontrib.exe';args=((Get-ReferenceOptions)+@('-y','6144','-m','sky_glow','-m','solar114270','hour-1904.oct'))}
 )
 $records=@();$members=[ordered]@{}
 for($i=0;$i -lt 6;$i++){
  Write-TestText $names[$i] 'synthetic output'
  $record=Clone $process;$record.executable=$commands[$i].exe;$record.arguments=$commands[$i].args;$record.output_bytes=16;$records+=,$record
  $members[$names[$i]]=@{sha256=(Reference-Hash $testRoot $names[$i]);bytes=16}
 }
 Write-TestText 'hour-1904-rounds256.pts' ($sensorText*256)
 $members['hour-1904-rounds256.pts']=@{sha256=(Reference-Hash $testRoot 'hour-1904-rounds256.pts');bytes=(Get-Item -LiteralPath (Join-Path $testRoot 'hour-1904-rounds256.pts')).Length}
 SaveJson 'recovery-original-processes.json' $records;SaveJson 'processes.json' $records
 $declHash=Reference-Hash $testRoot 'declaration.json'
 $manifest=[ordered]@{schema_version='greenlens.controlled-reference-recovery.v1';status='reused_successful_initial_processes';copied_process_count=6;reused_process_count=6;corrected_declaration_sha256=$declHash;original_declaration_sha256=(Reference-Hash $testRoot 'recovery-original-declaration.json');original_harness_sha256=$oldDecl.harness_sha256;original_processes_sha256=(Reference-Hash $testRoot 'recovery-original-processes.json');helper_sha256=(Reference-Hash $testRoot 'recovery-helper.ps1');identity_helper_sha256=(Reference-Hash $testRoot 'recovery-identity-helper.ps1');copied_members=$members}
 SaveJson 'recovery.json' $manifest
 Assert-True ((Assert-ReferenceRecovery $testRoot $newDecl $declHash $sensorText).ready) 'Exact synthetic recovery lineage rejected.'
 $changed=Clone $newDecl;$changed.scientific.samples=4096
 Reject {Assert-ReferenceRecovery $testRoot $changed $declHash $sensorText} 'scientific declaration'
 $changed=Clone $newDecl;$changed.executed_interval_indices=@(1908,1904)
 Reject {Assert-ReferenceRecovery $testRoot $changed $declHash $sensorText} 'scientific declaration'
 $manifest.reused_process_count=5;SaveJson 'recovery.json' $manifest
 Reject {Assert-ReferenceRecovery $testRoot $newDecl $declHash $sensorText} 'status/count'
 $manifest.status='prepared';SaveJson 'recovery.json' $manifest
 Assert-True (!(Assert-ReferenceRecovery $testRoot $newDecl $declHash $sensorText).ready) 'Partial recovery claimed ready.'
 $manifest.status='reused_successful_initial_processes';$manifest.reused_process_count=6;SaveJson 'recovery.json' $manifest
 $changedRecords=Clone $records;$changedRecords[0].finished_utc='2026-10-11T00:00:02Z';SaveJson 'processes.json' $changedRecords
 Reject {Assert-ReferenceRecovery $testRoot $newDecl $declHash $sensorText} 'reused process record'
 SaveJson 'processes.json' $records
 Write-TestText 'hour-1904-direct.rgb' 'tampered evidence'
 Reject {Assert-ReferenceRecovery $testRoot $newDecl $declHash $sensorText} 'bytes changed'
}
Test-SyntheticRecovery

# These explicitly mocked reader outputs exercise only collection qualification;
# native inputs above are never implied, and main readers remain separately used.
$script:mockCases=@(foreach($a in @('A','B')){foreach($i in (Get-ReferenceFixedIntervals)){@{alternative=$a;interval_index=$i;qualified=$true}}})
$script:mockSensitivities=@(@{case_key='B:8512';completed=$true;qualified=$true},@{case_key='A:4116';completed=$true;qualified=$true})
function Assert-ReferenceShardIdentity {return @{declarations=@();common_identity='MOCK ONLY'}}
function Read-ReferenceBaseline {return @{declaration=@{};cases=$script:mockCases;contexts=@(@{key='B:8512'},@{key='A:4116'});source_directory_name='MOCK';declaration_sha256=('0'*64);report_sha256=('1'*64);status='Synthetic reader stub'}}
function Read-ReferenceJson([string]$Root,[string]$Name){return @{baseline_declaration=@{alternative=$Root.Split(':')[0]};interval_index=[int]$Root.Split(':')[1]}}
function Read-ReferenceSensitivity([string]$Root,$Context){return @($script:mockSensitivities|Where-Object{$_.case_key -ceq $Context.key})[0]}
$collection=Collect-ReferenceEvidence @('mock') @('B:8512','A:4116')
Assert-True ($collection.qualified -and $collection.coverage.completed_case_count -eq 24 -and !$collection.release_acceptance_complete) 'Complete synthetic coverage did not enforce bounded scope.'
$collection=Collect-ReferenceEvidence @('mock') @()
Assert-True (!$collection.qualified -and $collection.missing_sensitivity_case_keys.Count -eq 2) 'Missing sensitivities qualified.'
$script:mockSensitivities[0].qualified=$false
Assert-True (!(Collect-ReferenceEvidence @('mock') @('B:8512','A:4116')).qualified) 'Failed sensitivity qualified.'
$script:mockSensitivities[0].qualified=$true;$script:mockCases[0].qualified=$false
Assert-True (!(Collect-ReferenceEvidence @('mock') @('B:8512','A:4116')).qualified) 'Failed baseline qualified.'
$script:mockCases=$script:mockCases[1..23]
Assert-True (!(Collect-ReferenceEvidence @('mock') @('B:8512','A:4116')).qualified) 'Partial cases qualified.'
$script:mockCases+=,$script:mockCases[0]
Reject {Collect-ReferenceEvidence @('mock') @()} 'Duplicate'
$script:mockCases=@()
Reject {Collect-ReferenceEvidence @('mock') @('B:8512','B:8512')} 'duplicate fixed'
$failureDirectory=Join-Path $testRoot 'failure-wrapper'
$wrapperLog=Join-Path $testRoot 'wrapper.log'
& pwsh -NoProfile -File "$PSScriptRoot/annual-collect-reference.ps1" -BaselineDirectories (Join-Path $testRoot 'missing-baseline') -OutputDirectory $failureDirectory *> $wrapperLog
Assert-True ($LASTEXITCODE -ne 0) 'Malformed evidence wrapper exited successfully.'
$failure=Get-Content -LiteralPath (Join-Path $failureDirectory 'collection-failure.json') -Raw|ConvertFrom-Json
Assert-True (!$failure.qualified -and !$failure.release_acceptance_complete -and !(Test-Path -LiteralPath (Join-Path $failureDirectory 'reference-collection.json'))) 'Wrapper failure advertised a completed collection.'
Write-Host "Collector synthetic regressions: $script:passed passed. No native/scientific acceptance claim. Retained test artifacts: $testRoot"
