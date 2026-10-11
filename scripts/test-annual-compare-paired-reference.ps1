#Requires -Version 7.5
# Explicitly synthetic retained-data checks. No engine, native process or
# scientific qualification is executed or inferred from this test.
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-compare-paired-reference.ps1" -FunctionsOnly
Initialize-PairedComparator
$passed=0
function Assert-Test([bool]$Value,[string]$Message){if(!$Value){throw $Message};$script:passed++}
function Reject-Test([scriptblock]$Action,[string]$Pattern){
 $failure=$null;try{& $Action|Out-Null}catch{$failure=$_.Exception.Message}
 if(!$failure -or $failure -notmatch $Pattern){throw "Expected /$Pattern/; observed: $failure"};$script:passed++
}
function Copy-Test($Value){ConvertTo-Json -InputObject $Value -Depth 100|ConvertFrom-Json -Depth 100 -DateKind String}
$root=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/paired-comparator-synthetic-'+[Guid]::NewGuid().ToString('N')+' 空間')
New-Item -ItemType Directory -Path $root|Out-Null
function Write-Test([string]$Name,[string]$Text){[IO.File]::WriteAllText((Join-Path $root $Name),$Text,[Text.UTF8Encoding]::new($false))}
Reject-Test {Get-PairedTuple 'annual'} 'Unknown paired profile'
Reject-Test {Get-PairedTuple 'Candidate'} 'Unknown paired profile'
Assert-Test ((Get-PairedTuple candidate).primary -eq 16384 -and (Get-PairedTuple higher).finite -eq 4096) 'Frozen profile tuple differs.'
Assert-Test ((Get-PairedArguments candidate) -notcontains '-I+' -and (Get-PairedArguments higher) -contains '65536') 'Radiance query/average mode differs.'

# A deliberately non-gray matrix catches swapped channels, transposed time,
# duplicated patches and a second division by the number of primary samples.
$a=[pscustomobject]@{rows=1;columns=2;components=3;values=[double[]]@(1,2,3,4,5,6)}
$sky=[pscustomobject]@{rows=2;columns=2;components=3;values=[double[]]@(2,3,4,5,6,7,8,9,10,11,12,13)}
$product=[pscustomobject]@{rows=1;columns=2;components=3;values=[double[]]@(34,51,72,49,72,99)}
$result=Assert-IndependentRgbProduct -Coefficients $a -Sky $sky -Product $product
Assert-Test ($result.passed -and $result.component_checks -eq 6) 'Independent non-gray multiplication failed.'
$bad=Copy-Test $product;$bad.values[0]+=1
Reject-Test {Assert-IndependentRgbProduct $a $sky $bad} 'multiplication failed'
$bad=Copy-Test $product;$bad.values[1]=[double]::NaN
Reject-Test {Assert-IndependentRgbProduct $a $sky $bad} 'invalid output'
$bad=Copy-Test $product;$bad.values[0]/=16384
Reject-Test {Assert-IndependentRgbProduct $a $sky $bad} 'multiplication failed'
$year=[pscustomobject]@{rows=1;columns=3;components=3;values=[double[]]@(1,2,3,4,5,6,7,8,9)}
$selected=[pscustomobject]@{rows=1;columns=2;components=3;values=[double[]]@(7,8,9,1,2,3)}
Assert-PairedSelectedColumns $year $selected @(2,0);$passed++
Reject-Test {Assert-PairedSelectedColumns $year $selected @(2,2)} 'duplicate selected'
Reject-Test {Assert-PairedSelectedColumns $year $selected @(0,2)} 'different annual RGB'

$text="#?RADIANCE`nNROWS=1`nNCOLS=2`nNCOMP=3`nFORMAT=ascii`n`n1 2 3 4 5 6`n"
Write-Test 'matrix.mtx' $text
$matrix=Read-PairedMatrix $root 'matrix.mtx' 1 2
Assert-Test ($matrix.values.Length -eq 6 -and $matrix.values[5] -eq 6) 'Bounded matrix reader failed.'
Write-Test 'matrix.mtx' ($text.Replace('1 2 3 4 5 6','1 2 3'))
Reject-Test {Read-PairedMatrix $root 'matrix.mtx' 1 2} 'Truncated'
Write-Test 'matrix.mtx' ($text.Replace('1 2 3 4 5 6','1 2 3 4 NaN 6'))
Reject-Test {Read-PairedMatrix $root 'matrix.mtx' 1 2} 'invalid matrix'
Write-Test 'matrix.mtx' ($text.Replace('NROWS=1',"NROWS=1`nNROWS=1"))
Reject-Test {Read-PairedMatrix $root 'matrix.mtx' 1 2} 'Duplicate matrix header'
Write-Test 'matrix.mtx' ($text.Replace('NROWS=1','NROWS=2147483647'))
Reject-Test {Read-PairedMatrix $root 'matrix.mtx' 1 2} 'bounded expected shape'
Write-Test 'matrix.mtx' $text
$hash=Reference-Hash $root 'matrix.mtx';Assert-ReferenceHash $root 'matrix.mtx' $hash;$passed++
Write-Test 'matrix.mtx' ($text+' ')
Reject-Test {Assert-ReferenceHash $root 'matrix.mtx' $hash} 'bytes changed'
Reject-Test {Read-PairedMatrix $root '../matrix.mtx' 1 2} 'Unsafe reference'

$full=[pscustomobject]@{rows=1;columns=1;components=3;values=[double[]]@(1,3,4)}
$direct=[pscustomobject]@{rows=1;columns=1;components=3;values=[double[]]@(2,2,4)}
$negative=Get-PairedNegativeBins $full $direct
Assert-Test ($negative.Count -eq 1 -and $negative[0].residual -eq -1 -and $negative[0].channel -eq 0) 'Negative bin was clamped or not retained.'
$diffuse=[pscustomobject]@{rows=1;columns=1;components=3;values=[double[]]@(10,20,30)}
$solar=[pscustomobject]@{rows=1;columns=1;components=3;values=[double[]]@(5,10,15)}
$centroid=[pscustomobject]@{rows=1;columns=1;components=3;values=[double[]]@(1,2,3)}
$row=Get-PairedSensor $diffuse $solar $centroid @(1,2,3) 0 0
$expected=@((1+14*[Math]::PI/100),(2+28*[Math]::PI/100),(3+42*[Math]::PI/100))
$expectedLux=179*(.265074126*$expected[0]+.670114631*$expected[1]+.064811243*$expected[2])
Assert-Test ($row.valid -and [Math]::Abs($row.lux-$expectedLux) -lt 1e-12) 'Independent hand-computed RGB/lux assembly differs.'
$row=Get-PairedSensor $diffuse $centroid $solar @(1,2,3) 0 0
Assert-Test (!$row.valid -and $row.reflected_solar_rgb[0] -lt 0) 'Negative assembled solar term was clamped.'
Reject-Test {Get-PairedSensor $diffuse $solar $centroid @(1,[double]::PositiveInfinity,3) 0 0} 'Invalid numeric'

# Both compiled finite policies reproduce the independent projected cap formula
# for constant non-gray directional transmission at an upward central source.
$sun="void light solar30`n0`n0`n3 100 200 300`nsolar30 source sun30`n0`n0`n4 0 0 1 0.533`n"
foreach($variant in @('candidate','higher')){
 $n=(Get-PairedTuple $variant).finite;$type="GreenLensPairedVerify_$variant.Direct" -as [type]
 Write-Test 'finite.rgb' ("0.5 0.25 0.75`n"*$n)
 $actual=$type::Finite("0 0 0.8 0 0 1`n",$sun,(Join-Path $root 'finite.rgb'))
 $projected=[Math]::PI*[Math]::Pow([Math]::Sin(.533*[Math]::PI/360),2)
 foreach($channel in 0..2){$expected=@(50,50,225)[$channel]*$projected;Assert-Test ([Math]::Abs($actual[0][$channel]-$expected) -lt 1e-10) 'Independent non-gray finite-cap arithmetic differs.'}
 Write-Test 'finite.rgb' ("0.5 0.25 0.75`n"*($n-1))
 Reject-Test {$type::Finite("0 0 0.8 0 0 1`n",$sun,(Join-Path $root 'finite.rgb'))} 'Finite output count'
}

# A one-sensor stream regeneration to Stream.Null writes no ray file or child.
$receipt=Get-PairedExpectedReceipt "1 2 0.8 0 0 1`n" 16384
Assert-Test ($receipt.Rays -eq 16384 -and $receipt.Bytes -gt 0 -and $receipt.Sha256 -cmatch '^[0-9a-f]{64}$') 'Primary receipt regeneration failed.'
$again=Get-PairedExpectedReceipt "1 2 0.8 0 0 1`n" 16384
Assert-ReferenceSame $again $receipt 'Repeat receipt';$passed++
Write-Test 'process.out' 'synthetic output'
$process=[pscustomobject]@{executable='rcontrib.exe';arguments=@('fixed');exit_code=0;completed=$true;terminated_tree_reason=$null;limits=@{timeout_seconds=7200.0;cleanup_seconds=5;memory_bytes=2147483648;scratch_bytes=2147483648;file_bytes=536870912};started_utc='2026-10-11T00:00:00Z';finished_utc='2026-10-11T00:00:01Z';wall_seconds=1;sampled_runner_plus_engine_peak_bytes=100;sampled_scratch_peak_bytes=100;output_bytes=16;input_bytes=$receipt.Bytes;input_receipt=$receipt;input_transport='typed-continuous-generated-stream-65536';job_assignment='CreateProcess STARTUPINFOEX JOB_LIST';job_empty=$true;stream_tasks_completed=$true}
Assert-PairedProcess $process rcontrib @('fixed') $root 'process.out' $receipt.Bytes $receipt;$passed++
foreach($kind in @('exit','job','stream','argv','receipt','input','input_type','limits','size')){
 $bad=Copy-Test $process
 switch($kind){exit{$bad.exit_code=1} job{$bad.job_empty=$false} stream{$bad.stream_tasks_completed=$false} argv{$bad.arguments=@('changed')} receipt{$bad.input_receipt.Sha256='a'*64} input{$bad.input_bytes++} input_type{$bad.input_bytes=[string]$bad.input_bytes} limits{$bad.limits.memory_bytes++} size{$bad.output_bytes++}}
 Reject-Test {Assert-PairedProcess $bad rcontrib @('fixed') $root 'process.out' $receipt.Bytes $receipt} 'did not complete|completion|differs|differ|Invalid numeric'
}

# Complete synthetic in-memory arithmetic fixture. These numbers are not engine
# results; only the pure comparison function is exercised, never the public
# collector/acceptance entry point. No synthetic acceptance JSON is published.
$cases=@();$candidates=@()
foreach($alternative in @('A','B')){
 $candidateCases=@()
 foreach($index in Get-ReferenceFixedIntervals){
  $rows=@(0..23|ForEach-Object{[pscustomobject]@{sensor_index=$_;sensor_id=('s'+($_+1).ToString('D6'));lux=100.0;valid=$true}})
  $candidateCases+=,[pscustomobject]@{alternative=$alternative;interval_index=$index;sensors=$rows}
  $cases+=,[pscustomobject]@{alternative=$alternative;interval_index=$index;qualified=$true;reference=@{sensors=(Copy-Test $rows)}}
 }
 $candidates+=,[pscustomobject]@{alternative=$alternative;variant='candidate';cases=$candidateCases;valid=$true;negative_coefficient_bins=@()}
}
$refs=[pscustomobject]@{qualified=$true;coverage=@{complete=$true};all_required_sensitivities_complete=$true;cases=$cases;sensitivities=@(@{case_key='B:8512';completed=$true;qualified=$true},@{case_key='A:4116';completed=$true;qualified=$true})}
$result=Compare-PairedValues $candidates $refs
Assert-Test ($result.compared_values -eq 576 -and $result.failure_count -eq 0 -and $result.matched_sky_passed) 'Complete synthetic comparison failed.'
$bad=Copy-Test $candidates;$bad[0].cases[0].sensors[0].lux=105
Assert-Test ((Compare-PairedValues $bad $refs).matched_sky_passed) 'Inclusive fixed tolerance boundary rejected.'
$bad[0].cases[0].sensors[0].lux=105.0001
$result=Compare-PairedValues $bad $refs
Assert-Test (!$result.matched_sky_passed -and $result.failure_count -eq 1 -and $result.checks.Count -eq 24 -and $result.checks[0].sensors.Count -eq 24) 'Failure was dropped or tolerance loosened.'
$bad=Copy-Test $candidates;$bad[1].cases[11].sensors[23].lux=-1;$bad[1].cases[11].sensors[23].valid=$false
$result=Compare-PairedValues $bad $refs
Assert-Test (!$result.matched_sky_passed -and $result.invalid_value_count -eq 1 -and $result.failure_count -eq 1) 'Invalid numeric candidate was silently accepted.'
foreach($kind in @('partial','sensitivity','sensitivity_key','duplicate','profile','order','sensor','nan','unknown')){
 $badCandidates=Copy-Test $candidates;$badRefs=Copy-Test $refs
 switch($kind){
  partial{$badRefs.qualified=$false}
  sensitivity{$badRefs.sensitivities=@($badRefs.sensitivities[0])}
  sensitivity_key{$badRefs.sensitivities[1].case_key='B:8512'}
  duplicate{$badRefs.cases[1]=$badRefs.cases[0]}
  profile{$badCandidates[1].variant='higher'}
  order{$badCandidates[0].cases[1].interval_index=1904}
  sensor{$badCandidates[0].cases[0].sensors[1].sensor_id='s000001'}
  nan{$badCandidates[0].cases[0].sensors[0].lux=[double]::NaN}
  unknown{$badCandidates[0].variant='invented';$badCandidates[1].variant='invented'}
 }
 Reject-Test {Compare-PairedValues $badCandidates $badRefs} 'mandatory|Duplicate|profile|order|identit|numeric|Unknown'
}
$record=[ordered]@{schema_version='greenlens.paired-comparison-synthetic-check.v1';scope='Original synthetic retained-data and arithmetic tests only; no scientific result or qualified-reference acceptance.';passed=$true;checks=$passed;engine_processes=0;comparator_sha256=(Get-MatchedHash "$PSScriptRoot/annual-compare-paired-reference.ps1");test_sha256=(Get-MatchedHash $PSCommandPath)}
$record|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $root 'checks.json') -Encoding utf8NoBOM
Write-Host "Paired comparator synthetic checks passed: $passed. Retained $root"
