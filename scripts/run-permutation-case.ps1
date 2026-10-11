#Requires -Version 7.5
param([Parameter(Mandatory)][ValidateSet('Prepare','Run','Read')][string]$Mode,[Parameter(Mandatory)][string]$Case,[Parameter(Mandatory)][string]$BaselineDirectory,[Parameter(Mandatory)][string]$EndpointDirectory,[Parameter(Mandatory)][string]$OutputDirectory,[Parameter(Mandatory)][string]$TrustedSourceRoot)
$ErrorActionPreference='Stop'
$ownedOutput=$OutputDirectory
. "$PSScriptRoot/permutation-helpers.ps1"
$setting=Get-PermutationCase $Case
$plan=Get-Content -LiteralPath "$PSScriptRoot/permutation-plan.json" -Raw|ConvertFrom-Json -Depth 60 -DateKind String
Assert-PermutationPlan $plan
if($Mode-ceq'Run' -and !$plan.execution_authorized){throw 'Diagnostic execution is not yet authorized in this reviewed declaration.'}
function Assert-DepthSources {
foreach($property in $plan.trusted_source_sha256.PSObject.Properties){if((Get-FileHash -LiteralPath (Join-Path $TrustedSourceRoot $property.Name)).Hash.ToLowerInvariant()-cne$property.Value){throw "Trusted source pin differs: $($property.Name)"}}
foreach($property in $plan.diagnostic_source_sha256.PSObject.Properties){if((Get-FileHash -LiteralPath (Join-Path $PSScriptRoot $property.Name)).Hash.ToLowerInvariant()-cne$property.Value){throw "Diagnostic source pin differs: $($property.Name)"}}
}
Assert-DepthSources
$verified=& {
 param($source,$baselinePath,$endpointPath)
 . (Join-Path $source 'scripts/annual-collect-reference.ps1') -FunctionsOnly
 Initialize-ReferenceCollector
 $baseline=Read-ReferenceBaseline $baselinePath
 if($baseline.cases.Count-ne1 -or $baseline.contexts.Count-ne1){throw 'Exactly one baseline case required.'}
 $endpoint=Read-ReferenceSensitivity $endpointPath $baseline.contexts[0]
 @{baseline=$baseline;endpoint=$endpoint}
} $TrustedSourceRoot $BaselineDirectory $EndpointDirectory
$baseline=$verified.baseline;$endpoint=$verified.endpoint;$context=$baseline.contexts[0]
if($baseline.declaration.alternative-cne'A' -or $context.key-cne'A:4116' -or !$baseline.cases[0].qualified -or $baseline.cases[0].attempts[-1].rounds-ne256){throw 'Exact qualified A4116 R256 baseline required.'}
if(!$endpoint.completed -or $endpoint.qualified -or ($endpoint.executed_variant_names-join'|')-cne'depth10-24' -or $endpoint.variants.Count-ne1){throw 'Exact completed failed depth endpoint required.'}
foreach($key in @('declaration_sha256','report_sha256','processes_sha256')){
 if($baseline.$key-cne$plan.baseline.$key -or $endpoint.$key-cne$plan.endpoint.$key){throw 'Original endpoint identity differs.'}
}
if(($baseline.declaration.full_arguments-join'|')-cne ((Get-PermutationArguments 'seed0-baseline' 'solar246990')[0..38]-join'|')){throw 'Fixed reference argument prefix differs.'}
$inputNames=@('scene.rad','sensors.pts','matched-basis.cal','hour-4116.dat','hour-4116-sun.rad','hour-4116-scaled.rad','hour-4116-direct.rgb','hour-4116-finite.rgb')
# The strict endpoint reader has already tied these digests to the validated baseline.
$inputs=[ordered]@{};foreach($file in $inputNames){$pin=$endpoint.binding.input_sha256.$file;if($pin-isnot[string] -or $pin-cnotmatch'^[0-9a-f]{64}$'){throw 'Strict endpoint input binding is absent.'};$inputs[$file]=$pin;if((Get-FileHash -LiteralPath (Join-Path $BaselineDirectory $file)).Hash.ToLowerInvariant()-cne$pin){throw 'Bound baseline input changed after strict validation.'}}
$sensorBytes=Read-DepthFile (Join-Path $BaselineDirectory 'sensors.pts') 65536
if((Get-DepthBytesHash $sensorBytes)-cne$inputs['sensors.pts']){throw 'Sensor bytes differ from strict endpoint binding.'}
$sensors=[Text.UTF8Encoding]::new($false,$true).GetString($sensorBytes)
$stream=New-PermutationStream $sensors $setting.seed
$argsList=Get-PermutationArguments $Case $context.modifier
$streamIdentity=[ordered]@{};foreach($key in $stream.Keys){if($key-cne'text'){$streamIdentity[$key]=$stream[$key]}}
$expected=[ordered]@{schema_version='greenlens.permutation-investigation.v1';scope='Fixed two-seed baseline/depth ordering investigation; original reference family remains unqualified';case=$setting;full_fixed_plan=@(Get-PermutationCases);alternative='A';interval_index=4116;sensors=24;rounds=256;source_scale=100;source_engine_sha256=$baseline.declaration.engine_sha256;baseline=$plan.baseline;failed_depth_endpoint=$plan.endpoint;input_sha256=$inputs;stream=$streamIdentity;oconv_arguments=@('-f','scene.rad','hour-4116-scaled.rad');rcontrib_arguments=$argsList;supervision='Atomic Job Object; same numerical argv and7200s/2GiB/512MiB limits';limits=@{timeout_seconds=7200;memory_bytes=2147483648;scratch_bytes=2147483648;file_bytes=536870912};trusted_source_sha256=$plan.trusted_source_sha256;diagnostic_source_sha256=$plan.diagnostic_source_sha256;plan_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/permutation-plan.json").Hash.ToLowerInvariant();reference_family_qualified=$false;release_accepted=$false}
$utf8=[Text.UTF8Encoding]::new($false)
function Write-DepthJson([string]$Name,$Value){[IO.File]::WriteAllText((Join-Path $ownedOutput $Name),($Value|ConvertTo-Json -Depth 100)+"`n",$utf8)}
function Canonical-Depth($Value){& {param($v,$s). (Join-Path $s 'scripts/annual-reference-identity.ps1');ConvertTo-ReferenceCanonicalJson $v} $Value $TrustedSourceRoot}
if($Mode-cne'Read'){
 if(Test-Path -LiteralPath $ownedOutput){throw 'Fresh output directory required.'};$null=New-Item -ItemType Directory -Path $ownedOutput
 foreach($file in $inputNames){Copy-Item -LiteralPath (Join-Path $BaselineDirectory $file) -Destination (Join-Path $ownedOutput $file)}
 [IO.File]::WriteAllText((Join-Path $ownedOutput 'primary.pts'),$stream.text,$utf8)
 $declaration=[ordered]@{declared_utc=[DateTime]::UtcNow.ToString('o');recipe=$expected};Write-DepthJson 'declaration.json' $declaration
 foreach($file in $inputNames){if((Get-FileHash -LiteralPath (Join-Path $ownedOutput $file)).Hash.ToLowerInvariant()-cne$inputs[$file]){throw 'Copied input differs before preparation/launch.'}}
 if((Get-FileHash (Join-Path $ownedOutput 'primary.pts')).Hash.ToLowerInvariant()-cne$stream.sha256){throw 'Copied primary stream differs.'}
 Assert-DepthSources
 if($Mode-ceq'Prepare'){return [ordered]@{prepared=$true;executed=$false;case=$Case;stream_sha256=$stream.sha256}}
 $hardware=Get-PermutationHost;Write-DepthJson 'runtime-host.json' $hardware
 $EngineRoot=Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'
 foreach($property in $baseline.declaration.engine_sha256.PSObject.Properties){$sub=if($property.Name.EndsWith('.exe')){'bin'}else{'lib'};if((Get-FileHash -LiteralPath (Join-Path $EngineRoot "$sub/$($property.Name)")).Hash.ToLowerInvariant()-cne$property.Value){throw 'Pinned engine/library bytes differ.'}}
 foreach($file in $inputNames){if((Get-FileHash -LiteralPath (Join-Path $ownedOutput $file)).Hash.ToLowerInvariant()-cne$inputs[$file]){throw 'Input changed before engine launch.'}}
 if((Get-FileHash (Join-Path $ownedOutput 'primary.pts')).Hash.ToLowerInvariant()-cne$stream.sha256){throw 'Primary stream changed before launch.'}
 Assert-DepthSources
 $OutputDirectory=$ownedOutput;$records=[Collections.Generic.List[object]]::new()
 . (Join-Path $TrustedSourceRoot 'scripts/annual-paired-process.ps1')
 try{
  Invoke-Engine 'oconv' $expected.oconv_arguments 'diagnostic.oct'
  Invoke-PairedProcess -Executable (Join-Path $EngineRoot 'bin/rcontrib.exe') -Name 'rcontrib' -Arguments $argsList -Output 'raw.rgb' -InputFile 'primary.pts'
 }catch{Write-DepthJson 'failure.json' @{state='failed';case=$Case;error=($_.Exception.Message.Replace($ownedOutput,'<diagnostic>').Replace($EngineRoot,'<engine>'));recorded_utc=[DateTime]::UtcNow.ToString('o');completed=$false};throw}
}else{
 $declaration=Read-PermutationJson (Join-Path $ownedOutput 'declaration.json')
 if((Canonical-Depth $declaration.recipe)-cne(Canonical-Depth $expected)){throw 'Retained diagnostic declaration differs from exact original inputs/settings/sources.'}
}
foreach($file in $inputNames){if((Get-FileHash -LiteralPath (Join-Path $ownedOutput $file)).Hash.ToLowerInvariant()-cne$inputs[$file]){throw 'Diagnostic input bytes changed.'}}
if((Get-FileHash -LiteralPath (Join-Path $ownedOutput 'primary.pts')).Hash.ToLowerInvariant()-cne$stream.sha256){throw 'Primary order or bytes changed.'}
$processes=@(Read-PermutationJson (Join-Path $ownedOutput 'processes.json'))
if($processes.Count-ne2){throw 'Exactly two successful process records required.'}
$previousFinish=Assert-PermutationUtc $declaration.declared_utc
$hardware=Read-PermutationJson (Join-Path $ownedOutput 'runtime-host.json');Assert-PermutationHost $hardware
$hardwareUtc=Assert-PermutationUtc $hardware.recorded_utc
if($hardwareUtc-lt$previousFinish){throw 'Hardware observation predates this declaration.'}
for($i=0;$i-lt2;$i++){
 $wanted=if($i-eq0){$expected.oconv_arguments}else{$argsList};$exe=if($i-eq0){'oconv.exe'}else{'rcontrib.exe'}
 $outputName=if($i-eq0){'diagnostic.oct'}else{'raw.rgb'};$inputBytes=if($i-eq0){0L}else{$stream.bytes};$transport=if($i-eq0){'small-text'}else{'owned-file-stream-65536'}
 $times=Assert-PermutationProcess $processes[$i] $exe $wanted (Get-Item (Join-Path $ownedOutput $outputName)).Length $inputBytes $transport $previousFinish
 if($i-eq0 -and $times.start-lt$hardwareUtc){throw 'Hardware observation must precede engine execution.'}
 if($previousFinish -and $times.start-lt$previousFinish){throw 'Sequential process times overlap.'};$previousFinish=$times.finish
}
Assert-DepthSources
if($Mode-ceq'Run'){foreach($property in $baseline.declaration.engine_sha256.PSObject.Properties){$sub=if($property.Name.EndsWith('.exe')){'bin'}else{'lib'};if((Get-FileHash -LiteralPath (Join-Path $EngineRoot "$sub/$($property.Name)")).Hash.ToLowerInvariant()-cne$property.Value){throw 'Engine bytes changed during run.'}}}
$rawPath=Join-Path $ownedOutput 'raw.rgb';if((Get-Item $rawPath).Length-ne$processes[1].output_bytes){throw 'Native output length differs.'}
# Fixed 36,864 ASCII scalars have a conservative 2 MiB read bound, independent
# of the supervisor's unchanged 512 MiB file cap. Parse the same bytes we hash.
$rawBytes=Read-DepthFile $rawPath 2097152
$rawSha=Get-DepthBytesHash $rawBytes
$rawText=[Text.UTF8Encoding]::new($false,$true).GetString($rawBytes)
$values=ConvertFrom-PermutationAscii $rawText
$canonical=ConvertFrom-PermutationOrder $values $stream.emitted_to_original $setting.seed
$diagnostics=Measure-PermutationStream $canonical $context.direct $context.finite
$mean=$diagnostics.prefix_means['256']
Assert-DepthMean $mean
$result=[ordered]@{schema_version='greenlens.permutation-case-result.v1';case=$Case;recipe=$expected;declaration_sha256=(Get-FileHash (Join-Path $ownedOutput 'declaration.json')).Hash.ToLowerInvariant();raw_sha256=$rawSha;runtime_host=$hardware;runtime_host_sha256=(Get-FileHash (Join-Path $ownedOutput 'runtime-host.json')).Hash.ToLowerInvariant();processes_sha256=(Get-FileHash (Join-Path $ownedOutput 'processes.json')).Hash.ToLowerInvariant();mean=$mean;diagnostics=$diagnostics;investigation_passed=$diagnostics.passed;calculations_finished=$true;completed=$true;reference_family_qualified=$false;release_accepted=$false}
if($Mode-ceq'Read'){
 $stored=Read-PermutationJson (Join-Path $ownedOutput 'result.json')
 if((Canonical-Depth $stored)-cne(Canonical-Depth $result)){throw 'Stored means/report differ from independently recomputed diagnostic.'}
 return $result
}
Write-DepthJson 'result.json' $result
if(!$diagnostics.passed){throw 'Fixed per-stream diagnostic comparisons failed; complete raw/report evidence retained.'}
return $result
