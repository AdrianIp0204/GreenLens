# Pure restoration/inventory/archive regressions; no engine or original project is touched.
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
. "$PSScriptRoot/energy-portability.ps1" -FunctionsOnly
$root=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/portability restore '+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root|Out-Null
$moves=[Collections.Generic.List[object]]::new()
foreach($name in @('project','job1','job2')){
 $original=Join-Path $root $name; $hidden=$original+'.hidden'
 New-Item -ItemType Directory -Path $hidden|Out-Null
 Set-Content -LiteralPath (Join-Path $hidden 'identity.txt') -Value $name
 $moves.Add(@{original=$original;hidden=$hidden})
}
New-Item -ItemType Directory -Path $moves[1].original|Out-Null
Set-Content -LiteralPath (Join-Path $moves[1].original 'new.txt') -Value 'external change must survive'
$failed=$false
try{Restore-EnergyVerificationPaths $moves}catch{$failed=$_.Exception.Message.Contains('restore conflicts')}
if(!$failed){throw 'Expected explicit conflict'}
foreach($index in @(0,2)){
 if(!(Test-Path -LiteralPath (Join-Path $moves[$index].original 'identity.txt')) -or (Test-Path -LiteralPath $moves[$index].hidden)){throw 'Independent evidence was not restored'}
}
if(!(Test-Path -LiteralPath (Join-Path $moves[1].hidden 'identity.txt')) -or !(Test-Path -LiteralPath (Join-Path $moves[1].original 'new.txt'))){throw 'Conflict overwrote new or retained evidence'}
function Assert-Rejected([scriptblock]$Action,[string]$Reason){
 $rejected=$false
 try { & $Action|Out-Null } catch { $rejected=$true }
 if(!$rejected){throw "Expected rejection: $Reason"}
}
if((Assert-EnergyArchiveSizes ([long[]]@(64MB,64MB,64MB,64MB))) -ne 256MB){throw 'Exact archive byte boundary was rejected'}
if((Assert-EnergyArchiveSizes ([long[]]::new(5000))) -ne 0){throw 'Exact archive entry boundary was rejected'}
Assert-Rejected {Assert-EnergyArchiveSizes ([long[]]@(64MB+1))} 'entry limit'
Assert-Rejected {Assert-EnergyArchiveSizes ([long[]]@(64MB,64MB,64MB,64MB,1))} 'total limit'
Assert-Rejected {Assert-EnergyArchiveSizes ([long[]]::new(5001))} 'entry-count limit'
Assert-Rejected {Assert-EnergyArchiveSizes ([long[]]@(-1))} 'negative size'
foreach($name in @('../outside','C:/absolute','references\bad','references//bad','references/CON.txt','references/bad.','references/bad/')){
 Assert-Rejected {Assert-EnergyPortableMember $name} "unsafe member $name"
}

$project=Join-Path $root 'synthetic inventory'
$assetId='926b3a32-c1bf-433c-8a72-24e34f17c1b0'
$assetPath="references/$assetId"
foreach($dir in @("$assetPath/geometry",'attachments/original','runs/synthetic','weather/synthetic','energy-assessments/synthetic')){
 New-Item -ItemType Directory -Path (Join-Path $project $dir) -Force|Out-Null
}
# This is a tiny inventory fixture, not admitted geometry or simulation evidence.
$chunk=[Text.Encoding]::UTF8.GetBytes('original synthetic normalized inventory bytes')
$chunkPath=Join-Path $project "$assetPath/geometry/00000.bin"
[IO.File]::WriteAllBytes($chunkPath,$chunk)
$chunkHash=(Get-FileHash -LiteralPath $chunkPath).Hash.ToLowerInvariant()
$manifest=[ordered]@{schema_version='greenlens.reference-model.v1';revision_sha256=('a'*64);geometry_sha256=('b'*64);chunks=@(@{path='geometry/00000.bin';sha256=$chunkHash;byte_length=$chunk.Length})}
$manifestPath=Join-Path $project "$assetPath/reference.json"
$manifest|ConvertTo-Json -Depth 10|Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
'synthetic seal bytes'|Set-Content -LiteralPath (Join-Path $project "$assetPath/checksums.json") -Encoding utf8NoBOM
$document=[ordered]@{architecture=@{assets=@(@{id=$assetId;path=$assetPath;manifest_sha256=(Get-FileHash -LiteralPath $manifestPath).Hash.ToLowerInvariant();revision_sha256=('a'*64);geometry_sha256=('b'*64)})}}
$document|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $project 'project.json') -Encoding utf8NoBOM
foreach($name in @('attachments/original/evidence.txt','runs/synthetic/results.json','weather/synthetic/original.epw','energy-assessments/synthetic/plan.json')){
 'synthetic immutable bytes'|Set-Content -LiteralPath (Join-Path $project $name) -Encoding utf8NoBOM
}
$snapshot=Get-EnergyPortableSnapshot $project
if($snapshot.Count -ne 8 -or !$snapshot.Contains('project.json') -or !$snapshot.Contains("$assetPath/geometry/00000.bin") -or !$snapshot.Contains('attachments/original/evidence.txt')){throw 'Complete portable snapshot omitted required categories'}
Assert-EnergyPortableSnapshot $project $snapshot
foreach($name in @('project.json',"$assetPath/geometry/00000.bin",'attachments/original/evidence.txt')){
 $path=Join-Path $project $name; $original=[IO.File]::ReadAllBytes($path)
 try {[IO.File]::WriteAllBytes($path,[byte[]]@(1,2,3));Assert-Rejected {Assert-EnergyPortableSnapshot $project $snapshot} "changed $name"}
 finally {[IO.File]::WriteAllBytes($path,$original)}
}
$extra=Join-Path $project 'attachments/original/extra.txt'
try {'extra'|Set-Content -LiteralPath $extra;Assert-Rejected {Assert-EnergyPortableSnapshot $project $snapshot} 'added evidence'}
finally {Remove-Item -LiteralPath $extra}
$members=Get-EnergyReferenceMembers $project $document
if($members.Count -ne 3){throw 'Expected exact reference manifest, seal and chunk'}

function New-PortabilityTestArchive([string]$Path,[object[]]$Entries){
 $zip=[IO.Compression.ZipFile]::Open($Path,[IO.Compression.ZipArchiveMode]::Create)
 try {foreach($item in $Entries){
  $entry=$zip.CreateEntry($item.name);$stream=$entry.Open()
  try {$stream.Write($item.bytes,0,$item.bytes.Length)}finally{$stream.Dispose()}
 }}finally{$zip.Dispose()}
}
$entries=@($members.Keys|ForEach-Object {[pscustomobject]@{name=$_;bytes=[IO.File]::ReadAllBytes((Join-Path $project $_))}})
$validZip=Join-Path $root 'valid-reference.zip'
New-PortabilityTestArchive $validZip $entries
$check=Assert-EnergyReferenceArchive $validZip $members
if($check.normalized_members.Count -ne 3 -or !$check.original_source_and_conversion_excluded){throw 'Valid reference archive check failed'}
$badCases=[ordered]@{
 missing=@($entries|Where-Object name -NotLike '*/00000.bin')
 altered=@($entries|ForEach-Object {if($_.name -like '*/00000.bin'){[pscustomobject]@{name=$_.name;bytes=[byte[]]::new($_.bytes.Length)}}else{$_}})
 extra=@($entries)+@([pscustomobject]@{name="$assetPath/extra.bin";bytes=[byte[]]@(1)})
 duplicate=@($entries)+@($entries[0])
 unsafe=@($entries)+@([pscustomobject]@{name='../outside';bytes=[byte[]]@(1)})
 source=@($entries)+@([pscustomobject]@{name='source.glb';bytes=[byte[]]@(1)})
 case=@($entries|ForEach-Object {[pscustomobject]@{name=$_.name.Replace('references/','References/');bytes=$_.bytes}})
}
foreach($case in $badCases.GetEnumerator()){
 $zip=Join-Path $root ($case.Key+'.zip');New-PortabilityTestArchive $zip $case.Value
 Assert-Rejected {Assert-EnergyReferenceArchive $zip $members} ("archive "+$case.Key)
}
$legacy=Join-Path $root 'legacy.zip'
New-PortabilityTestArchive $legacy @([pscustomobject]@{name='project.json';bytes=[Text.Encoding]::UTF8.GetBytes('{}')})
$legacyCheck=Assert-EnergyReferenceArchive $legacy ([ordered]@{})
if($legacyCheck.normalized_members.Count -ne 0){throw 'Legacy archive gained architectural requirements'}
[ordered]@{schema='greenlens.energy-portability-restore-test.v1';conflict_reported=$true;independent_paths_restored=$true;conflicted_evidence_preserved=$true;complete_evidence_snapshot_checked=$true;archive_boundaries_checked=$true;normalized_reference_archive_cases=8;legacy_archive_checked=$true;scope='Synthetic guard fixtures only; no real CLI, engine, report, geometry admission or installed workflow was executed.'}|ConvertTo-Json
