#Requires -Version 7.5
# Read-only verification of retained research data. Never launches an engine or
# executes commands/source from an evidence directory. No partial acceptance.
param([string]$CandidateA='', [string]$CandidateB='',
 [ValidateSet('candidate','higher')][string]$Variant='candidate',
 [string[]]$BaselineDirectories=@(), [string[]]$SensitivityDirectories=@(),
 [string]$OutputDirectory='', [switch]$FunctionsOnly)
$ErrorActionPreference='Stop'
$script:pairedComparatorPath=$PSCommandPath
$pairedRequest=@{OutputDirectory=$OutputDirectory;BaselineDirectories=$BaselineDirectories;SensitivityDirectories=$SensitivityDirectories;FunctionsOnly=[bool]$FunctionsOnly}
. "$PSScriptRoot/annual-collect-reference.ps1" -FunctionsOnly
. "$PSScriptRoot/verify-matrix-multiplication.ps1"
$OutputDirectory=$pairedRequest.OutputDirectory;$BaselineDirectories=$pairedRequest.BaselineDirectories;$SensitivityDirectories=$pairedRequest.SensitivityDirectories;$FunctionsOnly=$pairedRequest.FunctionsOnly
$script:pairedReceiptCache=@{}

function Get-PairedSources {
 return @(
  @('harness-source.ps1','harness_sha256','annual-paired-hammersley.ps1','1d519c54b493c80e77536db8ea86a79cc594e9120674df398ccd8694135837d2'),
  @('primary-source.cs','primary_helper_sha256','annual-paired-primary-stream.cs','49ccb427479ee2951c1b94753dafecd318b519511c3aca7c6b629eed695045cd'),
  @('process-source.ps1','process_helper_sha256','annual-paired-process.ps1','43827a490a82a5173cfa5398ff1d1b479805a218b13a8dac7835042a07669c00'),
  @('native-process-source.cs','native_process_helper_sha256','annual-paired-native-process.cs','7646f693d3f5e431b4fab84ed5d2f1d8d99126459ac50fc4adaac54516d4022b')
 )
}
function Get-PairedTuple([string]$Name){
 switch -CaseSensitive ($Name){
  candidate {return @{primary=16384;secondary=64;bounces=7;finite=1024;side=32}}
  higher {return @{primary=65536;secondary=128;bounces=8;finite=4096;side=64}}
  default {throw 'Unknown paired profile; only candidate or higher is declared.'}
 }
}
function Get-PairedArguments([string]$Name){
 $p=Get-PairedTuple $Name
 return @('-n','1','-faa','-y','24','-ad',"$($p.secondary)",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','-16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-V+','-c',"$($p.primary)",'-e','MF:2','-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow')
}
function Initialize-PairedComparator {
 Initialize-ReferenceCollector
 foreach($source in Get-PairedSources){
  if((Get-MatchedHash "$PSScriptRoot/$($source[2])") -cne $source[3]){throw "Reviewed paired helper changed: $($source[2])"}
 }
 if(!('GreenLensPairedStream.HammersleyProducer' -as [type])){Add-Type -LiteralPath "$PSScriptRoot/annual-paired-primary-stream.cs"}
 foreach($name in @('candidate','higher')){
  $typeName="GreenLensPairedVerify_$name.Direct"
  if(!($typeName -as [type])){Add-Type -TypeDefinition (Get-PairedFiniteSource $name)}
 }
}
function Get-PairedFiniteSource([string]$Name,[switch]$RetainedNamespace){
 $tuple=Get-PairedTuple $Name
 $source=[IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1")
 $kernel=[regex]::Match($source,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
 if(!$kernel){throw 'Missing trusted finite-direct kernel.'}
 $namespace=if($RetainedNamespace){'GreenLensPairedFinite'}else{"GreenLensPairedVerify_$Name"}
 return ($kernel.Replace('GreenLensMatched',$namespace) -replace '\b4096\b',"$($tuple.finite)" -replace '\b64\b',"$($tuple.side)").Replace("`r`n","`n")
}
function Assert-PairedRoot([string]$Root){
 $path=[IO.Path]::GetFullPath($Root)
 $item=Get-Item -LiteralPath $path -Force
 if(!$item.PSIsContainer){throw 'Paired evidence root must be a directory.'}
 for($ancestor=$path;$ancestor;$ancestor=[IO.Path]::GetDirectoryName($ancestor)){
  if((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Evidence paths must not traverse reparse points.'}
 }
}
function Read-PairedMatrix([string]$Root,[string]$Name,[int]$Rows,[int]$Columns){
 if($Rows -lt 1 -or $Rows -gt 578 -or $Columns -lt 1 -or $Columns -gt 8760){throw 'Unbounded matrix request.'}
 $path=Read-ReferenceFile $Root $Name
 $reader=[IO.StreamReader]::new($path);$header=@{};$ended=$false
 try{
  for($i=0;$i -lt 50;$i++){
   $line=$reader.ReadLine();if($null -eq $line){break};if($line.Length -gt 2048){throw 'Excess matrix header.'}
   if($line -ceq ''){$ended=$true;break}
   if($line -match '^(NROWS|NCOLS|NCOMP|FORMAT)=(.*)$'){
    if($header.ContainsKey($Matches[1])){throw 'Duplicate matrix header field.'};$header[$Matches[1]]=$Matches[2]
   }
  }
 }finally{$reader.Dispose()}
 if(!$ended -or $header.NROWS -cne "$Rows" -or $header.NCOLS -cne "$Columns" -or $header.NCOMP -cne '3' -or $header.FORMAT -cne 'ascii'){throw 'Matrix dimensions/order differ from the bounded expected shape.'}
 return [GreenLensFeasibility.Matrix]::Read($path)
}
function Assert-PairedNumeric($Value,[string]$Description,[switch]$Nonnegative){
 if($null -eq $Value -or $Value -is [string] -or $Value -is [bool] -or $Value -isnot [ValueType] -or ![double]::IsFinite([double]$Value) -or ($Nonnegative -and $Value -lt 0)){throw "Invalid numeric $Description."}
}
function Assert-PairedSelectedColumns($Year,$Selected,[int[]]$Indices){
 if($Year.rows -ne $Selected.rows -or $Year.components -ne 3 -or $Selected.components -ne 3 -or $Selected.columns -ne $Indices.Count){throw 'Selected sky matrix shape mismatch.'}
 $seen=[Collections.Generic.HashSet[int]]::new()
 foreach($index in $Indices){if($index -lt 0 -or $index -ge $Year.columns -or !$seen.Add($index)){throw 'Missing/duplicate selected sky interval.'}}
 for($patch=0;$patch -lt $Year.rows;$patch++){for($time=0;$time -lt $Indices.Count;$time++){for($c=0;$c -lt 3;$c++){
  $expected=$Year.values[($patch*$Year.columns+$Indices[$time])*3+$c]
  $actual=$Selected.values[($patch*$Selected.columns+$time)*3+$c]
  Assert-PairedNumeric $expected 'annual sky channel' -Nonnegative;Assert-PairedNumeric $actual 'selected sky channel' -Nonnegative
  if($actual -ne $expected){throw 'Selected sky bytes encode a different annual RGB column/order.'}
 }}}
}
function Assert-PairedDeclaration([string]$Root,$Declaration,[string]$Alternative,[string]$Name,$ReferenceDeclaration){
 $tuple=Get-PairedTuple $Name
 $fields=@('schema_version','declared_utc','scope','alternative','variant','fixed_interval_indices','executed_interval_indices','sensor_count','condition_coverage','weather_binding','normalized_model','input_sha256','engine_sha256','sky_basis_mf','primary_count','secondary_divisions','full_ambient_bounces','direct_ambient_bounces','receiver_scale','finite_samples','primary_rule','conversion','base_arguments','ray_arguments','paired_only','nonnegative_policy','reference_policy','sensitivities','maximum_process_seconds','maximum_runner_plus_engine_working_set_bytes','maximum_scratch_bytes','maximum_single_file_bytes','harness_sha256','primary_helper_sha256','finite_kernel_sha256','process_helper_sha256','native_process_helper_sha256','release_acceptance_complete')
 Assert-ReferenceSame @($Declaration.PSObject.Properties.Name|Sort-Object) @($fields|Sort-Object) 'Paired declaration member set'
 if($Alternative -cnotin @('A','B') -or $Declaration.alternative -cne $Alternative -or $Declaration.variant -cne $Name -or $Declaration.schema_version -cne 'greenlens.paired-primary-hammersley.v1'){throw 'Candidate alternative, profile or schema differs.'}
 $date=[DateTimeOffset]::Parse($Declaration.declared_utc,[Globalization.CultureInfo]::InvariantCulture)
 if($date.Offset.Ticks -ne 0){throw 'Declaration timestamp must be UTC.'}
 $descriptions=@{
  scope='Experimental full-grid candidates; no comparison with unqualified controlled references'
  nonnegative_policy='Reject negative coefficient and assembled reflected terms; retain every failed value, never clamp'
  reference_policy='Only qualified controlled references may establish max(5lx,5%) matched agreement; MF2 sky is identical, annual MF4 convergence remains separate'
  sensitivities='Controlled reference qualification and its fixed sensitivities remain separate; none counted by this candidate experiment'
 }
 foreach($field in $descriptions.Keys){if($Declaration.$field -cne $descriptions[$field]){throw 'Unknown paired declaration policy.'}}
 foreach($pair in @(@('sensor_count',24),@('sky_basis_mf',2),@('primary_count',$tuple.primary),@('secondary_divisions',$tuple.secondary),@('full_ambient_bounces',$tuple.bounces),@('direct_ambient_bounces',0),@('receiver_scale',100),@('finite_samples',$tuple.finite),@('maximum_process_seconds',7200),@('maximum_runner_plus_engine_working_set_bytes',2147483648),@('maximum_scratch_bytes',2147483648),@('maximum_single_file_bytes',536870912))){
  Assert-PairedNumeric $Declaration.($pair[0]) $pair[0] -Nonnegative
  if($Declaration.($pair[0]) -ne $pair[1]){throw "Paired declared setting differs: $($pair[0])"}
 }
 if($Declaration.paired_only -isnot [bool] -or !$Declaration.paired_only -or $Declaration.release_acceptance_complete -isnot [bool] -or $Declaration.release_acceptance_complete){throw 'Invalid paired-only declaration flags.'}
 Assert-ReferenceSame @($Declaration.fixed_interval_indices) (Get-ReferenceFixedIntervals) 'Paired fixed interval order'
 Assert-ReferenceSame @($Declaration.executed_interval_indices) (Get-ReferenceFixedIntervals) 'Paired complete execution plan'
 Assert-ReferenceSame @($Declaration.base_arguments) (Get-PairedArguments $Name) 'Paired coefficient arguments'
 Assert-ReferenceSame @($Declaration.ray_arguments) (Get-ReferenceDirectOptions) 'Paired finite arguments'
 $primaryRule='Hammersley cosine hemisphere: radius=sqrt((i+0.5)/N), azimuth=2*pi*base2_radical_inverse(i), z=sqrt(1-radius^2); sensor-major, i=0..N-1; no rotation/scramble; sensor origin+1e-4 upward; identical uninterrupted full/direct streams'
 if($Declaration.primary_rule -cne $primaryRule -or $Declaration.conversion -cne 'pi/100 times full coefficient diffuse product plus pi/100 times full-minus-direct solar product plus finite direct'){throw 'Unknown primary or assembly policy.'}
 foreach($source in Get-PairedSources){
  if($Declaration.($source[1]) -cne $source[3]){throw 'Unknown paired source identity.'};Assert-ReferenceHash $Root $source[0] $source[3]
 }
 Assert-ReferenceHash $Root 'finite-kernel.cs' $Declaration.finite_kernel_sha256
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root 'finite-kernel.cs')).Replace("`r`n","`n") -cne (Get-PairedFiniteSource $Name -RetainedNamespace)){throw 'Retained finite-direct policy differs from the trusted compiled policy.'}
 $expectedInputs=@('scene.rad','sensors.pts','diffuse-year.mtx','solar-year.mtx','suns.rad','weather.epw','source-recipe.json','model.json','study.json')
 Assert-ReferenceSame @($Declaration.input_sha256.PSObject.Properties.Name|Sort-Object) @($expectedInputs|Sort-Object) 'Candidate input member set'
 foreach($file in $expectedInputs){Assert-ReferenceHash $Root $file $Declaration.input_sha256.$file}
 # The disposable solar year is not a compact recipe field. Both original A/B
 # jobs produced these exact bytes; this fixed experiment pins that observation.
 # This is input provenance, not an independently regenerated solar-sky claim.
 if($Declaration.input_sha256.'solar-year.mtx' -cne '70b9560f9f461bf1db26985f3b3df894546b92c27bc94b623a9d0f6a84c416c4'){throw 'Fixed original solar matrix changed.'}
 foreach($pair in @(@('scene.rad','scene.rad'),@('sensors.pts','sensors.pts'),@('diffuse-year.mtx','identical-sky.mtx'),@('suns.rad','suns.rad'),@('weather.epw','weather.epw'),@('source-recipe.json','source-recipe.json'),@('model.json','model.json'),@('study.json','study.json'))){
  if($Declaration.input_sha256.($pair[0]) -cne $ReferenceDeclaration.input_sha256.($pair[1])){throw "Candidate/reference physical input differs: $($pair[0])"}
 }
 Assert-ReferenceSame $Declaration.normalized_model $ReferenceDeclaration.normalized_model 'Candidate/reference normalized model'
 Assert-ReferenceSame $Declaration.weather_binding $ReferenceDeclaration.weather_binding 'Candidate/reference weather binding'
 $weather=[IO.File]::ReadAllLines((Read-ReferenceFile $Root 'weather.epw'))
 if($weather.Length -ne 8768){throw 'Expected complete hourly non-leap weather year.'}
 $conditions=@(foreach($index in Get-ReferenceFixedIntervals){
  $fields=$weather[8+$index].Split(',');$dni=[double]::Parse($fields[14],[Globalization.CultureInfo]::InvariantCulture);$dhi=[double]::Parse($fields[15],[Globalization.CultureInfo]::InvariantCulture)
  Assert-PairedNumeric $dni 'DNI' -Nonnegative;Assert-PairedNumeric $dhi 'DHI' -Nonnegative
  $condition=if($dhi -lt 50){'low_diffuse'}elseif($dni/$dhi -ge 3){'clear'}elseif($dni/$dhi -ge .5){'mixed'}else{'overcast'}
  @{interval_index=$index;condition=$condition}
 })
 Assert-ReferenceSame $Declaration.condition_coverage $conditions 'Fixed weather condition labels'
 foreach($required in @('overcast','mixed','clear')){if($required -notin $conditions.condition){throw 'Required fixed sky condition coverage missing.'}}
 $inventory=Get-Content -LiteralPath "$PSScriptRoot/../dependencies/radiance.json" -Raw|ConvertFrom-Json -DateKind String
 $engine=[ordered]@{}
 foreach($exe in @('rtrace','oconv')){$engine["$exe.exe"]=$inventory.executables.$exe.sha256}
 foreach($exe in @('rcontrib','dctimestep')){$engine["$exe.exe"]=$inventory.annual_workflow_review.executables.$exe.sha256}
 foreach($lib in @('rayinit.cal','skybright.cal')){$engine[$lib]=$inventory.critical_libraries.$lib.sha256}
 $engine['reinhart.cal']=$inventory.annual_workflow_review.additional_critical_libraries.'reinhart.cal'.sha256
 Assert-ReferenceSame $Declaration.engine_sha256 $engine 'Candidate engine/library identity'
 foreach($property in $ReferenceDeclaration.engine_sha256.PSObject.Properties){if($Declaration.engine_sha256.($property.Name) -cne $property.Value){throw 'Candidate/reference engine identity differs.'}}
 $sensorText=[IO.File]::ReadAllText((Read-ReferenceFile $Root 'sensors.pts'))
 if(@($sensorText.Trim() -split '\r?\n').Count -ne 24 -or !$sensorText.EndsWith("`n")){throw 'Exactly24 newline-terminated sensors required.'}
 # Constructor validates all six numeric fields and upward normals; it runs no process.
 $null=[GreenLensPairedStream.HammersleyProducer]::new($sensorText,$tuple.primary)
 return $sensorText
}
function Get-PairedExpectedReceipt([string]$Sensors,[int]$Count){
 $key=$Count.ToString()+':'+[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Sensors)))
 if(!$script:pairedReceiptCache.ContainsKey($key)){
  $producer=[GreenLensPairedStream.HammersleyProducer]::new($Sensors,$Count)
  $script:pairedReceiptCache[$key]=$producer.GenerateAsync([IO.Stream]::Null,[Threading.CancellationToken]::None).GetAwaiter().GetResult()
 }
 return $script:pairedReceiptCache[$key]
}
function Assert-PairedProcess($Record,[string]$Executable,[string[]]$Arguments,[string]$Root,[string]$Output,[long]$InputBytes,$Receipt=$null){
 if($Record.job_assignment -cne 'CreateProcess STARTUPINFOEX JOB_LIST' -or $Record.job_empty -isnot [bool] -or !$Record.job_empty -or $Record.stream_tasks_completed -isnot [bool] -or !$Record.stream_tasks_completed){throw 'Paired process job/stream completion is missing.'}
 $expectedLimits=@{timeout_seconds=7200.0;cleanup_seconds=5;memory_bytes=2147483648;scratch_bytes=2147483648;file_bytes=536870912}
 Assert-ReferenceSame $Record.limits $expectedLimits 'Paired process limits'
 # Shared collector enforces successful exits, exact argv and bounded time/bytes.
 $copy=Copy-ReferenceJsonObject $Record;$null=$copy.limits.Remove('cleanup_seconds')
 Assert-ReferenceProcess $copy $Executable $Arguments $Root $Output
 Assert-PairedNumeric $Record.input_bytes 'process input bytes' -Nonnegative
 if($Record.input_bytes -ne [Math]::Floor([double]$Record.input_bytes) -or $Record.input_bytes -ne $InputBytes){throw 'Paired process input byte count differs.'}
 if($null -eq $Receipt){
  if($Record.input_transport -cne 'small-text' -or $null -ne $Record.input_receipt){throw 'Unexpected paired text-input transport/receipt.'}
 }else{
  if($Record.input_transport -cne 'typed-continuous-generated-stream-65536'){throw 'Paired process must use one continuous generated stream.'}
  Assert-ReferenceSame $Record.input_receipt $Receipt 'Actual generated process receipt'
 }
}
function Get-PairedNegativeBins($Full,$Direct){
 if($Full.rows -ne $Direct.rows -or $Full.columns -ne $Direct.columns -or $Full.values.Length -ne $Direct.values.Length){throw 'Paired coefficient shape differs.'}
 $negative=[Collections.Generic.List[object]]::new()
 for($i=0;$i -lt $Full.values.Length;$i++){
  Assert-PairedNumeric $Full.values[$i] 'full coefficient' -Nonnegative;Assert-PairedNumeric $Direct.values[$i] 'direct coefficient' -Nonnegative
  if($Full.values[$i] -lt $Direct.values[$i]){$negative.Add(@{sensor_index=[int][Math]::Floor($i/($Full.columns*3));patch_index=[int][Math]::Floor(($i%($Full.columns*3))/3);channel=$i%3;full=$Full.values[$i];direct=$Direct.values[$i];residual=$Full.values[$i]-$Direct.values[$i]})}
 }
 return ,$negative.ToArray()
}
function Get-PairedSensor($Diffuse,$Solar,$Direct,[double[]]$Finite,[int]$Sensor,[int]$Hour){
 $offset=($Sensor*$Diffuse.columns+$Hour)*3
 $d=@();$s=@();$c=@();$r=@();$total=@();$valid=$true
 if($Finite.Length -ne 3){throw 'Finite-direct RGB shape differs.'}
 for($channel=0;$channel -lt 3;$channel++){
  foreach($matrix in @($Diffuse,$Solar,$Direct)){Assert-PairedNumeric $matrix.values[$offset+$channel] 'product RGB' -Nonnegative}
  Assert-PairedNumeric $Finite[$channel] 'finite-direct RGB' -Nonnegative
  $d+=$Diffuse.values[$offset+$channel]*[Math]::PI/100
  $s+=$Solar.values[$offset+$channel]*[Math]::PI/100
  $c+=$Direct.values[$offset+$channel]*[Math]::PI/100
  $r+=$s[$channel]-$c[$channel];$total+=$d[$channel]+$r[$channel]+$Finite[$channel]
  if(![double]::IsFinite($total[$channel]) -or $total[$channel] -lt 0 -or $r[$channel] -lt 0){$valid=$false}
 }
 $lux=179*(.265074126*$total[0]+.670114631*$total[1]+.064811243*$total[2])
 if(![double]::IsFinite($lux)){throw 'Nonfinite recomputed illuminance.'}
 return @{sensor_index=$Sensor;sensor_id=('s'+($Sensor+1).ToString('D6'));diffuse_rgb=$d;full_solar_rgb=$s;patch_direct_rgb=$c;reflected_solar_rgb=$r;finite_direct_rgb=$Finite;total_rgb=$total;lux=$lux;valid=$valid}
}
function Read-PairedCandidate([string]$Root,[string]$Alternative,[string]$Name,$ReferenceDeclaration,[string]$Diagnostics){
 Assert-PairedRoot $Root
 $declHash=Reference-Hash $Root 'declaration.json';$reportHash=Reference-Hash $Root 'paired-grid.json';$processHash=Reference-Hash $Root 'processes.json'
 $decl=Read-ReferenceJson $Root 'declaration.json';$report=Read-ReferenceJson $Root 'paired-grid.json'
 $sensors=Assert-PairedDeclaration $Root $decl $Alternative $Name $ReferenceDeclaration
 Assert-ReferenceSame $report.declaration $decl 'Embedded paired declaration'
 if($report.completed -isnot [bool] -or !$report.completed -or @($report.cases).Count -ne 12){throw 'Incomplete paired result; all12 cases are required.'}
 $outputs=@('full-coefficients.mtx','direct-coefficients.mtx','primary-stream.json','diffuse-selected.mtx','solar-selected.mtx','diffuse-rgb.mtx','solar-rgb.mtx','direct-solar-rgb.mtx')
 Assert-ReferenceSame @($report.output_sha256.PSObject.Properties.Name|Sort-Object) @($outputs|Sort-Object) 'Paired output member set'
 foreach($file in $outputs){Assert-ReferenceHash $Root $file $report.output_sha256.$file}
 $tuple=Get-PairedTuple $Name;$indices=Get-ReferenceFixedIntervals
 $receipt=Get-PairedExpectedReceipt $sensors $tuple.primary
 $stream=Read-ReferenceJson $Root 'primary-stream.json'
 if($stream.schema_version -cne 'greenlens.paired-primary-stream.v1' -or $stream.generator_sha256 -cne $decl.primary_helper_sha256 -or $stream.rule -cne $decl.primary_rule){throw 'Invalid stream declaration.'}
 Assert-ReferenceSame $stream.full $receipt 'Regenerated full receipt';Assert-ReferenceSame $stream.direct $receipt 'Regenerated direct receipt'
 $processes=@(Read-ReferenceJson $Root 'processes.json');if($processes.Count -ne 19){throw 'Missing/excess/duplicate paired process records.'}
 if([IO.File]::ReadAllText((Read-ReferenceFile $Root 'scaled-receiver.rad')) -cne "void glow sky_glow`n0`n0`n4 100 100 100 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n" -or [IO.File]::ReadAllText((Read-ReferenceFile $Root 'unit-receiver.rad')) -cne "void glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"){throw 'Coefficient or transmission receiver changed.'}
 $cursor=0
 Assert-PairedProcess $processes[$cursor++] oconv @('-f','scene.rad','scaled-receiver.rad') $Root 'coefficient.oct' 0
 Assert-PairedProcess $processes[$cursor++] oconv @('-f','scene.rad','unit-receiver.rad') $Root 'transmission.oct' 0
 Assert-PairedProcess $processes[$cursor++] rcontrib ((Get-PairedArguments $Name)+@('-ab',"$($tuple.bounces)",'coefficient.oct')) $Root 'full-coefficients.mtx' $receipt.Bytes $receipt
 Assert-PairedProcess $processes[$cursor++] rcontrib ((Get-PairedArguments $Name)+@('-ab','0','coefficient.oct')) $Root 'direct-coefficients.mtx' $receipt.Bytes $receipt
 $full=Read-PairedMatrix $Root 'full-coefficients.mtx' 24 578;$direct=Read-PairedMatrix $Root 'direct-coefficients.mtx' 24 578
 $negative=Get-PairedNegativeBins $full $direct
 Assert-ReferenceSame @($report.negative_coefficient_bins) $negative 'Every negative coefficient bin'
 $diffuseSky=Read-PairedMatrix $Root 'diffuse-selected.mtx' 578 12;$solarSky=Read-PairedMatrix $Root 'solar-selected.mtx' 578 12
 foreach($kind in @('diffuse','solar')){
  $year=Read-PairedMatrix $Root "$kind-year.mtx" 578 8760
  $selected=if($kind -eq 'diffuse'){$diffuseSky}else{$solarSky}
  Assert-PairedSelectedColumns $year $selected $indices
  $year=$null
 }
 $products=@{};$arithmetic=@()
 foreach($item in @(@('diffuse-rgb.mtx','full-coefficients.mtx','diffuse-selected.mtx'),@('solar-rgb.mtx','full-coefficients.mtx','solar-selected.mtx'),@('direct-solar-rgb.mtx','direct-coefficients.mtx','solar-selected.mtx'))){
  Assert-PairedProcess $processes[$cursor++] dctimestep @('-oa',$item[1],$item[2]) $Root $item[0] 0
  $product=Read-PairedMatrix $Root $item[0] 24 12;$products[$item[0]]=$product
  $coeff=if($item[1] -eq 'full-coefficients.mtx'){$full}else{$direct};$sky=if($item[2] -eq 'diffuse-selected.mtx'){$diffuseSky}else{$solarSky}
  $arithmetic+=,(Assert-IndependentRgbProduct -Coefficients $coeff -Sky $sky -Product $product -EvidencePath (Join-Path $Diagnostics "$Alternative-$($item[0]).arithmetic.json"))
 }
 $sunText=[IO.File]::ReadAllText((Read-ReferenceFile $Root 'suns.rad'));$sources=@{}
 foreach($match in [regex]::Matches($sunText,'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')){
  $minute=[int]$match.Groups[1].Value
  if(($minute-30)%60 -ne 0 -or $minute -lt 30 -or $minute -gt 525570 -or $sources.ContainsKey($minute)){throw 'Malformed or duplicate midpoint solar source.'}
  $sources[$minute]=$match.Value.Replace("`r`n","`n")
 }
 $kernelType="GreenLensPairedVerify_$Name.Direct" -as [type];$cases=@();$caseHashes=@()
 for($hour=0;$hour -lt 12;$hour++){
  $index=$indices[$hour];$stored=$report.cases[$hour]
  if($stored.interval_index -ne $index -or @($stored.sensors).Count -ne 24){throw 'Missing/duplicate/reordered candidate case/sensors.'}
  $sun=$sources[$index*60+30];if(!$sun){throw 'Required midpoint source missing.'}
  Assert-ReferenceHash $Root "hour-$index-sun.rad" $stored.sun_sha256
  if([IO.File]::ReadAllText((Read-ReferenceFile $Root "hour-$index-sun.rad")) -cne $sun){throw 'Candidate hourly sun differs from exact annual source.'}
  Assert-ReferenceHash $Root "hour-$index-finite.rgb" $stored.finite_sha256
  $rays=$kernelType::Rays($sensors,$sun)
  Assert-PairedProcess $processes[$cursor++] rtrace ((Get-ReferenceDirectOptions)+@('transmission.oct')) $Root "hour-$index-finite.rgb" ([Text.Encoding]::UTF8.GetByteCount($rays))
  $finite=$kernelType::Finite($sensors,$sun,(Read-ReferenceFile $Root "hour-$index-finite.rgb"));$rows=@()
  for($sensor=0;$sensor -lt 24;$sensor++){
   $row=Get-PairedSensor $products['diffuse-rgb.mtx'] $products['solar-rgb.mtx'] $products['direct-solar-rgb.mtx'] $finite[$sensor] $sensor $hour
   Assert-ReferenceSame $stored.sensors[$sensor] $row 'Stored RGB components/illuminance/sensor order'
   $rows+=,$row
  }
  $cases+=,@{alternative=$Alternative;interval_index=$index;sensors=$rows}
  $caseHashes+=,@{interval_index=$index;finite_sha256=$stored.finite_sha256;sun_sha256=$stored.sun_sha256}
 }
 $valid=$negative.Count -eq 0 -and @($cases.sensors|Where-Object{!$_.valid}).Count -eq 0
 Assert-ReferenceSame $report.all_values_finite_nonnegative $valid 'Paired numerical validity flag'
 foreach($field in @('matched_reference_qualified','candidate_accuracy_accepted','release_acceptance_complete')){if($report.$field -isnot [bool] -or $report.$field){throw 'Research result improperly claims prior acceptance.'}}
 # Detect changing input/report snapshots rather than silently mixing observations.
 foreach($file in $outputs){Assert-ReferenceHash $Root $file $report.output_sha256.$file}
 foreach($property in $decl.input_sha256.PSObject.Properties){Assert-ReferenceHash $Root $property.Name $property.Value}
 foreach($case in $caseHashes){Assert-ReferenceHash $Root "hour-$($case.interval_index)-finite.rgb" $case.finite_sha256;Assert-ReferenceHash $Root "hour-$($case.interval_index)-sun.rad" $case.sun_sha256}
 Assert-ReferenceHash $Root 'declaration.json' $declHash;Assert-ReferenceHash $Root 'paired-grid.json' $reportHash;Assert-ReferenceHash $Root 'processes.json' $processHash
 return [ordered]@{alternative=$Alternative;variant=$Name;declaration=$decl;declaration_sha256=$declHash;report_sha256=$reportHash;processes_sha256=$processHash;output_sha256=$report.output_sha256;case_raw_sha256=$caseHashes;primary_receipt=$receipt;verified_processes=$processes;arithmetic=$arithmetic;negative_coefficient_bins=$negative;valid=$valid;cases=$cases}
}
function Compare-PairedValues($Candidates,$References){
 if($References.qualified -isnot [bool] -or !$References.qualified -or !$References.coverage.complete -or !$References.all_required_sensitivities_complete -or @($References.cases).Count -ne 24 -or @($References.sensitivities).Count -ne 2 -or @($References.sensitivities|Where-Object{!$_.completed -or !$_.qualified}).Count){throw 'Complete qualified reference and both sensitivities are mandatory.'}
 if(@($Candidates).Count -ne 2 -or $Candidates[0].alternative -cne 'A' -or $Candidates[1].alternative -cne 'B' -or $Candidates[0].variant -cne $Candidates[1].variant){throw 'Exactly ordered A/B from one paired profile are required.'}
 $null=Test-ReferenceCaseCoverage @($References.cases) -RequireComplete
 Assert-ReferenceSame @($References.sensitivities.case_key|Sort-Object) @('A:4116','B:8512') 'Fixed sensitivity identities'
 $null=Get-PairedTuple $Candidates[0].variant
 $referenceByKey=@{};foreach($case in $References.cases){$key="$($case.alternative):$($case.interval_index)";if($referenceByKey.ContainsKey($key) -or !$case.qualified){throw 'Duplicate or unqualified reference case.'};$referenceByKey[$key]=$case}
 $checks=@();$failures=0;$invalid=0;$maximum=0.0
 foreach($candidate in $Candidates){
  if(@($candidate.cases).Count -ne 12){throw 'Incomplete candidate cases.'}
  $indexPosition=0
  foreach($index in Get-ReferenceFixedIntervals){
   $case=$candidate.cases[$indexPosition++];$key="$($candidate.alternative):$index"
   if($case.alternative -cne $candidate.alternative -or $case.interval_index -ne $index -or !$referenceByKey.ContainsKey($key)){throw 'Candidate/reference order or coverage mismatch.'}
   $reference=$referenceByKey[$key];if(@($case.sensors).Count -ne 24 -or @($reference.reference.sensors).Count -ne 24){throw 'Expected every ordered sensor.'}
   $rows=@();$caseFailures=0
   for($sensor=0;$sensor -lt 24;$sensor++){
    $a=$case.sensors[$sensor];$b=$reference.reference.sensors[$sensor];$id='s'+($sensor+1).ToString('D6')
    if($a.sensor_index -ne $sensor -or $b.sensor_index -ne $sensor -or $a.sensor_id -cne $id -or $b.sensor_id -cne $id){throw 'Stable sensor order/identity differs.'}
    Assert-PairedNumeric $a.lux 'candidate illuminance';Assert-PairedNumeric $b.lux 'reference illuminance' -Nonnegative
    if($a.valid -isnot [bool]){throw 'Invalid candidate validity type.'}
    $delta=[double]$a.lux-[double]$b.lux;$error=[Math]::Abs($delta);$allowed=[Math]::Max(5.0,.05*$b.lux)
    $pass=$a.valid -and $a.lux -ge 0 -and $error -le $allowed
    if(!$a.valid -or $a.lux -lt 0){$invalid++};if(!$pass){$caseFailures++;$failures++}
    $maximum=[Math]::Max($maximum,$error/$allowed)
    $rows+=,@{sensor_index=$sensor;sensor_id=$id;candidate_lux=$a.lux;reference_lux=$b.lux;delta_lux=$delta;absolute_error_lux=$error;allowed_error_lux=$allowed;candidate_valid=$a.valid;passed=$pass}
   }
   $checks+=,@{alternative=$candidate.alternative;interval_index=$index;failure_count=$caseFailures;passed=($caseFailures -eq 0);sensors=$rows}
  }
 }
 $negative=0;foreach($candidate in $Candidates){if($null -ne $candidate.negative_coefficient_bins){$negative+=$candidate.negative_coefficient_bins.Count}}
 return [ordered]@{completed=$true;compared_values=576;failure_count=$failures;invalid_value_count=$invalid;negative_coefficient_bin_count=$negative;maximum_error_over_tolerance=$maximum;matched_sky_passed=($failures -eq 0 -and $negative -eq 0 -and @($Candidates|Where-Object{!$_.valid}).Count -eq 0);checks=$checks}
}
if($FunctionsOnly){return}
if(!$OutputDirectory -or (Test-Path -LiteralPath $OutputDirectory)){throw 'Choose a new comparison directory; previous evidence is immutable.'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$utf8=[Text.UTF8Encoding]::new($false)
function Write-PairedEvidence([string]$Name,$Value){[IO.File]::WriteAllText((Join-Path $OutputDirectory $Name),(ConvertTo-Json -InputObject $Value -Depth 100).Replace("`r`n","`n")+"`n",$utf8)}
try{
 Initialize-PairedComparator
 $references=Collect-ReferenceEvidence -Baselines $BaselineDirectories -Sensitivities $SensitivityDirectories
 Write-PairedEvidence 'reference-collection.json' $references
 if(!$references.qualified){throw 'Reference remains incomplete or unqualified; no paired accuracy acceptance is possible.'}
 $candidates=@()
 foreach($alternative in @('A','B')){
  $baseline=@($references.identity.declarations|Where-Object alternative -eq $alternative)[0]
  $directory=if($alternative -eq 'A'){$CandidateA}else{$CandidateB}
  $candidate=Read-PairedCandidate $directory $alternative $Variant $baseline $OutputDirectory
  Write-PairedEvidence "$alternative-recomputed.json" $candidate;$candidates+=,$candidate
 }
 foreach($file in @('sensors.pts','diffuse-year.mtx','solar-year.mtx','suns.rad','weather.epw')){if($candidates[0].declaration.input_sha256.$file -cne $candidates[1].declaration.input_sha256.$file){throw 'A/B weather, sky or world-space sensors differ.'}}
 $comparison=Compare-PairedValues $candidates $references
 $record=[ordered]@{schema_version='greenlens.paired-qualified-reference-comparison.v1';captured_utc=[DateTime]::UtcNow.ToString('o');variant=$Variant;comparator_sha256=(Get-MatchedHash $script:pairedComparatorPath);reference_collection_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'reference-collection.json'));candidate_evidence_sha256=@{A=(Get-MatchedHash (Join-Path $OutputDirectory 'A-recomputed.json'));B=(Get-MatchedHash (Join-Path $OutputDirectory 'B-recomputed.json'))};tolerance='Every sensor: max(5 lx, 5% of qualified reference)';result=$comparison;scope='Matched-sky research comparison only. Full annual metric convergence, independent analytical checks, production integration and installed release acceptance remain separate.';release_acceptance_complete=$false}
 Write-PairedEvidence 'comparison.json' $record
 if(!$comparison.matched_sky_passed){throw 'Fixed paired matched-sky tolerance or nonnegative policy failed; all576 values and negative bins are retained.'}
 Write-Host 'All576 paired values passed against complete qualified references; release acceptance remains separate.'
}catch{
 $message=$_.Exception.Message
 foreach($path in @($CandidateA,$CandidateB)+@($BaselineDirectories)+@($SensitivityDirectories)+@($OutputDirectory)){if($path){$message=$message.Replace($path,'<evidence-directory>')}}
 $message=[regex]::Replace($message,'[A-Za-z]:[\\/][^\r\n"'']+','<local-path>')
 Write-PairedEvidence 'failure.json' ([ordered]@{schema_version='greenlens.paired-comparison-failure.v1';captured_utc=[DateTime]::UtcNow.ToString('o');error=$message;matched_sky_passed=$false;release_acceptance_complete=$false})
 throw
}finally{
 $files=@(Get-ChildItem -LiteralPath $OutputDirectory -File|Where-Object Name -ne 'checksums.json'|ForEach-Object{[ordered]@{path=$_.Name;bytes=$_.Length;sha256=(Get-MatchedHash $_.FullName)}})
 Write-PairedEvidence 'checksums.json' ([ordered]@{schema_version='greenlens.paired-comparison-seal.v1';files=$files})
}
