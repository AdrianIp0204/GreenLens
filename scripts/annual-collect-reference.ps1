#Requires -Version 7.2
# Read-only reference qualification. Never executes retained scripts or commands.
# Supply all baseline shard directories and the two sensitivity directories.
# A new OutputDirectory receives a sealed compact snapshot. Partial snapshots are
# allowed, but -RequireQualified exits unsuccessfully unless all24 cases and both
# three-variant sensitivities verify. No candidate arrays are read or compared.
param([string[]]$BaselineDirectories=@(),[string[]]$SensitivityDirectories=@(),[string]$OutputDirectory='',[switch]$RequireQualified,[switch]$FunctionsOnly)
$ErrorActionPreference='Stop'
$script:referenceCollectorPath=$PSCommandPath
. "$PSScriptRoot/annual-reference-identity.ps1"
. "$PSScriptRoot/annual-matched-evidence.ps1"

function Read-ReferenceFile([string]$Root,[string]$Name,[long]$Maximum=536870912){
 if($Name -notmatch '^[A-Za-z0-9_.-]+$'){throw 'Unsafe reference member name.'}
 $rootItem=Get-Item -LiteralPath $Root -Force
 $item=Get-Item -LiteralPath (Join-Path $rootItem.FullName $Name) -Force
 if(!$rootItem.PSIsContainer -or $item.PSIsContainer -or ($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -gt $Maximum){throw "Reference member is not a bounded regular file: $Name"}
 return $item.FullName
}
function Read-ReferenceJson([string]$Root,[string]$Name){
 $text=[IO.File]::ReadAllText((Read-ReferenceFile $Root $Name 67108864))
 # Retain source strings rather than allowing new PowerShell date inference.
 $options=@{InputObject=$text;Depth=100}
 if((Get-Command ConvertFrom-Json).Parameters.ContainsKey('DateKind')){$options.DateKind='String'}
 return ConvertFrom-Json @options
}
function Assert-ReferenceSame($Actual,$Expected,[string]$Description){
 if((ConvertTo-ReferenceCanonicalJson $Actual) -cne (ConvertTo-ReferenceCanonicalJson $Expected)){throw "$Description differs from recomputed or declared evidence."}
}
function Reference-Hash([string]$Root,[string]$Name){return Get-MatchedHash (Read-ReferenceFile $Root $Name)}
function Assert-ReferenceHash([string]$Root,[string]$Name,[string]$Expected){
 if($Expected -cnotmatch '^[0-9a-f]{64}$' -or (Reference-Hash $Root $Name) -cne $Expected){throw "Retained reference bytes changed: $Name"}
}
function Initialize-ReferenceCollector {
 if(!('GreenLensReference.Aggregate' -as [type])){Add-Type -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs"}
 $source=[IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1")
 if(!('GreenLensMatched.Direct' -as [type])){
  $kernel=[regex]::Match($source,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
  if(!$kernel){throw 'Trusted finite-direct kernel is absent.'};Add-Type -TypeDefinition $kernel
 }
 if(!('GreenLensFeasibility.Matrix' -as [type])){
  $matrixSource=[IO.File]::ReadAllText("$PSScriptRoot/annual-feasibility.ps1")
  $matrix=[regex]::Match($matrixSource,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
  if(!$matrix){throw 'Trusted matrix reader is absent.'};Add-Type -TypeDefinition $matrix
 }
}
function Get-ReferenceOptions {
 return @('-I+','-h','-faa','-n','1','-ab','9','-ad','8192','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-c','1','-V+')
}
function Get-ReferenceDirectOptions {
 return @('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
}
function Assert-ReferenceDeclaration([string]$Root,$Declaration){
 $null=Assert-ReferenceShardIdentity @($Declaration)
 foreach($pair in @(@('sensor_count',24),@('sky_basis_mf',2),@('source_scale',100),@('ambient_divisions',8192),@('ambient_bounces',9),@('ray_depth',-16),@('ambient_weight',1e-9),@('finite_samples',4096),@('initial_process_rounds',256),@('optional_final_rung',512),@('maximum_process_seconds',7200),@('maximum_scratch_bytes',2147483648),@('maximum_single_file_bytes',536870912),@('maximum_runner_plus_engine_working_set_bytes',2147483648))){
  if($Declaration.($pair[0]) -ne $pair[1]){throw "Unsupported declared reference setting: $($pair[0])"}
 }
 Assert-ReferenceSame $Declaration.initial_rungs @(64,128,256) 'Initial repeat ladder'
 Assert-ReferenceSame $Declaration.full_arguments (Get-ReferenceOptions) 'Full reference arguments'
 Assert-ReferenceSame $Declaration.direct_arguments (Get-ReferenceDirectOptions) 'Direct reference arguments'
 Assert-ReferenceSame $Declaration.sensitivities.fixed_cases @('B:8512','A:4116') 'Fixed sensitivity cases'
 if($Declaration.sensitivities.scale_variant -ne 10000 -or $Declaration.sensitivities.weight_variant -ne 1e-10 -or $Declaration.sensitivities.depth_variant.ambient_bounces -ne 10 -or $Declaration.sensitivities.depth_variant.ray_depth -ne -24){throw 'Sensitivity declaration differs from the fixed protocol.'}
 foreach($pair in @(@('harness-source.ps1','harness_sha256','annual-controlled-reference.ps1'),@('aggregation-source.cs','aggregation_sha256','annual-reference-aggregation.cs'),@('process-source.ps1','process_helper_sha256','annual-reference-process.ps1'))){
  Assert-ReferenceHash $Root $pair[0] $Declaration.($pair[1])
  if((Get-MatchedHash "$PSScriptRoot/$($pair[2])") -cne $Declaration.($pair[1])){throw "Trusted collector helper differs from the declared source: $($pair[2])"}
 }
 if((Get-MatchedHash "$PSScriptRoot/../docs/v0.3-reference-protocol.md") -cne $Declaration.protocol_sha256 -or (Get-MatchedKernelHash ([IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1"))) -cne $Declaration.finite_kernel_sha256){throw 'Reference protocol or finite kernel identity differs.'}
 $kernel=[regex]::Match([IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1"),"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value.Replace("`r`n","`n")
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root 'finite-kernel.cs')).Replace("`r`n","`n") -cne $kernel){throw 'Retained finite kernel text differs.'}
 $inventory=Get-Content -LiteralPath "$PSScriptRoot/../dependencies/radiance.json" -Raw|ConvertFrom-Json
 $engine=[ordered]@{}
 foreach($name in @('rtrace','oconv')){$engine["$name.exe"]=$inventory.executables.$name.sha256}
 $engine['rcontrib.exe']=$inventory.annual_workflow_review.executables.rcontrib.sha256
 foreach($name in @('rayinit.cal','skybright.cal')){$engine[$name]=$inventory.critical_libraries.$name.sha256}
 $engine['reinhart.cal']=$inventory.annual_workflow_review.additional_critical_libraries.'reinhart.cal'.sha256
 Assert-ReferenceSame $Declaration.engine_sha256 $engine 'Pinned engine/library identity'
 $expectedNames=@('scene.rad','sensors.pts','identical-sky.mtx','suns.rad','weather.epw','source-recipe.json','model.json','study.json','weather-asset.json')
 Assert-ReferenceSame @($Declaration.input_sha256.PSObject.Properties.Name|Sort-Object) @($expectedNames|Sort-Object) 'Declared input member set'
 foreach($name in $expectedNames){Assert-ReferenceHash $Root $name $Declaration.input_sha256.$name}
 $model=Read-ReferenceJson $Root 'model.json';$study=Read-ReferenceJson $Root 'study.json';$recipe=Read-ReferenceJson $Root 'source-recipe.json';$asset=Read-ReferenceJson $Root 'weather-asset.json'
 $expectedModel=Get-Content -LiteralPath "$PSScriptRoot/../examples/synthetic-$($Declaration.alternative.ToLowerInvariant()).json" -Raw|ConvertFrom-Json
 $expectedModel.sensors.requested_spacing_m=1.0
 Assert-ReferenceSame $model $expectedModel 'Fixed complete A/B reference model'
 Assert-ReferenceSame $model $Declaration.normalized_model 'Normalized model'
 Assert-ReferenceSame $study.calculation.weather $Declaration.weather_binding 'Study weather binding'
 Assert-ReferenceSame $asset.binding $Declaration.weather_binding 'Retained weather asset binding'
 if($Declaration.weather_binding.original_sha256 -cne '60719262a01e8827e8984537800f97f508ab4ae5ae8abc55fce571893c55b2d8' -or $Declaration.weather_binding.normalized_sha256 -cne '5dec99cf1e82f5492f62640241618d2b591bc8852145a5c7a56ea5d60c4e82ed' -or $Declaration.weather_binding.time_axis_sha256 -cne 'f11e5fbc0273736b9a9f08afa8ea06503706a1c6489ca123ff5b2e260b893d2f'){throw 'Weather differs from the fixed reviewed year.'}
 foreach($pair in @(@('scene.rad','scene_sha256'),@('sensors.pts','sensor_sha256'),@('weather.epw','normalized_weather_sha256'))){if($recipe.($pair[1]) -cne $Declaration.input_sha256.($pair[0])){throw 'Source recipe physical hash mismatch.'}}
 if($recipe.sensor_count -ne 24 -or $recipe.profile.sky_basis_mf -ne 2 -or $Declaration.input_sha256.'weather.epw' -cne $Declaration.weather_binding.normalized_sha256){throw 'Source recipe shape/weather differs.'}
 Assert-ReferenceSame $recipe.interval_indices @(0..8759) 'Source weather interval order'
 $sensorText=[IO.File]::ReadAllText((Read-ReferenceFile $Root 'sensors.pts'))
 if(!$sensorText.EndsWith("`n") -or @($sensorText.Trim() -split '\r?\n').Count -ne 24){throw 'Expected24 ordered newline-terminated sensors.'}
 foreach($line in ($sensorText.Trim() -split '\r?\n')){ $tokens=$line -split '\s+';if($tokens.Count -ne 6){throw 'Malformed sensor row.'};foreach($token in $tokens){if(![double]::IsFinite([double]::Parse($token,[Globalization.CultureInfo]::InvariantCulture))){throw 'Nonfinite sensor row.'}} }
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root 'scene.rad')) -match '(?m)^\s*!'){throw 'Scene shell directives are prohibited.'}
 if($model.room.width_m -ne 4 -or $model.room.depth_m -ne 6 -or $model.room.height_m -ne 3 -or $model.room.rotation_deg -ne 0 -or $model.sensors.requested_spacing_m -ne 1 -or $null -ne $model.obstruction -or ($Declaration.alternative -eq 'A' -and $null -ne $model.overhang) -or ($Declaration.alternative -eq 'B' -and $model.overhang.projection_m -ne .6)){throw 'Model differs from the fixed24-sensor A/B example.'}
 return $sensorText
}
function Assert-ReferenceProcess($Record,[string]$Executable,[string[]]$Arguments,[string]$Root,[string]$Output){
 if($Record.executable -cne "$Executable.exe" -or $Record.exit_code -ne 0 -or $Record.completed -isnot [bool] -or !$Record.completed -or $null -ne $Record.terminated_tree_reason){throw 'Reference required process did not complete successfully.'}
 Assert-ReferenceSame @($Record.arguments) @($Arguments) 'Actual process arguments'
 $limits=@{timeout_seconds=7200.0;memory_bytes=2147483648;scratch_bytes=2147483648;file_bytes=536870912}
 Assert-ReferenceSame $Record.limits $limits 'Actual process resource limits'
 foreach($field in @('wall_seconds','sampled_runner_plus_engine_peak_bytes','sampled_scratch_peak_bytes','output_bytes')){
  $value=$Record.$field
  if($null -eq $value -or $value -is [string] -or $value -is [bool] -or $value -isnot [ValueType] -or ![double]::IsFinite([double]$value) -or $value -lt 0){throw "Invalid required process resource measurement: $field"}
  if($field -ne 'wall_seconds' -and $value -ne [Math]::Floor([double]$value)){throw "Noninteger process byte measurement: $field"}
 }
 if($Record.sampled_runner_plus_engine_peak_bytes -le 0){throw 'Missing positive runner-plus-engine measurement.'}
 if($null -ne $Record.sampled_peak_working_set_bytes -and ($Record.sampled_peak_working_set_bytes -is [string] -or $Record.sampled_peak_working_set_bytes -is [bool] -or ![double]::IsFinite([double]$Record.sampled_peak_working_set_bytes) -or $Record.sampled_peak_working_set_bytes -le 0 -or $Record.sampled_peak_working_set_bytes -gt 2147483648)){throw 'Invalid optional child working-set measurement.'}
 $start=[DateTimeOffset]::Parse($Record.started_utc);$finish=[DateTimeOffset]::Parse($Record.finished_utc)
 if($start.Offset.Ticks -ne 0 -or $finish.Offset.Ticks -ne 0 -or $finish -lt $start -or ![double]::IsFinite([double]$Record.wall_seconds) -or $Record.wall_seconds -lt 0 -or $Record.wall_seconds -gt 7200 -or $Record.sampled_runner_plus_engine_peak_bytes -gt 2147483648 -or $Record.sampled_scratch_peak_bytes -gt 2147483648){throw 'Invalid completed process time/resources.'}
 $file=Read-ReferenceFile $Root $Output
 if($Record.output_bytes -ne ([IO.FileInfo]$file).Length){throw 'Process output size differs from retained file.'}
}
function Get-ReferenceAttempt([string]$Root,[int]$Index,$Attempt,[double[][]]$Direct,[double[][]]$Finite,[string]$Sensors,[double[]]$Previous){
 $rounds=Assert-ReferenceIntervalValue $Attempt.rounds
 if($rounds -notin @(256,512)){throw 'Unsupported repeat rung.'}
 $name="hour-$Index-rounds$rounds"
 Assert-ReferenceHash $Root "$name.rgb" $Attempt.raw_sha256
 Assert-ReferenceHash $Root "$name.pts" $Attempt.primary_sha256
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root "$name.pts")) -cne ($Sensors.Replace("`r`n","`n")*$rounds)){throw 'Primary records do not preserve contiguous round-major24-sensor order.'}
 $raw=[GreenLensReference.Aggregate]::Read((Read-ReferenceFile $Root "$name.rgb"),(24*$rounds*6))
 if($rounds -eq 512){if(!$Previous.Count){throw '512-round escalation lacks its retained256 prefix.'};[GreenLensReference.Aggregate]::EqualPrefix($Previous,$raw)}
 $means=@(foreach($n in @(($rounds/4),($rounds/2),$rounds)){[GreenLensReference.Aggregate]::Calculate($raw,24,0,[int]$n,100,$Direct,$Finite)})
 $first=[GreenLensReference.Aggregate]::Compare($means[0],$means[1]);$second=[GreenLensReference.Aggregate]::Compare($means[1],$means[2])
 $half1=[GreenLensReference.Aggregate]::Calculate($raw,24,0,($rounds/2),100,$Direct,$Finite)
 $half2=[GreenLensReference.Aggregate]::Calculate($raw,24,($rounds/2),($rounds/2),100,$Direct,$Finite)
 $blocks=[GreenLensReference.Aggregate]::CompareWithScale($half1,$half2,$means[2])
 $qualified=@(@($first)+@($second)+@($blocks)|Where-Object{!$_.passed}).Count -eq 0
 foreach($pair in @(@('rungs',$means),@('first_transition',$first),@('second_transition',$second),@('block1',$half1),@('block2',$half2),@('block_comparison',$blocks),@('qualified',$qualified))){Assert-ReferenceSame $Attempt.($pair[0]) $pair[1] "Stored $($pair[0]) at interval$Index rung$rounds"}
 return @{raw=$raw;mean=$means[2];record=[ordered]@{rounds=$rounds;first_transition=$first;second_transition=$second;block_comparison=$blocks;qualified=$qualified;raw_sha256=$Attempt.raw_sha256;primary_sha256=$Attempt.primary_sha256;final_mean=$means[2]}}
}
function Assert-ReferenceHourlyInputs([string]$Root,[int]$Index,$Check,$Sky,[string]$AllSuns){
 $culture=[Globalization.CultureInfo]::InvariantCulture;$name="hour-$Index"
 $sources=@([regex]::Matches($AllSuns,'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')|Where-Object{[int]$_.Groups[1].Value -eq $Index*60+30})
 if($sources.Count -ne 1){throw 'Missing/duplicate exact-midpoint source.'};$sun=$sources[0].Value
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root "$name-sun.rad")) -cne $sun){throw 'Hourly sun differs from the hashed annual source.'}
 $data=[Text.StringBuilder]::new("1`n0 577 578`n")
 for($patch=0;$patch -lt 578;$patch++){$offset=($patch*8760+$Index)*3;$v=$Sky.values[$offset];if($v -ne $Sky.values[$offset+1] -or $v -ne $Sky.values[$offset+2]){throw 'Reference sky must be neutral.'};$null=$data.Append($v.ToString('R',$culture)).Append("`n")}
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root "$name.dat")) -cne $data.ToString()){throw 'Hourly sky differs from the hashed annual column.'}
 $pattern='(?s)(void light (solar\d+)\s+0\s+0\s+3)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)'
 $modifier=[regex]::Match($sun,$pattern).Groups[2].Value;if(!$modifier){throw 'Malformed solar modifier.'}
 $scaled=[regex]::Replace($sun,$pattern,{param($m)$rgb=@(3..5|ForEach-Object{([double]::Parse($m.Groups[$_].Value,$culture)*100).ToString('R',$culture)});$m.Groups[1].Value+"`n"+($rgb -join ' ')})
 $expected="void brightdata distribution`n4 noop $name.dat matched-basis.cal rbin`n0`n0`ndistribution glow sky_glow`n0`n0`n4 100 100 100 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"+$scaled
 Assert-ReferenceHash $Root "$name-scaled.rad" $Check.sky_scene_sha256
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root "$name-scaled.rad")) -cne $expected.Replace("`r`n","`n")){throw 'Scaled hourly scene differs from generated fixed-scale sky/source.'}
 return @{sun=$sun;modifier=$modifier}
}
function Assert-ReferenceRecovery([string]$Root,$Declaration,[string]$DeclarationHash,[string]$Sensors){
 $recoveryNames=@('recovery-original-declaration.json','recovery-original-harness.ps1','recovery-original-processes.json','recovery-helper.ps1','recovery-identity-helper.ps1')
 if(!(Test-Path -LiteralPath (Join-Path $Root 'recovery.json'))){
  foreach($file in $recoveryNames){if(Test-Path -LiteralPath (Join-Path $Root $file)){throw 'Recovery artifacts lack the explicit lineage manifest.'}}
  return $null
 }
 $hash=Reference-Hash $Root 'recovery.json';$manifest=Read-ReferenceJson $Root 'recovery.json'
 if($manifest.schema_version -cne 'greenlens.controlled-reference-recovery.v1' -or $manifest.status -cnotin @('prepared','reused_successful_initial_processes') -or $manifest.copied_process_count -ne 6 -or $manifest.corrected_declaration_sha256 -cne $DeclarationHash -or $manifest.original_harness_sha256 -cne '18646902250606280f96eff3325ff47f669772bb1f9f1f7154401f9429ec017f'){throw 'Unsupported recovery lineage or reporting-only source.'}
 $reused=Assert-ReferenceIntervalValue $manifest.reused_process_count
 if($reused -gt 6 -or ($manifest.status -ceq 'reused_successful_initial_processes') -ne ($reused -eq 6)){throw 'Recovery status/count mismatch.'}
 foreach($pair in @(@('recovery-original-declaration.json','original_declaration_sha256'),@('recovery-original-harness.ps1','original_harness_sha256'),@('recovery-original-processes.json','original_processes_sha256'),@('recovery-helper.ps1','helper_sha256'),@('recovery-identity-helper.ps1','identity_helper_sha256'))){Assert-ReferenceHash $Root $pair[0] $manifest.($pair[1])}
 if($manifest.helper_sha256 -cne (Get-MatchedHash "$PSScriptRoot/annual-reference-recovery.ps1") -or $manifest.identity_helper_sha256 -cne (Get-MatchedHash "$PSScriptRoot/annual-reference-identity.ps1")){throw 'Recovery uses an unsupported trusted helper.'}
 $original=Read-ReferenceJson $Root 'recovery-original-declaration.json'
 if($original.harness_sha256 -cne $manifest.original_harness_sha256){throw 'Recovery original harness binding differs.'}
 $old=Copy-ReferenceJsonObject $original;$new=Copy-ReferenceJsonObject $Declaration
 foreach($field in @('declared_utc','harness_sha256')){$null=$old.Remove($field);$null=$new.Remove($field)}
 Assert-ReferenceSame $old $new 'Recovery scientific declaration, including execution order'
 $index=[int]$Declaration.executed_interval_indices[0];$name="hour-$index"
 $outputs=@('transmission.oct',"$name.oct","$name-sun.oct","$name-direct.rgb","$name-finite.rgb","$name-rounds256.rgb")
 $members=@($outputs)+@("$name-rounds256.pts")
 Assert-ReferenceSame @($manifest.copied_members.PSObject.Properties.Name|Sort-Object) @($members|Sort-Object) 'Recovery copied member set'
 foreach($file in $members){
  $member=$manifest.copied_members.$file;Assert-ReferenceHash $Root $file $member.sha256
  if($member.bytes -is [string] -or $member.bytes -is [bool] -or $member.bytes -ne (Get-Item -LiteralPath (Read-ReferenceFile $Root $file)).Length){throw 'Recovery copied byte size differs.'}
 }
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root "$name-rounds256.pts")) -cne ($Sensors.Replace("`r`n","`n")*256)){throw 'Recovery changed the original repeated primary inputs.'}
 $processes=@(Read-ReferenceJson $Root 'recovery-original-processes.json')
 if($processes.Count -ne 6){throw 'Recovery must retain exactly six successful original processes.'}
 $expected=@(
  @{name='oconv';args=@('-f','scene.rad','receiver.rad')},
  @{name='oconv';args=@('-f','scene.rad',"$name-scaled.rad")},
  @{name='oconv';args=@('-f','scene.rad',"$name-sun.rad")},
  @{name='rtrace';args=(@('-I+')+(Get-ReferenceDirectOptions)+@("$name-sun.oct"))},
  @{name='rtrace';args=((Get-ReferenceDirectOptions)+@('transmission.oct'))},
  @{name='rcontrib';args=((Get-ReferenceOptions)+@('-y','6144','-m','sky_glow','-m',"solar$($index*60+30)","$name.oct"))}
 )
 for($i=0;$i -lt 6;$i++){Assert-ReferenceProcess $processes[$i] $expected[$i].name $expected[$i].args $Root $outputs[$i]}
 if($reused){
  $current=@(Read-ReferenceJson $Root 'processes.json')
  if($current.Count -lt $reused){throw 'Reused original process records are missing.'}
  for($i=0;$i -lt $reused;$i++){Assert-ReferenceSame $current[$i] $processes[$i] 'Original reused process record'}
 }
 Assert-ReferenceHash $Root 'recovery.json' $hash
 return [ordered]@{manifest_sha256=$hash;manifest=$manifest;ready=($reused -eq 6)}
}
function Read-ReferenceSky([string]$Root){
 $path=Read-ReferenceFile $Root 'identical-sky.mtx'
 # Authorize dimensions before the shared research reader allocates its array.
 $reader=[IO.StreamReader]::new($path);$dimensions=@{};$ended=$false
 try{
  for($i=0;$i -lt 50;$i++){
   $line=$reader.ReadLine();if($null -eq $line){break};if($line.Length -gt 2048){throw 'Excess reference sky header length.'}
   if($line -ceq ''){$ended=$true;break}
   if($line -match '^(NROWS|NCOLS|NCOMP|FORMAT)=(.*)$'){
    if($dimensions.ContainsKey($Matches[1])){throw 'Duplicate reference sky header field.'}
    $dimensions[$Matches[1]]=$Matches[2]
   }
  }
 }finally{$reader.Dispose()}
 if(!$ended -or $dimensions.NROWS -cne '578' -or $dimensions.NCOLS -cne '8760' -or $dimensions.NCOMP -cne '3' -or $dimensions.FORMAT -cne 'ascii'){throw 'Reference annual sky header differs from the bounded578x8760RGB shape.'}
 return [GreenLensFeasibility.Matrix]::Read($path)
}
function Read-ReferenceBaseline([string]$Root){
 $decl=Read-ReferenceJson $Root 'declaration.json';$declarationHash=Reference-Hash $Root 'declaration.json'
 $sensors=Assert-ReferenceDeclaration $Root $decl
 $recovery=Assert-ReferenceRecovery $Root $decl $declarationHash $sensors
 $answer=@{declaration=$decl;declaration_sha256=$declarationHash;cases=@();contexts=@();report_sha256=$null;processes_sha256=$null;verified_processes=@();recovery=$recovery;status='No completed case report';source_directory_name=[IO.Path]::GetFileName([IO.Path]::GetFullPath($Root))}
 if(!(Test-Path -LiteralPath (Join-Path $Root 'controlled-reference.json'))){return $answer}
 $reportHash=Reference-Hash $Root 'controlled-reference.json';$report=Read-ReferenceJson $Root 'controlled-reference.json'
 Assert-ReferenceSame $report.declaration $decl 'Embedded baseline declaration'
 $checks=@($report.checks);if($checks.Count -gt @($decl.executed_interval_indices).Count){throw 'Excess completed reference cases.'}
 if($checks.Count -and $null -ne $recovery -and !$recovery.ready){throw 'Completed case report precedes successful recovery.'}
 for($i=0;$i -lt $checks.Count;$i++){if($checks[$i].interval_index -ne $decl.executed_interval_indices[$i]){throw 'Completed reference cases are not the declared execution prefix.'}}
 if(!$checks.Count){$answer.report_sha256=$reportHash;return $answer}
 $processHash=Reference-Hash $Root 'processes.json';$processes=@(Read-ReferenceJson $Root 'processes.json');$cursor=0
 Assert-ReferenceProcess $processes[$cursor++] 'oconv' @('-f','scene.rad','receiver.rad') $Root 'transmission.oct'
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root 'receiver.rad')) -cne "void glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"){throw 'Finite receiver differs from the unit dome.'}
 $basis=[IO.File]::ReadAllText((Read-ReferenceFile $Root 'matched-basis.cal'))
 if(!$basis.StartsWith("MF:2;`n")){throw 'Reference basis lacks the fixedMF2 prefix.'}
 $basisBytes=[Text.Encoding]::UTF8.GetBytes($basis.Substring(6))
 # Write-Input normalizes the pinned Windows library CRLF to LF. This is the
 # separately verified LF digest of the exact original 1517d597... library;
 # the original artifact digest is checked in Assert-ReferenceDeclaration.
 if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($basisBytes)).ToLowerInvariant() -cne '92b8c09984de7a551773530b3146ba79a51e1b003245f230b6e7b430eb6c26cd'){throw 'Retained basis differs from normalized pinned library bytes.'}
 $sky=Read-ReferenceSky $Root
 if($sky.rows -ne 578 -or $sky.columns -ne 8760){throw 'Reference annual sky shape differs.'}
 $allSuns=[IO.File]::ReadAllText((Read-ReferenceFile $Root 'suns.rad'))
 foreach($check in $checks){
  $index=Assert-ReferenceIntervalValue $check.interval_index;$name="hour-$index"
  $hour=Assert-ReferenceHourlyInputs $Root $index $check $sky $allSuns
  Assert-ReferenceProcess $processes[$cursor++] 'oconv' @('-f','scene.rad',"$name-scaled.rad") $Root "$name.oct"
  Assert-ReferenceProcess $processes[$cursor++] 'oconv' @('-f','scene.rad',"$name-sun.rad") $Root "$name-sun.oct"
  Assert-ReferenceHash $Root "$name-direct.rgb" $check.direct_sha256
  Assert-ReferenceHash $Root "$name-finite.rgb" $check.finite_sha256
  Assert-ReferenceProcess $processes[$cursor++] 'rtrace' (@('-I+')+(Get-ReferenceDirectOptions)+@("$name-sun.oct")) $Root "$name-direct.rgb"
  Assert-ReferenceProcess $processes[$cursor++] 'rtrace' ((Get-ReferenceDirectOptions)+@('transmission.oct')) $Root "$name-finite.rgb"
  $direct=[GreenLensMatched.Direct]::Rgb((Read-ReferenceFile $Root "$name-direct.rgb"),24)
  $finite=[GreenLensMatched.Direct]::Finite($sensors,$hour.sun,(Read-ReferenceFile $Root "$name-finite.rgb"))
  $attempts=@($check.attempts);if($attempts.Count -notin @(1,2) -or $attempts[0].rounds -ne 256 -or ($attempts.Count -eq 2 -and $attempts[1].rounds -ne 512)){throw 'Invalid or reordered repeat ladder.'}
  $summaries=@();$previous=@();$last=$null
  foreach($attempt in $attempts){
   if($last -and $last.record.qualified){throw 'Unnecessary escalation after a qualifying256-round attempt.'}
   $rounds=[int]$attempt.rounds
   Assert-ReferenceProcess $processes[$cursor++] 'rcontrib' ((Get-ReferenceOptions)+@('-y',"$(24*$rounds)",'-m','sky_glow','-m',$hour.modifier,"$name.oct")) $Root "$name-rounds$rounds.rgb"
   $last=Get-ReferenceAttempt $Root $index $attempt $direct $finite $sensors $previous;$previous=$last.raw;$summaries+=,$last.record
  }
  Assert-ReferenceSame $check.qualified $last.record.qualified 'Stored case qualification'
  $record=[ordered]@{alternative=$decl.alternative;interval_index=$index;qualified=$last.record.qualified;sensor_count=24;attempts=$summaries;reference=$last.mean;direct_sha256=$check.direct_sha256;finite_sha256=$check.finite_sha256;sky_scene_sha256=$check.sky_scene_sha256;declaration_sha256=$declarationHash}
  $answer.cases+=,$record
  $answer.contexts+=,@{key="$($decl.alternative):$index";root=$Root;declaration=$decl;declaration_sha256=$declarationHash;record=$record;direct=$direct;finite=$finite;sensors=$sensors;sun=$hour.sun;modifier=$hour.modifier}
 }
 # A live next case may already have successful setup processes. They are not
 # counted as completed evidence. Full qualification permits no extra tail.
 if($checks.Count -eq @($decl.executed_interval_indices).Count -and $cursor -ne $processes.Count){throw 'Completed shard has unexpected process records.'}
 if($cursor -gt $processes.Count){throw 'Required process records are missing.'}
 Assert-ReferenceSame $report.completed ($checks.Count -eq @($decl.executed_interval_indices).Count) 'Shard completed flag'
 Assert-ReferenceSame $report.all_executed_references_qualified ($report.completed -and @($answer.cases|Where-Object{!$_.qualified}).Count -eq 0) 'Shard numerical flag'
 Assert-ReferenceHash $Root 'declaration.json' $declarationHash
 # Pin the report snapshot; an append during collection requires another call.
 Assert-ReferenceHash $Root 'controlled-reference.json' $reportHash
 Assert-ReferenceHash $Root 'processes.json' $processHash
 $answer.report_sha256=$reportHash;$answer.processes_sha256=$processHash;$answer.verified_processes=@($processes|Select-Object -First $cursor);$answer.status='Completed cases recomputed from retained raw records'
 return $answer
}

function Get-ReferenceSensitivityVariants {return @(@{name='scale10000';scale=10000;lw='1e-9';ab=9;lr=-16},@{name='weight1e10';scale=100;lw='1e-10';ab=9;lr=-16},@{name='depth10-24';scale=100;lw='1e-9';ab=10;lr=-24})}
function Compare-ReferenceScaleBins($Variant,$Baseline){
 $checks=@();for($i=0;$i -lt 24;$i++){foreach($bin in @('diffuse_rgb','full_solar_rgb')){for($c=0;$c -lt 3;$c++){
  $a=$Variant.sensors[$i].$bin[$c];$b=$Baseline.sensors[$i].$bin[$c];$difference=[Math]::Abs($a-$b);$allowed=1e-5*[Math]::Abs($b)
  $checks+=,[ordered]@{sensor_index=$i;bin=$bin;channel=$c;variant=$a;baseline=$b;absolute_difference=$difference;allowed_difference=$allowed;passed=($difference -le $allowed)}
 }}}
 return $checks
}
function Read-ReferenceSensitivity([string]$Root,$Context){
 $decl=Read-ReferenceJson $Root 'declaration.json';$declHash=Reference-Hash $Root 'declaration.json'
 if($decl.schema_version -cne 'greenlens.controlled-reference-sensitivity.v1' -or $decl.interval_index -ne $Context.record.interval_index -or $decl.baseline_declaration_sha256 -cne $Context.declaration_sha256){throw 'Sensitivity baseline identity differs.'}
 Assert-ReferenceSame $decl.baseline_declaration $Context.declaration 'Sensitivity embedded baseline'
 Assert-ReferenceSame $decl.variants (Get-ReferenceSensitivityVariants) 'Sensitivity variants'
 Assert-ReferenceSame $decl.limits @{maximum_process_seconds=7200;maximum_runner_plus_engine_working_set_bytes=2147483648;maximum_scratch_bytes=2147483648;maximum_single_file_bytes=536870912} 'Sensitivity resource bounds'
 if(!$Context.record.qualified -or $decl.final_baseline_rounds -ne $Context.record.reference.rounds){throw 'Sensitivity requires its exact qualified baseline rung.'}
 Assert-ReferenceHash $Root 'harness-source.ps1' $decl.harness_sha256
 if($decl.harness_sha256 -cne (Get-MatchedHash "$PSScriptRoot/annual-reference-sensitivity.ps1") -or $decl.supervisor_sha256 -cne $Context.declaration.process_helper_sha256){throw 'Sensitivity trusted source differs.'}
 Assert-ReferenceHash $Root 'process-source.ps1' $Context.declaration.process_helper_sha256
 Assert-ReferenceHash $Root 'aggregation-source.cs' $Context.declaration.aggregation_sha256
 Assert-ReferenceHash $Root 'finite-kernel.cs' (Reference-Hash $Context.root 'finite-kernel.cs')
 $index=[int]$decl.interval_index;$name="hour-$index";$rounds=[int]$decl.final_baseline_rounds
 $expected=@('scene.rad','sensors.pts','matched-basis.cal',"$name.dat","$name-sun.rad","$name-scaled.rad","$name-direct.rgb","$name-finite.rgb","$name-rounds$rounds.rgb")
 if($rounds -eq 512){$expected+=,"$name-rounds256.rgb"}
 Assert-ReferenceSame @($decl.input_sha256.PSObject.Properties.Name|Sort-Object) @($expected|Sort-Object) 'Sensitivity input member set'
 foreach($file in $expected){Assert-ReferenceHash $Root $file $decl.input_sha256.$file;Assert-ReferenceHash $Context.root $file $decl.input_sha256.$file}
 $result=[ordered]@{case_key=$Context.key;completed=$false;qualified=$false;declaration_sha256=$declHash;variants=@();report_sha256=$null;processes_sha256=$null;verified_processes=@()}
 if(!(Test-Path -LiteralPath (Join-Path $Root 'sensitivity.json'))){return $result}
 $reportHash=Reference-Hash $Root 'sensitivity.json';$report=Read-ReferenceJson $Root 'sensitivity.json';Assert-ReferenceSame $report.declaration $decl 'Embedded sensitivity declaration'
 $processHash=Reference-Hash $Root 'processes.json';$processes=@(Read-ReferenceJson $Root 'processes.json');$results=@($report.results);$variants=Get-ReferenceSensitivityVariants
 if($results.Count -gt 3){throw 'Excess sensitivity results.'};$cursor=0
 for($i=0;$i -lt $results.Count;$i++){
  $stored=$results[$i];$variant=$variants[$i];Assert-ReferenceSame $stored.variant $variant 'Stored sensitivity order/settings'
  $label=$variant.name;Assert-ReferenceHash $Root "$label.rgb" $stored.raw_sha256;Assert-ReferenceHash $Root "$label.rad" $stored.scene_sha256
  $skyText=[IO.File]::ReadAllText((Read-ReferenceFile $Context.root "$name-scaled.rad"))
  if($variant.scale -eq 10000){
   $pattern='(?s)(void light (solar\d+)\s+0\s+0\s+3)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)'
   $culture=[Globalization.CultureInfo]::InvariantCulture
   $skyText=[regex]::Replace($skyText.Replace('4 100 100 100 0','4 10000 10000 10000 0'),$pattern,{param($m)$rgb=@(3..5|ForEach-Object{([double]::Parse($m.Groups[$_].Value,$culture)*100).ToString('R',$culture)});$m.Groups[1].Value+"`n"+($rgb -join ' ')})
  }
  if([IO.File]::ReadAllText((Read-ReferenceFile $Root "$label.rad")) -cne $skyText){throw 'Sensitivity scene changes more than its declared source scale.'}
  Assert-ReferenceProcess $processes[$cursor++] 'oconv' @('-f','scene.rad',"$label.rad") $Root "$label.oct"
  $options=Get-ReferenceOptions;$options[[Array]::IndexOf($options,'-ab')+1]="$($variant.ab)";$options[[Array]::IndexOf($options,'-lr')+1]="$($variant.lr)";$options[[Array]::IndexOf($options,'-lw')+1]=$variant.lw
  Assert-ReferenceProcess $processes[$cursor++] 'rcontrib' ($options+@('-y',"$(24*$rounds)",'-m','sky_glow','-m',$Context.modifier,"$label.oct")) $Root "$label.rgb"
  $raw=[GreenLensReference.Aggregate]::Read((Read-ReferenceFile $Root "$label.rgb"),(24*$rounds*6))
  $mean=[GreenLensReference.Aggregate]::Calculate($raw,24,0,$rounds,$variant.scale,$Context.direct,$Context.finite)
  $checks=if($variant.scale -eq 10000){@(Compare-ReferenceScaleBins $mean $Context.record.reference)}else{@([GreenLensReference.Aggregate]::CompareWithScale($Context.record.reference,$mean,$Context.record.reference))}
  $passed=@($checks|Where-Object{!$_.passed}).Count -eq 0
  Assert-ReferenceSame $stored.mean $mean 'Stored sensitivity means';Assert-ReferenceSame $stored.checks $checks 'Stored sensitivity comparisons';Assert-ReferenceSame $stored.passed $passed 'Stored sensitivity outcome'
  $result.variants+=,[ordered]@{variant=$variant;mean=$mean;checks=$checks;passed=$passed;raw_sha256=$stored.raw_sha256;scene_sha256=$stored.scene_sha256}
 }
 $result.completed=$results.Count -eq 3;$result.qualified=$result.completed -and @($result.variants|Where-Object{!$_.passed}).Count -eq 0
 if($result.completed -and $cursor -ne $processes.Count){throw 'Unexpected completed sensitivity process records.'}
 Assert-ReferenceSame $report.completed $result.completed 'Sensitivity completion';Assert-ReferenceSame $report.passed $result.qualified 'Sensitivity qualification'
 Assert-ReferenceHash $Root 'declaration.json' $declHash;Assert-ReferenceHash $Root 'sensitivity.json' $reportHash;Assert-ReferenceHash $Root 'processes.json' $processHash
 $result.report_sha256=$reportHash;$result.processes_sha256=$processHash;$result.verified_processes=@($processes|Select-Object -First $cursor)
 return $result
}

function Collect-ReferenceEvidence([string[]]$Baselines,[string[]]$Sensitivities){
 Initialize-ReferenceCollector
 if(!$Baselines.Count){throw 'Supply at least one baseline shard.'}
 $shards=@(foreach($directory in $Baselines){Read-ReferenceBaseline $directory})
 $identity=Assert-ReferenceShardIdentity @($shards|ForEach-Object{$_.declaration})
 $cases=@($shards|ForEach-Object{$_.cases});$coverage=Test-ReferenceCaseCoverage $cases
 $contexts=@{};foreach($shard in $shards){foreach($context in $shard.contexts){$contexts[$context.key]=$context}}
 $sensitivityResults=@();$sensitivitySeen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($directory in $Sensitivities){
  $decl=Read-ReferenceJson $directory 'declaration.json';$key="$($decl.baseline_declaration.alternative):$($decl.interval_index)"
  if($key -cnotin @('B:8512','A:4116') -or !$sensitivitySeen.Add($key)){throw 'Unexpected or duplicate fixed sensitivity case.'}
  if(!$contexts.ContainsKey($key)){throw 'Sensitivity lacks its collected completed baseline case.'}
  $sensitivityResults+=,(Read-ReferenceSensitivity $directory $contexts[$key])
 }
 $missingSensitivity=@(@('B:8512','A:4116')|Where-Object{!$sensitivitySeen.Contains($_)})
 $numerical=@($cases|Where-Object{!$_.qualified}).Count -eq 0
 $sensitivityComplete=$missingSensitivity.Count -eq 0 -and @($sensitivityResults|Where-Object{!$_.completed}).Count -eq 0
 $qualified=$coverage.complete -and $numerical -and $sensitivityComplete -and @($sensitivityResults|Where-Object{!$_.qualified}).Count -eq 0
 return [ordered]@{schema_version='greenlens.controlled-reference-collection.v1';captured_utc=[DateTime]::UtcNow.ToString('o');scope='Reference-only qualification from verified retained raw records; no candidate comparison or annual acceptance';collector_sha256=(Get-MatchedHash $script:referenceCollectorPath);identity_helper_sha256=(Get-MatchedHash "$PSScriptRoot/annual-reference-identity.ps1");identity=$identity;coverage=$coverage;cases=$cases;sensitivities=$sensitivityResults;missing_sensitivity_case_keys=$missingSensitivity;all_completed_case_numerics_qualified=$numerical;all_required_sensitivities_complete=$sensitivityComplete;qualified=$qualified;release_acceptance_complete=$false;sources=@($shards|ForEach-Object{[ordered]@{directory_name=$_.source_directory_name;declaration_sha256=$_.declaration_sha256;report_sha256=$_.report_sha256;processes_sha256=$_.processes_sha256;verified_processes=$_.verified_processes;recovery=$_.recovery;status=$_.status}})}
}
if($FunctionsOnly){return}
if(!$OutputDirectory -or (Test-Path -LiteralPath $OutputDirectory)){throw 'Choose a new collector output directory; existing evidence is never overwritten.'}
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
try {
 $collected=Collect-ReferenceEvidence $BaselineDirectories $SensitivityDirectories
 $json=ConvertTo-Json -InputObject $collected -Depth 100
 [IO.File]::WriteAllText((Join-Path $OutputDirectory 'reference-collection.json'),$json,[Text.UTF8Encoding]::new($false))
 [IO.File]::WriteAllText((Join-Path $OutputDirectory 'checksums.json'),(ConvertTo-Json -InputObject @{schema_version='greenlens.reference-collection-seal.v1';sha256=@{'reference-collection.json'=(Reference-Hash $OutputDirectory 'reference-collection.json')}}),[Text.UTF8Encoding]::new($false))
 Write-Host "Reference snapshot: $($collected.coverage.completed_case_count)/24 complete cases; qualified=$($collected.qualified). No candidate comparison."
 if($RequireQualified -and !$collected.qualified){throw 'Reference remains incomplete or unqualified; all observed outcomes were retained.'}
} catch {
 $failure=[ordered]@{schema_version='greenlens.reference-collection-failure.v1';captured_utc=[DateTime]::UtcNow.ToString('o');qualified=$false;error=$_.Exception.Message;release_acceptance_complete=$false}
 [IO.File]::WriteAllText((Join-Path $OutputDirectory 'collection-failure.json'),(ConvertTo-Json -InputObject $failure),[Text.UTF8Encoding]::new($false))
 throw
}
