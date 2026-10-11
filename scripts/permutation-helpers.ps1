# Research preparation only. No engine policy or existing reference is modified.
. "$PSScriptRoot/depth-order-helpers.ps1"
if(-not ('GreenLensPermutation.Order' -as[type])){Add-Type -Path "$PSScriptRoot/permutation-order.cs"}
function Get-PermutationCases {
 @(
  [ordered]@{id='seed0-baseline';seed=0;ab=9;lr=-16},
  [ordered]@{id='seed0-depth';seed=0;ab=10;lr=-24},
  [ordered]@{id='seed1-baseline';seed=1;ab=9;lr=-16},
  [ordered]@{id='seed1-depth';seed=1;ab=10;lr=-24}
 )
}
function Get-PermutationCase([string]$Id){$v=@(Get-PermutationCases|Where-Object id -CEQ $Id);if($v.Count-ne1){throw 'Unknown fixed permutation case.'};return $v[0]}
function Assert-PermutationSeed($Seed){Assert-DepthNumber $Seed;if($Seed-ne0 -and $Seed-ne1){throw 'Only the two declared seeds are permitted.'}}
function Get-PermutationTable($Seed){
 Assert-PermutationSeed $Seed;$rows=[Collections.Generic.List[object]]::new()
 for($r=0;$r-lt256;$r++){$rows.Add([GreenLensPermutation.Order]::Round([int]$Seed,$r))}
 return ,$rows.ToArray()
}
function Get-PermutationTableHash($Table){Get-DepthBytesHash ([Text.Encoding]::ASCII.GetBytes(($Table|ConvertTo-Json -Depth 5 -Compress)+"`n"))}
function Assert-PermutationTable($Table,$Seed){
 Assert-PermutationSeed $Seed;if(@($Table).Count-ne256){throw 'Exactly256 mapped rounds required.'}
 for($r=0;$r-lt256;$r++){
  if(@($Table[$r]).Count-ne24){throw 'Exactly24 mapped sensors per round required.'};$expected=[GreenLensPermutation.Order]::Round([int]$Seed,$r)
  for($p=0;$p-lt24;$p++){Assert-DepthNumber $Table[$r][$p];if($Table[$r][$p]-ne$expected[$p]){throw 'Seed/domain/endian/round/sensor permutation differs.'}}
 }
}
function New-PermutationStream([string]$Sensors,$Seed){
 $null=New-DepthStream $Sensors 'original'
 $lines=$Sensors.Substring(0,$Sensors.Length-1).Split("`n");$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($line in $lines){
  $tokens=$line.Split(' ');if($tokens.Count-ne6 -or !$seen.Add($line)){throw 'Unique canonical six-token sensor rows required.'}
  foreach($token in $tokens){if($token-cnotmatch'^[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[Ee][+-]?[0-9]+)?$'){throw 'Canonical ASCII sensor tokens required.'}}
 }
 $table=Get-PermutationTable $Seed;$text=[Text.StringBuilder]::new()
 for($r=0;$r-lt256;$r++){for($p=0;$p-lt24;$p++){$null=$text.Append($lines[$table[$r][$p]]).Append("`n")}}
 $str=$text.ToString();$bytes=[Text.Encoding]::ASCII.GetBytes($str)
 [ordered]@{text=$str;bytes=$bytes.LongLength;sha256=Get-DepthBytesHash $bytes;records=6144;rounds=256;sensors=24;seed=$Seed;emitted_to_original=$table;table_sha256=Get-PermutationTableHash $table;ordering='round-major; per-round emitted position maps to canonical sensor ID'}
}
function ConvertFrom-PermutationOrder([double[]]$Values,$Table,$Seed){
 Assert-PermutationTable $Table $Seed
 if($Values.Length-ne36864){throw 'Exactly256x24x6 raw scalars required.'};$out=[double[]]::new(36864)
 for($r=0;$r-lt256;$r++){for($p=0;$p-lt24;$p++){for($c=0;$c-lt6;$c++){
  $v=$Values[($r*24+$p)*6+$c];if(![double]::IsFinite($v) -or $v-lt0){throw 'Finite nonnegative raw bins required.'}
  $out[($r*24+$Table[$r][$p])*6+$c]=$v
 }}}
 return ,$out
}
function Get-PermutationArguments([string]$Id,[string]$Modifier){
 $case=Get-PermutationCase $Id;$name=if($case.ab-eq9){'baseline-reversed'}else{'depth-reversed'}
 return Get-DepthArguments $name $Modifier
}
function Assert-PermutationPlan($Plan){
 if($Plan.schema_version-cne'greenlens.permutation-investigation-plan.v1' -or $Plan.execution_authorized-isnot[bool]){throw 'Unknown permutation plan.'}
 foreach($p in @(@('sensors',24),@('rounds',256),@('ambient_divisions',8192),@('source_scale',100),@('interval_index',4116),@('maximum_parallel_jobs',2),@('engine_workers_per_job',1),@('process_seconds',7200),@('maximum_file_bytes',536870912),@('maximum_scratch_bytes',2147483648),@('maximum_memory_bytes',2147483648))){Assert-DepthNumber $Plan.($p[0]);if($Plan.($p[0])-ne$p[1]){throw 'Changed fixed setting or bound.'}}
 if($Plan.alternative-cne'A' -or $Plan.ambient_weight-cne'1e-9' -or $Plan.sampling_mode-cne'-u-' -or $Plan.accumulation-cne'-c 1'){throw 'Changed fixed estimator.'}
 $expected=@(Get-PermutationCases);if(@($Plan.cases).Count-ne4){throw 'Exactly four fixed cases required.'}
 for($i=0;$i-lt4;$i++){foreach($key in @('seed','ab','lr')){Assert-DepthNumber $Plan.cases[$i].$key};foreach($key in @('id','seed','ab','lr')){if($Plan.cases[$i].$key-cne$expected[$i].$key){throw 'Changed fixed case/order.'}}}
 if($Plan.trusted_checkout_commit-cne'a29b768522dd41d700b6e53fd585bd1706a80ecd'){throw 'Unreviewed trusted checkout.'}
 if($Plan.proposal_sha256-cne'77c93eab343c7091d6cf23b0a53b1c2eb5a5525d7436872d3fd0b405f1a26c8f'){throw 'Unreviewed data proposal.'}
 foreach($seed in 0,1){if($Plan.table_sha256."$seed"-cne(Get-PermutationTableHash (Get-PermutationTable $seed))){throw 'Fixed mapping table hash differs.'}}
}
function Compare-PermutationMeans($Low,$High,[string]$Name,$Reference){
 foreach($mean in @($Low,$High,$Reference)){Assert-DepthMean $mean};$checks=@()
 for($s=0;$s-lt24;$s++){
  $difference=[Math]::Abs($High.sensors[$s].lux-$Low.sensors[$s].lux);$limit=[Math]::Max(0.1,0.01*$Reference.sensors[$s].lux)
  $checks+=,[ordered]@{comparison=$Name;sensor_index=$s;sensor_id=$Low.sensors[$s].sensor_id;low_lux=$Low.sensors[$s].lux;high_lux=$High.sensors[$s].lux;reference_lux=$Reference.sensors[$s].lux;absolute_difference_lux=$difference;tolerance_lux=$limit;passed=($difference-le$limit)}
 }
 return ,$checks
}
function Measure-PermutationStream([double[]]$Canonical,$Direct,$Finite){
 if($Canonical.Length-ne36864){throw 'Fixed raw shape required.'};$means=[ordered]@{}
 foreach($rounds in 64,128,256){$mean=[GreenLensReference.Aggregate]::Calculate($Canonical,24,0,$rounds,100,$Direct,$Finite);Assert-DepthMean $mean;$means["$rounds"]=$mean}
 $first=[GreenLensReference.Aggregate]::Calculate($Canonical,24,0,128,100,$Direct,$Finite)
 $second=[GreenLensReference.Aggregate]::Calculate($Canonical,24,128,128,100,$Direct,$Finite)
 $checks=@();$checks+=Compare-PermutationMeans $means['64'] $means['128'] '64-to128' $means['128'];$checks+=Compare-PermutationMeans $means['128'] $means['256'] '128-to256' $means['256'];$checks+=Compare-PermutationMeans $first $second 'disjoint-halves' $means['256']
 # Flatten only the three explicit24-row check arrays; preserve every failed row.
 $rows=@($checks|ForEach-Object{$_});if($rows.Count-ne72){throw 'Expected72 local checks.'}
 [ordered]@{prefix_means=$means;first_half=$first;second_half=$second;checks=$rows;passed=(@($rows|Where-Object{-not$_.passed}).Count-eq0)}
}
function Read-PermutationJson([string]$Path){
 $bytes=Read-DepthFile $Path 2097152
 return ([Text.UTF8Encoding]::new($false,$true).GetString($bytes)|ConvertFrom-Json -Depth 100 -DateKind String)
}

function Assert-PermutationUtc($Value){
 if($Value-isnot[string] -or $Value-cnotmatch'(Z|\+00:00)$'){throw 'Explicit UTC timestamp required.'}
 $v=[DateTimeOffset]::Parse($Value,[Globalization.CultureInfo]::InvariantCulture)
 if($v.Offset-ne[TimeSpan]::Zero){throw 'UTC offset differs.'};return $v
}
function Assert-PermutationProcess($Process,[string]$Executable,[string[]]$Arguments,[long]$OutputBytes,[long]$InputBytes,[string]$InputTransport,$NotBefore){
 if('input_receipt'-cnotin@($Process.PSObject.Properties.Name)){throw 'Missing explicit input receipt field.'}
 $times=Assert-DepthProcess $Process $Executable $Arguments $OutputBytes $InputBytes $InputTransport
 $null=Assert-PermutationUtc $Process.started_utc;$null=Assert-PermutationUtc $Process.finished_utc
 if($times.start-lt$NotBefore){throw 'Process starts before declaration or preceding process.'}
 if($InputTransport-ceq'owned-file-stream-65536'){
  if($Process.input_receipt-isnot[PSCustomObject] -or @($Process.input_receipt.PSObject.Properties).Count-ne0){throw 'Exact empty file-input receipt object required.'}
 }elseif($InputTransport-ceq'small-text'){
  if($null-ne$Process.input_receipt){throw 'Null small-text receipt required.'}
 }else{throw 'Unknown fixed input transport.'}
 return $times
}
function ConvertFrom-PermutationAscii([string]$Text){
 if($Text.Length-gt2097152){throw 'Raw ASCII exceeds bounded size.'}
 $tokens=@($Text -split '[ \t\r\n]+'|Where-Object{$_});if($tokens.Count-ne36864){throw 'Exactly36864 ASCII scalars required.'}
 $values=[double[]]::new(36864)
 for($i=0;$i-lt$tokens.Count;$i++){
  $token=$tokens[$i];if($token-cnotmatch'^[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?$'){throw 'Noncanonical ASCII number.'}
  $v=0.;if(![double]::TryParse($token,[Globalization.NumberStyles]::Float,[Globalization.CultureInfo]::InvariantCulture,[ref]$v) -or ![double]::IsFinite($v) -or $v-lt0){throw 'Finite nonnegative raw scalar required.'}
  if($v-eq0 -and ($token -split '[eE]')[0]-match'[1-9]'){throw 'Nonzero raw scalar underflowed.'};$values[$i]=$v
 }
 return ,$values
}
function Get-PermutationHost {
 $os=Get-CimInstance Win32_OperatingSystem;$cpus=@(Get-CimInstance Win32_Processor);$system=Get-CimInstance Win32_ComputerSystem
 $value=[ordered]@{schema_version='greenlens.sanitized-host.v1';recorded_utc=[DateTime]::UtcNow.ToString('o');os=[ordered]@{caption=$os.Caption;version=$os.Version;build=$os.BuildNumber;architecture=$os.OSArchitecture};processors=@($cpus|ForEach-Object{[ordered]@{model=$_.Name;cores=$_.NumberOfCores;logical_processors=$_.NumberOfLogicalProcessors}});physical_memory_bytes=$system.TotalPhysicalMemory;powershell_version=$PSVersionTable.PSVersion.ToString()}
 # Roundtrip ensures the same strict JSON shapes will be used by the reader.
 $result=$value|ConvertTo-Json -Depth 10|ConvertFrom-Json -Depth 10 -DateKind String
 Assert-PermutationHost $result;return $result
}
function Assert-PermutationKeys($Value,[string[]]$Keys){
 if($Value-isnot[PSCustomObject] -or (@($Value.PSObject.Properties.Name|Sort-Object)-join'|')-cne(@($Keys|Sort-Object)-join'|')){throw 'Unknown or missing metadata fields.'}
}
function Assert-PermutationHost($Value){
 Assert-PermutationKeys $Value @('schema_version','recorded_utc','os','processors','physical_memory_bytes','powershell_version')
 if($Value.schema_version-cne'greenlens.sanitized-host.v1'){throw 'Unknown host schema.'};$null=Assert-PermutationUtc $Value.recorded_utc
 Assert-PermutationKeys $Value.os @('caption','version','build','architecture')
 foreach($v in @($Value.os.caption,$Value.os.version,$Value.os.build,$Value.os.architecture,$Value.powershell_version)){
  if($v-isnot[string] -or !$v.Trim() -or $v.Length-gt256 -or $v-match'[\x00-\x1f]'){throw 'Invalid bounded host description.'}
 }
 if(@($Value.processors).Count-lt1 -or @($Value.processors).Count-gt16){throw 'Invalid processor inventory.'}
 foreach($cpu in $Value.processors){
  Assert-PermutationKeys $cpu @('model','cores','logical_processors')
  if($cpu.model-isnot[string] -or !$cpu.model.Trim() -or $cpu.model.Length-gt256 -or $cpu.model-match'[\x00-\x1f]'){throw 'Invalid CPU model.'}
  foreach($key in 'cores','logical_processors'){Assert-DepthNumber $cpu.$key;if($cpu.$key-lt1 -or $cpu.$key-gt4096 -or $cpu.$key-ne[Math]::Truncate($cpu.$key)){throw 'Invalid CPU count.'}}
  if($cpu.logical_processors-lt$cpu.cores){throw 'CPU counts disagree.'}
 }
 Assert-DepthNumber $Value.physical_memory_bytes;if($Value.physical_memory_bytes-lt1048576 -or $Value.physical_memory_bytes-ne[Math]::Truncate($Value.physical_memory_bytes)){throw 'Invalid physical memory.'}
}
function Get-PermutationComparisons($Records){
 if($Records.Count-ne4){throw 'Exactly four complete records required.'};$checks=[Collections.Generic.List[object]]::new()
 foreach($case in Get-PermutationCases){
  if(!$Records.Contains($case.id)){throw 'Missing fixed case.'};$record=$Records[$case.id]
  if($record.completed-isnot[bool] -or !$record.completed -or @($record.diagnostics.checks).Count-ne72){throw 'Incomplete per-stream diagnostics.'}
  foreach($check in $record.diagnostics.checks){$checks.Add([ordered]@{case=$case.id;check=$check})}
 }
 foreach($seed in 0,1){foreach($check in (Compare-PermutationMeans $Records["seed$seed-baseline"].mean $Records["seed$seed-depth"].mean "seed$seed-depth" $Records["seed$seed-baseline"].mean)){$checks.Add([ordered]@{case="seed$seed-pair";check=$check})}}
 foreach($level in 'baseline','depth'){foreach($check in (Compare-PermutationMeans $Records["seed0-$level"].mean $Records["seed1-$level"].mean "$level-cross-seed" $Records["seed0-$level"].mean)){$checks.Add([ordered]@{case="$level-cross-seed";check=$check})}}
 if($checks.Count-ne384){throw 'Exactly384 comparisons required.'};return ,$checks.ToArray()
}
