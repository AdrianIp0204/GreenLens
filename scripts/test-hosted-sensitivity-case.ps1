#Requires -Version 7.5
param([string]$OutputDirectory='')
$ErrorActionPreference='Stop'
$requestedOutput=$OutputDirectory
. "$PSScriptRoot/run-hosted-sensitivity-case.ps1" -FunctionsOnly
$wrapperHash=(Get-FileHash -LiteralPath "$PSScriptRoot/run-hosted-sensitivity-case.ps1").Hash.ToLowerInvariant()
$OutputDirectory=$requestedOutput
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/hosted sensitivity wrapper synthetic '+[guid]::NewGuid().ToString('N')+' 空間')}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Fresh test directory required.'}
$null=New-Item -ItemType Directory -Path $OutputDirectory
$plan=Get-Content -LiteralPath "$PSScriptRoot/hosted-sensitivity-plan.json" -Raw|ConvertFrom-Json -Depth 60 -DateKind String
$checks=[Collections.Generic.List[object]]::new();$utf8=[Text.UTF8Encoding]::new($false)
function Pass([string]$Name){$checks.Add([ordered]@{name=$Name;passed=$true})}
function Reject([string]$Name,[scriptblock]$Action){$failed=$false;try{& $Action}catch{$failed=$true};if(!$failed){throw "Expected rejection: $Name"};Pass $Name}
function Clone($Object){($Object|ConvertTo-Json -Depth 70)|ConvertFrom-Json -Depth 70 -DateKind String}
function Hash([byte[]]$Bytes){[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant()}
Assert-HostedSensitivityPlan $plan;Pass 'exact six tuples and unchanged limits'
foreach($case in Get-HostedSensitivityCases){Assert-HostedSensitivityCase $case.alternative $case.interval_index $case.variant};Pass 'all six fixed tuples accepted'
Reject 'unknown design' {Assert-HostedSensitivityCase 'C' 4116 'scale10000'}
Reject 'wrong fixed index' {Assert-HostedSensitivityCase 'A' 8512 'scale10000'}
Reject 'unknown variant' {Assert-HostedSensitivityCase 'A' 4116 'new'}
$changed=Clone $plan;$changed.cases[1]=$changed.cases[0];Reject 'duplicate plan tuple' {Assert-HostedSensitivityPlan $changed}
$changed=Clone $plan;$changed.cases=$changed.cases[0..4];Reject 'missing plan tuple' {Assert-HostedSensitivityPlan $changed}
$changed=Clone $plan;$changed.variants[0].scale=10001;Reject 'changed numerical variant' {Assert-HostedSensitivityPlan $changed}
foreach($field in @('maximum_parallel_jobs','engine_workers_per_job','maximum_other_baseline_jobs','job_minutes','process_seconds','maximum_file_bytes','maximum_scratch_bytes','maximum_runner_plus_engine_bytes','ambient_divisions')){$changed=Clone $plan;$changed.$field++;Reject "changed $field" {Assert-HostedSensitivityPlan $changed}}
function New-Fixture([string]$Label,[string]$Design='A',[string]$Mutation=''){
 $index=if($Design -ceq 'A'){4116}else{8512};$names=@(Get-SensitivityMemberNames $Design $index)
 $manifest=[ordered]@{schema_version='greenlens.hosted-sensitivity-inputs.v1';alternative=$Design;interval_index=$index;sensor_count=24;qualified_final_repeats=256;member_count=$names.Count;source_sha256=(Clone $plan.source_sha256);baseline_declaration_sha256=('1'*64);baseline_report_sha256=('2'*64);baseline_processes_sha256=('3'*64);members=@();uncompressed_bytes=0L}
 $entries=[Collections.Generic.List[object]]::new()
 foreach($name in $names){$bytes=$utf8.GetBytes("Explicitly synthetic non-scientific fixture: $name 空間`n");$entries.Add([ordered]@{name=$name;bytes=$bytes;attributes=0});$manifest.members+=,[ordered]@{name=$name;bytes=$bytes.Length;sha256=(Hash $bytes)};$manifest.uncompressed_bytes+=$bytes.Length}
 switch($Mutation){
  'wrongDesign' {$manifest.alternative='C'}
  'wrongInterval' {$manifest.interval_index++}
  'wrongSensors' {$manifest.sensor_count=25}
  'wrongRung' {$manifest.qualified_final_repeats=512}
  'baselineIdentity' {$manifest.baseline_report_sha256='4'*64}
  'sourceDigest' {$manifest.source_sha256.'scripts/annual-reference-sensitivity.ps1'='0'*64}
  'sourceMissing' {$manifest.source_sha256.PSObject.Properties.Remove('scripts/annual-reference-sensitivity.ps1')}
  'sourceExtra' {$manifest.source_sha256|Add-Member 'scripts/untrusted.ps1' ('0'*64)}
  'memberHash' {$manifest.members[0].sha256='0'*64}
  'memberLength' {$manifest.members[0].bytes++}
  'duplicateManifest' {$manifest.members[1]=$manifest.members[0]}
  'uncompressedTotal' {$manifest.uncompressed_bytes++}
  'pathTraversal' {$entries[0].name='../escape.txt'}
  'duplicateZip' {$entries[1].name=$entries[0].name}
  'caseDuplicateZip' {$entries[1].name=$entries[0].name.ToUpperInvariant()}
  'symlink' {$entries[0].attributes=0xA000 -shl 16}
  'unlistedMember' {$entries[0].name='baseline/unlisted.txt'}
 }
 $manifestBytes=$utf8.GetBytes(($manifest|ConvertTo-Json -Depth 25));if($Mutation -eq 'invalidManifestUtf8'){$manifestBytes=[byte[]]@(255,254,255)}
 $path=Join-Path $OutputDirectory "$Label.zip";$zip=[IO.Compression.ZipFile]::Open($path,[IO.Compression.ZipArchiveMode]::Create)
 try{foreach($entry in $entries){$member=$zip.CreateEntry($entry.name);$member.ExternalAttributes=$entry.attributes;$stream=$member.Open();try{$stream.Write($entry.bytes)}finally{$stream.Dispose()}};$stream=$zip.CreateEntry('input-manifest.json').Open();try{$stream.Write($manifestBytes)}finally{$stream.Dispose()}}finally{$zip.Dispose()}
 $actualTotal=($entries|ForEach-Object{$_.bytes.Length}|Measure-Object -Sum).Sum+$manifestBytes.Length
 return [ordered]@{path=$path;pinned=[pscustomobject]@{alternative=$Design;interval_index=$index;bytes=(Get-Item -LiteralPath $path).Length;sha256=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant();manifest_sha256=(Hash $manifestBytes);member_count=$names.Count+1;uncompressed_bytes=$actualTotal;baseline_declaration_sha256=('1'*64);baseline_report_sha256=('2'*64);baseline_processes_sha256=('3'*64)}}
}
$validA=New-Fixture 'valid-A' 'A';$validB=New-Fixture 'valid-B' 'B'
foreach($fixture in @($validA,$validB)){$dest=Join-Path $OutputDirectory ($fixture.pinned.alternative+'-expanded');$manifest=Expand-SensitivityInputs $fixture.path $dest $fixture.pinned $plan.source_sha256;if(@(Get-ChildItem -LiteralPath $dest -Recurse -File).Count -ne $fixture.pinned.member_count){throw 'Extracted closure count differs'};foreach($member in $manifest.members){if((Get-FileHash -LiteralPath (Join-Path $dest $member.name)).Hash.ToLowerInvariant() -cne $member.sha256){throw 'Extracted fixture changed'}};Pass "exact synthetic $($fixture.pinned.alternative) archive closure"}
$existing=Join-Path $OutputDirectory 'A-expanded';$marker=Join-Path $existing 'do-not-overwrite.txt';[IO.File]::WriteAllText($marker,'keep',$utf8);Reject 'existing destination retained' {Expand-SensitivityInputs $validA.path $existing $validA.pinned $plan.source_sha256|Out-Null};if([IO.File]::ReadAllText($marker) -cne 'keep'){throw 'Existing destination changed'}
foreach($mutation in @('wrongDesign','wrongInterval','wrongSensors','wrongRung','baselineIdentity','sourceDigest','sourceMissing','sourceExtra','memberHash','memberLength','duplicateManifest','uncompressedTotal','pathTraversal','duplicateZip','caseDuplicateZip','symlink','unlistedMember','invalidManifestUtf8')){$fixture=New-Fixture $mutation 'A' $mutation;$dest=Join-Path $OutputDirectory ($mutation+'-expanded');Reject "$mutation rejected before extraction" {Expand-SensitivityInputs $fixture.path $dest $fixture.pinned $plan.source_sha256|Out-Null};if(Test-Path -LiteralPath $dest){throw 'Rejected input created destination'}}
foreach($field in @('bytes','sha256','manifest_sha256')){$pin=Clone $validA.pinned;if($field -eq 'bytes'){$pin.bytes++}else{$pin.$field='0'*64};$dest=Join-Path $OutputDirectory ($field+'-expanded');Reject "wrong pinned $field" {Expand-SensitivityInputs $validA.path $dest $pin $plan.source_sha256|Out-Null};if(Test-Path -LiteralPath $dest){throw 'Wrong pin created destination'}}
Reject 'oversized archive member' {Assert-SensitivityZipName 'baseline/a.rgb' 0 67108865}
Reject 'rooted archive path' {Assert-SensitivityZipName '/baseline/a.rgb' 0 10}
Reject 'Windows path separators' {Assert-SensitivityZipName 'baseline\a.rgb' 0 10}
$pin=$validA.pinned
$baseline=[pscustomobject]@{declaration=[pscustomobject]@{alternative='A'};declaration_sha256=$pin.baseline_declaration_sha256;report_sha256=$pin.baseline_report_sha256;processes_sha256=$pin.baseline_processes_sha256;cases=@([pscustomobject]@{qualified=$true;interval_index=4116;sensor_count=24;attempts=@([pscustomobject]@{rounds=256})});contexts=@([pscustomobject]@{synthetic=$true})}
Assert-HostedSensitivityBaseline $baseline $pin;Pass 'strict-reader result shape accepted'
foreach($change in @('unqualified','differentCase','wrongRung','wrongSensors','duplicateCase','missingContext','identity')){$bad=Clone $baseline;switch($change){'unqualified'{$bad.cases[0].qualified=$false};'differentCase'{$bad.cases[0].interval_index=4112};'wrongRung'{$bad.cases[0].attempts[0].rounds=512};'wrongSensors'{$bad.cases[0].sensor_count=23};'duplicateCase'{$bad.cases+=@($bad.cases[0])};'missingContext'{$bad.contexts=@()};'identity'{$bad.processes_sha256='0'*64}};Reject "invalid baseline $change" {Assert-HostedSensitivityBaseline $bad $pin}}
$bytes=[byte[]]::new(131089);for($i=0;$i -lt $bytes.Length;$i++){$bytes[$i]=$i%251}
$sourceStream=[IO.MemoryStream]::new($bytes,$false);$copiedStream=[IO.MemoryStream]::new()
try{$receipt=Copy-BoundedSensitivityStream $sourceStream $bytes.Length $bytes.Length $copiedStream;if($receipt.bytes -ne $bytes.Length -or $receipt.sha256 -cne (Hash $bytes) -or (Hash $copiedStream.ToArray()) -cne $receipt.sha256){throw 'Exact streamed receipt or copied bytes differ'}}finally{$sourceStream.Dispose();$copiedStream.Dispose()};Pass 'actual multi-buffer stream length hash and copied bytes'
$sourceStream=[IO.MemoryStream]::new();try{$receipt=Copy-BoundedSensitivityStream $sourceStream 0 0;if($receipt.bytes -ne 0 -or $receipt.sha256 -cne (Hash ([byte[]]@()))){throw 'Empty stream receipt differs'}}finally{$sourceStream.Dispose()};Pass 'exact empty bounded stream'
$sourceStream=[IO.MemoryStream]::new([byte[]]::new(8),$false);try{Reject 'actual truncated stream' {Copy-BoundedSensitivityStream $sourceStream 9 16|Out-Null}}finally{$sourceStream.Dispose()}
$sourceStream=[IO.MemoryStream]::new([byte[]]::new(1024),$false);$copiedStream=[IO.MemoryStream]::new();try{Reject 'actual oversized stream' {Copy-BoundedSensitivityStream $sourceStream 16 16 $copiedStream|Out-Null};if($sourceStream.Position -ne 17 -or $copiedStream.Length -gt 16){throw 'Stream consumed or wrote beyond the bounded overread'};Pass 'overread stops at expected plus one'}finally{$sourceStream.Dispose();$copiedStream.Dispose()}
$sourceStream=[IO.MemoryStream]::new([byte[]]::new(17),$false);try{Reject 'declared cap plus one' {Copy-BoundedSensitivityStream $sourceStream 17 16|Out-Null};if($sourceStream.Position -ne 0){throw 'Invalid cap read input'}}finally{$sourceStream.Dispose()}
$sourceStream=[IO.MemoryStream]::new();try{Reject 'negative stream limit' {Copy-BoundedSensitivityStream $sourceStream -1 16|Out-Null}}finally{$sourceStream.Dispose()}
$sourceStream=[IO.MemoryStream]::new([byte[]]@(1),$false);try{Reject 'actual byte in declared empty stream' {Copy-BoundedSensitivityStream $sourceStream 0 0|Out-Null}}finally{$sourceStream.Dispose()}
if((Get-FileHash -LiteralPath "$PSScriptRoot/run-hosted-sensitivity-case.ps1").Hash.ToLowerInvariant() -cne $wrapperHash){throw 'Wrapper changed during test'}
$result=[ordered]@{schema_version='greenlens.hosted-sensitivity-wrapper-tests.v1';scope='Original synthetic plan/archive/path, actual-stream bounds and strict-reader-shape guard tests; no engine or scientific qualification';wrapper_sha256=$wrapperHash;passed=$true;checks=$checks}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'tests.json'),($result|ConvertTo-Json -Depth 15)+"`n",$utf8)
[ordered]@{passed=$true;checks=$checks.Count;output=$OutputDirectory}|ConvertTo-Json
$global:LASTEXITCODE=0
