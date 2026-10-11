#Requires -Version 7.5
param([string]$OutputDirectory='', [string]$TrustedSourceRoot=(Get-Location).Path)
$testOutput=$OutputDirectory
$testTrustedSource=$TrustedSourceRoot
$ErrorActionPreference='Stop'
. "$PSScriptRoot/depth-order-helpers.ps1"
$checks=[Collections.Generic.List[string]]::new()
function Pass([string]$Name,[scriptblock]$Check){& $Check;$checks.Add($Name)}
function Reject([string]$Name,[scriptblock]$Check){$caught=$false;try{& $Check}catch{$caught=$true};if(!$caught){throw "Accepted invalid fixture: $Name"};$checks.Add($Name)}
$sensors=(@(0..23|ForEach-Object{"$_ 0 0.8 0 0 1"})-join"`n")+"`n"
Pass 'Fixed four tuples and exact argument differences' {
 $cases=@(Get-DepthOrderCases);if($cases.Count-ne4){throw 'Cases differ'}
 foreach($c in $cases){$a=Get-DepthArguments $c.id 'solar246990';if($a.Count-ne46 -or $a[6]-ne"$($c.ab)" -or $a[22]-ne"$($c.lr)" -or $a[38]-cne'-V+' -or $a[40]-cne'6144'){throw 'Arguments differ'}}
}
$original=New-DepthStream $sensors 'original';$reverse=New-DepthStream $sensors 'reversed'
Pass 'All6144 emitted rows preserve exact round boundaries and sensor coordinates' {
 $origRows=$original.text.TrimEnd("`n").Split("`n");$revRows=$reverse.text.TrimEnd("`n").Split("`n")
 if($origRows.Count-ne6144 -or $revRows.Count-ne6144){throw 'Count differs'}
 for($r=0;$r-lt256;$r++){for($i=0;$i-lt24;$i++){if($origRows[$r*24+$i]-cne"$i 0 0.8 0 0 1" -or $revRows[$r*24+$i]-cne"$(23-$i) 0 0.8 0 0 1"){throw 'Ordered primary differs'}}}
 if($original.sha256-ceq$reverse.sha256 -or $original.bytes-ne$reverse.bytes){throw 'Stream identity differs'}
}
Pass 'Raw six-channel unpermutation preserves every round/channel exactly' {
 $raw=[double[]]::new(36864);for($r=0;$r-lt256;$r++){for($i=0;$i-lt24;$i++){for($c=0;$c-lt6;$c++){$raw[($r*24+$i)*6+$c]=$r*10000+(23-$i)*100+$c}}}
 $out=ConvertFrom-DepthOrder $raw $reverse.emitted_to_original 'reversed'
 for($r=0;$r-lt256;$r++){for($i=0;$i-lt24;$i++){for($c=0;$c-lt6;$c++){if($out[($r*24+$i)*6+$c]-ne($r*10000+$i*100+$c)){throw 'Component permutation differs'}}}}
}
Reject 'Unknown case' {Get-DepthCase 'adaptive'}
Reject 'Wrong solar interval' {Get-DepthArguments 'ab10-original' 'solar510750'}
Reject 'Unknown sensor order' {Get-DepthMapping 'random'}
Reject 'Missing sensor row' {New-DepthStream (($sensors.TrimEnd("`n").Split("`n")[0..22]-join"`n")+"`n") 'original'}
Reject 'Additional trailing blank record' {New-DepthStream ($sensors+"`n") 'original'}
Reject 'Nonfinite sensor input' {New-DepthStream $sensors.Replace('0.8','NaN') 'original'}
Reject 'Noncanonical CRLF stream' {New-DepthStream $sensors.Replace("`n","`r`n") 'original'}
Reject 'Duplicate sensor mapping' {$m=[int[]](23..0);$m[1]=$m[0];Assert-DepthMapping $m 'reversed'}
Reject 'Original mapping labelled reversed' {Assert-DepthMapping ([int[]](0..23)) 'reversed'}
Reject 'String mapping entry' {$m=[object[]](23..0);$m[0]='23';Assert-DepthMapping $m 'reversed'}
Reject 'Truncated native output' {ConvertFrom-DepthOrder ([double[]]::new(36863)) ([int[]](0..23)) 'original'}
Reject 'Nonfinite native output' {$v=[double[]]::new(36864);$v[100]=[double]::NaN;ConvertFrom-DepthOrder $v ([int[]](0..23)) 'original'}
Reject 'Negative native output' {$v=[double[]]::new(36864);$v[100]=-1;ConvertFrom-DepthOrder $v ([int[]](0..23)) 'original'}
function Mean([double[]]$Extra){$rows=@();for($i=0;$i-lt24;$i++){$d=@((1.+$Extra[0]),(2.+$Extra[1]),(3.+$Extra[2]));$s=@(.1,.2,.3);$rgb=@(($d[0]+.1),($d[1]+.2),($d[2]+.3));$rows+=,[ordered]@{sensor_index=$i;sensor_id=('s{0:D6}'-f($i+1));diffuse_rgb=$d;full_solar_rgb=$s;centroid_direct_rgb=@(0.,0.,0.);finite_direct_rgb=@(0.,0.,0.);total_rgb=$rgb;lux=179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])}};@{sensors=$rows}}
$means=@((Mean @(0,0,0)),(Mean @(2,0,0)),(Mean @(0,3,0)),(Mean @(2,3,4)),(Mean @(5,0,0)),(Mean @(9,3,4)))
Pass 'Independent non-gray factorial and order interactions' {
 $e=Get-DepthEffects @means
 foreach($r in $e.rows){
  if(($r.effects.ab_lr_interaction.diffuse_rgb.delta_rgb-join',')-cne'0,0,4'){throw 'Factorial interaction differs'}
  if(($r.effects.depth_gap_order_interaction.total_rgb.delta_rgb-join',')-cne'2,0,0'){throw 'Order-gap interaction differs'}
  if([Math]::Abs($r.effects.ab_lr_interaction.total_rgb.delta_lux-179*.064811243*4)-gt1e-10){throw 'Independent non-gray lux differs'}
  if(($r.effects.lr_at_ab9.diffuse_rgb.delta_rgb-join',')-cne'0,3,0'){throw 'Isolated setting difference differs'}
 }
 if($e.reference_family_qualified -or $e.release_accepted){throw 'Diagnostic claimed qualification'}
}
Reject 'Cross-case stable sensor ID changed' {$m=Mean @(0,0,0);$m.sensors[0].sensor_id='s000024';Assert-DepthMean $m}
Reject 'Lux cannot be Boolean' {$m=Mean @(0,0,0);$m.sensors[0].lux=$true;Assert-DepthMean $m}
Reject 'RGB cannot be numeric string' {$m=Mean @(0,0,0);$m.sensors[0].diffuse_rgb[0]='1';Assert-DepthMean $m}
Reject 'Wrong scientific component sum' {$m=Mean @(0,0,0);$m.sensors[0].total_rgb[0]+=1;Assert-DepthMean $m}
Reject 'Changed finite correction' {$m=Mean @(0,0,0);$m.sensors[0].finite_direct_rgb[0]=1;$m.sensors[0].total_rgb[0]+=1;$m.sensors[0].lux+=179*.265074126;Get-DepthEffects $means[0] $m $means[2] $means[3] $means[4] $means[5]}

Pass 'ASCII scientific raw parser preserves every channel and order' {$raw=ConvertFrom-DepthAscii ((0..36863)-join ' ');if($raw.Length-ne36864){throw 'Count differs'};for($i=0;$i-lt36864;$i++){if($raw[$i]-ne$i){throw 'Raw order differs'}}}
Reject 'ASCII raw parser rejects truncated count' {ConvertFrom-DepthAscii ((0..36862)-join ' ')}
Reject 'ASCII raw parser rejects nonnumeric token' {ConvertFrom-DepthAscii ('bad '+((1..36863)-join ' '))}
Reject 'ASCII raw parser rejects nonfinite token' {ConvertFrom-DepthAscii ('NaN '+((1..36863)-join ' '))}
Reject 'ASCII raw parser rejects negative token' {ConvertFrom-DepthAscii ('-1 '+((1..36863)-join ' '))}

function ProcessFixture { [ordered]@{executable='rcontrib.exe';arguments=@('synthetic');exit_code=0;stderr='';started_utc='2026-10-11T00:00:00.0000000Z';finished_utc='2026-10-11T00:00:01.0000000Z';wall_seconds=1.;sampled_peak_working_set_bytes=1024;sampled_runner_plus_engine_peak_bytes=2048;sampled_scratch_peak_bytes=500;input_bytes=100;input_transport='owned-file-stream-65536';input_receipt=$null;output_bytes=200;terminated_tree_reason=$null;job_assignment='CreateProcess STARTUPINFOEX JOB_LIST';job_empty=$true;stream_tasks_completed=$true;completed=$true;limits=@{timeout_seconds=7200;cleanup_seconds=5;memory_bytes=2147483648L;scratch_bytes=2147483648L;file_bytes=536870912L}} }
function CheckProcess($p){$null=Assert-DepthProcess $p 'rcontrib.exe' @('synthetic') 200 100 'owned-file-stream-65536'}
Pass 'Finite typed complete bounded synthetic process accepted' {CheckProcess (ProcessFixture)}
foreach($field in @('exit_code','wall_seconds','sampled_runner_plus_engine_peak_bytes','sampled_scratch_peak_bytes','output_bytes','input_bytes')){
 foreach($bad in @($null,$true,'1',[double]::NaN,[double]::PositiveInfinity)){
  Reject "Process $field rejects invalid numeric type/value [$bad]" {$p=ProcessFixture;$p[$field]=$bad;CheckProcess $p}
 }
}
foreach($field in @('timeout_seconds','cleanup_seconds','memory_bytes','scratch_bytes','file_bytes')){
 Reject "Process limit $field rejects numeric string" {$p=ProcessFixture;$p.limits[$field]="$($p.limits[$field])";CheckProcess $p}
}
foreach($field in @('completed','job_empty','stream_tasks_completed')){
 foreach($bad in @($false,1,'true',$null)){Reject "Process Boolean $field rejects [$bad]" {$p=ProcessFixture;$p[$field]=$bad;CheckProcess $p}}
}
Reject 'Process argument mutation' {$p=ProcessFixture;$p.arguments=@('changed');CheckProcess $p}
Reject 'Process failure reason cannot be retained success' {$p=ProcessFixture;$p.terminated_tree_reason='deadline';CheckProcess $p}
Reject 'Process output must match actual bytes' {$p=ProcessFixture;$p.output_bytes=199;CheckProcess $p}
Reject 'Process input must match complete primary bytes' {$p=ProcessFixture;$p.input_bytes=99;CheckProcess $p}
Reject 'Process timeout cap' {$p=ProcessFixture;$p.wall_seconds=7206;CheckProcess $p}
Reject 'Process memory cap' {$p=ProcessFixture;$p.sampled_runner_plus_engine_peak_bytes=2147483649L;CheckProcess $p}
Reject 'Process scratch cap' {$p=ProcessFixture;$p.sampled_scratch_peak_bytes=2147483649L;CheckProcess $p}
Reject 'Process negative measured size' {$p=ProcessFixture;$p.sampled_scratch_peak_bytes=-1;CheckProcess $p}
Reject 'Non-UTC process time' {$p=ProcessFixture;$p.started_utc='2026-10-11T00:00:00+08:00';CheckProcess $p}
Reject 'Reversed process times' {$p=ProcessFixture;$p.finished_utc='2026-10-10T00:00:00Z';CheckProcess $p}
Reject 'Invalid process time string' {$p=ProcessFixture;$p.finished_utc='bogusZ';CheckProcess $p}
$fixtureRoot=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/depth pure fixtures '+[guid]::NewGuid().ToString('N')+' 空間')
$null=New-Item -ItemType Directory -Path $fixtureRoot
$bounded=Join-Path $fixtureRoot 'bounded.txt';[IO.File]::WriteAllBytes($bounded,[byte[]](0..255))
Pass 'Bounded reader reads exact bytes and hash' {$b=Read-DepthFile $bounded 256;if($b.Length-ne256 -or (Get-DepthBytesHash $b)-cne'40aff2e9d2d8922e47afd4648e6967497158785fbd1da870e7110266bf944880'){throw 'Bounded bytes differ'}}
Reject 'Bounded reader rejects excess byte' {Read-DepthFile $bounded 255}
Reject 'Bounded reader rejects directory' {Read-DepthFile $fixtureRoot 256}
Reject 'Bounded reader rejects policy expansion' {Read-DepthFile $bounded 2097153}

if(Test-Path "$PSScriptRoot/depth-order-plan.json"){
 $plan=Get-Content "$PSScriptRoot/depth-order-plan.json" -Raw|ConvertFrom-Json -Depth 60 -DateKind String
 Pass 'Exact frozen plan accepted' {Assert-DepthPlan $plan}
 Reject 'Missing fixed case' {$p=$plan|ConvertTo-Json -Depth 60|ConvertFrom-Json -Depth 60;$p.cases=@($p.cases|Select-Object -First 3);Assert-DepthPlan $p}
 Reject 'Unapproved larger repeat count' {$p=$plan|ConvertTo-Json -Depth 60|ConvertFrom-Json -Depth 60;$p.rounds=512;Assert-DepthPlan $p}
 Reject 'Changed deadline' {$p=$plan|ConvertTo-Json -Depth 60|ConvertFrom-Json -Depth 60;$p.process_seconds=7201;Assert-DepthPlan $p}
}

# Original tiny archive fixtures test the actual archive guard, never a scientific engine.
. "$PSScriptRoot/run-hosted-depth-order.ps1" -FunctionsOnly
$memberNames=@('aggregation-source.cs','declaration.json','depth10-24.oct','depth10-24.rad','depth10-24.rgb','finite-kernel.cs','harness-source.ps1','hour-4116-direct.rgb','hour-4116-finite.rgb','hour-4116-rounds256.rgb','hour-4116-scaled.rad','hour-4116-sun.rad','hour-4116.dat','matched-basis.cal','process-source.ps1','processes.json','scene.rad','sensitivity.json','sensors.pts')
$identity=@{declaration_sha256=('a'*64);report_sha256=('b'*64);processes_sha256=('c'*64)}
function ArchiveFixture([string]$Name,[string]$Mutation=''){
 $utf8=[Text.UTF8Encoding]::new($false);$items=@();$payloads=[ordered]@{}
 foreach($n in $memberNames){$bytes=$utf8.GetBytes("synthetic $n`n");$items+=@{name=$n;bytes=$bytes.Length;sha256=(Get-DepthBytesHash $bytes)};$payloads[$n]=$bytes}
 if($Mutation-ceq'member-hash'){$items[0].sha256='0'*64}
 $manifest=@{schema_version='greenlens.failed-depth-inputs.v1';alternative='A';interval_index=4116;members=$items;declaration_sha256=$identity.declaration_sha256;report_sha256=$identity.report_sha256;processes_sha256=$identity.processes_sha256}
 if($Mutation-ceq'cross-case'){$manifest.interval_index=8512}
 $mb=$utf8.GetBytes(($manifest|ConvertTo-Json -Depth 20)+"`n");$archive=Join-Path $fixtureRoot ($Name+'.zip');$zip=[IO.Compression.ZipFile]::Open($archive,[IO.Compression.ZipArchiveMode]::Create)
 $total=$mb.Length
 try{
  $e=$zip.CreateEntry('input-manifest.json');$o=$e.Open();try{$o.Write($mb)}finally{$o.Dispose()}
  foreach($n in $memberNames){$entryName=if($Mutation-ceq'traversal' -and $n-ceq'scene.rad'){'../scene.rad'}else{$n};$e=$zip.CreateEntry($entryName);$o=$e.Open();try{$o.Write($payloads[$n])}finally{$o.Dispose()};$total+=$payloads[$n].Length}
 }finally{$zip.Dispose()}
 @{path=$archive;pin=@{bytes=(Get-Item $archive).Length;sha256=(Get-FileHash $archive).Hash.ToLowerInvariant();manifest_sha256=(Get-DepthBytesHash $mb);member_count=20;uncompressed_bytes=$total}}
}
Pass 'Exact tiny endpoint archive extracted with all member seals' {$a=ArchiveFixture 'valid';$dest=Join-Path $fixtureRoot 'valid-extracted';$null=Expand-DepthEndpoint $a.path $dest $a.pin $identity $testTrustedSource;if(@(Get-ChildItem $dest -File).Count-ne20){throw 'Extraction count differs'}}
foreach($mutation in @('member-hash','cross-case','traversal')){Pass "Endpoint archive rejects $mutation before extraction" {$a=ArchiveFixture $mutation $mutation;$dest=Join-Path $fixtureRoot ($mutation+'-extracted');$caught=$false;try{$null=Expand-DepthEndpoint $a.path $dest $a.pin $identity $testTrustedSource}catch{$caught=$true};if(!$caught){throw 'Invalid archive accepted'};if(Test-Path $dest){throw 'Invalid archive created destination'}}}
Reject 'Endpoint archive source digest mutation' {$a=ArchiveFixture 'wrong-archive';$a.pin.sha256='0'*64;$null=Expand-DepthEndpoint $a.path (Join-Path $fixtureRoot 'wrong-archive-output') $a.pin $identity $testTrustedSource}
Reject 'Endpoint archive total-size mismatch' {$a=ArchiveFixture 'wrong-total';$a.pin.uncompressed_bytes++;$null=Expand-DepthEndpoint $a.path (Join-Path $fixtureRoot 'wrong-total-output') $a.pin $identity $testTrustedSource}

$result=[ordered]@{schema_version='greenlens.depth-order-synthetic-tests.v1';recorded_utc=[DateTime]::UtcNow.ToString('o');passed=$checks.Count;checks=$checks;scope='Synthetic original records and hand-derived arithmetic only; no native scientific process';helper_sha256=(Get-FileHash "$PSScriptRoot/depth-order-helpers.ps1").Hash.ToLowerInvariant();test_sha256=(Get-FileHash $PSCommandPath).Hash.ToLowerInvariant()}
if($testOutput){if(Test-Path $testOutput){throw 'Fresh test output required'};New-Item -ItemType Directory -Path $testOutput|Out-Null;$result|ConvertTo-Json -Depth 20|Set-Content (Join-Path $testOutput 'tests.json') -Encoding utf8}
$result|ConvertTo-Json -Depth 20
