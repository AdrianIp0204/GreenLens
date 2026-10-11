#Requires -Version 7.5
# Original synthetic raw arrays and fictional process records only. No engine
# launch, real reference qualification, or production acceptance is claimed.
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-collect-reference.ps1" -FunctionsOnly
Initialize-ReferenceCollector
$script:checks=0
function Assert([bool]$Condition,[string]$Message){if(!$Condition){throw $Message};$script:checks++}
function Reject([scriptblock]$Action,[string]$Pattern){$failure=$null;try{& $Action|Out-Null}catch{$failure=$_.Exception.Message};if(!$failure -or $failure -notmatch $Pattern){throw "Expected /$Pattern/, observed: $failure"};$script:checks++}
function Clone($Value){return ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject $Value -Depth 100) -Depth 100 -DateKind String}
function Text([string]$Root,[string]$Name,[string]$Value){[IO.File]::WriteAllText((Join-Path $Root $Name),$Value,[Text.UTF8Encoding]::new($false))}
function Json([string]$Root,[string]$Name,$Value){Text $Root $Name (ConvertTo-Json -InputObject $Value -Depth 100)}
$testRoot=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/sensitivity-shards-synthetic-'+[guid]::NewGuid()+' 空間')
$baseRoot=Join-Path $testRoot 'baseline';New-Item -ItemType Directory -Path $baseRoot|Out-Null
$plan=@(Get-ReferenceSensitivityVariants)
$names=@($plan|ForEach-Object{$_.name})
$sensors=((0..23|ForEach-Object{"$_ 0 0.8 0 0 1"}) -join "`n")+"`n"
$zero=[double[][]]@(0..23|ForEach-Object{,[double[]]@(0,0,0)})
$baseRaw=[double[]]::new(24*256*6)
for($i=0;$i -lt $baseRaw.Length;$i++){$baseRaw[$i]=if(($i%6) -lt 3){100}else{50}}
$mean=[GreenLensReference.Aggregate]::Calculate($baseRaw,24,0,256,100,$zero,$zero)
$sky="void brightdata distribution`n4 noop hour-8512.dat matched-basis.cal rbin`n0`n0`ndistribution glow sky_glow`n0`n0`n4 100 100 100 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`nvoid light solar510750`n0`n0`n3`n100 200 300`nsolar510750 source sun510750`n0`n0`n4`n0 0 1 .533`n"
$inputNames=@('scene.rad','sensors.pts','matched-basis.cal','hour-8512.dat','hour-8512-sun.rad','hour-8512-scaled.rad','hour-8512-direct.rgb','hour-8512-finite.rgb','hour-8512-rounds256.rgb')
foreach($name in $inputNames){Text $baseRoot $name 'SYNTHETIC fixture data, not engine output'}
Text $baseRoot 'sensors.pts' $sensors
Text $baseRoot 'hour-8512-scaled.rad' $sky
Text $baseRoot 'finite-kernel.cs' 'SYNTHETIC retained text, never compiled'
$inputs=[ordered]@{};foreach($name in $inputNames){$inputs[$name]=Reference-Hash $baseRoot $name}
$baseDeclaration=[ordered]@{alternative='B';process_helper_sha256=(Get-MatchedHash "$PSScriptRoot/annual-reference-process.ps1");aggregation_sha256=(Get-MatchedHash "$PSScriptRoot/annual-reference-aggregation.cs");unknown_physical_field=@{retain='all values'}}
Json $baseRoot 'declaration.json' $baseDeclaration
$context=@{key='B:8512';root=$baseRoot;declaration=(Clone $baseDeclaration);declaration_sha256=(Reference-Hash $baseRoot 'declaration.json');record=@{interval_index=8512;qualified=$true;reference=$mean};direct=$zero;finite=$zero;modifier='solar510750'}

function New-Process([string]$Root,[string]$Exe,[string[]]$Arguments,[string]$Output){
 return [ordered]@{executable="$Exe.exe";arguments=$Arguments;exit_code=0;completed=$true;terminated_tree_reason=$null;limits=@{timeout_seconds=7200.0;memory_bytes=2147483648;scratch_bytes=2147483648;file_bytes=536870912};started_utc='2026-10-11T00:00:00Z';finished_utc='2026-10-11T00:00:01Z';wall_seconds=1;sampled_runner_plus_engine_peak_bytes=100;sampled_scratch_peak_bytes=100;output_bytes=(Get-Item -LiteralPath (Join-Path $Root $Output)).Length}
}
function New-Shard([string]$Label,[string[]]$Selected){
 $root=Join-Path $testRoot $Label;New-Item -ItemType Directory -Path $root|Out-Null
 foreach($name in $inputNames+@('finite-kernel.cs')){Copy-Item -LiteralPath (Join-Path $baseRoot $name) -Destination (Join-Path $root $name)}
 Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-sensitivity.ps1" -Destination (Join-Path $root 'harness-source.ps1')
 Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-process.ps1" -Destination (Join-Path $root 'process-source.ps1')
 Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs" -Destination (Join-Path $root 'aggregation-source.cs')
 $decl=[ordered]@{schema_version='greenlens.controlled-reference-sensitivity-shard.v1';declared_utc='2026-10-11T00:00:00Z';scope='Synthetic fixture only';baseline_declaration=$baseDeclaration;baseline_declaration_sha256=$context.declaration_sha256;interval_index=8512;final_baseline_rounds=256;variants=$plan;executed_variant_names=$Selected;input_sha256=$inputs;raw_output_order='Round-major 24-sensor records, sky RGB followed by full-solar RGB';scale_tolerance='abs(normalized_variant-bin - baseline-bin) <= 1e-5*abs(baseline-bin), every RGB channel of both bins; exact zero requires zero';weight_depth_tolerance='max(0.1 lx, 1% baseline mean) at every sensor';centroid_and_finite_policy='Reuse byte-verified original baseline centroid and 4096-ray finite arrays unchanged';harness_sha256=(Reference-Hash $root 'harness-source.ps1');supervisor_sha256=$baseDeclaration.process_helper_sha256;limits=@{maximum_process_seconds=7200;maximum_runner_plus_engine_working_set_bytes=2147483648;maximum_scratch_bytes=2147483648;maximum_single_file_bytes=536870912};release_acceptance_complete=$false}
 Json $root 'declaration.json' $decl
 $results=@();$processes=@()
 foreach($name in $Selected){
  $variant=@($plan|Where-Object{$_.name -ceq $name})[0]
  $scaled=$sky
  if($name -ceq 'scale10000'){$scaled=$sky.Replace('4 100 100 100 0','4 10000 10000 10000 0').Replace("3`n100 200 300`n","3`n10000 20000 30000`n")}
  Text $root "$name.rad" $scaled;Text $root "$name.oct" 'Synthetic octree marker, never executed'
  $raw=[double[]]::new($baseRaw.Length);for($i=0;$i -lt $raw.Length;$i++){$raw[$i]=$baseRaw[$i]*$variant.scale/100}
  Text $root "$name.rgb" (($raw|ForEach-Object{$_.ToString('R',[Globalization.CultureInfo]::InvariantCulture)}) -join ' ')
  $variantMean=[GreenLensReference.Aggregate]::Calculate($raw,24,0,256,$variant.scale,$zero,$zero)
  $checks=if($variant.scale -eq 10000){@(Compare-ReferenceScaleBins $variantMean $mean)}else{@([GreenLensReference.Aggregate]::CompareWithScale($mean,$variantMean,$mean))}
  $results+=,[ordered]@{variant=$variant;mean=$variantMean;checks=$checks;passed=$true;raw_sha256=(Reference-Hash $root "$name.rgb");scene_sha256=(Reference-Hash $root "$name.rad")}
  $options=Get-ReferenceOptions;$options[[Array]::IndexOf($options,'-ab')+1]="$($variant.ab)";$options[[Array]::IndexOf($options,'-lr')+1]="$($variant.lr)";$options[[Array]::IndexOf($options,'-lw')+1]=$variant.lw
  $processes+=,(New-Process $root oconv @('-f','scene.rad',"$name.rad") "$name.oct")
  $processes+=,(New-Process $root rcontrib ($options+@('-y','6144','-m','sky_glow','-m','solar510750',"$name.oct")) "$name.rgb")
 }
 Json $root 'sensitivity.json' ([ordered]@{declaration=$decl;results=$results;completed=$true;passed=$true;case_complete=($Selected.Count -eq 3);release_acceptance_complete=$false})
 Json $root 'processes.json' $processes
 return $root
}
$paths=@(foreach($name in $names){New-Shard $name @($name)})
$serialPath=New-Shard 'serial-new-schema' $names
$records=@(foreach($root in $paths){Read-ReferenceSensitivity $root $context})
Assert ($records.Count -eq 3 -and @($records|Where-Object{!$_.completed -or !$_.qualified}).Count -eq 0) 'Each complete shard failed its unchanged raw/process checks.'
$merged=@(Merge-ReferenceSensitivities $records)
$serial=Read-ReferenceSensitivity $serialPath $context
Assert ($merged.Count -eq 1 -and $merged[0].completed -and $merged[0].qualified -and $merged[0].variants.Count -eq 3) 'Three independent variants did not complete their case.'
Assert-ReferenceSame $merged[0].variants $serial.variants 'Serial versus independent shards';$script:checks++
Assert ($merged[0].sources.Count -eq 3 -and @($merged[0].sources|ForEach-Object{$_.verified_processes}).Count -eq 6) 'Source/process lineage was lost.'
Assert-ReferenceSame @(Merge-ReferenceSensitivities @($records[2],$records[0],$records[1]))[0].variants $serial.variants 'Reordered directory input';$script:checks++
Assert (!(@(Merge-ReferenceSensitivities @($records[0],$records[2]))[0].qualified)) 'Missing weight variant qualified.'
Assert ((@(Merge-ReferenceSensitivities @($records[0],$records[2]))[0].missing_variant_names -join '|') -ceq 'weight1e10') 'Missing variant identity was not retained.'
Reject {Merge-ReferenceSensitivities @($records[0],$records[0])} 'duplicate fixed'
Reject {Merge-ReferenceSensitivities @($serial,$records[0])} 'duplicate fixed'
foreach($field in @('final_baseline_rounds','baseline_declaration_sha256','input_sha256','harness_sha256','unknown_future_physics')){
 $changed=Clone $records[1];$binding=Copy-ReferenceJsonObject $changed.binding
 $binding[$field]=if($field -eq 'final_baseline_rounds'){512}else{'different'};$changed.binding=$binding
 Reject {Merge-ReferenceSensitivities @($records[0],$changed,$records[2])} 'shard identity'
}
$changed=Clone $records[1];$changed.variants[0].passed=$false
Assert (!(@(Merge-ReferenceSensitivities @($records[0],$changed,$records[2]))[0].qualified)) 'Failed sensitivity qualified.'
$changed=Clone $records[1];$changed.completed=$false;$changed.variants=@()
Assert (!(@(Merge-ReferenceSensitivities @($records[0],$changed,$records[2]))[0].completed)) 'Partial process sequence completed its case.'
$changed=Clone $records[0];$changed.case_key='B:8511'
Reject {Merge-ReferenceSensitivities @($changed)} 'Unexpected'
$both=@($records)+@(foreach($record in $records){$copy=Clone $record;$copy.case_key='A:4116';$copy})
$two=@(Merge-ReferenceSensitivities $both)
Assert ($two.Count -eq 2 -and @($two|Where-Object{!$_.qualified}).Count -eq 0) 'Six pure synthetic variants did not cover exactly two cases.'

$decl=Read-ReferenceJson $paths[0] 'declaration.json'
$legacy=Clone $decl;$legacy.schema_version='greenlens.controlled-reference-sensitivity.v1';$legacy.harness_sha256='88400209ed579398253903feb7fe7abb22c53bfafec85433b89001578297db01'
Assert ((@(Get-ReferenceSensitivityExecution $legacy) -join '|') -ceq ($names -join '|')) 'Reviewed original serial schema was rejected.'
$legacy.harness_sha256='0'*64;Reject {Get-ReferenceSensitivityExecution $legacy} 'Legacy.*source'
foreach($bad in @(@('scale10000','weight1e10'),@('scale10000','scale10000','depth10-24'),@('depth10-24','weight1e10','scale10000'),@('UNKNOWN'))){
 $changed=Clone $decl;$changed.executed_variant_names=$bad
 Reject {Get-ReferenceSensitivityExecution $changed} 'execution|Unknown|duplicate'
}
$changed=Clone $decl;$changed.schema_version='unknown';Reject {Get-ReferenceSensitivityExecution $changed} 'Unsupported'
$changed=Clone $decl;$changed.harness_sha256='0'*64;Reject {Get-ReferenceSensitivityExecution $changed} 'trusted source'
$changed=Clone $decl;$changed.variants[0].scale=9999;Reject {Get-ReferenceSensitivityExecution $changed} 'variants'

# Each mutation is restored; only synthetic files in this new owned directory change.
function Mutate-Json([string]$Root,[string]$Name,[scriptblock]$Change,[string]$Expected){
 $path=Join-Path $Root $Name;$saved=[IO.File]::ReadAllBytes($path)
 try{$value=Read-ReferenceJson $Root $Name;& $Change $value;Json $Root $Name $value;Reject {Read-ReferenceSensitivity $Root $context} $Expected}
 finally{[IO.File]::WriteAllBytes($path,$saved)}
}
Mutate-Json $paths[0] 'declaration.json' {param($x)$x.baseline_declaration_sha256='0'*64} 'baseline identity'
Mutate-Json $paths[0] 'declaration.json' {param($x)$x.baseline_declaration.unknown_physical_field.retain='changed'} 'embedded baseline'
Mutate-Json $paths[0] 'declaration.json' {param($x)$x.final_baseline_rounds=512} 'baseline rung'
Mutate-Json $paths[0] 'declaration.json' {param($x)$x.input_sha256.'sensors.pts'='0'*64} 'bytes changed'
Mutate-Json $paths[0] 'declaration.json' {param($x)$x.supervisor_sha256='0'*64} 'trusted source'
Mutate-Json $paths[0] 'declaration.json' {param($x)$x.limits.maximum_process_seconds=7201} 'resource bounds'
Mutate-Json $paths[0] 'processes.json' {param($x)$x[1].exit_code=1} 'did not complete'
Mutate-Json $paths[0] 'processes.json' {param($x)$x[1].arguments[0]='-I-'} 'arguments'
Mutate-Json $paths[0] 'processes.json' {param($x)$x[1].limits.file_bytes=536870913} 'resource limits'
Mutate-Json $paths[0] 'sensitivity.json' {param($x)$x.results[0].mean.sensors[23].lux+=1} 'Stored sensitivity means'
Mutate-Json $paths[0] 'sensitivity.json' {param($x)$x.results[0].variant.scale=9999} 'order/settings'
Mutate-Json $paths[0] 'sensitivity.json' {param($x)$x.case_complete=$true} 'case completion'
foreach($badText in @('1 2 3','NaN',(('0 '*36864)+' 0'))){
 $root=$paths[0];$rawPath=Join-Path $root 'scale10000.rgb';$saved=[IO.File]::ReadAllBytes($rawPath);$reportBytes=[IO.File]::ReadAllBytes((Join-Path $root 'sensitivity.json'));$processBytes=[IO.File]::ReadAllBytes((Join-Path $root 'processes.json'))
 try{Text $root 'scale10000.rgb' $badText;$report=Read-ReferenceJson $root 'sensitivity.json';$report.results[0].raw_sha256=Reference-Hash $root 'scale10000.rgb';Json $root 'sensitivity.json' $report;$process=Read-ReferenceJson $root 'processes.json';$process[1].output_bytes=(Get-Item -LiteralPath $rawPath).Length;Json $root 'processes.json' $process;Reject {Read-ReferenceSensitivity $root $context} 'Truncated|Nonfinite|Invalid|Excess'}
 finally{[IO.File]::WriteAllBytes($rawPath,$saved);[IO.File]::WriteAllBytes((Join-Path $root 'sensitivity.json'),$reportBytes);[IO.File]::WriteAllBytes((Join-Path $root 'processes.json'),$processBytes)}
}
Json $testRoot 'checks.json' ([ordered]@{schema_version='greenlens.sensitivity-shards-synthetic.v1';scope='Synthetic raw/process data, not native engine or scientific qualification';passed=$true;checks=$script:checks;engine_processes_launched=0;collector_sha256=(Get-MatchedHash "$PSScriptRoot/annual-collect-reference.ps1");harness_sha256=(Get-MatchedHash "$PSScriptRoot/annual-reference-sensitivity.ps1");test_sha256=(Get-MatchedHash $PSCommandPath)})
Write-Host "$script:checks sensitivity shard checks passed; no engine launched. Retained: $testRoot"
