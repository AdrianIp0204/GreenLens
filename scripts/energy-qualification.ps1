param(
 [Parameter(Mandatory,ParameterSetName='Production')][string]$CliPath,
 [Parameter(ParameterSetName='Production')][ValidateSet(2,3)][int]$ExampleRevision=3,
 [Parameter(Mandatory,ParameterSetName='Direct')][string]$ProductionRoot,
 [Parameter(Mandatory,ParameterSetName='Control')][string]$ControlInputs,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [string]$EngineRoot=(Join-Path $env:LOCALAPPDATA 'GreenLens/engines/energyplus-26.2.0/package/EnergyPlus-26.2.0-4bd7a1f26f-Windows-x86_64')
)
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $OutputDirectory){throw 'Qualification output must be a fresh directory'}
if($PSCmdlet.ParameterSetName -eq 'Control'){
 $controlRoot=$OutputDirectory
 $protocolPath=Join-Path $PSScriptRoot 'energy-control-protocol.json'
 $protocolHash='062737b8ff28874bd453095a19006edd0be2e427d37dfcd7e3efd8fb53bd2ec4'
 if((Get-FileHash -LiteralPath $protocolPath).Hash.ToLowerInvariant() -cne $protocolHash -or (Get-FileHash -LiteralPath (Join-Path $ControlInputs 'proposal.json')).Hash.ToLowerInvariant() -cne $protocolHash){throw 'Fixed control proposal differs'}
 $plan=Get-Content -LiteralPath $protocolPath -Raw|ConvertFrom-Json -AsHashtable
 if(($plan.cases.case -join ',') -cne 'original6,original12,zero6,zero12'){throw 'Fixed four-case order differs'}
 New-Item -ItemType Directory -Path $controlRoot|Out-Null
 Copy-Item -LiteralPath $protocolPath -Destination (Join-Path $controlRoot 'proposal.json')
 $sourcePaths=@($PSCommandPath,$protocolPath,(Join-Path $PSScriptRoot 'energy-gate0-run.ps1'),(Join-Path $PSScriptRoot 'energy-gate0-fixture.py'),(Join-Path $PSScriptRoot 'energy-gate0-audit.py'),(Join-Path $PSScriptRoot 'annual-paired-process.ps1'),(Join-Path $PSScriptRoot 'annual-paired-native-process.cs'),(Join-Path $PSScriptRoot '../dependencies/energyplus.json'))
 $sourcePins=[ordered]@{}
 foreach($path in $sourcePaths){$sourcePins[[IO.Path]::GetFileName($path)]=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()}
 foreach($case in $plan.cases){
  $caseRoot=Join-Path $controlRoot $case.case;New-Item -ItemType Directory -Path $caseRoot|Out-Null
  foreach($name in $case.input_sha256.Keys){
   Copy-Item -LiteralPath (Join-Path $ControlInputs ($case.case+'/'+$name)) -Destination (Join-Path $caseRoot $name)
   if((Get-FileHash -LiteralPath (Join-Path $caseRoot $name)).Hash.ToLowerInvariant() -cne $case.input_sha256[$name]){throw 'Copied control input differs'}
  }
 }
 $start=[DateTime]::UtcNow;$watch=[Diagnostics.Stopwatch]::StartNew()
 $declaration=[ordered]@{schema='greenlens.energy-fixed-control-execution.v1';declared_utc=$start.ToString('o');deadline_utc=$start.AddSeconds($plan.limits.global_seconds).ToString('o');proposal_sha256=$protocolHash;source_sha256=$sourcePins;workers=1;native_processes=4;qualification_asserted=$false;child_environment=@{OMP_NUM_THREADS='1';removed=@('PYTHONHOME','PYTHONPATH','ENERGYPLUS_WEATHER','ENERGYPLUS_OUTPUT')}}
 $declaration|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $controlRoot 'declaration.json') -Encoding utf8
 $env:OMP_NUM_THREADS='1';foreach($name in $declaration.child_environment.removed){[Environment]::SetEnvironmentVariable($name,$null,'Process')}
 function AssertControl {
  foreach($path in $sourcePaths){if((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $sourcePins[[IO.Path]::GetFileName($path)]){throw 'Control source changed'}}
  $files=@(Get-ChildItem -LiteralPath $controlRoot -Recurse -File)
  if(($files|Measure-Object Length -Sum).Sum -gt $plan.limits.scratch_bytes -or @($files|Where-Object Length -gt $plan.limits.file_bytes).Count){throw 'Complete experiment exceeds resource limits'}
  $remaining=[Math]::Floor($plan.limits.global_seconds-$watch.Elapsed.TotalSeconds)
  if($remaining -lt 1){throw 'Fixed global deadline exhausted'}
  return [int][Math]::Min($plan.limits.process_seconds,$remaining)
 }
 try{
  foreach($case in $plan.cases){
   $remaining=AssertControl;$caseRoot=Join-Path $controlRoot $case.case
   $fixture=[ordered]@{schema='greenlens.energy-control-fixture.v1';case=$case.case;expected_intervals=$case.expected_intervals;proposal_sha256=$protocolHash;input_sha256=$case.input_sha256;native_limits=@{timeout_seconds=$remaining;memory_bytes=$plan.limits.memory_bytes;scratch_bytes=$plan.limits.scratch_bytes;file_bytes=$plan.limits.file_bytes}}
   $fixture|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $caseRoot 'fixture-declaration.json') -Encoding utf8
   & (Join-Path $PSScriptRoot 'energy-gate0-run.ps1') -FixtureDirectory $caseRoot -EngineRoot $EngineRoot -Execute
   $null=AssertControl
   Write-Host "Fixed control case completed: $($case.case)"
  }
  $null=AssertControl
  [ordered]@{schema='greenlens.energy-fixed-control-completion.v1';completed=$true;finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$watch.Elapsed.TotalSeconds;qualification_asserted=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $controlRoot 'execution.json') -Encoding utf8
 }catch{
  [ordered]@{schema='greenlens.energy-fixed-control-completion.v1';completed=$false;finished_utc=[DateTime]::UtcNow.ToString('o');message=$_.Exception.Message;qualification_asserted=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $controlRoot 'failure.json') -Encoding utf8
  throw
 }
 return
}
if($PSCmdlet.ParameterSetName -eq 'Direct'){
 $parityRoot=$OutputDirectory
 New-Item -ItemType Directory -Path $parityRoot|Out-Null
 $dependencyPath=Join-Path $PSScriptRoot '../dependencies/energyplus.json'
 $dependency=Get-Content -LiteralPath $dependencyPath -Raw|ConvertFrom-Json -AsHashtable
 function CheckEngine {
  foreach($name in $dependency.critical_files.Keys){$file=Join-Path $EngineRoot $name;if((Get-Item -LiteralPath $file).Length -ne $dependency.critical_files[$name].bytes -or (Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant() -cne $dependency.critical_files[$name].sha256){throw "Engine differs: $name"}}
 }
 CheckEngine
 $index=Get-Content -LiteralPath (Join-Path $ProductionRoot 'run-index.json') -Raw|ConvertFrom-Json -AsHashtable
 if(($index.case -join ',') -cne 'A10,B10,A5,B5'){throw 'Require original fixed four-run index'}
 $sourcePins=[ordered]@{}
 $sourcePaths=@($PSCommandPath,(Join-Path $PSScriptRoot 'annual-paired-process.ps1'),(Join-Path $PSScriptRoot 'annual-paired-native-process.cs'),$dependencyPath)
 foreach($path in $sourcePaths){$sourcePins[[IO.Path]::GetFileName($path)]=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()}
 $prepared=[Collections.Generic.List[object]]::new()
 foreach($item in $index[0..1]){
  if($item.minutes -ne 10 -or $item.project -cne 'project-10' -or $item.case -cne ($item.alternative+'10') -or $item.id -cnotmatch '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'){throw 'Only fixed original ten-minute cases with canonical run IDs'}
  $run=Join-Path $ProductionRoot ($item.project+'/runs/'+$item.id)
  $seals=(Get-Content -LiteralPath (Join-Path $run 'checksums.json') -Raw|ConvertFrom-Json -AsHashtable).sha256
  foreach($name in $seals.Keys){if($name -match '(^/|\\|:|(^|/)\.\.?(/|$))' -or (Get-FileHash -LiteralPath (Join-Path $run $name)).Hash.ToLowerInvariant() -cne $seals[$name]){throw "Original member differs or unsafe: $name"}}
  $process=Get-Content -LiteralPath (Join-Path $run 'engine/process.json') -Raw|ConvertFrom-Json -AsHashtable
  $result=Get-Content -LiteralPath (Join-Path $run 'results.json') -Raw|ConvertFrom-Json -AsHashtable
  if($result.id -cne $item.id -or $result.alternative_id -cne $item.alternative -or $result.study.zone_timesteps_per_hour -ne 6){throw 'Original run identity differs from selected fixed case'}
  $arguments=@('--jobs','1','--weather','weather.epw','--output-directory','.','--output-prefix','eplus','../model.epJSON')
  if($process.state -cne 'completed' -or $process.exit_code -ne 0 -or !$process.process_tree_empty -or $null -ne $process.failure -or $process.arguments.Count -ne $arguments.Count){throw 'Original native record did not complete with owned arguments'}
  for($i=0;$i -lt $arguments.Count;$i++){if($process.arguments[$i] -cne $arguments[$i]){throw 'Original argument token differs'}}
  $caseRoot=Join-Path $parityRoot $item.case;New-Item -ItemType Directory -Path (Join-Path $caseRoot 'engine')|Out-Null
  Copy-Item -LiteralPath (Join-Path $run 'model.epJSON') -Destination (Join-Path $caseRoot 'model.epJSON')
  Copy-Item -LiteralPath (Join-Path $run 'weather/normalized.epw') -Destination (Join-Path $caseRoot 'engine/weather.epw')
  $inputs=[ordered]@{'model.epJSON'=$seals['model.epJSON'];'engine/weather.epw'=$seals['weather/normalized.epw']}
  foreach($name in $inputs.Keys){if((Get-FileHash -LiteralPath (Join-Path $caseRoot $name)).Hash.ToLowerInvariant() -cne $inputs[$name]){throw 'Copied input differs'}}
  $prepared.Add([ordered]@{case=$item.case;original_run_id=$item.id;original_checksums_sha256=(Get-FileHash -LiteralPath (Join-Path $run 'checksums.json')).Hash.ToLowerInvariant();original_process_sha256=$seals['engine/process.json'];input_sha256=$inputs;arguments=$arguments})
 }
 $plan=[ordered]@{schema='greenlens.energy-direct-native-parity.v1';declared_utc=[DateTime]::UtcNow.ToString('o');source_sha256=$sourcePins;cases=@($prepared.ToArray());native_processes=2;workers=1;limits=@{seconds=1800;memory_bytes=2147483648;scratch_bytes=1073741824;file_bytes=134217728};criteria='Exact labels/units/counts/times; all native values and derived energy/demand within max(1e-6 reported unit,0.1% reference). Independent integration only; preserved timestep failure is unchanged.';child_environment=@{OMP_NUM_THREADS='1';removed=@('PYTHONHOME','PYTHONPATH','ENERGYPLUS_WEATHER','ENERGYPLUS_OUTPUT');legacy_supervisor_addition='RAYPATH is irrelevant to EnergyPlus and contains only owned working directory and engine lib.'}}
 $plan|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $parityRoot 'declaration.json') -Encoding utf8
 $env:OMP_NUM_THREADS='1'
 foreach($name in $plan.child_environment.removed){[Environment]::SetEnvironmentVariable($name,$null,'Process')}
 . (Join-Path $PSScriptRoot 'annual-paired-process.ps1')
 try{
  foreach($case in $prepared){
   foreach($path in $sourcePaths){if((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $sourcePins[[IO.Path]::GetFileName($path)]){throw 'Parity harness changed'}}
   CheckEngine
   $caseRoot=Join-Path $parityRoot $case.case;$OutputDirectory=Join-Path $caseRoot 'engine';$records=[Collections.Generic.List[object]]::new()
   Invoke-PairedProcess -Executable (Join-Path $EngineRoot 'energyplus.exe') -Name energyplus -Arguments $case.arguments -Output 'stdout.txt' -TimeoutSeconds 1800 -MemoryLimitBytes 2147483648 -ScratchLimitBytes 1073741824 -FileLimitBytes 134217728
   foreach($name in $case.input_sha256.Keys){if((Get-FileHash -LiteralPath (Join-Path $caseRoot $name)).Hash.ToLowerInvariant() -cne $case.input_sha256[$name]){throw 'Direct input changed'}}
   $files=@(Get-ChildItem -LiteralPath $caseRoot -File -Recurse);if(($files|Measure-Object Length -Sum).Sum -gt 1073741824 -or @($files|Where-Object Length -gt 134217728).Count){throw 'Complete case exceeds fixed file/scratch caps'}
   CheckEngine
  }
  foreach($path in $sourcePaths){if((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $sourcePins[[IO.Path]::GetFileName($path)]){throw 'Parity harness changed'}}
  [ordered]@{schema='greenlens.energy-direct-native-execution.v1';completed=$true;finished_utc=[DateTime]::UtcNow.ToString('o');qualification_asserted=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $parityRoot 'execution.json') -Encoding utf8
 }catch{
  [ordered]@{schema='greenlens.energy-direct-native-execution.v1';completed=$false;message=$_.Exception.Message;qualification_asserted=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $parityRoot 'failure.json') -Encoding utf8
  throw
 }
 return
}
New-Item -ItemType Directory -Path (Join-Path $OutputDirectory 'tools') -Force|Out-Null
$protocolNames=@{2='energy-zero-cutout-protocol.json';3='energy-fan-example-protocol.json'}
$protocolHashes=@{2='64e3d780fee5ae5441c5150d864c5f4638c238591ed3988772fa55ff35bc3f40';3='6c2d6defc48553d0380ee47a8e107eacab6d9853e7d690b01ea4b0be249af781'}
$annualProtocolPath=Join-Path $PSScriptRoot $protocolNames[$ExampleRevision]
$historicalPath=Join-Path $PSScriptRoot '../examples/energy/revision1-inputs.json'
if((Get-FileHash -LiteralPath $annualProtocolPath).Hash.ToLowerInvariant() -cne $protocolHashes[$ExampleRevision]){throw 'Revised annual protocol differs'}
$annualProtocol=Get-Content -LiteralPath $annualProtocolPath -Raw|ConvertFrom-Json -AsHashtable
if((Get-FileHash -LiteralPath $historicalPath).Hash.ToLowerInvariant() -cne $annualProtocol.historical_inputs.sha256){throw 'Historical model snapshots differ'}
$historical=Get-Content -LiteralPath $historicalPath -Raw|ConvertFrom-Json -AsHashtable
function Assert-EnergySameValue($Actual,$Expected,[string]$Name){
 if($null -eq $Expected){if($null -ne $Actual){throw "Input differs: $Name"};return}
 if($null -eq $Actual){throw "Missing input: $Name"}
 if($Expected -is [Collections.IDictionary]){
  if($Actual -isnot [Collections.IDictionary] -or $Actual.Count -ne $Expected.Count){throw "Object shape differs: $Name"}
  foreach($key in $Expected.Keys){if(!$Actual.Contains($key)){throw "Missing input key: $Name/$key"};Assert-EnergySameValue $Actual[$key] $Expected[$key] "$Name/$key"}
 }elseif($Expected -is [Collections.IList]){
  if($Actual -isnot [Collections.IList] -or $Actual.Count -ne $Expected.Count){throw "Array shape differs: $Name"}
  for($i=0;$i -lt $Expected.Count;$i++){Assert-EnergySameValue $Actual[$i] $Expected[$i] "$Name/$i"}
 }elseif($Expected -is [bool]){
  if($Actual -isnot [bool] -or $Actual -ne $Expected){throw "Boolean differs: $Name"}
 }elseif($Expected -is [string]){
  if($Actual -isnot [string] -or $Actual -cne $Expected){throw "String differs: $Name"}
 }else{
  if($Actual -is [bool] -or $Actual -is [string] -or $Actual -isnot [ValueType] -or ![double]::IsFinite([double]$Actual) -or [double]$Actual -ne [double]$Expected){throw "Number differs: $Name"}
 }
}
function Get-EnergyExpectedInput($Historical,[string]$Alternative,[int]$Minutes,[int]$Revision){
 if($Alternative -cnotin @('A','B') -or $Minutes -notin @(5,10) -or $Revision -notin @(2,3)){throw 'Undeclared example case'}
 $expected=$Historical.cases[$Alternative]|ConvertTo-Json -Depth 100|ConvertFrom-Json -AsHashtable
 if($expected.model.zones.Count -ne 4){throw 'Require four original zones'}
 foreach($zone in $expected.model.zones){
  if($zone.cooling_system.cooling_deadband_k -ne .2 -or $zone.cooling_system.supply_fan.total_efficiency -ne .5){throw 'Historical cutout or fan efficiency differs'}
  $zone.cooling_system.cooling_deadband_k=0.0
  if($Revision -eq 3 -and $Alternative -ceq 'B'){$zone.cooling_system.supply_fan.total_efficiency=.6}
 }
 $expected.study.zone_timesteps_per_hour=60/$Minutes
 return $expected
}
$cli=Join-Path $OutputDirectory 'tools/greenlens.exe'
Copy-Item -LiteralPath $CliPath -Destination $cli
$cliHash=(Get-FileHash -LiteralPath $cli).Hash.ToLowerInvariant()
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$sources=[ordered]@{}
foreach($path in @($PSCommandPath,$annualProtocolPath,$historicalPath,(Join-Path $PSScriptRoot 'energy-interzone-protocol.json'))+@(Get-ChildItem (Join-Path $repo 'crates/greenlens-energyplus/src') -File).FullName){
 $sources[[IO.Path]::GetRelativePath($repo,$path).Replace('\','/')]=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()
}
$plan=[ordered]@{schema='greenlens.energy-production-qualification.v1';declared_utc=[DateTime]::UtcNow.ToString('o');cli_sha256=$cliHash;source_sha256=$sources;example_revision=$ExampleRevision;protocol_sha256=(Get-FileHash -LiteralPath $annualProtocolPath).Hash.ToLowerInvariant();
 stages=@('A10','B10','A5','B5');native_processes=4;workers=1;native_limits=@{seconds=1800;memory_bytes=2147483648;scratch_bytes=1073741824;file_bytes=134217728};
 criteria=@{annual_facility_relative_max=.01;aligned_ten_minute_peak_relative_max=.03;occupied_unmet_percent_max=1;comfort_tolerance_c=.2;no_threshold_tuning=$true};
 scope="Explicit example revision $ExampleRevision; only the selected pinned protocol changes. Synthetic non-climatic weather. All four numerical outcomes retained; failed scientific checks do not trigger retries. Process/input failure stops schedule."}
$plan|ConvertTo-Json -Depth 20|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
$records=[Collections.Generic.List[object]]::new()
function Invoke-Cli([string]$Name,[string[]]$Argv){
 if((Get-FileHash -LiteralPath $cli).Hash.ToLowerInvariant() -cne $cliHash){throw 'Frozen qualification CLI changed'}
 $start=[DateTime]::UtcNow; $watch=[Diagnostics.Stopwatch]::StartNew()
 & $cli @Argv 1> (Join-Path $OutputDirectory "$Name.json") 2> (Join-Path $OutputDirectory "$Name.stderr.txt")
 $code=$LASTEXITCODE; $watch.Stop()
 $records.Add([ordered]@{name=$Name;arguments=$Argv;started_utc=$start.ToString('o');finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$watch.Elapsed.TotalSeconds;exit_code=$code})
 ConvertTo-Json -InputObject @($records.ToArray()) -Depth 20|Set-Content -LiteralPath (Join-Path $OutputDirectory 'commands.json') -Encoding utf8
 if($code -ne 0){throw "Qualification CLI $Name failed: $code; retained stderr and original job are authoritative"}
}
try{
 $runs=[Collections.Generic.List[object]]::new()
 foreach($minutes in @(10,5)){
  $project=Join-Path $OutputDirectory "project-$minutes"
  Invoke-Cli "example-$minutes" @('energy','example',$project)
  foreach($alternative in @('A','B')){
   $expected=Get-EnergyExpectedInput $historical $alternative $minutes $ExampleRevision
   $doc=Get-Content -LiteralPath (Join-Path $project 'project.json') -Raw|ConvertFrom-Json -AsHashtable
   $design=$doc.energy.designs[$alternative]
   foreach($zone in $design.model.zones){
    $zone.cooling_system.cooling_deadband_k=0.0
    $zone.cooling_system.supply_fan.total_efficiency=if($ExampleRevision -eq 3 -and $alternative -ceq 'B'){.6}else{.5}
   }
   $design.study.zone_timesteps_per_hour=60/$minutes
   Assert-EnergySameValue $design.model $expected.model 'configured-model'
   Assert-EnergySameValue $design.study $expected.study 'configured-study'
   $model=Join-Path $OutputDirectory "model-$alternative-$minutes.json"; $study=Join-Path $OutputDirectory "study-$alternative-$minutes.json"
   $design.model|ConvertTo-Json -Depth 100|Set-Content -LiteralPath $model -Encoding utf8
   $design.study|ConvertTo-Json -Depth 100|Set-Content -LiteralPath $study -Encoding utf8
   Invoke-Cli "configure-$alternative-$minutes" @('energy','configure',$project,$model,$study,'--alternative',$alternative)
   $name="$alternative$minutes"
   foreach($path in $sources.Keys){if((Get-FileHash -LiteralPath (Join-Path $repo $path)).Hash.ToLowerInvariant() -cne $sources[$path]){throw 'Frozen annual source changed'}}
   $doc=Get-Content -LiteralPath (Join-Path $project 'project.json') -Raw|ConvertFrom-Json -AsHashtable
   Assert-EnergySameValue $doc.energy.designs[$alternative].model $expected.model 'model'
   Assert-EnergySameValue $doc.energy.designs[$alternative].study $expected.study 'study'
   [ordered]@{schema="greenlens.energy-revision$ExampleRevision-input-preflight.v1";case=$name;example_revision=$ExampleRevision;protocol_sha256=$plan.protocol_sha256;historical_inputs_sha256=$annualProtocol.historical_inputs.sha256;project_sha256=(Get-FileHash -LiteralPath (Join-Path $project 'project.json')).Hash.ToLowerInvariant();only_declared_changes=$true}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $OutputDirectory "input-preflight-$name.json") -Encoding utf8
   Invoke-Cli "run-$name" @('energy','run',$project,'--alternative',$alternative,'--engine-dir',$EngineRoot)
   $doc=Get-Content -LiteralPath (Join-Path $project 'project.json') -Raw|ConvertFrom-Json -AsHashtable
   $run=@($doc.energy.runs|Where-Object alternative_id -eq $alternative)
   if($run.Count -ne 1){throw 'Expected exactly one retained run per alternative'}
   Invoke-Cli "inspect-$name" @('energy','inspect',$project,$run[0].id)
   $retained=Get-Content -LiteralPath (Join-Path $project ('runs/'+$run[0].id+'/results.json')) -Raw|ConvertFrom-Json -AsHashtable
   Assert-EnergySameValue $retained.model $expected.model 'retained-model'
   Assert-EnergySameValue $retained.study $expected.study 'retained-study'
   $protocolCase=@($annualProtocol.cases|Where-Object case -ceq $name)
   if($protocolCase.Count -ne 1){throw 'Annual case not declared'}
   Assert-EnergySameValue $retained.method $protocolCase[0].expected_method 'method'
   $runs.Add([ordered]@{case=$name;minutes=$minutes;alternative=$alternative;project="project-$minutes";id=$run[0].id})
   ConvertTo-Json -InputObject @($runs.ToArray()) -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'run-index.json') -Encoding utf8
   Write-Host "$name completed: $($run[0].id)"
  }
 }
 foreach($path in $sources.Keys){if((Get-FileHash -LiteralPath (Join-Path $repo $path)).Hash.ToLowerInvariant() -cne $sources[$path]){throw 'Frozen annual source changed at finalization'}}
 [ordered]@{schema='greenlens.energy-production-execution.v1';completed=$true;finished_utc=[DateTime]::UtcNow.ToString('o');cli_sha256=$cliHash;runs=@($runs.ToArray());qualification_asserted=$false}|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'execution.json') -Encoding utf8
}catch{
 [ordered]@{schema='greenlens.energy-production-execution.v1';completed=$false;finished_utc=[DateTime]::UtcNow.ToString('o');message=$_.Exception.Message;qualification_asserted=$false}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'execution-failure.json') -Encoding utf8
 throw
}
