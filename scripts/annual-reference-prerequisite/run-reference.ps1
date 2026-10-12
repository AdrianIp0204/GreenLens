#Requires -Version 7.2
# Original GreenLens research-only prerequisite; no application Python dependency.
param(
 [Parameter(Mandatory)][string]$RepositoryRoot,
 [Parameter(Mandatory)][string]$BaselineA,
 [Parameter(Mandatory)][string]$BaselineB,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [Parameter(Mandatory)][string]$Python,
 [string]$EngineRoot=(Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'),
 [switch]$Execute
)
$ErrorActionPreference='Stop'
$targetRoot=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $targetRoot){throw 'A fresh evidence directory is required.'}
$RepositoryRoot=[IO.Path]::GetFullPath($RepositoryRoot)
$Python=[IO.Path]::GetFullPath($Python)
$EngineRoot=[IO.Path]::GetFullPath($EngineRoot)
$cachePrefix=Join-Path $targetRoot 'absent-python-bytecode-cache'
$planFile=if(Test-Path -LiteralPath "$PSScriptRoot/proposal.public.json"){"$PSScriptRoot/proposal.public.json"}else{"$PSScriptRoot/proposal.json"}
function HashFile([string]$Path){return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}
function SaveJson([string]$Path,$Value){[IO.File]::WriteAllText($Path,(ConvertTo-Json -InputObject $Value -Depth 100),[Text.UTF8Encoding]::new($false))}
if((HashFile $planFile) -cne 'a74e9e574cae8ad12806c8b0caadb22a3b7dd99eb30c7af916147517d1e3c6bf'){throw 'Unreviewed scientific proposal.'}
$plan=Get-Content -LiteralPath $planFile -Raw|ConvertFrom-Json -AsHashtable
$pinsFile=Join-Path $PSScriptRoot 'runtime-pins.json'
if(!(Test-Path -LiteralPath $pinsFile)){throw 'Freeze reviewed runtime-pins.json before PrepareOnly or Execute.'}
$pins=Get-Content -LiteralPath $pinsFile -Raw|ConvertFrom-Json -AsHashtable
function AssertSources {
 foreach($pair in $pins.draft.GetEnumerator()){if((HashFile (Join-Path $PSScriptRoot $pair.Key)) -cne $pair.Value){throw "Draft source drift: $($pair.Key)"}}
 foreach($pair in $pins.repository.GetEnumerator()){if((HashFile (Join-Path $RepositoryRoot $pair.Key)) -cne $pair.Value){throw "Trusted source drift: $($pair.Key)"}}
}
AssertSources
# Build a trusted source snapshot. Only the historical protocol DOCUMENT is
# restored to its exact original bytes; executable helpers are current pinned
# checkout code. Never execute retained package source or rewrite old evidence.
New-Item -ItemType Directory -Path $targetRoot | Out-Null
$readerRoot=Join-Path $targetRoot 'trusted-baseline-reader'
$readerHashes=@{}
foreach($pair in $pins.repository.GetEnumerator()){
 $dest=Join-Path $readerRoot $pair.Key
 New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($dest)) -Force|Out-Null
 $source=if($pair.Key -ceq 'docs/v0.3-reference-protocol.md'){Join-Path $PSScriptRoot 'legacy-reference-protocol.md'}else{Join-Path $RepositoryRoot $pair.Key}
 $expectedHash=if($pair.Key -ceq 'docs/v0.3-reference-protocol.md'){'4641f6c2991396beb934d80eaec40e84efecfaea557de92a5543be1fbb1d71b7'}else{$pair.Value}
 Copy-Item -LiteralPath $source -Destination $dest
 if((HashFile $dest) -cne $expectedHash){throw 'Trusted baseline reader snapshot differs'}
 $readerHashes[$pair.Key]=$expectedHash
}
SaveJson (Join-Path $readerRoot 'source-manifest.json') @{schema='greenlens.trusted-baseline-reader-snapshot.v1';source_sha256=$readerHashes;historical_document_git_commit='8e7fe4bc95c07856f89ba008fdd4d6cc751b1615';document_only_restore=$true}
. (Join-Path $readerRoot 'scripts/annual-collect-reference.ps1') -FunctionsOnly
Initialize-ReferenceCollector
$fixtureRoots=@{A=$BaselineA;B=$BaselineB};$caseRoots=@{};$finiteHashes=@{};$inputHashes=@{};$strictHashes=@{}
foreach($case in $plan.cases){
 $key="$($case.alternative)$($case.interval_index)";$original=$fixtureRoots[$case.alternative]
 foreach($m in $case.members.GetEnumerator()){if((HashFile (Join-Path $original $m.Key)) -cne $m.Value.sha256 -or (Get-Item -LiteralPath (Join-Path $original $m.Key)).Length -ne $m.Value.bytes){throw "Original baseline binding differs: $key/$($m.Key)"}}
 $read=Read-ReferenceBaseline $original
 $matches=@($read.contexts|Where-Object{$_.key -ceq "$($case.alternative):$($case.interval_index)"})
 if($matches.Count -ne 1 -or !$matches[0].record.qualified){throw 'Required retained baseline case is not freshly repeat-qualified.'}
 $ctx=$matches[0];$folder=Join-Path $targetRoot $key;New-Item -ItemType Directory -Path $folder|Out-Null;$caseRoots[$key]=$folder
 $inputHashes[$key]=@{}
 foreach($m in $case.members.GetEnumerator()){
  $ownedName=if($m.Key -ceq 'processes.json'){'baseline-processes.json'}else{$m.Key}
  $dest=Join-Path $folder $ownedName;Copy-Item -LiteralPath (Join-Path $original $m.Key) -Destination $dest
  if((HashFile $dest) -cne $m.Value.sha256){throw 'Copied original bytes changed'};$inputHashes[$key][$ownedName]=$m.Value.sha256
 }
 foreach($m in $case.derived_members.GetEnumerator()){
  $dest=Join-Path $folder $m.Key;Copy-Item -LiteralPath (Join-Path $PSScriptRoot "derived-inputs/$key/$($m.Key)") -Destination $dest
  if((HashFile $dest) -cne $m.Value.sha256){throw 'Derived fixture binding differs'};$inputHashes[$key][$m.Key]=$m.Value.sha256
 }
 SaveJson (Join-Path $folder 'finite-original.json') @($ctx.finite)
 $finiteHashes[$key]=HashFile (Join-Path $folder 'finite-original.json')
 SaveJson (Join-Path $folder 'fresh-baseline-reader.json') $read
 $strictHashes[$key]=HashFile (Join-Path $folder 'fresh-baseline-reader.json')
 foreach($pair in $case.engine_sha256.GetEnumerator()){
  $where=if($pair.Key.EndsWith('.exe')){'bin'}else{'lib'}
  if((HashFile (Join-Path $EngineRoot "$where/$($pair.Key)")) -cne $pair.Value){throw 'Pinned native executable/library differs'}
 }
}
# Pure generation only: fixed complete-domain receipts, no persisted huge rays.
Add-Type -LiteralPath (Join-Path $PSScriptRoot 'reference_primary.cs')
$sensorText=[IO.File]::ReadAllText((Join-Path $caseRoots.A4116 'sensors.pts'))
if((HashFile (Join-Path $caseRoots.A4116 'sensors.pts')) -cne (HashFile (Join-Path $caseRoots.B8512 'sensors.pts'))){throw 'Sensor order differs between cases'}
$receipts=@{}
foreach($n in @(16384,32768,65536)){
 $producer=[GreenLensReferenceStream.HammersleyProducer]::new($sensorText,$n)
 $r=$producer.GenerateAsync([IO.Stream]::Null,[Threading.CancellationToken]::None).GetAwaiter().GetResult()
 if($r.Rays -ne 48L*$n){throw 'Expected full two-block ray count'};$receipts["$n"]=$r
}
$pythonVersion=(& $Python -B --version 2>&1|Out-String).Trim();if($LASTEXITCODE -ne 0){throw 'Research Python unavailable'}
$runtime=Join-Path $targetRoot 'runtime';New-Item -ItemType Directory -Path $runtime|Out-Null
foreach($name in $pins.draft.Keys){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $runtime $name)}
Copy-Item -LiteralPath $planFile -Destination (Join-Path $targetRoot 'proposal.json')
Copy-Item -LiteralPath $pinsFile -Destination (Join-Path $targetRoot 'runtime-pins.json')
$declaration=[ordered]@{schema='greenlens.external-primary-reference-execution.v1';declared_utc=[DateTime]::UtcNow.ToString('o');proposal_sha256=(HashFile $planFile);runtime_pins_sha256=(HashFile $pinsFile);primary_receipts=$receipts;input_sha256=$inputHashes;finite_sha256=$finiteHashes;fresh_baseline_reader_sha256=$strictHashes;trusted_reader_source_sha256=$readerHashes;python=@{sha256=(HashFile $Python);version=$pythonVersion;no_bytecode=$true;fresh_absent_cache_prefix=$cachePrefix};powershell=$PSVersionTable.PSVersion.ToString();execution_requested=[bool]$Execute;experiment_seconds=43200;native_stage_cap=42;postprocessing='Same atomic Job supervisor; one archive process per completed rcontrib (34 maximum), one R1 and four R2 collectors (5 maximum). Both count toward global12h. No Radiance in postprocessing.';reference_family_qualified=$false;accuracy_accepted=$false}
SaveJson (Join-Path $targetRoot 'execution-declaration.json') $declaration
if(!$Execute){Write-Host "Prepared exact42-stage declaration only: $targetRoot";return}
. (Join-Path $RepositoryRoot 'scripts/annual-paired-process.ps1')
$clock=[Diagnostics.Stopwatch]::StartNew();$nativeRecords=@{};$postRecords=@{};$completedNative=0;$gateFiles=[Collections.Generic.List[string]]::new()
foreach($key in $caseRoots.Keys){$nativeRecords[$key]=[Collections.Generic.List[object]]::new();$postRecords[$key]=[Collections.Generic.List[object]]::new()}
$rootPost=[Collections.Generic.List[object]]::new()
function Remaining { $v=43200-$clock.Elapsed.TotalSeconds;if($v -le 0){throw 'Experiment12h deadline exceeded'};return [Math]::Min(7200.,$v) }
function AssertInputs([string]$Key){
 AssertSources
 foreach($m in $readerHashes.GetEnumerator()){if((HashFile (Join-Path $readerRoot $m.Key)) -cne $m.Value){throw 'Trusted reader snapshot drift'}}
 if((HashFile $Python) -cne $declaration.python.sha256){throw 'Python executable drift'}
 if(Test-Path -LiteralPath $cachePrefix){throw 'Python bytecode prefix must remain absent'}
 foreach($m in $inputHashes[$Key].GetEnumerator()){if((HashFile (Join-Path $caseRoots[$Key] $m.Key)) -cne $m.Value){throw 'Owned physical input drift'}}
 if((HashFile (Join-Path $caseRoots[$Key] 'finite-original.json')) -cne $finiteHashes[$Key]){throw 'Finite reuse drift'}
 $case=@($plan.cases|Where-Object{"$($_.alternative)$($_.interval_index)" -ceq $Key})[0]
 foreach($m in $case.engine_sha256.GetEnumerator()){$where=if($m.Key.EndsWith('.exe')){'bin'}else{'lib'};if((HashFile (Join-Path $EngineRoot "$where/$($m.Key)")) -cne $m.Value){throw 'Native executable/library drift'}}
}
function InvokeGate([string]$Phase,[string]$Variant='base'){
 $OutputDirectory=$targetRoot;$records=$rootPost;$name="$Phase-$Variant-checks.json"
 $seconds=Remaining
 Invoke-PairedProcess -Executable $Python -Name 'research-python' -Arguments @('-B','-X',"pycache_prefix=$cachePrefix",(Join-Path $PSScriptRoot 'reference_report.py'),'--root',$targetRoot,'--phase',$Phase,'--variant',$Variant,'--seconds',([Math]::Max(0.001,$seconds-1)).ToString('R',[Globalization.CultureInfo]::InvariantCulture)) -Output $name -TimeoutSeconds $seconds
 if(Test-Path -LiteralPath $cachePrefix){throw 'Python bytecode prefix unexpectedly exists'}
 Copy-Item -LiteralPath (Join-Path $targetRoot 'processes.json') -Destination (Join-Path $targetRoot 'postprocesses.json')
 $gate=Get-Content -LiteralPath (Join-Path $targetRoot $name) -Raw|ConvertFrom-Json
 $gateFiles.Add($name)
 if(!$gate.completed -or !$gate.all_declared_checks_passed){throw "Declared numerical gate failed: $Phase/$Variant"}
}
try{
 for($i=0;$i -lt $plan.stages.Count;$i++){
  $stage=$plan.stages[$i];$key=$stage.case;$OutputDirectory=$caseRoots[$key]
  AssertInputs $key
  $size=0L;foreach($f in [IO.Directory]::EnumerateFiles($OutputDirectory)){$size+=[GreenLensPairedProcess.NativeProcess]::FileSize($f)}
  if($size+1073741824 -gt 2147483648){throw 'Insufficient fixed raw-plus-gzip peak reserve; resource branch stopped'}
  $records=$nativeRecords[$key];$name=[IO.Path]::GetFileNameWithoutExtension($stage.executable)
  $nativeArgs=@{Executable=(Join-Path $EngineRoot "bin/$($stage.executable)");Name=$name;Arguments=[string[]]$stage.argv;Output=$stage.output;TimeoutSeconds=(Remaining)}
  if($name -eq 'rcontrib'){$nativeArgs.InputProducer=[GreenLensReferenceStream.HammersleyProducer]::new($sensorText,[int]$stage.primary_count_per_sensor_per_block)}
  $actual=Invoke-PairedProcess @nativeArgs
  Copy-Item -LiteralPath (Join-Path $OutputDirectory 'processes.json') -Destination (Join-Path $OutputDirectory 'native-processes.json')
  $completedNative++
  if($name -eq 'rcontrib'){
   $expected=$receipts["$($stage.primary_count_per_sensor_per_block)"]
   if($actual.Sha256 -cne $expected.Sha256 -or $actual.Bytes -ne $expected.Bytes -or $actual.Rays -ne $expected.Rays){throw 'Native primary stream differs from frozen complete blocks'}
   $records=$postRecords[$key];$seconds=Remaining
   Invoke-PairedProcess -Executable $Python -Name 'research-python' -Arguments @('-B','-X',"pycache_prefix=$cachePrefix",(Join-Path $PSScriptRoot 'reference_stream.py'),'archive','--root',$OutputDirectory,'--name',$stage.output,'--N',"$($stage.primary_count_per_sensor_per_block)",'--scale',"$($stage.source_scale)",'--seconds',([Math]::Max(0.001,$seconds-1)).ToString('R',[Globalization.CultureInfo]::InvariantCulture)) -Output "$($stage.output).archive.json" -TimeoutSeconds $seconds
   if(Test-Path -LiteralPath $cachePrefix){throw 'Python bytecode prefix unexpectedly exists'}
   Copy-Item -LiteralPath (Join-Path $OutputDirectory 'processes.json') -Destination (Join-Path $OutputDirectory 'postprocesses.json')
   # Prune only after the archive child and all its streams close, avoiding a
   # deletion race with the supervisor's live-file inventory.
   $archiveFile=Join-Path $OutputDirectory "$($stage.output).archive.json"
   $archive=Get-Content -LiteralPath $archiveFile -Raw|ConvertFrom-Json
   $rawPath=[IO.Path]::GetFullPath((Join-Path $OutputDirectory $stage.output))
   if([IO.Path]::GetDirectoryName($rawPath) -cne [IO.Path]::GetFullPath($OutputDirectory) -or !$archive.independent_exact_decompression_verified -or $archive.uncompressed_duplicate_removed -or $archive.raw_name -cne $stage.output -or $archive.archive_name -cne "$($stage.output).gz"){throw 'Unverified archive/prune identity'}
   if((HashFile $rawPath) -cne $archive.raw_sha256 -or (Get-Item -LiteralPath $rawPath).Length -ne $archive.raw_bytes -or (HashFile (Join-Path $OutputDirectory $archive.archive_name)) -cne $archive.gzip_sha256){throw 'Postprocess raw/archive changed before prune'}
   $prune=[ordered]@{schema='greenlens.verified-raw-duplicate-prune.v1';archive_receipt_sha256=(HashFile $archiveFile);raw_name=$stage.output;raw_sha256=$archive.raw_sha256;raw_bytes=$archive.raw_bytes;gzip_sha256=$archive.gzip_sha256;removed=$false}
   $pruneFile=Join-Path $OutputDirectory "$($stage.output).prune.json";SaveJson $pruneFile $prune
   [IO.File]::Delete($rawPath);$prune.removed=$true;SaveJson $pruneFile $prune
  }
  if($stage.ordinal -eq 22){InvokeGate 'R1'}
  if($stage.phase -ceq 'R2'){
   $variant=@('scale10000','weight1e10','depth24','secondary256')|Where-Object{$stage.id.Contains("-$_-")}|Select-Object -First 1
   $isLast=($i -eq $plan.stages.Count-1)
   if($isLast -or !$plan.stages[$i+1].id.Contains("-$variant-")){InvokeGate 'R2' $variant}
  }
 }
 foreach($key in $caseRoots.Keys){AssertInputs $key}
 $null=Remaining
 SaveJson (Join-Path $targetRoot 'experiment-result.json') @{schema='greenlens.external-primary-reference-prerequisite-result.v1';execution_complete=$true;native_stages_completed=$completedNative;checks=$gateFiles;all_estimator_prerequisites_passed=$true;reference_family_qualified=$false;accuracy_accepted=$false;release_accepted=$false;wall_seconds=$clock.Elapsed.TotalSeconds}
}catch{
 SaveJson (Join-Path $targetRoot 'experiment-failure.json') @{schema='greenlens.external-primary-reference-prerequisite-failure.v1';failed_utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;native_stages_completed=$completedNative;checks=$gateFiles;wall_seconds=$clock.Elapsed.TotalSeconds;execution_complete=$false;reference_family_qualified=$false;accuracy_accepted=$false;release_accepted=$false}
 throw
}
