#Requires -Version 7.5
param([string]$OutputDirectory='')
$ErrorActionPreference='Stop'
$requestedOutputDirectory=$OutputDirectory
. "$PSScriptRoot/run-hosted-reference-case.ps1" -FunctionsOnly
$OutputDirectory=$requestedOutputDirectory
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/hosted reference wrapper synthetic '+[guid]::NewGuid().ToString('N'))}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Fresh test directory required.'}
$null=New-Item -ItemType Directory -Path $OutputDirectory
$plan=Get-Content -LiteralPath "$PSScriptRoot/hosted-reference-plan.json" -Raw|ConvertFrom-Json -Depth 50 -DateKind String
$checks=[Collections.Generic.List[object]]::new()
function Pass([string]$Name){$checks.Add([ordered]@{name=$Name;passed=$true})}
function Reject([string]$Name,[scriptblock]$Action){$failed=$false;try{& $Action}catch{$failed=$true};if(!$failed){throw "Expected rejection: $Name"};Pass $Name}
Assert-HostedReferencePlan $plan
Pass 'exact seventeen tuples and fixed caps accepted'
foreach($case in Get-HostedReferenceCases){Assert-HostedReferenceCase $case.alternative $case.interval_index}
Pass 'all seventeen allowed tuples accepted'
foreach($key in $plan.excluded_qualified){$a,$i=$key.Split(':');Reject "already qualified $key excluded" {Assert-HostedReferenceCase $a ([int]$i)}}
Reject 'unknown alternative' {Assert-HostedReferenceCase 'C' 1908}
Reject 'unplanned interval' {Assert-HostedReferenceCase 'A' 1}
$changed=($plan|ConvertTo-Json -Depth 50)|ConvertFrom-Json -Depth 50
$changed.cases[1]=$changed.cases[0]
Reject 'duplicate plan tuple' {Assert-HostedReferencePlan $changed}
$changed=($plan|ConvertTo-Json -Depth 50)|ConvertFrom-Json -Depth 50;$changed.process_seconds=7201
Reject 'changed timeout' {Assert-HostedReferencePlan $changed}
function New-Fixture([string]$FixtureName,[string]$Change='') {
 $manifest=[ordered]@{schema_version='greenlens.hosted-reference-inputs.v1';alternative='A';interval_index=8504;required_input_count=11;members=@();source_sha256=$plan.archives.A.source_sha256}
 $names=@('original-job/annual-scene.rad','original-job/annual-sensors.pts','original-job/annual-sky.mtx','original-job/annual-suns.rad','original-job/annual-weather.epw','original-job/annual-recipe.json','original-job/input.json','original-job/study.json','original-job/weather/original.epw','original-job/weather/normalized.epw','original-job/weather/asset.json','notices/GreenLens-LICENSE.txt','notices/PVGIS-ERA5-NOTICE.txt','notices/Radiance-LICENSE.txt')
 $payloads=[ordered]@{}
 foreach($name in $names){
  $bytes=[Text.Encoding]::UTF8.GetBytes("explicitly synthetic wrapper fixture $name")
  $payloads[$name]=$bytes
  $manifest.members+=,[ordered]@{path=$name;bytes=$bytes.Length;sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()}
 }
 if($Change -eq 'source'){$manifest.source_sha256=($manifest.source_sha256|ConvertTo-Json)|ConvertFrom-Json;$manifest.source_sha256.'scripts/annual-controlled-reference.ps1'='0'*64}
 if($Change -eq 'alternative'){$manifest.alternative='B'}
 if($Change -eq 'member'){$manifest.members[0].sha256='0'*64}
 if($Change -eq 'duplicateManifest'){$manifest.members[1]=$manifest.members[0]}
 if($Change -eq 'path'){$payloads['../escaped.txt']=$payloads[$names[0]];$payloads.Remove($names[0])}
 $manifestBytes=[Text.Encoding]::UTF8.GetBytes(($manifest|ConvertTo-Json -Depth 20))
 $path=Join-Path $OutputDirectory "$FixtureName.zip"
 $zip=[IO.Compression.ZipFile]::Open($path,[IO.Compression.ZipArchiveMode]::Create)
 try{
  foreach($key in $payloads.Keys){$stream=$zip.CreateEntry($key).Open();try{$stream.Write($payloads[$key])}finally{$stream.Dispose()}}
  $stream=$zip.CreateEntry('input-manifest.json').Open();try{$stream.Write($manifestBytes)}finally{$stream.Dispose()}
 }finally{$zip.Dispose()}
 return [ordered]@{path=$path;pin=[pscustomobject]@{bytes=(Get-Item -LiteralPath $path).Length;sha256=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant();manifest_schema='greenlens.hosted-reference-inputs.v1';manifest_sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($manifestBytes)).ToLowerInvariant();source_sha256=$plan.archives.A.source_sha256}}
}
$valid=New-Fixture 'valid'
$destination=Join-Path $OutputDirectory 'valid-expanded'
$answer=Expand-HostedReferenceInputs $valid.path $destination 'A' $valid.pin
if(@(Get-ChildItem -LiteralPath $destination -Recurse -File).Count -ne 15){throw 'Valid closure shape failed'}
Pass 'all fifteen synthetic members validated and extracted'
Reject 'destination no overwrite' {Expand-HostedReferenceInputs $valid.path $destination 'A' $valid.pin|Out-Null}
foreach($mutation in @('source','alternative','member','duplicateManifest','path')){
 $fixture=New-Fixture $mutation $mutation
 $dest=Join-Path $OutputDirectory "$mutation-expanded"
 Reject "$mutation rejected before extraction" {Expand-HostedReferenceInputs $fixture.path $dest 'A' $fixture.pin|Out-Null}
 if(Test-Path -LiteralPath $dest){throw 'Invalid input was extracted'}
}
foreach($field in @('sha256','manifest_sha256','bytes')){
 $bad=($valid.pin|ConvertTo-Json -Depth 20)|ConvertFrom-Json -Depth 20
 if($field -eq 'bytes'){$bad.bytes++}else{$bad.$field='0'*64}
 $dest=Join-Path $OutputDirectory "$field-expanded"
 Reject "wrong $field" {Expand-HostedReferenceInputs $valid.path $dest 'A' $bad|Out-Null}
 if(Test-Path -LiteralPath $dest){throw 'Wrong pinned identity created destination'}
}
$result=[ordered]@{schema_version='greenlens.hosted-reference-wrapper-tests.v1';scope='Synthetic archive, path, exact-case and source-pin wrapper regressions only; no scientific or engine verification';checks=$checks;passed=$true}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'tests.json'),($result|ConvertTo-Json -Depth 15)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
[ordered]@{passed=$true;checks=$checks.Count;output=$OutputDirectory}|ConvertTo-Json
$global:LASTEXITCODE=0
