#Requires -Version 7.5
param([Parameter(Mandatory)][string]$OutputDirectory,[Parameter(Mandatory)][string]$TrustedSourceRoot)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Fresh synthetic output required.'}
. "$PSScriptRoot/permutation-helpers.ps1"
$checks=[Collections.Generic.List[string]]::new()
function Pass([string]$Name,[scriptblock]$Body){& $Body;$checks.Add($Name)}
function Reject([string]$Name,[scriptblock]$Body){$bad=$false;try{& $Body|Out-Null}catch{$bad=$true};if(!$bad){throw "Invalid fixture accepted: $Name"};$checks.Add($Name)}
$proposal=Get-Content -Raw "$PSScriptRoot/permutation-proposal.json"|ConvertFrom-Json -Depth 100 -DateKind String
$plan=Get-Content -Raw "$PSScriptRoot/permutation-plan.json"|ConvertFrom-Json -Depth 100 -DateKind String
Pass 'Exact preparation plan, fixed four tuples and unchanged argument policy' {
 Assert-PermutationPlan $plan
 $cases=@(Get-PermutationCases)
 for($i=0;$i-lt4;$i++){$a=Get-PermutationArguments $cases[$i].id 'solar246990';if(($a-join'|')-cne($proposal.cases[$i].processes[1].arguments-join'|')){throw 'Proposed argv changed'}}
}
Pass 'Independent BigInteger rejection bounds including exactly2^64 ceiling' {
 $all=[System.Numerics.BigInteger]::Pow(2,64)
 for($bound=2;$bound-le24;$bound++){
  $ceiling=$all-($all%$bound)
  foreach($word in @([ulong]0,[ulong]1,[ulong]($ceiling-1),[ulong]::MaxValue)){
   $expected=([System.Numerics.BigInteger]$word)-lt$ceiling
   if([GreenLensPermutation.Order]::Accept($word,$bound)-ne$expected){throw 'Rejection boundary differs'}
  }
  if($ceiling-lt$all -and [GreenLensPermutation.Order]::Accept([ulong]$ceiling,$bound)){throw 'Rejected boundary accepted'}
 }
}
Pass 'Forced rejection consumes a fresh draw, never modulo-reduces rejected word' {
 $words=[ulong[]]::new(24);$words[0]=[ulong]::MaxValue;$used=0;$map=[GreenLensPermutation.Order]::FromWords($words,[ref]$used)
 if($used-ne24){throw 'Rejection did not consume extra draw'}
 $expected=@(1..23)+0;if(($map-join',')-cne($expected-join',')){throw 'Wrong forced-draw permutation'}
}
Reject 'Exhausted draw sequence fails' {$used=0;[GreenLensPermutation.Order]::FromWords([ulong[]]@(0),[ref]$used)}
foreach($bad in -1,2,$true,'0',$null){Reject "Invalid seed $bad" {Get-PermutationTable $bad}}
Reject 'Unknown case' {Get-PermutationCase 'adaptive'}
Reject 'Wrong source interval' {Get-PermutationArguments 'seed0-baseline' 'solar510750'}
$sensors=(@(0..23|ForEach-Object{"$_ 0 0.8 0 0 1"})-join"`n")+"`n"
foreach($seed in 0,1){
 $table=Get-PermutationTable $seed;$stream=New-PermutationStream $sensors $seed
 Pass "Seed$seed exact Python golden tables, boundaries and primary receipt" {
  $golden=Get-Content -Raw "$PSScriptRoot/permutation-seed$seed.json"|ConvertFrom-Json -Depth 10
  Assert-PermutationTable $golden $seed
  if((Get-PermutationTableHash $table)-cne$proposal.tables."$seed".sha256){throw 'Golden table hash differs'}
  if(([GreenLensPermutation.Order]::Round($seed,256)-join',')-cne($proposal.tables."$seed".extension_round256-join',')){throw 'Prefix extension boundary differs'}
  $lines=$stream.text.TrimEnd("`n").Split("`n");if($lines.Count-ne6144 -or $stream.records-ne6144){throw 'Wrong primary count'}
  for($r=0;$r-lt256;$r++){for($p=0;$p-lt24;$p++){if($lines[$r*24+$p]-cne"$($table[$r][$p]) 0 0.8 0 0 1"){throw 'Primary row changed or moved across round'}}}
  if((Get-DepthBytesHash ([Text.Encoding]::ASCII.GetBytes($stream.text)))-cne$stream.sha256){throw 'Receipt hash differs'}
 }
 Pass "Seed$seed inverse all36864 non-gray round/sensor/channel tags" {
  $values=[double[]]::new(36864)
  for($r=0;$r-lt256;$r++){for($p=0;$p-lt24;$p++){for($c=0;$c-lt6;$c++){$values[($r*24+$p)*6+$c]=10000*$r+100*$table[$r][$p]+$c}}}
  $out=ConvertFrom-PermutationOrder $values $table $seed
  for($r=0;$r-lt256;$r++){for($s=0;$s-lt24;$s++){for($c=0;$c-lt6;$c++){if($out[($r*24+$s)*6+$c]-ne(10000*$r+100*$s+$c)){throw 'Inverse row/channel mapping differs'}}}}
 }
 Reject "Seed$seed wrong seed table" {Assert-PermutationTable $table (1-$seed)}
 Reject "Seed$seed missing round" {Assert-PermutationTable $table[0..254] $seed}
 Reject "Seed$seed reordered rounds" {$bad=$table.Clone();$bad[0]=$table[1];Assert-PermutationTable $bad $seed}
 Reject "Seed$seed duplicate mapped sensor" {$bad=$table|ConvertTo-Json -Depth 5|ConvertFrom-Json -Depth 5;$bad[0][0]=$bad[0][1];Assert-PermutationTable $bad $seed}
 Reject "Seed$seed string aliases rejected" {$bad=$table|ConvertTo-Json -Depth 5|ConvertFrom-Json -Depth 5;$bad[0][0]="$($bad[0][0])";Assert-PermutationTable $bad $seed}
 Reject "Seed$seed truncated raw" {ConvertFrom-PermutationOrder ([double[]]::new(36863)) $table $seed}
 foreach($badValue in [double]::NaN,[double]::PositiveInfinity,-1.){Reject "Seed$seed invalid raw $badValue" {$v=[double[]]::new(36864);$v[4]=$badValue;ConvertFrom-PermutationOrder $v $table $seed}}
}
Reject 'Duplicate canonical sensor rows rejected' {New-PermutationStream (("0 0 0.8 0 0 1`n")*24) 0}
Reject 'Missing canonical sensor row rejected' {New-PermutationStream (($sensors.TrimEnd("`n").Split("`n")[0..22]-join"`n")+"`n") 0}
Reject 'Changed whitespace not silently normalized' {New-PermutationStream ($sensors.Replace('0.8',' 0.8')) 0}
Reject 'CRLF canonical input rejected' {New-PermutationStream ($sensors.Replace("`n","`r`n")) 0}
foreach($change in 'rounds','deadline','seed','proposal'){
 Reject "Plan rejects changed $change" {
  $p=$plan|ConvertTo-Json -Depth 100|ConvertFrom-Json -Depth 100
  switch($change){rounds{$p.rounds=512}deadline{$p.process_seconds=7201}seed{$p.cases[0].seed=1}proposal{$p.proposal_sha256='0'*64}}
  Assert-PermutationPlan $p
 }
}
$aggregate=Join-Path $TrustedSourceRoot 'scripts/annual-reference-aggregation.cs'
if((Get-FileHash -LiteralPath $aggregate).Hash.ToLowerInvariant()-cne'fdee643c733450a84b2c4d5c59d4af35174f25ff214496eb0a5a0249e3090c84'){throw 'Unreviewed aggregation source'}
if(-not('GreenLensReference.Aggregate'-as[type])){Add-Type -Path $aggregate}
$zero=[double[][]]::new(24);for($i=0;$i-lt24;$i++){$zero[$i]=[double[]]::new(3)};$raw=[double[]]::new(36864)
for($r=0;$r-lt256;$r++){for($s=0;$s-lt24;$s++){for($c=0;$c-lt3;$c++){$raw[($r*24+$s)*6+$c]=if($r-lt64){100.}elseif($r-lt128){300.}else{200.}}}}
Pass 'Hand-derived means and all72 fixed stream comparisons retain24 failures' {
 $m=Measure-PermutationStream $raw $zero $zero
 if($m.checks.Count-ne72 -or @($m.checks|Where-Object{-not$_.passed}).Count-ne24 -or $m.passed){throw 'Expected24 ladder failures'}
 foreach($s in $m.prefix_means['64'].sensors){if([Math]::Abs($s.lux-179)-gt1e-9){throw '64 mean differs'}}
 foreach($key in '128','256'){foreach($s in $m.prefix_means[$key].sensors){if([Math]::Abs($s.lux-358)-gt1e-9){throw '128/256 mean differs'}}}
}
Pass 'Fixed baseline side distinguishes an asymmetric boundary without loosening threshold' {
 $low=[GreenLensReference.Aggregate]::Calculate([double[]]($raw|ForEach-Object{0}),24,0,256,100,$zero,$zero)
 $high=$low|ConvertTo-Json -Depth 20|ConvertFrom-Json -Depth 20
 foreach($s in $low.sensors){$s.diffuse_rgb=@(1.,1.,1.);$s.total_rgb=@(1.,1.,1.);$s.lux=179.}
 foreach($s in $high.sensors){$x=180.8/179;$s.diffuse_rgb=@($x,$x,$x);$s.total_rgb=@($x,$x,$x);$s.lux=180.8}
 $rows=Compare-PermutationMeans $low $high 'fixed-side' $low
 if($rows.Count-ne24 -or @($rows|Where-Object{$_.passed}).Count-ne0){throw 'Wrong fixed reference side'}
}

Pass 'All384 fixed comparisons include192ladder96halves48depth48cross-seed' {
 $m=Measure-PermutationStream $raw $zero $zero;$records=@{}
 foreach($case in Get-PermutationCases){$records[$case.id]=@{completed=$true;diagnostics=$m;mean=$m.prefix_means['256']}}
 $rows=Get-PermutationComparisons $records
 if($rows.Count-ne384 -or @($rows|Where-Object{-not$_.check.passed}).Count-ne96){throw 'Missing or hidden per-stream failures'}
 foreach($group in @(@('64-to128',96),@('128-to256',96),@('disjoint-halves',96),@('seed0-depth',24),@('seed1-depth',24),@('baseline-cross-seed',24),@('depth-cross-seed',24))){if(@($rows|Where-Object{$_.check.comparison-ceq$group[0]}).Count-ne$group[1]){throw 'Fixed comparison identities/counts changed'}}
}
Reject 'Missing stream never promotes aggregate' {Get-PermutationComparisons @{}}
Reject 'Wrong stream never substitutes one seed' {$records=@{};foreach($case in Get-PermutationCases){$records[$case.id]=@{completed=$true;diagnostics=@{checks=@()}}};$records.Remove('seed1-depth');$records['other']=1;Get-PermutationComparisons $records}
$rawAscii=((0..36863)-join' ')
Pass 'Strict ASCII exact scalars preserved' {$v=ConvertFrom-PermutationAscii $rawAscii;for($i=0;$i-lt36864;$i++){if($v[$i]-ne$i){throw 'Raw scalar differs'}}}
foreach($bad in '1e-999','-1e-999','NaN','Infinity','1,000','1e999','-1',"1`v2") {Reject "Strict raw rejects $bad" {ConvertFrom-PermutationAscii ($bad+' '+((1..36863)-join' '))}}
Pass 'Textual exact zero exponent is accepted' {$v=ConvertFrom-PermutationAscii ('0e-999 '+((1..36863)-join' '));if($v[0]-ne0){throw 'Exact zero differs'}}
function ProcessFixture {
 [pscustomobject]@{executable='rcontrib.exe';arguments=@('synthetic');exit_code=0;stderr='';started_utc='2026-10-11T00:00:00.0000000Z';finished_utc='2026-10-11T00:00:01.0000000Z';wall_seconds=1.;sampled_peak_working_set_bytes=1024;sampled_runner_plus_engine_peak_bytes=2048;sampled_scratch_peak_bytes=500;input_bytes=100;input_transport='owned-file-stream-65536';input_receipt=[pscustomobject]@{};output_bytes=200;terminated_tree_reason=$null;job_assignment='CreateProcess STARTUPINFOEX JOB_LIST';job_empty=$true;stream_tasks_completed=$true;completed=$true;limits=@{timeout_seconds=7200;cleanup_seconds=5;memory_bytes=2147483648L;scratch_bytes=2147483648L;file_bytes=536870912L}}
}
function CheckProcess($p){$null=Assert-PermutationProcess $p 'rcontrib.exe' @('synthetic') 200 100 'owned-file-stream-65536' (Assert-PermutationUtc '2026-10-10T23:59:59Z')}
Pass 'Exact complete file-input receipt and UTC process accepted' {CheckProcess (ProcessFixture)}
foreach($bad in @($null,$false,1,'{}',[pscustomobject]@{Bytes=100},@{})) {Reject "Wrong owned-file receipt type $bad" {$p=ProcessFixture;$p.input_receipt=$bad;CheckProcess $p}}
Pass 'No-input receipt is exactly null' {$p=ProcessFixture;$p.input_receipt=$null;$p.input_bytes=0;$p.input_transport='small-text';$null=Assert-PermutationProcess $p 'rcontrib.exe' @('synthetic') 200 0 'small-text' (Assert-PermutationUtc '2026-10-10T23:59:59Z')}
Reject 'No-input empty object is not null receipt' {$p=ProcessFixture;$p.input_bytes=0;$p.input_transport='small-text';Assert-PermutationProcess $p 'rcontrib.exe' @('synthetic') 200 0 'small-text' (Assert-PermutationUtc '2026-10-10T23:59:59Z')}
Reject 'Missing receipt field rejected' {$p=ProcessFixture;$p.PSObject.Properties.Remove('input_receipt');CheckProcess $p}
Reject 'First process cannot precede declaration' {$p=ProcessFixture;Assert-PermutationProcess $p 'rcontrib.exe' @('synthetic') 200 100 'owned-file-stream-65536' (Assert-PermutationUtc '2026-10-11T01:00:00Z')}
foreach($bad in '2026-10-11T00:00:00','2026-10-11T08:00:00+08:00',$null,1) {Reject "NonUTC declaration $bad" {Assert-PermutationUtc $bad}}
foreach($field in 'wall_seconds','sampled_scratch_peak_bytes','sampled_runner_plus_engine_peak_bytes','input_bytes','output_bytes'){
 foreach($bad in $null,$true,'1',[double]::NaN,[double]::PositiveInfinity){Reject "Process $field invalid $bad" {$p=ProcessFixture;$p.$field=$bad;CheckProcess $p}}
}
foreach($field in 'completed','job_empty','stream_tasks_completed'){Reject "Incomplete process $field" {$p=ProcessFixture;$p.$field=$false;CheckProcess $p}}
$fixtureHost=[pscustomobject]@{schema_version='greenlens.sanitized-host.v1';recorded_utc='2026-10-11T00:00:00Z';os=[pscustomobject]@{caption='Synthetic OS';version='1';build='1';architecture='64-bit'};processors=@([pscustomobject]@{model='Synthetic processor';cores=2;logical_processors=4});physical_memory_bytes=1073741824;powershell_version='7.5.0'}
Pass 'Strict allowlisted synthetic hardware record' {Assert-PermutationHost $fixtureHost}
Reject 'Host inventory cannot retain account or hostname field' {$h=$fixtureHost|ConvertTo-Json -Depth 10|ConvertFrom-Json -Depth 10 -DateKind String;$h|Add-Member NoteProperty hostname 'private';Assert-PermutationHost $h}
Reject 'Host CPU count cannot be numeric string' {$h=$fixtureHost|ConvertTo-Json -Depth 10|ConvertFrom-Json -Depth 10 -DateKind String;$h.processors[0].cores='2';Assert-PermutationHost $h}

$null=New-Item -ItemType Directory -Path $OutputDirectory
$result=[ordered]@{schema_version='greenlens.permutation-synthetic-checks.v1';recorded_utc=[DateTime]::UtcNow.ToString('o');passed=$checks.Count;checks=$checks;scope='Original synthetic tagged arrays, forced rejection and arithmetic boundaries only; no engine';mapped_raw_scalar_checks=73728;primary_line_checks=12288;proposal_sha256=(Get-FileHash "$PSScriptRoot/permutation-proposal.json").Hash.ToLowerInvariant();source_sha256=@{helper=(Get-FileHash "$PSScriptRoot/permutation-helpers.ps1").Hash.ToLowerInvariant();order=(Get-FileHash "$PSScriptRoot/permutation-order.cs").Hash.ToLowerInvariant();test=(Get-FileHash $PSCommandPath).Hash.ToLowerInvariant()};engine_executed=$false;reference_family_qualified=$false}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'tests.json'),($result|ConvertTo-Json -Depth 30)+"`n",[Text.UTF8Encoding]::new($false))
$result|ConvertTo-Json -Depth 30
