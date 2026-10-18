#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. Build/native owner invokes explicitly.
# Observes an installed UI session; it never operates that UI or writes a project.
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$InstalledExe,
 [Parameter(Mandatory)][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$InstalledSha256,
 [Parameter(Mandatory)][string]$Project,
 [Parameter(Mandatory)][string]$OutputDirectory,
 [Parameter(Mandatory)][ValidatePattern('^[A-Fa-f0-9]{40}$')][string]$ExpectedSource,
 [Parameter(Mandatory)][string]$TypstDir,
 [Parameter(Mandatory)][string]$AllowedRuntimeIdentities
)
$ErrorActionPreference='Stop'
function Assert-FreshObserverHost {
 $loaded=@('GreenLensPairedProcess.NativeProcess','GreenLensObservation.HeldProcessImage','GreenLensReportObservation.Capture','GreenLensObservedJob.Capture')|Where-Object {$_ -as [type]}
 if($loaded){throw ('A fresh PowerShell process is required; loaded helper identities cannot be bound to current source bytes: '+($loaded -join ', '))}
}
Assert-FreshObserverHost
$scopeHelper=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'observed-scope-history.ps1')).Path
. $scopeHelper
$sessionHelper=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'observed-job-session.ps1')).Path
. $sessionHelper
$cache=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
$output=[IO.Path]::GetFullPath($OutputDirectory)
$projectRoot=(Resolve-Path -LiteralPath $Project).Path.TrimEnd('\','/')
if(!$output.StartsWith($cache+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or !$projectRoot.StartsWith($cache+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Use isolated project and receipt directories under LOCALAPPDATA/GreenLens.'}
if($output.StartsWith($projectRoot+'\',[StringComparison]::OrdinalIgnoreCase) -or $projectRoot.StartsWith($output+'\',[StringComparison]::OrdinalIgnoreCase) -or $output.Equals($projectRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Project and receipt directories must be separate.'}
if(Test-Path -LiteralPath $output){throw 'Receipt directory must be new; previous observations are immutable.'}
$exe=(Resolve-Path -LiteralPath $InstalledExe).Path
$typst=(Resolve-Path -LiteralPath (Join-Path $TypstDir 'typst.exe')).Path
$runtimeDeclaration=(Resolve-Path -LiteralPath $AllowedRuntimeIdentities).Path
$native=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'annual-paired-native-process.cs')).Path
$imageHelper=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'observed-process-image.cs')).Path
function File-Hash([string]$Path){
 if([string]::IsNullOrWhiteSpace($Path)){throw 'Cannot verify bytes without an observed nonempty file path.'}
 $stream=[IO.FileStream]::new($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
 try{[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()}finally{$stream.Dispose()}
}
if((File-Hash $exe) -ne $InstalledSha256.ToLowerInvariant()){throw 'Installed executable differs from the declared artifact.'}
$existing=@([Diagnostics.Process]::GetProcessesByName([IO.Path]::GetFileNameWithoutExtension($exe)))
try{if($existing.Count){throw 'An installed app instance is already running; preserve and close its document explicitly before supervision.'}}finally{foreach($p in $existing){$p.Dispose()}}
$identities=[ordered]@{source_commit_declared=$ExpectedSource.ToLowerInvariant();source_binding='Declared build receipt plus exact executable SHA256; source commit is not inferred from binary bytes';installed_executable_sha256=(File-Hash $exe);typst_executable_sha256=(File-Hash $typst);supervisor_sha256=(File-Hash $PSCommandPath);native_helper_sha256=(File-Hash $native);scope_history_helper_sha256=(File-Hash $scopeHelper);process_image_helper_sha256=(File-Hash $imageHelper);session_helper_sha256=(File-Hash $sessionHelper);runtime_declaration_sha256=(File-Hash $runtimeDeclaration)}
Add-Type -LiteralPath $native
Add-Type -LiteralPath $imageHelper
Initialize-ObservedOutputCapture
function Read-Json([string]$Path,[long]$Cap){
 $item=Get-Item -LiteralPath $Path -Force
 if($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -gt $Cap){throw 'Invalid or oversized JSON evidence/marker.'}
 # Share deletion so observing an old manifest handle cannot block the app's
 # atomic manifest replacement. Read the bounded bytes of this opened file.
 $stream=[IO.FileStream]::new($item.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
 try{if($stream.Length -gt $Cap){throw 'JSON grew beyond its bound.'};$bytes=[byte[]]::new([int]$stream.Length);$stream.ReadExactly($bytes,0,$bytes.Length);if($stream.ReadByte() -ne -1){throw 'JSON changed during its bounded read.'};$text=[Text.UTF8Encoding]::new($false,$true).GetString($bytes).TrimStart([char]0xFEFF);$parsed=ConvertFrom-Json -InputObject $text -AsHashtable -DateKind String -Depth 100 -NoEnumerate;return ,$parsed}finally{$stream.Dispose()}
}
function Stable-Value($Value){
 if($null -eq $Value){return ,$null}
 if($Value -is [Collections.IDictionary]){$r=[ordered]@{};$keys=[string[]]@($Value.Keys);[Array]::Sort($keys,[StringComparer]::Ordinal);foreach($k in $keys){$r[$k]=Stable-Value $Value[$k]};return $r}
 # Pipeline enumeration collapses empty/singleton arrays and can discard null
 # children. Add each recursive result as one value, retaining its JSON shape.
 if($Value -is [Collections.IEnumerable] -and $Value -isnot [string]){$r=[Collections.Generic.List[object]]::new();foreach($v in $Value){$child=Stable-Value -Value $v;$r.Add($child)};return ,$r.ToArray()}
 return ,$Value
}
function Json-Identity($Value){$text=ConvertTo-Json -InputObject (Stable-Value -Value $Value) -Compress -Depth 100;[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($text))).ToLowerInvariant()}
function Member-Path([string]$Root,[string]$Relative){
 if(!$Relative -or $Relative.Contains('\') -or $Relative.Contains(':') -or $Relative.StartsWith('/') -or @($Relative.Split('/')|Where-Object {!$_ -or $_ -in @('.','..')}).Count){throw 'Invalid portable member path.'}
 $value=$Root
 foreach($segment in $Relative.Split('/')){$value=Join-Path $value $segment;if(Test-Path -LiteralPath $value){$item=Get-Item -LiteralPath $value -Force;if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Reparse points are outside this evidence observation.'}}}
 $full=[IO.Path]::GetFullPath($value)
 if(!$full.StartsWith($Root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Portable member escaped its project.'}
 return $full
}
function Snapshot-Project {
 $files=[ordered]@{};$pending=[Collections.Generic.Stack[string]]::new();$pending.Push($projectRoot);[long]$bytes=0;$directories=0
 while($pending.Count){$dir=$pending.Pop();$directories++;if($directories -gt 5000){throw 'Project directory capacity exceeded.'};foreach($item in Get-ChildItem -LiteralPath $dir -Force){
  if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Project contains a reparse point.'}
  if($item.PSIsContainer){$pending.Push($item.FullName);continue}
  $relative=[IO.Path]::GetRelativePath($projectRoot,$item.FullName).Replace('\','/')
  if($relative -eq '.greenlens.lock'){continue}
  if($files.Count -ge 5000 -or $item.Length -gt 64MB){throw 'Project member capacity exceeded.'};$bytes+=$item.Length;if($bytes -gt 256MB){throw 'Project uncompressed capacity exceeded.'}
  $files[$relative]=[ordered]@{bytes=$item.Length;sha256=(File-Hash $item.FullName)}
 }}
 return [ordered]@{files=$files;member_count=$files.Count;total_bytes=$bytes}
}
function Assert-Preserved($Before,$After){foreach($name in $Before.files.Keys){if($name -eq 'project.json'){continue};if(!$After.files.Contains($name) -or (Json-Identity $After.files[$name]) -cne (Json-Identity $Before.files[$name])){throw "Previously retained member changed or disappeared: $name"}}}
function Issues($Manifest){if($Manifest.Contains('report_issues') -and $null -ne $Manifest.report_issues){return @($Manifest.report_issues.issues)};return @()}
function Assert-OldIssues($Before,$After){$current=@(Issues $After);foreach($old in @(Issues $Before)){$same=@($current|Where-Object {$_.id -ceq $old.id});if($same.Count -ne 1 -or (Json-Identity $same[0]) -cne (Json-Identity $old)){throw 'A historical issue summary changed or disappeared.'}}}
function Assert-NoNewRuns($Before,$After){
 if((Json-Identity $Before.runs) -cne (Json-Identity $After.runs) -or (Json-Identity $Before.energy.runs) -cne (Json-Identity $After.energy.runs) -or (Json-Identity $Before.climate.runs) -cne (Json-Identity $After.climate.runs)){throw 'Run inventory changed during a no-simulation report observation.'}
}
function Assert-ArmSelection($Candidate,$Manifest,[string]$ExpectedProjectId){
 if($Manifest.id -cne $ExpectedProjectId -or $Candidate.project_id -cne $ExpectedProjectId){throw 'The arm marker and saved manifest must identify the declared project.'}
 if(!$Candidate.baseline_run_id -or !$Candidate.proposed_run_id -or $Candidate.baseline_run_id -ceq $Candidate.proposed_run_id -or $Candidate.scope_revision_sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'The arm marker requires distinct run IDs and an exact scope revision identity.'}
 $baseline=@($Manifest.energy.runs|Where-Object {$_.id -ceq $Candidate.baseline_run_id})
 $proposed=@($Manifest.energy.runs|Where-Object {$_.id -ceq $Candidate.proposed_run_id})
 if($baseline.Count -ne 1 -or $proposed.Count -ne 1){throw 'Each armed run must occur exactly once in the saved energy run inventory.'}
 foreach($run in @($baseline[0],$proposed[0])){if(!$run.alternative_id -or $run.record_sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'An armed run lacks its saved design or record binding.'}}
 $scope=Get-ObservedScopeSelection $projectRoot $Manifest $Candidate.scope_revision_sha256
 # A historical scope is permitted. Do not substitute the active scope or
 # current designs; the later immutable issue must bind these retained runs.
 return [ordered]@{baseline=[ordered]@{run_id=$baseline[0].id;record_sha256=$baseline[0].record_sha256;design_id=$baseline[0].alternative_id};proposed=[ordered]@{run_id=$proposed[0].id;record_sha256=$proposed[0].record_sha256;design_id=$proposed[0].alternative_id};scope_revision_sha256=$scope.revision.revision_sha256;scope_schema_version=$scope.revision.schema_version;scope_storage_schema=$scope.storage_schema;scope_member_path=$scope.member_path;scope_member_bytes=$scope.member_bytes;scope_member_sha256=$scope.member_sha256;scope_observation=$scope.verification_scope}
}
function Assert-IssueProducts($Issue){
 if($Issue.id -cnotmatch '^[a-f0-9]{64}$' -or $Issue.path -cne ('report-issues/'+$Issue.id) -or !$Issue.include_pdf){throw 'Unexpected published issue identity/path/profile.'}
 $directory=Member-Path $projectRoot $Issue.path
 foreach($pair in @(@('issue-products.json','products_file_sha256'),@('issue-content.json','content_file_sha256'),@('issue-runtime.json','receipt_file_sha256'))){if((File-Hash (Member-Path $directory $pair[0])) -cne $Issue[$pair[1]]){throw 'Published issue metadata bytes do not bind the project summary.'}}
 $products=Read-Json (Member-Path $directory 'issue-products.json') 256KB
 if($products.schema_version -cne 'greenlens.report-issue-products.v1' -or $products.products_sha256 -cne $Issue.products_sha256 -or $products.content_sha256 -cne $Issue.content_sha256 -or $products.products.Count -lt 1 -or $products.products.Count -gt 252){throw 'Published product inventory does not bind its issue.'}
 $names=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase);[long]$total=0;$pdfs=0
 foreach($p in $products.products){if(!$names.Add($p.path) -or $p.bytes -le 0 -or $p.bytes -gt 64MB -or $p.sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'Invalid published product inventory member.'};$path=Member-Path $directory $p.path;$f=Get-Item -LiteralPath $path -Force;if($f.PSIsContainer -or $f.Length -ne $p.bytes -or (File-Hash $path) -cne $p.sha256){throw 'Published product bytes do not match the frozen inventory.'};$total+=$f.Length;if($p.path.EndsWith('.pdf',[StringComparison]::OrdinalIgnoreCase)){$pdfs++}}
 if($total -gt 256MB -or !$pdfs){throw 'Published product package is oversized or lacks its requested PDFs.'}
 return [ordered]@{id=$Issue.id;scope_revision_sha256=$Issue.scope_revision_sha256;baseline=$Issue.baseline;proposed=$Issue.proposed;product_count=$products.products.Count;product_bytes=$total;pdf_count=$pdfs;products_file_sha256=$Issue.products_file_sha256;check='Actual-byte inventory/hash verification only; Rust semantic/numerical inspection remains separate'}
}
function Normalize-ObservedImagePath([string]$Path){
 if([string]::IsNullOrWhiteSpace($Path)){throw 'Observed executable identity is unavailable; configured paths cannot substitute for it.'}
 if($Path.StartsWith('\\?\UNC\',[StringComparison]::OrdinalIgnoreCase)){$Path='\\'+$Path.Substring(8)}elseif($Path.StartsWith('\\?\',[StringComparison]::OrdinalIgnoreCase)){$Path=$Path.Substring(4)}
 if(![IO.Path]::IsPathFullyQualified($Path)){throw 'Executable identity requires a fully qualified observed path.'}
 return [IO.Path]::GetFullPath($Path)
}
function Read-AllowedRuntimeIdentities([string]$Path){
 $declaration=Read-Json $Path 64KB
 if($declaration.schema_version -cne 'greenlens.observer-runtime-allowlist.v1' -or $declaration.members -isnot [array] -or $declaration.members.Count -gt 8){throw 'Expected a bounded explicit runtime identity declaration.'}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase);$members=[Collections.Generic.List[object]]::new()
 foreach($member in $declaration.members){
  $path=Normalize-ObservedImagePath $member.image_path
  $expectedName=switch($member.role){'webview_runtime'{'msedgewebview2.exe'}'console_host'{'conhost.exe'}default{throw 'Only explicitly identified WebView runtime and canonical console host are supported auxiliary members.'}}
  if([IO.Path]::GetFileName($path) -ine $expectedName -or $member.sha256 -cnotmatch '^[a-f0-9]{64}$' -or !$seen.Add($path)){throw 'Invalid, misnamed or duplicate runtime identity.'}
  if($member.role -ceq 'console_host' -and !$path.Equals((Join-Path ([Environment]::GetFolderPath('System')) 'conhost.exe'),[StringComparison]::OrdinalIgnoreCase)){throw 'Console host must use the canonical operating-system path.'}
  if((File-Hash $path) -cne $member.sha256){throw 'Runtime executable bytes differ from the prospective declaration.'}
  $members.Add([ordered]@{role=$member.role;image_path=$path;sha256=$member.sha256})
 }
 return ,$members.ToArray()
}
function Assert-MemberPolicy($Member,$ExpectedApp,$ExpectedTypst,$AllowedRuntime){
 # Called outside the inspection-race catch: a policy failure remains a failure
 # even when the process exits immediately after its identity was observed.
 if($Member.name -imatch '^(energyplus|expandobjects|readvarseso)$'){throw 'Simulation-related executable observed during the no-engine protocol.'}
 $path=Normalize-ObservedImagePath $Member.image_path
 if($Member.image_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $Member.name -ine [IO.Path]::GetFileNameWithoutExtension($path)){throw 'Observed executable name/hash is not a valid kernel image identity.'}
 foreach($expected in @(@{role='pinned_application';image_path=$ExpectedApp.image_path;sha256=$ExpectedApp.sha256},@{role='pinned_typst';image_path=$ExpectedTypst.image_path;sha256=$ExpectedTypst.sha256})+@($AllowedRuntime)){
  if($path.Equals((Normalize-ObservedImagePath $expected.image_path),[StringComparison]::OrdinalIgnoreCase) -and $Member.image_sha256 -ceq $expected.sha256){return $expected.role}
 }
 throw 'An observed executable is outside the exact prospective application/Typst/runtime identity policy.'
}

$allowedRuntime=Read-AllowedRuntimeIdentities $runtimeDeclaration
$expectedApp=@{image_path=$exe;sha256=$identities.installed_executable_sha256};$expectedTypst=@{image_path=$typst;sha256=$identities.typst_executable_sha256}
$identities.allowed_runtime=$allowedRuntime
$manifestPath=Join-Path $projectRoot 'project.json'
$initial=Read-Json $manifestPath 16MB;$before=Snapshot-Project
if(!$initial.id -or !$initial.energy){throw 'A saved energy project is required.'}
$initialIssues=@(Issues $initial)
if($initialIssues.Count -ne 1){throw 'This declared continuation requires exactly one prior issue; do not prune or substitute another project.'}
New-Item -ItemType Directory -Path $output|Out-Null
$before|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $output 'before-members.json') -Encoding utf8NoBOM
$protocol=[ordered]@{schema_version='greenlens.native-report-review-supervision.v1';declared_utc=[DateTime]::UtcNow.ToString('o');identities=$identities;project_id=$initial.id;initial_manifest_sha256=$before.files['project.json'].sha256;initial_issue_ids=@($initialIssues.id);expected_final_issue_count=2;kernel_job_commit_limit_bytes=1GB;acceptance_raw_peak_limit_bytes=2GB;active_process_limit=16;overall_deadline_seconds=3600;armed_report_deadline_seconds=600;normal_close_grace_seconds=30;sample_milliseconds=200;manifest_poll_milliseconds=1000;maximum_samples=18000;stdout_cap_bytes=1MB;stderr_cap_bytes=1MB;maximum_observed_identities=4096;receipt_cap_bytes=64MB;success_receipt_commit='Only the final receipt commits acceptance; a summary or .writing file alone does not';scope='Installed UI and one manually armed report issuance; no GUI commands, project mutations, simulation launches or automatic retry';historical_failures='The384 MiB CLR and native raw-peak tests remain failed. The instrumented native request was denied and remained reserved while raw peak increased by the attempted160 MiB. A stricter1 GiB kernel limit here is separate from the unchanged2 GiB acceptance ceiling, not a reinterpretation or converter qualification.';publication_check='Exactly one new issue appended after arming; selected pair/scope, actual products and prior member hashes checked. Neither successful rendering nor these checks establish scientific qualification.';unobserved_children='Polling can miss lifetimes entirely; kernel peak remains recorded. Every enumerated member requires a held PID/creation-bound executable identity; unknown membership fails explicitly. Missing pinned Typst observation also prevents success.'}
$protocol|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $output 'protocol.json') -Encoding utf8NoBOM
$protocolHash=File-Hash (Join-Path $output 'protocol.json')
$armTemplate=[ordered]@{schema_version='greenlens.native-report-arm.v1';protocol_sha256=$protocolHash;project_id=$initial.id;baseline_run_id='REPLACE';proposed_run_id='REPLACE';scope_revision_sha256='REPLACE';expected_new_issue_count=1}
$armTemplate|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $output 'arm-render.template.json') -Encoding utf8NoBOM
@{schema_version='greenlens.native-report-stop.v1';protocol_sha256=$protocolHash;reason='Finished native review; closing normally';report_error=$null}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $output 'stop-observation.template.json') -Encoding utf8NoBOM
Write-Host "Root UI owner: open the declared project manually. Write arm-render.json from the template immediately before one Issue action. After review, write stop-observation.json then close GreenLens normally. Receipt directory: $output"
$session=$null;$proc=$null;$stdout=[GreenLensObservedJob.Capture]::new();$stderr=[GreenLensObservedJob.Capture]::new();$outTask=$null;$errTask=$null
$timer=[Diagnostics.Stopwatch]::StartNew();$seen=@{};$samples=[Collections.Generic.List[object]]::new();$arm=$null;$armedAt=$null;$armedBefore=$null;$armedManifest=$null;$published=$null;$publishedSummary=$null;$publishedMembers=$null;$stop=$null;$stopAt=$null;$nextManifest=0L;$lastManifestHash=$null;$typstCount=0
$record=[ordered]@{schema_version=$protocol.schema_version;protocol_sha256=$protocolHash;identities=$identities;completed=$false;failure=$null;stage='startup-and-editing';raw_peak_max_bytes=0;sampled_private_max_bytes=0;member_count_max=0;energyplus_observed=$false;normal_close=$false;cleanup_job_empty=$false;initial_issue_ids=@($initialIssues.id)}
try{
 $proc=[GreenLensPairedProcess.NativeProcess]::StartBounded($exe,@(),(Split-Path -Parent $exe),'.',1GB,16);$proc.Input.Close();$record.root_pid=$proc.Id
 @{stage='startup-and-editing';root_pid=$proc.Id;protocol_sha256=$protocolHash}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $output 'state.json') -Encoding utf8NoBOM
 $outTask=$stdout.Drain($proc.Output);$errTask=$stderr.Drain($proc.Error)
 $session=New-ObservedJobSession $proc $record {param($row)
  if($row.name -imatch '^(energyplus|expandobjects|readvarseso)$'){$record.energyplus_observed=$true}
  return (Assert-MemberPolicy $row $expectedApp $expectedTypst $allowedRuntime)
 } $stdout $stderr $outTask $errTask
 $seen=$session.Seen
 while($true){
  $elapsed=$timer.ElapsedMilliseconds
  $sample=Get-ObservedJobSample $session $elapsed 3600000 18000
  $sample.stage=$record.stage;$samples.Add($sample)
  $typstCount=[int]$session.PolicyCounts['pinned_typst']
  $stopPath=Join-Path $output 'stop-observation.json'
  if(!$stop -and (Test-Path -LiteralPath $stopPath)){$stop=Read-Json $stopPath 16KB;if($stop.schema_version -cne 'greenlens.native-report-stop.v1' -or $stop.protocol_sha256 -cne $protocolHash -or !$stop.reason){throw 'Invalid observation-stop marker.'};$record.stop=$stop;$stopAt=$elapsed;if($stop.report_error){throw ('UI owner reported render/review failure: '+[string]$stop.report_error)}}
  $armPath=Join-Path $output 'arm-render.json'
  if(!$arm -and (Test-Path -LiteralPath $armPath)){
   $candidate=Read-Json $armPath 16KB
   if($candidate.schema_version -cne 'greenlens.native-report-arm.v1' -or $candidate.protocol_sha256 -cne $protocolHash -or $candidate.project_id -cne $initial.id -or $candidate.scope_revision_sha256 -cnotmatch '^[a-f0-9]{64}$' -or !$candidate.baseline_run_id -or !$candidate.proposed_run_id -or $candidate.baseline_run_id -ceq $candidate.proposed_run_id -or $candidate.expected_new_issue_count -ne 1){throw 'Invalid exact-pair/scope arm marker.'}
   $armObservedAt=$elapsed;$armManifestHash=File-Hash $manifestPath
   $armedManifest=Read-Json $manifestPath 16MB;Assert-OldIssues $initial $armedManifest;Assert-NoNewRuns $initial $armedManifest
   if(@(Issues $armedManifest).Count -ne 1){throw 'An issue appeared before the explicit arm boundary.'}
   $armSelection=Assert-ArmSelection $candidate $armedManifest $initial.id
   $armedBefore=Snapshot-Project;Assert-Preserved $before $armedBefore
   if($armedBefore.files['project.json'].sha256 -cne $armManifestHash -or (File-Hash $manifestPath) -cne $armManifestHash){throw 'The saved manifest changed during arm preflight; the observation was not armed.'}
   $armedAt=$armObservedAt;$record.stage='armed-report';$arm=$candidate;$record.arm=$arm;$record.arm_selection=$armSelection;$record.armed_utc=[DateTime]::UtcNow.ToString('o')
   $armedBefore|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $output 'armed-members.json') -Encoding utf8NoBOM
   $record.armed_manifest_sha256=$armedBefore.files['project.json'].sha256
   @{stage='armed-report';root_pid=$proc.Id;armed_utc=$record.armed_utc;arm=$arm;manifest_sha256=$record.armed_manifest_sha256}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $output 'armed.json') -Encoding utf8NoBOM
   Write-Host 'Exact pair/scope observation armed; the600-second window is running. Root may perform the single Issue action now.'
  }
  if($arm -and !$published){Assert-ObservedElapsed $session ($timer.ElapsedMilliseconds-$armedAt) 600000 'armed issuance'}
  if($elapsed -ge $nextManifest -or ($stop -and $proc.HasExited)){
   $nextManifest=$elapsed+1000;$current=Read-Json $manifestPath 16MB;$lastManifestHash=File-Hash $manifestPath
   if($current.id -cne $initial.id){throw 'Observed project identity changed.'};Assert-OldIssues $initial $current;Assert-NoNewRuns $initial $current
   $all=@(Issues $current);$new=@($all|Where-Object {$_.id -cnotin @($initialIssues.id)})
   if($all.Count -gt 2 -or $new.Count -gt 1 -or (!$arm -and $new.Count)){throw 'Unexpected or unarmed additional issue publication.'}
   if($published -and ($new.Count -ne 1 -or (Json-Identity $new[0]) -cne (Json-Identity $publishedSummary))){throw 'The newly published issue summary changed during review.'}
   if($new.Count -eq 1 -and !$published){
    if($all.Count -ne 2 -or $new[0].scope_revision_sha256 -cne $arm.scope_revision_sha256 -or (Json-Identity $new[0].baseline) -cne (Json-Identity $armSelection.baseline) -or (Json-Identity $new[0].proposed) -cne (Json-Identity $armSelection.proposed)){throw 'Published issue does not bind the explicitly armed scope/pair and retained record identities.'}
    $published=Assert-IssueProducts $new[0];$publishedSummary=$new[0];$after=Snapshot-Project;Assert-Preserved $before $after;Assert-Preserved $armedBefore $after;$publishedMembers=$after
    $after|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $output 'published-members.json') -Encoding utf8NoBOM
    $record.publication=$published;$record.publication_elapsed_ms=$timer.ElapsedMilliseconds-$armedAt;$record.published_utc=[DateTime]::UtcNow.ToString('o');$record.stage='published-native-review'
    @{stage='published-native-review';root_pid=$proc.Id;publication=$published;elapsed_ms=$record.publication_elapsed_ms}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $output 'state.json') -Encoding utf8NoBOM
    Assert-ObservedElapsed $session $record.publication_elapsed_ms 600000 'complete publication verification'
   }
  }
  if($stop -and $proc.HasExited -and $proc.JobEmpty){$record.normal_close=$true;break}
  if($proc.HasExited -and !$stop){throw 'Installed application exited before an explicit stop/normal-close observation.'}
  if($stop -and ($elapsed-$stopAt) -gt 30000){throw 'Normal-close grace expired; terminating only the owned job.'}
  Start-Sleep -Milliseconds 200
 }
 if($record.energyplus_observed){throw 'A forbidden simulation-related process was observed; the report observation cannot pass.'}
 if(!$arm -or !$published -or !$typstCount){throw 'Observation lacks an armed publication or an observed pinned Typst process.'}
 $final=Read-Json $manifestPath 16MB;Assert-OldIssues $initial $final;Assert-NoNewRuns $initial $final
 if(@(Issues $final).Count -ne 2){throw 'Final issue inventory differs from the declared two-issue result.'}
 $finalIssue=@((Issues $final)|Where-Object {$_.id -ceq $published.id})
 if($finalIssue.Count -ne 1 -or (Json-Identity $finalIssue[0]) -cne (Json-Identity $publishedSummary)){throw 'Final report issue differs from the observed publication.'}
 $finalMembers=Snapshot-Project;Assert-Preserved $before $finalMembers;Assert-Preserved $armedBefore $finalMembers;Assert-Preserved $publishedMembers $finalMembers
 $lastManifestHash=$finalMembers.files['project.json'].sha256
 $finalMembers|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $output 'final-members.json') -Encoding utf8NoBOM
 if((File-Hash $exe) -cne $identities.installed_executable_sha256 -or (File-Hash $typst) -cne $identities.typst_executable_sha256 -or (File-Hash $native) -cne $identities.native_helper_sha256 -or (File-Hash $scopeHelper) -cne $identities.scope_history_helper_sha256 -or (File-Hash $imageHelper) -cne $identities.process_image_helper_sha256 -or (File-Hash $sessionHelper) -cne $identities.session_helper_sha256 -or (File-Hash $PSCommandPath) -cne $identities.supervisor_sha256 -or (File-Hash $runtimeDeclaration) -cne $identities.runtime_declaration_sha256){throw 'Declared tooling changed during observation.'}
 foreach($runtime in $allowedRuntime){if((File-Hash $runtime.image_path) -cne $runtime.sha256){throw 'A declared runtime executable changed during observation.'}}
 Assert-ObservedElapsed $session $timer.ElapsedMilliseconds 3600000 'final completion'
 $record.completed=$true
}catch{$record.failure=$_.Exception.Message;$record.failure_detail=@{exception_type=$_.Exception.GetType().FullName;category=$_.CategoryInfo.Category.ToString();script_stack_trace=$_.ScriptStackTrace;position=$_.InvocationInfo.PositionMessage}}
finally{
 if($session){Close-ObservedJobSession $session;Save-ObservedJobOutput $session $output}
 elseif($proc){try{if(!$proc.JobEmpty){$proc.Kill()}}finally{$proc.Dispose()};$record.completed=$false}
 if($stdout.Overflow -or $stderr.Overflow -or $record.energyplus_observed){$record.completed=$false}
 $record.samples=$samples.ToArray();$record.members=@($seen.Values|Sort-Object first_observed_utc);$record.observed_typst_processes=$typstCount;$record.elapsed_seconds=$timer.Elapsed.TotalSeconds;$record.final_manifest_sha256=$lastManifestHash;$record.completed_utc=[DateTime]::UtcNow.ToString('o')
 Write-ObservedJobReceipt $session (Join-Path $output 'report.json') $record $output
}
if(!$record.completed){throw "Native report observation incomplete/failed; retained receipt: $output; $($record.failure)"}
Write-Host "One native issued report observed with prior retained bytes preserved. Receipt: $output"
