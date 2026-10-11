#Requires -Version 7.2
# Execute only the three sensitivity variants sealed before baseline tracing.
# This reads numeric evidence, never commands or code from the evidence directory.
param([Parameter(Mandatory)][string]$BaselineDirectory,[Parameter(Mandatory)][string]$OutputDirectory,[Parameter(Mandatory)][int]$IntervalIndex,[ValidateSet('scale10000','weight1e10','depth10-24')][string]$Variant,[switch]$PrepareOnly)
$ErrorActionPreference='Stop'
function ConvertTo-SensitivityGeneratedText([string]$Text) {
 # Match trusted Write-Input exactly; never change original retained bytes.
 return $Text.Replace("`r`n","`n")
}
function Get-SensitivityVariantSelection([object[]]$Plan,[string]$Selected) {
 $names=@($Plan|ForEach-Object{$_.name})
 if(($names -join '|') -cne 'scale10000|weight1e10|depth10-24'){throw 'Sensitivity full plan differs.'}
 if(!$Selected){return $Plan}
 if($Selected -cnotin $names){throw 'Unknown sensitivity variant.'}
 return @($Plan|Where-Object{$_.name -ceq $Selected})
}
function Read-SensitivityJson([string]$Path) {
 $options=@{InputObject=[IO.File]::ReadAllText($Path);Depth=100}
 if((Get-Command ConvertFrom-Json).Parameters.ContainsKey('DateKind')){$options.DateKind='String'}
 ConvertFrom-Json @options
}
function New-SensitivitySky([string]$SkyText) {
 $pattern='(?s)(void light (solar\d+)\s+0\s+0\s+3)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)'
 if([regex]::Matches($SkyText,'4 100 100 100 0').Count -ne 1 -or [regex]::Matches($SkyText,$pattern).Count -ne 1){throw 'Expected one scaled neutral sky and one solar source.'}
 $invariant=[Globalization.CultureInfo]::InvariantCulture
 $result=$SkyText.Replace('4 100 100 100 0','4 10000 10000 10000 0')
 return [regex]::Replace($result,$pattern,{
  param($m)
  $rgb=@(3..5|ForEach-Object{
   $value=[double]::Parse($m.Groups[$_].Value,$invariant)*100
   if(![double]::IsFinite($value) -or $value -lt 0){throw 'Invalid scaled source value.'}
   $value.ToString('R',$invariant)
  })
  $m.Groups[1].Value+"`n"+($rgb -join ' ')
 })
}
function Compare-SensitivityBins($VariantMean,$BaselineMean) {
 if($VariantMean.sensors.Count -ne $BaselineMean.sensors.Count){throw 'Sensitivity sensor counts differ.'}
 $checks=@()
 for($sensor=0;$sensor -lt $BaselineMean.sensors.Count;$sensor++){
  foreach($bin in @('diffuse_rgb','full_solar_rgb')){
   if($VariantMean.sensors[$sensor].$bin.Count -ne 3 -or $BaselineMean.sensors[$sensor].$bin.Count -ne 3){throw 'Sensitivity bin is not RGB.'}
   for($channel=0;$channel -lt 3;$channel++){
    $a=$VariantMean.sensors[$sensor].$bin[$channel];$b=$BaselineMean.sensors[$sensor].$bin[$channel]
    if(![double]::IsFinite($a) -or ![double]::IsFinite($b) -or $a -lt 0 -or $b -lt 0){throw 'Invalid normalized sensitivity bin.'}
    $difference=[Math]::Abs($a-$b);$allowed=1e-5*[Math]::Abs($b)
    $checks+=[ordered]@{sensor_index=$sensor;bin=$bin;channel=$channel;variant=$a;baseline=$b;absolute_difference=$difference;allowed_difference=$allowed;passed=($difference -le $allowed)}
   }
  }
 }
 return $checks
}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained sensitivity directory.'}
$baseline=Read-SensitivityJson (Join-Path $BaselineDirectory 'controlled-reference.json')
$decl=$baseline.declaration
$originalDeclaration=Read-SensitivityJson (Join-Path $BaselineDirectory 'declaration.json')
if(($decl|ConvertTo-Json -Depth 25 -Compress) -cne ($originalDeclaration|ConvertTo-Json -Depth 25 -Compress)){throw 'Embedded and retained baseline declarations differ.'}
if($decl.schema_version -ne 'greenlens.controlled-reference.v1' -or $decl.sensor_count -ne 24 -or $decl.sky_basis_mf -ne 2 -or $decl.ambient_divisions -ne 8192 -or $decl.ambient_bounces -ne 9 -or $decl.ray_depth -ne -16 -or $decl.ambient_weight -ne 1e-9 -or $decl.source_scale -ne 100){throw 'Unsupported baseline declaration.'}
if("$($decl.alternative):$IntervalIndex" -notin @('B:8512','A:4116')){throw 'Outside the predeclared all-sensor sensitivity cases.'}
$case=@($baseline.checks|Where-Object{$_.interval_index -eq $IntervalIndex})
if($case.Count -ne 1 -or !$case[0].qualified){throw 'The required complete24-sensor baseline has not qualified.'};$case=$case[0]
$attempt=$case.attempts[-1];$rounds=[int]$attempt.rounds
if($rounds -notin @(256,512) -or !$attempt.qualified){throw 'Unsupported or unqualified repeat rung.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
. "$PSScriptRoot/annual-reference-process.ps1"
. "$PSScriptRoot/annual-matched-evidence.ps1"
Add-Type -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs"
$source=[IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1")
if((Get-MatchedKernelHash $source) -ne $decl.finite_kernel_sha256 -or (Get-FileHash -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs").Hash.ToLowerInvariant() -ne $decl.aggregation_sha256){throw 'Reference numerical helper identity differs.'}
if((Get-FileHash -LiteralPath "$PSScriptRoot/annual-reference-process.ps1").Hash.ToLowerInvariant() -ne $decl.process_helper_sha256){throw 'Baseline bounded process supervisor differs.'}
$kernel=[regex]::Match($source,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
Add-Type -TypeDefinition $kernel
Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-aggregation.cs" -Destination (Join-Path $OutputDirectory 'aggregation-source.cs')
Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-process.ps1" -Destination (Join-Path $OutputDirectory 'process-source.ps1')
Write-Input 'finite-kernel.cs' $kernel
$name="hour-$IntervalIndex";$inputs=[ordered]@{}
foreach($file in @('scene.rad','sensors.pts','matched-basis.cal',"$name.dat","$name-sun.rad","$name-scaled.rad","$name-direct.rgb","$name-finite.rgb","$name-rounds$rounds.rgb")){
 Copy-Item -LiteralPath (Join-Path $BaselineDirectory $file) -Destination (Join-Path $OutputDirectory $file)
 $inputs[$file]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $file)).Hash.ToLowerInvariant()
}
foreach($file in @('scene.rad','sensors.pts')){if($inputs[$file] -ne $decl.input_sha256.$file){throw 'Baseline geometry or sensors changed.'}}
if($inputs["$name-scaled.rad"] -ne $case.sky_scene_sha256 -or $inputs["$name-direct.rgb"] -ne $case.direct_sha256 -or $inputs["$name-finite.rgb"] -ne $case.finite_sha256 -or $inputs["$name-rounds$rounds.rgb"] -ne $attempt.raw_sha256){throw 'Baseline component bytes changed.'}
foreach($property in $decl.engine_sha256.PSObject.Properties){$sub=if($property.Name.EndsWith('.exe')){'bin'}else{'lib'};if((Get-FileHash -LiteralPath (Join-Path $EngineRoot "$sub/$($property.Name)")).Hash.ToLowerInvariant() -ne $property.Value){throw 'Baseline engine/library identity differs.'}}
foreach($file in @('identical-sky.mtx','suns.rad')){
 if((Get-FileHash -LiteralPath (Join-Path $BaselineDirectory $file)).Hash.ToLowerInvariant() -ne $decl.input_sha256.$file){throw 'Baseline physical sky or solar inputs changed.'}
}
$expectedBasis=ConvertTo-SensitivityGeneratedText ("MF:2;`n"+[IO.File]::ReadAllText((Join-Path $EngineRoot 'lib/reinhart.cal')))
if([IO.File]::ReadAllText((Join-Path $OutputDirectory 'matched-basis.cal')) -cne $expectedBasis){throw 'Baseline sky basis differs from the reviewed MF2 basis.'}
# Read-Matrix resolves against OutputDirectory. Parse the retained complete sky
# in its original directory, then restore the new isolated output location.
$sensitivityOutputDirectory=$OutputDirectory
try{$OutputDirectory=$BaselineDirectory;$sky=Read-Matrix 'identical-sky.mtx'}finally{$OutputDirectory=$sensitivityOutputDirectory}
if($sky.rows -ne 578 -or $sky.columns -ne 8760){throw 'Baseline sky matrix shape differs.'}
$expectedData=[Text.StringBuilder]::new("1`n0 577 578`n")
for($patch=0;$patch -lt 578;$patch++){
 $offset=($patch*8760+$IntervalIndex)*3
 if($sky.values[$offset] -ne $sky.values[$offset+1] -or $sky.values[$offset] -ne $sky.values[$offset+2]){throw 'Expected the retained neutral sky.'}
 $null=$expectedData.Append($sky.values[$offset].ToString('R',$culture)).Append("`n")
}
if([IO.File]::ReadAllText((Join-Path $OutputDirectory "$name.dat")) -cne $expectedData.ToString()){throw 'Baseline hourly sky differs from its declared annual column.'}
$expectedSuns=@([regex]::Matches([IO.File]::ReadAllText((Join-Path $BaselineDirectory 'suns.rad')),'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')|Where-Object{[int](([int]$_.Groups[1].Value-30)/60) -eq $IntervalIndex})
if($expectedSuns.Count -ne 1 -or [IO.File]::ReadAllText((Join-Path $OutputDirectory "$name-sun.rad")) -cne (ConvertTo-SensitivityGeneratedText $expectedSuns[0].Value)){throw 'Baseline hourly source differs from its declared annual source.'}
$sensors=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'sensors.pts'));$sun=[IO.File]::ReadAllText((Join-Path $OutputDirectory "$name-sun.rad"));$scaledSky=[IO.File]::ReadAllText((Join-Path $OutputDirectory "$name-scaled.rad"))
if(!$sensors.EndsWith("`n") -or @($sensors.Trim() -split '\r?\n').Count -ne 24 -or [IO.File]::ReadAllText((Join-Path $OutputDirectory 'scene.rad')) -match '(?m)^\s*!'){throw 'Invalid trusted reference inputs.'}
$direct=[GreenLensMatched.Direct]::Rgb((Join-Path $OutputDirectory "$name-direct.rgb"),24)
$finite=[GreenLensMatched.Direct]::Finite($sensors,$sun,(Join-Path $OutputDirectory "$name-finite.rgb"))
$raw=[GreenLensReference.Aggregate]::Read((Join-Path $OutputDirectory "$name-rounds$rounds.rgb"),(24*$rounds*6))
if($rounds -eq 512){
 $initial=@($case.attempts|Where-Object{$_.rounds -eq 256})
 if($initial.Count -ne 1){throw 'Escalated baseline lacks its original complete 256-round attempt.'}
 $initialFile="$name-rounds256.rgb"
 Copy-Item -LiteralPath (Join-Path $BaselineDirectory $initialFile) -Destination (Join-Path $OutputDirectory $initialFile)
 $inputs[$initialFile]=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $initialFile)).Hash.ToLowerInvariant()
 if($inputs[$initialFile] -ne $initial[0].raw_sha256){throw 'Initial baseline output changed before escalation.'}
 $initialRaw=[GreenLensReference.Aggregate]::Read((Join-Path $OutputDirectory $initialFile),(24*256*6))
 [GreenLensReference.Aggregate]::EqualPrefix($initialRaw,$raw)
}
$rungs=@();foreach($n in @(($rounds/4),($rounds/2),$rounds)){$rungs+=[GreenLensReference.Aggregate]::Calculate($raw,24,0,[int]$n,100,$direct,$finite)}
$first=[GreenLensReference.Aggregate]::Compare($rungs[0],$rungs[1]);$second=[GreenLensReference.Aggregate]::Compare($rungs[1],$rungs[2]);$firstHalf=[GreenLensReference.Aggregate]::Calculate($raw,24,0,($rounds/2),100,$direct,$finite);$secondHalf=[GreenLensReference.Aggregate]::Calculate($raw,24,($rounds/2),($rounds/2),100,$direct,$finite);$blocks=[GreenLensReference.Aggregate]::CompareWithScale($firstHalf,$secondHalf,$rungs[2])
if(@(@($first)+@($second)+@($blocks)|Where-Object{!$_.passed}).Count){throw 'Recomputed baseline convergence fails.'}
$reference=$rungs[2]
for($i=0;$i -lt 24;$i++){if($reference.sensors[$i].lux -ne $attempt.rungs[-1].sensors[$i].lux){throw 'Stored baseline mean differs from its native raw records.'}}
$pattern='(?s)(void light (solar\d+)\s+0\s+0\s+3)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)\s+([\d.eE+\-]+)'
$modifier=[regex]::Match($sun,$pattern).Groups[2].Value;if(!$modifier){throw 'Missing original source modifier.'}
$base=@('-I+','-h','-faa','-n','1','-ab','9','-ad','8192','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-c','1','-V+')
if(($base -join '|') -cne ($decl.full_arguments -join '|')){throw 'Baseline arguments differ from the exact supported estimator.'}
$variants=@(@{name='scale10000';scale=10000;lw='1e-9';ab=9;lr=-16},@{name='weight1e10';scale=100;lw='1e-10';ab=9;lr=-16},@{name='depth10-24';scale=100;lw='1e-9';ab=10;lr=-24})
$executedVariants=@(Get-SensitivityVariantSelection $variants $Variant)
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
$declaration=[ordered]@{schema_version='greenlens.controlled-reference-sensitivity-shard.v1';declared_utc=[DateTime]::UtcNow.ToString('o');scope='Predeclared limited sensitivity at all 24 sensors; not arbitrary-building validation';baseline_declaration=$decl;baseline_declaration_sha256=(Get-FileHash -LiteralPath (Join-Path $BaselineDirectory 'declaration.json')).Hash.ToLowerInvariant();interval_index=$IntervalIndex;final_baseline_rounds=$rounds;variants=$variants;executed_variant_names=@($executedVariants|ForEach-Object{$_.name});input_sha256=$inputs;raw_output_order='Round-major 24-sensor records, sky RGB followed by full-solar RGB';scale_tolerance='abs(normalized_variant-bin - baseline-bin) <= 1e-5*abs(baseline-bin), every RGB channel of both bins; exact zero requires zero';weight_depth_tolerance='max(0.1 lx, 1% baseline mean) at every sensor';centroid_and_finite_policy='Reuse byte-verified original baseline centroid and 4096-ray finite arrays unchanged';harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();supervisor_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-reference-process.ps1").Hash.ToLowerInvariant();limits=@{maximum_process_seconds=7200;maximum_runner_plus_engine_working_set_bytes=2147483648;maximum_scratch_bytes=2147483648;maximum_single_file_bytes=536870912};release_acceptance_complete=$false}
$declaration|ConvertTo-Json -Depth 20|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
if($PrepareOnly){Write-Host 'Prepared sensitivity declaration only; native variants NOT EXECUTED.';return}
$results=[Collections.Generic.List[object]]::new()
foreach($variantSetting in $executedVariants){
 $variantName=$variantSetting.name;$skyText=$scaledSky
 if($variantSetting.scale -ne 100){
  $skyText=New-SensitivitySky $skyText
 }
 Write-Input "$variantName.rad" $skyText
 Invoke-Engine 'oconv' @('-f','scene.rad',"$variantName.rad") "$variantName.oct"
 $options=@($base);$options[[Array]::IndexOf($options,'-ab')+1]="$($variantSetting.ab)";$options[[Array]::IndexOf($options,'-lr')+1]="$($variantSetting.lr)";$options[[Array]::IndexOf($options,'-lw')+1]=$variantSetting.lw
 Invoke-Engine 'rcontrib' ($options+@('-y',"$(24*$rounds)",'-m','sky_glow','-m',$modifier,"$variantName.oct")) "$variantName.rgb" ($sensors*$rounds)
 $values=[GreenLensReference.Aggregate]::Read((Join-Path $OutputDirectory "$variantName.rgb"),(24*$rounds*6))
 $mean=[GreenLensReference.Aggregate]::Calculate($values,24,0,$rounds,$variantSetting.scale,$direct,$finite)
 $checks=@()
 if($variantSetting.scale -ne 100){
  $checks=@(Compare-SensitivityBins $mean $reference)
 }else{$checks=@([GreenLensReference.Aggregate]::CompareWithScale($reference,$mean,$reference))}
 $pass=@($checks|Where-Object{!$_.passed}).Count -eq 0
 $results.Add([ordered]@{variant=$variantSetting;mean=$mean;checks=$checks;passed=$pass;raw_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$variantName.rgb")).Hash.ToLowerInvariant();scene_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory "$variantName.rad")).Hash.ToLowerInvariant()})
 [ordered]@{declaration=$declaration;results=$results;completed=($results.Count -eq $executedVariants.Count);passed=($results.Count -eq $executedVariants.Count -and @($results|Where-Object{!$_.passed}).Count -eq 0);case_complete=($results.Count -eq 3);release_acceptance_complete=$false}|ConvertTo-Json -Depth 25|Set-Content -LiteralPath (Join-Path $OutputDirectory 'sensitivity.json') -Encoding utf8
 Write-Host "Sensitivity $($decl.alternative) interval$IntervalIndex $variantName passed=$pass"
}
if(@($results|Where-Object{!$_.passed}).Count){throw 'Predeclared sensitivity check failed; every variant is retained.'}
