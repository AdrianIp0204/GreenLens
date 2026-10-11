# Research preparation only: fixed A4116 diagnostic, not a reference qualification.
function Get-DepthOrderCases {
 @(
  [ordered]@{id='ab10-original';ab=10;lr=-16;order='original'},
  [ordered]@{id='lr24-original';ab=9;lr=-24;order='original'},
  [ordered]@{id='baseline-reversed';ab=9;lr=-16;order='reversed'},
  [ordered]@{id='depth-reversed';ab=10;lr=-24;order='reversed'}
 )
}
function Assert-DepthNumber($Value) {
 if($null -eq $Value -or $Value -is [bool] -or $Value -is [string] -or ![double]::IsFinite([double]$Value)){throw 'Expected a finite JSON number.'}
}
function Get-DepthCase([string]$Id) {
 $found=@(Get-DepthOrderCases|Where-Object id -CEQ $Id)
 if($found.Count -ne 1){throw 'Unknown fixed depth/order diagnostic case.'};return $found[0]
}
function Assert-DepthPlan($Plan) {
 if($Plan.schema_version-cne'greenlens.depth-order-plan.v1' -or $Plan.execution_authorized-isnot[bool]){throw 'Unknown diagnostic plan.'}
 foreach($p in @(@('sensors',24),@('rounds',256),@('ambient_divisions',8192),@('source_scale',100),@('interval_index',4116),@('maximum_parallel_jobs',2),@('engine_workers_per_job',1),@('process_seconds',7200),@('maximum_file_bytes',536870912),@('maximum_scratch_bytes',2147483648),@('maximum_memory_bytes',2147483648))){Assert-DepthNumber $Plan.($p[0]);if($Plan.($p[0])-ne$p[1]){throw 'Changed diagnostic setting or resource bound.'}}
 if($Plan.alternative-cne'A' -or $Plan.ambient_weight-cne'1e-9' -or $Plan.sampling_mode-cne'-u-' -or $Plan.accumulation-cne'-c 1'){throw 'Changed diagnostic estimator.'}
 $expected=@(Get-DepthOrderCases);if(@($Plan.cases).Count-ne4){throw 'Exactly four cases required.'}
 for($i=0;$i-lt4;$i++){Assert-DepthNumber $Plan.cases[$i].ab;Assert-DepthNumber $Plan.cases[$i].lr;foreach($key in @('id','ab','lr','order')){if($Plan.cases[$i].$key-cne$expected[$i].$key){throw 'Diagnostic tuple/order changed.'}}}
}
function Assert-DepthProcess($Process,[string]$Executable,[string[]]$Arguments,[long]$OutputBytes,[long]$InputBytes,[string]$InputTransport) {
 if($Process.executable-cne$Executable -or @($Process.arguments).Count-ne$Arguments.Count){throw 'Process executable/argv shape differs.'}
 for($i=0;$i-lt$Arguments.Count;$i++){if($Process.arguments[$i]-isnot[string] -or $Process.arguments[$i]-cne$Arguments[$i]){throw 'Process argument differs.'}}
 foreach($flag in @('completed','job_empty','stream_tasks_completed')){if($Process.$flag-isnot[bool] -or !$Process.$flag){throw 'Process did not complete with an empty owned job and drained streams.'}}
 if($Process.terminated_tree_reason -or $Process.job_assignment-cne'CreateProcess STARTUPINFOEX JOB_LIST' -or $Process.stderr-isnot[string]){throw 'Process failure or wrong supervision.'}
 foreach($key in @('exit_code','wall_seconds','sampled_runner_plus_engine_peak_bytes','sampled_scratch_peak_bytes','output_bytes','input_bytes')){Assert-DepthNumber $Process.$key}
 if($null-ne$Process.sampled_peak_working_set_bytes){Assert-DepthNumber $Process.sampled_peak_working_set_bytes;if($Process.sampled_peak_working_set_bytes-lt0 -or $Process.sampled_peak_working_set_bytes-gt2147483648){throw 'Invalid sampled child memory.'}}
 foreach($p in @(@('timeout_seconds',7200),@('cleanup_seconds',5),@('memory_bytes',2147483648),@('scratch_bytes',2147483648),@('file_bytes',536870912))){Assert-DepthNumber $Process.limits.($p[0]);if($Process.limits.($p[0])-ne$p[1]){throw 'Changed process bound.'}}
 if($Process.exit_code-ne0 -or $Process.wall_seconds-lt0 -or $Process.wall_seconds-gt7205 -or $Process.sampled_runner_plus_engine_peak_bytes-le0 -or $Process.sampled_runner_plus_engine_peak_bytes-gt2147483648 -or $Process.sampled_scratch_peak_bytes-lt0 -or $Process.sampled_scratch_peak_bytes-gt2147483648 -or $Process.output_bytes-lt0 -or $Process.output_bytes-gt536870912 -or $Process.output_bytes-ne$OutputBytes -or $Process.input_bytes-ne$InputBytes -or $Process.input_transport-cne$InputTransport){throw 'Measured process/resource/input evidence differs.'}
 foreach($field in @('started_utc','finished_utc')){if($Process.$field-isnot[string] -or $Process.$field-cnotmatch'(Z|\+00:00)$'){throw 'UTC timestamp string required.'}}
 $start=[DateTimeOffset]::Parse($Process.started_utc,[Globalization.CultureInfo]::InvariantCulture)
 $finish=[DateTimeOffset]::Parse($Process.finished_utc,[Globalization.CultureInfo]::InvariantCulture)
 if($finish-lt$start){throw 'Process time is reversed.'}
 return @{start=$start;finish=$finish}
}
function Get-DepthMapping([string]$Order) {
 if($Order -ceq 'original'){return ,([int[]](0..23))}
 if($Order -ceq 'reversed'){return ,([int[]](23..0))}
 throw 'Unknown sensor order.'
}
function Assert-DepthMapping($Mapping,[string]$Order) {
 if(@($Mapping).Count -ne 24){throw 'Exactly24 mapped sensors required.'}
 $expected=Get-DepthMapping $Order
 for($i=0;$i -lt 24;$i++){Assert-DepthNumber $Mapping[$i];if($Mapping[$i] -ne $expected[$i]){throw 'Sensor order differs from declared exact permutation.'}}
}
function New-DepthStream([string]$Sensors,[string]$Order) {
 # Input is already hash-bound by the strict reference reader; preserve each row.
 if(!$Sensors.EndsWith("`n") -or $Sensors.Contains("`r")){throw 'Canonical sensor input must use LF and end in LF.'}
 $lines=$Sensors.Substring(0,$Sensors.Length-1).Split("`n")
 if($lines.Count -ne 24){throw 'Exactly24 canonical sensor rows required.'}
 $culture=[Globalization.CultureInfo]::InvariantCulture
 foreach($line in $lines){$tokens=@($line -split '\s+'|Where-Object{$_});if($tokens.Count-ne6){throw 'Sensor row is not six values.'};foreach($token in $tokens){$v=0.;if(![double]::TryParse($token,[Globalization.NumberStyles]::Float,$culture,[ref]$v) -or ![double]::IsFinite($v)){throw 'Invalid sensor value.'}}}
 $map=Get-DepthMapping $Order;$one=(@($map|ForEach-Object{$lines[$_]})-join "`n")+"`n"
 $text=$one*256;$bytes=[Text.UTF8Encoding]::new($false,$true).GetBytes($text)
 [ordered]@{text=$text;bytes=$bytes.LongLength;sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant();records=6144;rounds=256;sensors=24;emitted_to_original=@($map);emitted_sensor_ids=@($map|ForEach-Object{'s{0:D6}' -f ($_+1)});ordering='round-major; emitted position maps to original stable sensor ID'}
}
function ConvertFrom-DepthOrder([double[]]$Values,$Mapping,[string]$Order) {
 Assert-DepthMapping $Mapping $Order
 if($Values.Length-ne36864){throw 'Truncated or excess24x256x6 raw array.'}
 $out=[double[]]::new($Values.Length)
 for($round=0;$round-lt256;$round++){for($emitted=0;$emitted-lt24;$emitted++){for($channel=0;$channel-lt6;$channel++){
  $value=$Values[($round*24+$emitted)*6+$channel]
  if(![double]::IsFinite($value) -or $value-lt0){throw 'Raw coefficient bins must be finite and nonnegative.'}
  $out[($round*24+$Mapping[$emitted])*6+$channel]=$value
 }}}
 return ,$out
}
function Get-DepthArguments([string]$Id,[string]$Modifier) {
 if($Modifier -cne 'solar246990'){throw 'Expected the fixed A4116 source.'}
 $c=Get-DepthCase $Id
 return @('-I+','-h','-faa','-n','1','-ab',"$($c.ab)",'-ad','8192','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr',"$($c.lr)",'-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-c','1','-V+','-y','6144','-m','sky_glow','-m',$Modifier,'diagnostic.oct')
}
function Assert-DepthMean($Mean) {
 if(@($Mean.sensors).Count-ne24){throw 'Mean lacks24 sensors.'}
 for($i=0;$i-lt24;$i++){
  $s=$Mean.sensors[$i];if($s.sensor_index-ne$i -or $s.sensor_id-cne('s{0:D6}' -f ($i+1))){throw 'Canonical sensor identity/order differs.'}
  foreach($key in @('diffuse_rgb','full_solar_rgb','centroid_direct_rgb','finite_direct_rgb','total_rgb')){
   if(@($s.$key).Count-ne3){throw 'Expected RGB.'};foreach($v in $s.$key){Assert-DepthNumber $v;if($v-lt0){throw 'Negative scientific component.'}}
  }
  Assert-DepthNumber $s.lux
  $lux=179*(.265074126*$s.total_rgb[0]+.670114631*$s.total_rgb[1]+.064811243*$s.total_rgb[2])
  if([Math]::Abs($lux-$s.lux)-gt1e-9){throw 'Stored lux differs from RGB.'}
  for($c=0;$c-lt3;$c++){if([Math]::Abs($s.total_rgb[$c]-($s.diffuse_rgb[$c]+$s.full_solar_rgb[$c]-$s.centroid_direct_rgb[$c]+$s.finite_direct_rgb[$c]))-gt1e-9){throw 'Component assembly differs.'}}
 }
}
function Get-DepthEffects($Baseline,$AbOnly,$LrOnly,$Depth,$BaselineReversed,$DepthReversed) {
 $all=@($Baseline,$AbOnly,$LrOnly,$Depth,$BaselineReversed,$DepthReversed);foreach($mean in $all){Assert-DepthMean $mean}
 $rows=@();$summaries=@{}
 $combinations=[ordered]@{
  ab_at_lr16=@(-1,1,0,0,0,0);ab_at_lr24=@(0,0,-1,1,0,0);
  lr_at_ab9=@(-1,0,1,0,0,0);lr_at_ab10=@(0,-1,0,1,0,0);
  ab_lr_interaction=@(1,-1,-1,1,0,0);baseline_order_effect=@(-1,0,0,0,1,0);
  depth_order_effect=@(0,0,0,-1,0,1);original_depth_gap=@(-1,0,0,1,0,0);
  reversed_depth_gap=@(0,0,0,0,-1,1);depth_gap_order_interaction=@(1,0,0,-1,-1,1)
 }
 for($i=0;$i-lt24;$i++){
  foreach($mean in $all){foreach($key in @('centroid_direct_rgb','finite_direct_rgb')){for($c=0;$c-lt3;$c++){if($mean.sensors[$i].$key[$c]-ne$Baseline.sensors[$i].$key[$c]){throw 'Finite/centroid correction changed.'}}}}
  $effects=[ordered]@{}
  foreach($name in $combinations.Keys){$components=[ordered]@{}
   foreach($key in @('diffuse_rgb','full_solar_rgb','centroid_direct_rgb','finite_direct_rgb','total_rgb')){
    $rgb=[double[]]::new(3);for($k=0;$k-lt6;$k++){for($c=0;$c-lt3;$c++){$rgb[$c]+=$combinations[$name][$k]*$all[$k].sensors[$i].$key[$c]}}
    $components[$key]=[ordered]@{delta_rgb=$rgb;delta_lux=179*(.265074126*$rgb[0]+.670114631*$rgb[1]+.064811243*$rgb[2])}
   }
   $effects[$name]=$components
   if(!$summaries.ContainsKey($name)){$summaries[$name]=[Collections.Generic.List[double]]::new()};$summaries[$name].Add($components.total_rgb.delta_lux)
  }
  $rows+=,[ordered]@{sensor_index=$i;sensor_id=$Baseline.sensors[$i].sensor_id;effects=$effects}
 }
 $summary=[ordered]@{};foreach($name in $combinations.Keys){$v=$summaries[$name];$sum=0.;$squares=0.;$max=0.;foreach($x in $v){$sum+=$x;$squares+=$x*$x;$max=[Math]::Max($max,[Math]::Abs($x))};$summary[$name]=[ordered]@{mean_signed_lux=$sum/24;maximum_absolute_lux=$max;rms_lux=[Math]::Sqrt($squares/24)}}
 [ordered]@{schema_version='greenlens.depth-order-effects.v1';scope='Descriptive fixed setting/order estimator effects; no unique physical cause, qualification, or threshold-based acceptance';sensor_count=24;rows=$rows;summary=$summary;reference_family_qualified=$false;release_accepted=$false}
}

function Get-DepthBytesHash([byte[]]$Bytes){[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant()}
function Read-DepthFile([string]$Path,[long]$MaximumBytes) {
 if($MaximumBytes-lt1 -or $MaximumBytes-gt2097152){throw 'Invalid bounded reader policy.'}
 $full=[IO.Path]::GetFullPath($Path);$item=Get-Item -LiteralPath $full -Force
 if($item.PSIsContainer -or ($item.Attributes-band([IO.FileAttributes]::ReparsePoint-bor[IO.FileAttributes]::Offline))){throw 'Local regular file required.'}
 for($parent=$item.Directory;$null-ne$parent;$parent=$parent.Parent){if($parent.Attributes-band[IO.FileAttributes]::ReparsePoint){throw 'Reparse ancestor rejected.'}}
 $input=[IO.File]::Open($full,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try{
  if($input.Length-gt$MaximumBytes){throw 'Bounded input is oversized.'}
  $memory=[IO.MemoryStream]::new();try{$buffer=[byte[]]::new(65536);$count=0L;while(($n=$input.Read($buffer,0,[int][Math]::Min($buffer.Length,$MaximumBytes+1-$count)))-gt0){$count+=$n;if($count-gt$MaximumBytes){throw 'Input grew beyond bound.'};$memory.Write($buffer,0,$n)};return ,$memory.ToArray()}finally{$memory.Dispose()}
 }finally{$input.Dispose()}
}

function ConvertFrom-DepthAscii([string]$Text){
 $tokens=@($Text -split '\s+'|Where-Object{$_})
 if($tokens.Count-ne36864){throw 'Expected exactly36,864 raw scalars.'}
 $values=[double[]]::new(36864)
 for($n=0;$n-lt$tokens.Count;$n++){$value=0.;if(![double]::TryParse($tokens[$n],[Globalization.NumberStyles]::Float,[Globalization.CultureInfo]::InvariantCulture,[ref]$value) -or ![double]::IsFinite($value) -or $value-lt0){throw 'Raw bins must be finite nonnegative numbers.'};$values[$n]=$value}
 return ,$values
}
