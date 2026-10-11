#Requires -Version 7.5
param([string]$Case,[string]$BaselineArchive,[string]$EndpointArchive,[string]$OutputDirectory,[string]$SourceRoot=(Get-Location).Path,[switch]$PrepareOnly,[switch]$FunctionsOnly)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/depth-order-helpers.ps1"
function Expand-DepthEndpoint([string]$Archive,[string]$Destination,$Pinned,$ExpectedIdentity,[string]$TrustedSource) {
 if(Test-Path -LiteralPath $Destination){throw 'Fresh endpoint destination required.'}
 $f=Get-Item -LiteralPath $Archive -Force
 if($f.PSIsContainer -or ($f.Attributes-band[IO.FileAttributes]::ReparsePoint) -or $f.Length-ne$Pinned.bytes -or (Get-FileHash $f.FullName).Hash.ToLowerInvariant()-cne$Pinned.sha256){throw 'Endpoint archive identity differs.'}
 # Import only trusted checkout helpers; extracted source remains inert data.
 . (Join-Path $TrustedSource 'scripts/run-hosted-sensitivity-case.ps1') -FunctionsOnly
 $names=@('aggregation-source.cs','declaration.json','depth10-24.oct','depth10-24.rad','depth10-24.rgb','finite-kernel.cs','harness-source.ps1','hour-4116-direct.rgb','hour-4116-finite.rgb','hour-4116-rounds256.rgb','hour-4116-scaled.rad','hour-4116-sun.rad','hour-4116.dat','matched-basis.cal','process-source.ps1','processes.json','scene.rad','sensitivity.json','sensors.pts')
 $zip=[IO.Compression.ZipFile]::OpenRead($f.FullName)
 try{
  if($zip.Entries.Count-ne20 -or $Pinned.member_count-ne20){throw 'Endpoint member count differs.'}
  $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
  foreach($e in $zip.Entries){Assert-SensitivityZipName $e.FullName $e.ExternalAttributes $e.Length;if(!$seen.Add($e.FullName)){throw 'Duplicate endpoint member.'}}
  if((@($seen|Sort-Object)-join'|')-cne(@(($names+@('input-manifest.json'))|Sort-Object)-join'|')){throw 'Unexpected endpoint member.'}
  $e=$zip.GetEntry('input-manifest.json');$input=$e.Open();$memory=[IO.MemoryStream]::new()
  try{$m=Copy-BoundedSensitivityStream $input $e.Length 1048576 $memory;$bytes=$memory.ToArray()}finally{$input.Dispose();$memory.Dispose()}
  if($m.sha256-cne$Pinned.manifest_sha256){throw 'Endpoint manifest seal differs.'}
  $manifest=[Text.UTF8Encoding]::new($false,$true).GetString($bytes)|ConvertFrom-Json -Depth 50 -DateKind String
  if($manifest.schema_version-cne'greenlens.failed-depth-inputs.v1' -or $manifest.alternative-cne'A' -or $manifest.interval_index-ne4116 -or $manifest.members.Count-ne19){throw 'Endpoint manifest identity differs.'}
  foreach($key in @('declaration_sha256','report_sha256','processes_sha256')){if($manifest.$key-cne$ExpectedIdentity.$key){throw 'Failed endpoint seals differ.'}}
  $memberSeen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal);$total=[long]$bytes.Length
  foreach($member in $manifest.members){
   if($member.name-cnotin$names -or !$memberSeen.Add($member.name) -or $member.sha256-cnotmatch'^[0-9a-f]{64}$'){throw 'Endpoint manifest member differs.'}
   $e=$zip.GetEntry($member.name);if($e.Length-ne$member.bytes){throw 'Endpoint member size differs.'};$total+=$e.Length
   $input=$e.Open();try{$v=Copy-BoundedSensitivityStream $input $e.Length 67108864}finally{$input.Dispose()};if($v.sha256-cne$member.sha256){throw 'Endpoint member hash differs.'}
  }
  if($total-ne$Pinned.uncompressed_bytes -or $total-gt16777216){throw 'Endpoint total bound differs.'}
  $null=New-Item -ItemType Directory -Path $Destination
  foreach($member in $manifest.members){$input=$zip.GetEntry($member.name).Open();$output=[IO.File]::Open((Join-Path $Destination $member.name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write);try{$v=Copy-BoundedSensitivityStream $input $member.bytes 67108864 $output}finally{$input.Dispose();$output.Dispose()};if($v.sha256-cne$member.sha256){throw 'Extracted endpoint differs.'}}
  [IO.File]::WriteAllBytes((Join-Path $Destination 'input-manifest.json'),$bytes)
  return $manifest
 }finally{$zip.Dispose()}
}
if($FunctionsOnly){return}
$selectedCase=Get-DepthCase $Case
$plan=Get-Content "$PSScriptRoot/depth-order-plan.json" -Raw|ConvertFrom-Json -Depth 60 -DateKind String
Assert-DepthPlan $plan
if(!$PrepareOnly -and !$plan.execution_authorized){throw 'Reviewed diagnostic dispatch remains disabled.'}
if(!$OutputDirectory -or (Test-Path -LiteralPath $OutputDirectory)){throw 'Fresh owned hosted output required.'}
$hostedOutput=$OutputDirectory
. (Join-Path $SourceRoot 'scripts/prepare-sensitivity-sources.ps1')
$preparation=Prepare-SensitivitySources $SourceRoot $plan.legacy_source_sha256
foreach($p in $plan.trusted_source_sha256.PSObject.Properties){if((Get-FileHash (Join-Path $SourceRoot $p.Name)).Hash.ToLowerInvariant()-cne$p.Value){throw "Reviewed source differs: $($p.Name)"}}
$engine=& {param($root). (Join-Path $root 'scripts/run-hosted-sensitivity-case.ps1') -FunctionsOnly;Get-HostedSensitivityEngine $root} $SourceRoot
$baselineManifest=& {param($root,$archive,$dest,$pinned,$pins). (Join-Path $root 'scripts/run-hosted-sensitivity-case.ps1') -FunctionsOnly;Expand-SensitivityInputs $archive $dest $pinned $pins} $SourceRoot $BaselineArchive (Join-Path $hostedOutput 'baseline-inputs') $plan.baseline_archive $plan.legacy_source_sha256
$endpointManifest=Expand-DepthEndpoint $EndpointArchive (Join-Path $hostedOutput 'endpoint-inputs') $plan.endpoint_archive $plan.endpoint $SourceRoot
$receipt=[ordered]@{schema_version='greenlens.hosted-depth-order-diagnostic.v1';started_utc=[DateTime]::UtcNow.ToString('o');case=$Case;full_plan=$plan.cases;baseline_archive_sha256=$plan.baseline_archive.sha256;endpoint_archive_sha256=$plan.endpoint_archive.sha256;engine_sha256=$engine;prepare_only=[bool]$PrepareOnly;completed=$false;reference_family_qualified=$false;release_accepted=$false}
try{
 $mode=if($PrepareOnly){'Prepare'}else{'Run'}
 $common=@('-NoProfile','-File',"$PSScriptRoot/run-depth-order-case.ps1",'-Mode',$mode,'-Case',$Case,'-BaselineDirectory',(Join-Path $hostedOutput 'baseline-inputs/baseline'),'-EndpointDirectory',(Join-Path $hostedOutput 'endpoint-inputs'),'-OutputDirectory',(Join-Path $hostedOutput 'diagnostic'),'-TrustedSourceRoot',$SourceRoot)
 & (Join-Path $PSHOME 'pwsh.exe') @common
 if($LASTEXITCODE){throw 'Diagnostic child failed; retained partial data are not complete.'}
 $receipt.completed=!$PrepareOnly
}finally{$receipt['finished_utc']=[DateTime]::UtcNow.ToString('o');$receipt['source_preparation']=$preparation;$receipt|ConvertTo-Json -Depth 60|Set-Content (Join-Path $hostedOutput 'case.json') -Encoding utf8}
$global:LASTEXITCODE=0
