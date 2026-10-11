#Requires -Version 7.5
param([ValidateSet('A','B')][string]$Alternative,[int]$IntervalIndex,[ValidateSet('scale10000','weight1e10','depth10-24')][string]$Variant,[string]$InputArchive,[string]$OutputDirectory,[string]$SourceRoot=(Get-Location).Path,[switch]$PrepareOnly,[switch]$FunctionsOnly)
$ErrorActionPreference='Stop'
function Get-HostedSensitivityCases {
 foreach($design in @('A','B')){foreach($variantName in @('scale10000','weight1e10','depth10-24')){[ordered]@{alternative=$design;interval_index=$(if($design -ceq 'A'){4116}else{8512});variant=$variantName;sensors=24;rounds=256}}}
}
function Assert-HostedSensitivityCase([string]$Design,[int]$Index,[string]$VariantName){
 if(@(Get-HostedSensitivityCases|Where-Object{$_.alternative -ceq $Design -and $_.interval_index -eq $Index -and $_.variant -ceq $VariantName}).Count -ne 1){throw 'Only the six sealed case/variant tuples are permitted.'}
}
function Assert-HostedSensitivityPlan($Plan){
 if($Plan.schema_version -cne 'greenlens.hosted-sensitivity-plan.v1' -or $Plan.maximum_parallel_jobs -ne 2 -or $Plan.engine_workers_per_job -ne 1 -or $Plan.maximum_other_baseline_jobs -ne 4 -or $Plan.job_minutes -ne 150 -or $Plan.process_seconds -ne 7200 -or $Plan.maximum_file_bytes -ne 536870912 -or $Plan.maximum_scratch_bytes -ne 2147483648 -or $Plan.maximum_runner_plus_engine_bytes -ne 2147483648 -or $Plan.ambient_divisions -ne 8192){throw 'Sensitivity plan limits differ.'}
 $actual=@($Plan.cases|ForEach-Object{"$($_.alternative):$($_.interval_index):$($_.variant):$($_.sensors):$($_.rounds)"})
 $expected=@(Get-HostedSensitivityCases|ForEach-Object{"$($_.alternative):$($_.interval_index):$($_.variant):$($_.sensors):$($_.rounds)"})
 if(($actual -join '|') -cne ($expected -join '|')){throw 'Sensitivity plan duplicates, omits or changes a fixed tuple.'}
 $settings=@($Plan.variants|ForEach-Object{"$($_.name):$($_.scale):$($_.lw):$($_.ab):$($_.lr)"})
 if(($settings -join '|') -cne 'scale10000:10000:1e-9:9:-16|weight1e10:100:1e-10:9:-16|depth10-24:100:1e-9:10:-24'){throw 'Sensitivity settings differ.'}
 if(@($Plan.archives).Count -ne 2 -or ($Plan.archives.alternative -join ',') -cne 'A,B'){throw 'Sensitivity archive alternatives differ.'}
 foreach($archive in $Plan.archives){
  $index=if($archive.alternative -ceq 'A'){4116}else{8512}
  if($archive.interval_index -ne $index -or $archive.bytes -le 0 -or $archive.bytes -gt 16777216 -or $archive.sha256 -cnotmatch '^[0-9a-f]{64}$' -or $archive.manifest_sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Invalid fixed archive metadata.'}
 }
}
function Get-SensitivityMemberNames([string]$Design,[int]$Index){
 $base=@('aggregation-source.cs','controlled-reference.json','declaration.json','finite-kernel.cs','harness-source.ps1',"hour-$Index-convergence.json","hour-$Index-direct.rgb","hour-$Index-finite.rgb","hour-$Index-rounds256.pts","hour-$Index-rounds256.rgb","hour-$Index-scaled.rad","hour-$Index-sun.oct","hour-$Index-sun.rad","hour-$Index.dat","hour-$Index.oct",'identical-sky.mtx','matched-basis.cal','model.json','process-source.ps1','processes.json','receiver.rad','scene.rad','sensors.pts','source-recipe.json','study.json','suns.rad','transmission.oct','weather-asset.json','weather.epw')
 if($Design -ceq 'B'){$base+=@('recovery-helper.ps1','recovery-identity-helper.ps1','recovery-original-declaration.json','recovery-original-harness.ps1','recovery-original-processes.json','recovery.json')}
 @($base|ForEach-Object{"baseline/$_"})+@('notices/GreenLens-LICENSE.txt','notices/PVGIS-ERA5-NOTICE.txt','notices/Radiance-LICENSE.txt')
}
function Assert-SensitivityZipName([string]$Name,[int]$Attributes,[long]$Bytes){
 if($Name -notmatch '^[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)*$' -or $Name -match '(^|/)\.{1,2}(/|$)' -or (($Attributes -shr 16) -band 0xF000) -eq 0xA000 -or $Bytes -lt 0 -or $Bytes -gt 67108864){throw 'Unsafe, linked or oversized archive member.'}
}
function Copy-BoundedSensitivityStream([IO.Stream]$InputStream,[long]$ExpectedBytes,[long]$MaximumBytes,[IO.Stream]$OutputStream=$null){
 if($ExpectedBytes -lt 0 -or $MaximumBytes -lt 0 -or $ExpectedBytes -gt $MaximumBytes){throw 'Invalid bounded stream limits.'}
 $buffer=[byte[]]::new(65536);$count=0L;$hash=[Security.Cryptography.IncrementalHash]::CreateHash([Security.Cryptography.HashAlgorithmName]::SHA256)
 try{
  while($true){
   # Read at most one byte beyond the declared bound, never allocate by untrusted length.
   $requested=[int][Math]::Min($buffer.Length,$ExpectedBytes-$count+1)
   $read=$InputStream.Read($buffer,0,$requested)
   if($read -eq 0){break}
   $count+=$read
   if($count -gt $ExpectedBytes -or $count -gt $MaximumBytes){throw 'Actual decompressed stream exceeds its bounded length.'}
   $hash.AppendData($buffer,0,$read)
   if($null -ne $OutputStream){$OutputStream.Write($buffer,0,$read)}
  }
  if($count -ne $ExpectedBytes){throw 'Actual decompressed stream is truncated.'}
  return @{bytes=$count;sha256=[Convert]::ToHexString($hash.GetHashAndReset()).ToLowerInvariant()}
 }finally{$hash.Dispose()}
}
function Expand-SensitivityInputs([string]$Archive,[string]$Destination,$Pinned,$Pins){
 if(Test-Path -LiteralPath $Destination){throw 'Fresh extraction directory required.'}
 $item=Get-Item -LiteralPath $Archive -Force
 if($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -ne $Pinned.bytes -or (Get-FileHash -LiteralPath $Archive).Hash.ToLowerInvariant() -cne $Pinned.sha256){throw 'Archive size or digest differs.'}
 $names=@(Get-SensitivityMemberNames $Pinned.alternative $Pinned.interval_index)
 $zip=[IO.Compression.ZipFile]::OpenRead($Archive)
 try{
  if($zip.Entries.Count -ne $names.Count+1 -or $zip.Entries.Count -ne $Pinned.member_count){throw 'Archive member count differs.'}
  $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
  foreach($entry in $zip.Entries){Assert-SensitivityZipName $entry.FullName $entry.ExternalAttributes $entry.Length;if(!$seen.Add($entry.FullName)){throw 'Duplicate archive member.'}}
  if((@($seen|Sort-Object)-join '|') -cne (@(($names+@('input-manifest.json'))|Sort-Object)-join '|')){throw 'Archive member allowlist differs.'}
  $entry=$zip.GetEntry('input-manifest.json');if($entry.Length -gt 1048576){throw 'Manifest cap exceeded.'}
  $stream=$entry.Open();$memory=[IO.MemoryStream]::new();try{$read=Copy-BoundedSensitivityStream $stream $entry.Length 1048576 $memory;$bytes=$memory.ToArray()}finally{$stream.Dispose();$memory.Dispose()}
  if($read.sha256 -cne $Pinned.manifest_sha256){throw 'Manifest digest differs.'}
  $manifest=[Text.UTF8Encoding]::new($false,$true).GetString($bytes)|ConvertFrom-Json -Depth 60 -DateKind String
  if($manifest.schema_version -cne 'greenlens.hosted-sensitivity-inputs.v1' -or $manifest.alternative -cne $Pinned.alternative -or $manifest.interval_index -ne $Pinned.interval_index -or $manifest.sensor_count -ne 24 -or $manifest.qualified_final_repeats -ne 256 -or $manifest.member_count -ne $names.Count -or @($manifest.members).Count -ne $names.Count){throw 'Manifest scope differs.'}
  foreach($field in @('baseline_declaration_sha256','baseline_report_sha256','baseline_processes_sha256')){if($manifest.$field -cne $Pinned.$field){throw 'Baseline identity differs.'}}
  $sourceNames=@($Pins.PSObject.Properties.Name|Sort-Object)
  if((@($manifest.source_sha256.PSObject.Properties.Name|Sort-Object)-join '|') -cne ($sourceNames-join '|')){throw 'Source pin set differs.'}
  foreach($name in $sourceNames){if($manifest.source_sha256.$name -cne $Pins.$name){throw 'Source digest differs.'}}
  $memberSeen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase);$total=0L
  foreach($member in $manifest.members){
   if(!$memberSeen.Add($member.name) -or $member.name -cnotin $names -or $member.sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Invalid or duplicate manifest member.'}
   $entry=$zip.GetEntry($member.name);if($entry.Length -ne $member.bytes){throw 'Member length differs.'};$total+=$entry.Length
   $stream=$entry.Open();try{$read=Copy-BoundedSensitivityStream $stream $entry.Length 67108864}finally{$stream.Dispose()}
   if($read.sha256 -cne $member.sha256){throw 'Member digest differs.'}
  }
  if($total -ne $manifest.uncompressed_bytes -or $total+$bytes.Length -ne $Pinned.uncompressed_bytes -or $total -gt 70000000){throw 'Archive total bound differs.'}
  # Validate every byte/path before creating a destination; retained source is data only.
  $null=New-Item -ItemType Directory -Path $Destination
  foreach($member in $manifest.members){$dest=Join-Path $Destination $member.name;$null=New-Item -ItemType Directory -Path (Split-Path -Parent $dest) -Force;$memberStream=$zip.GetEntry($member.name).Open();$output=[IO.File]::Open($dest,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write);try{$read=Copy-BoundedSensitivityStream $memberStream $member.bytes 67108864 $output}finally{$output.Dispose();$memberStream.Dispose()};if($read.sha256 -cne $member.sha256 -or (Get-FileHash -LiteralPath $dest).Hash.ToLowerInvariant() -cne $member.sha256){throw 'Extracted bytes differ.'}}
  [IO.File]::WriteAllBytes((Join-Path $Destination 'input-manifest.json'),$bytes)
  return $manifest
 }finally{$zip.Dispose()}
}
function Assert-HostedSensitivityBaseline($Baseline,$Pinned){
 if($Baseline.declaration.alternative -cne $Pinned.alternative -or $Baseline.declaration_sha256 -cne $Pinned.baseline_declaration_sha256 -or $Baseline.report_sha256 -cne $Pinned.baseline_report_sha256 -or $Baseline.processes_sha256 -cne $Pinned.baseline_processes_sha256){throw 'Strict baseline source identity differs.'}
 if(@($Baseline.cases).Count -ne 1 -or !$Baseline.cases[0].qualified -or $Baseline.cases[0].interval_index -ne $Pinned.interval_index -or $Baseline.cases[0].attempts[-1].rounds -ne 256){throw 'Exactly one qualified 256-repeat fixed baseline is required.'}
 if(@($Baseline.contexts).Count -ne 1 -or $Baseline.cases[0].sensor_count -ne 24){throw 'Baseline sensor context differs.'}
}
function Get-HostedSensitivityEngine([string]$TrustedSource){
 $inventory=Get-Content -LiteralPath (Join-Path $TrustedSource 'dependencies/radiance.json') -Raw|ConvertFrom-Json -Depth 40 -DateKind String
 if($inventory.tag -cne 'rad6R0P2' -or $inventory.sha256 -cne 'c4f02efcb4a33f4a43b7ce0ba58100544abc317a1be61f7b0425a64a66eb8a09'){throw 'Unexpected engine package identity.'}
 $expected=[ordered]@{'rtrace.exe'=$inventory.executables.rtrace.sha256;'oconv.exe'=$inventory.executables.oconv.sha256;'rcontrib.exe'=$inventory.annual_workflow_review.executables.rcontrib.sha256;'rayinit.cal'=$inventory.critical_libraries.'rayinit.cal'.sha256;'skybright.cal'=$inventory.critical_libraries.'skybright.cal'.sha256;'reinhart.cal'=$inventory.annual_workflow_review.additional_critical_libraries.'reinhart.cal'.sha256}
 $root=Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'
 foreach($name in $expected.Keys){$sub=if($name.EndsWith('.exe')){'bin'}else{'lib'};$file=Get-Item -LiteralPath (Join-Path $root "$sub/$name") -Force;if($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $file.Length -gt 67108864 -or $expected[$name] -cnotmatch '^[0-9a-f]{64}$' -or (Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant() -cne $expected[$name]){throw 'Installed pinned engine/library digest differs.'}}
 return $expected
}
if($FunctionsOnly){return}
Assert-HostedSensitivityCase $Alternative $IntervalIndex $Variant
$plan=Get-Content -LiteralPath "$PSScriptRoot/hosted-sensitivity-plan.json" -Raw|ConvertFrom-Json -Depth 60 -DateKind String
Assert-HostedSensitivityPlan $plan
$pinned=@($plan.archives|Where-Object{$_.alternative -ceq $Alternative})[0]
if(!$InputArchive -or !$OutputDirectory -or (Test-Path -LiteralPath $OutputDirectory)){throw 'Input archive and fresh output directory required.'}
. "$PSScriptRoot/prepare-sensitivity-sources.ps1"
$preparation=Prepare-SensitivitySources $SourceRoot $plan.source_sha256
$verifiedEngine=Get-HostedSensitivityEngine $SourceRoot
$manifest=Expand-SensitivityInputs $InputArchive (Join-Path $OutputDirectory 'inputs') $pinned $plan.source_sha256
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'trusted-source-preparation.json'),($preparation|ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))
$baselineRoot=Join-Path $OutputDirectory 'inputs/baseline'
$baseline=& { param($trusted,$root) . (Join-Path $trusted 'scripts/annual-collect-reference.ps1') -FunctionsOnly; Initialize-ReferenceCollector; Read-ReferenceBaseline $root } $SourceRoot $baselineRoot
Assert-HostedSensitivityBaseline $baseline $pinned
foreach($name in $verifiedEngine.Keys){if($baseline.declaration.engine_sha256.$name -cne $verifiedEngine[$name]){throw 'Qualified baseline engine differs from installed pinned engine.'}}
$receipt=[ordered]@{schema_version='greenlens.hosted-sensitivity-case.v1';started_utc=[DateTime]::UtcNow.ToString('o');alternative=$Alternative;interval_index=$IntervalIndex;variant=$Variant;fixed_full_plan=$plan.cases;archive_sha256=$pinned.sha256;manifest_sha256=$pinned.manifest_sha256;plan_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/hosted-sensitivity-plan.json").Hash.ToLowerInvariant();wrapper_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();source_sha256=$plan.source_sha256;baseline_verified=$true;engine_workers=1;process_timeout_seconds=7200;job_envelope_minutes=150;prepare_only=[bool]$PrepareOnly;completed=$false;variant_qualified=$false;reference_family_qualified=$false;release_accepted=$false}
$receipt['engine_sha256']=$verifiedEngine
try{
 $argsList=@('-NoProfile','-File',(Join-Path $SourceRoot 'scripts/annual-reference-sensitivity.ps1'),'-BaselineDirectory',$baselineRoot,'-OutputDirectory',(Join-Path $OutputDirectory 'sensitivity'),'-IntervalIndex',"$IntervalIndex",'-Variant',$Variant)
 if($PrepareOnly){$argsList+='-PrepareOnly'}
 & (Join-Path $PSHOME 'pwsh.exe') @argsList
 if($LASTEXITCODE -ne 0){throw "Sensitivity child failed with exit code $LASTEXITCODE; retained evidence is not complete."}
 if(!$PrepareOnly){
  $verified=& { param($trusted,$root,$context) . (Join-Path $trusted 'scripts/annual-collect-reference.ps1') -FunctionsOnly; Initialize-ReferenceCollector; Read-ReferenceSensitivity $root $context } $SourceRoot (Join-Path $OutputDirectory 'sensitivity') $baseline.contexts[0]
  if(!$verified.completed -or @($verified.executed_variant_names).Count -ne 1 -or $verified.executed_variant_names[0] -cne $Variant -or !$verified.qualified){throw 'Fresh strict sensitivity qualification failed.'}
  [IO.File]::WriteAllText((Join-Path $OutputDirectory 'strict-sensitivity-reader.json'),($verified|ConvertTo-Json -Depth 100),[Text.UTF8Encoding]::new($false))
  $receipt.completed=$true;$receipt.variant_qualified=$true
 }
}finally{$receipt['finished_utc']=[DateTime]::UtcNow.ToString('o');[IO.File]::WriteAllText((Join-Path $OutputDirectory 'case.json'),($receipt|ConvertTo-Json -Depth 60),[Text.UTF8Encoding]::new($false))}
$global:LASTEXITCODE=0
