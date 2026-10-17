#Requires -Version 7.5
# Original GreenLens source, Apache-2.0. The coordinated native owner explicitly
# invokes this one-attempt protocol. No retry/resume, conversion, or simulation.
[CmdletBinding(DefaultParameterSetName='Run')]
param(
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Cli,
 [Parameter(Mandatory,ParameterSetName='Run')][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$CliSha256,
 [Parameter(Mandatory,ParameterSetName='Run')][ValidatePattern('^[A-Fa-f0-9]{40}$')][string]$ExpectedSource,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Project,
 [Parameter(Mandatory,ParameterSetName='Run')][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$ExpectedManifestSha256,
 [Parameter(Mandatory,ParameterSetName='Run')][ValidatePattern('^[a-f0-9]{64}$')][string]$IssueId,
 [Parameter(Mandatory,ParameterSetName='Run')][ValidatePattern('^[a-f0-9]{64}$')][string]$ExpectedContentSha256,
 [Parameter(Mandatory,ParameterSetName='Run')][ValidatePattern('^[a-f0-9]{64}$')][string]$ExpectedProductsSha256,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$TypstDir,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$OutputDirectory,
 [Parameter(ParameterSetName='Run')][string[]]$ColdUnavailablePaths=@(),
 [Parameter(Mandatory,ParameterSetName='Helpers')][switch]$FunctionsOnly
)
$ErrorActionPreference='Stop'
$scopeHelper=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'observed-scope-history.ps1')).Path
. $scopeHelper

function Read-BoundedJson([string]$Path,[long]$Cap){
 $item=Get-Item -LiteralPath $Path -Force
 if($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -gt $Cap){throw 'Invalid or oversized JSON evidence.'}
 $stream=[IO.FileStream]::new($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try{if($stream.Length -gt $Cap){throw 'JSON grew beyond its bound.'};$bytes=[byte[]]::new([int]$stream.Length);$stream.ReadExactly($bytes,0,$bytes.Length);if($stream.ReadByte() -ne -1){throw 'JSON changed during its read.'};[Text.UTF8Encoding]::new($false,$true).GetString($bytes).TrimStart([char]0xFEFF)|ConvertFrom-Json -AsHashtable -DateKind String -Depth 100}finally{$stream.Dispose()}
}
function File-Hash([string]$Path){
 $stream=[IO.FileStream]::new($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try{[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()}finally{$stream.Dispose()}
}
function Portable-Child([string]$Root,[string]$Relative){
 if(!$Relative -or $Relative.Contains('\') -or $Relative.Contains(':') -or $Relative.StartsWith('/') -or @($Relative.Split('/')|Where-Object {!$_ -or $_ -in @('.','..')}).Count){throw 'Invalid portable member path.'}
 $value=$Root
 foreach($part in $Relative.Split('/')){$value=Join-Path $value $part;if(Test-Path -LiteralPath $value){$item=Get-Item -LiteralPath $value -Force;if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Reparse points are outside this protocol.'}}}
 $full=[IO.Path]::GetFullPath($value)
 if(!$full.StartsWith($Root.TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Member escaped its retained directory.'}
 return $full
}
function Assert-NoReparseAncestors([string]$Path){
 $current=[IO.Path]::GetFullPath($Path)
 while($current){if(Test-Path -LiteralPath $current){if((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'A protocol path traverses a reparse point.'}};$parent=[IO.Path]::GetDirectoryName($current);if($parent -eq $current){break};$current=$parent}
}
function Snapshot-Members([string]$Root){
 $files=[ordered]@{};$pending=[Collections.Generic.Stack[string]]::new();$pending.Push($Root);[long]$bytes=0;$directories=0
 while($pending.Count){$dir=$pending.Pop();$directories++;if($directories -gt 5000){throw 'Directory capacity exceeded.'};foreach($item in Get-ChildItem -LiteralPath $dir -Force){
  if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Evidence contains a reparse point.'}
  if($item.PSIsContainer){$pending.Push($item.FullName);continue}
  $relative=[IO.Path]::GetRelativePath($Root,$item.FullName).Replace('\','/')
  # The cooperative writer lock is ephemeral, not retained evidence. This
  # command must nevertheless preserve every other file, including project.json.
  if($relative -eq '.greenlens.lock'){continue}
  if($files.Count -ge 5000 -or $item.Length -gt 64MB){throw 'Member capacity exceeded.'};$bytes+=$item.Length;if($bytes -gt 256MB){throw 'Uncompressed evidence capacity exceeded.'}
  $files[$relative]=[ordered]@{bytes=[long]$item.Length;sha256=(File-Hash $item.FullName)}
 }}
 return [ordered]@{files=$files;member_count=$files.Count;total_bytes=$bytes}
}
function Assert-SameMembers($Before,$After){
 if($Before.member_count -ne $After.member_count -or $Before.total_bytes -ne $After.total_bytes){throw 'Retained member count or total size changed.'}
 foreach($name in $Before.files.Keys){if(!$After.files.Contains($name) -or $After.files[$name].bytes -ne $Before.files[$name].bytes -or $After.files[$name].sha256 -cne $Before.files[$name].sha256){throw "Retained member changed or disappeared: $name"}}
}
function Normalize-ImagePath([string]$Path){
 if($Path.StartsWith('\\?\UNC\',[StringComparison]::OrdinalIgnoreCase)){return '\\'+$Path.Substring(8)}
 if($Path.StartsWith('\\?\',[StringComparison]::OrdinalIgnoreCase)){return $Path.Substring(4)}
 return $Path
}
function Assert-ImageIdentity([string]$Name,[string]$Hash,[string]$Image,[string]$ExpectedCliName,[string]$ExpectedCliHash,[string]$ExpectedTypstHash,[string]$ConhostPath,[string]$ConhostHash){
 if($Name -imatch '^(energyplus|expandobjects|readvarseso)$'){throw 'Simulation executable observed in the no-engine protocol.'}
 if($Name -ieq $ExpectedCliName -and $Hash -ceq $ExpectedCliHash){return 'pinned_cli'}
 if($Name -ieq 'typst' -and $Hash -ceq $ExpectedTypstHash){return 'pinned_typst'}
 if($Name -ieq 'conhost' -and $Hash -ceq $ConhostHash -and (Normalize-ImagePath $Image).Equals((Normalize-ImagePath $ConhostPath),[StringComparison]::OrdinalIgnoreCase)){return 'system_console_host'}
 throw 'An observed process is not the pinned CLI/Typst or canonical operating-system console host.'
}
function Assert-Products([string]$Root,$Manifest){
 if($Manifest.schema_version -cne 'greenlens.report-issue-products.v1' -or $Manifest.products.Count -lt 1 -or $Manifest.products.Count -gt 252){throw 'Invalid issued product inventory.'}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase);[long]$total=0;$pdfs=0
 foreach($p in $Manifest.products){
  if(!$seen.Add($p.path) -or $p.bytes -le 0 -or $p.bytes -gt 64MB -or $p.sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'Invalid or duplicate product.'}
  $path=Portable-Child $Root $p.path;$item=Get-Item -LiteralPath $path -Force
  if($item.PSIsContainer -or $item.Length -ne $p.bytes -or (File-Hash $path) -cne $p.sha256){throw "Product bytes differ: $($p.path)"}
  $total+=$p.bytes;if($total -gt 256MB){throw 'Product inventory exceeds the package bound.'};if($p.path.EndsWith('.pdf',[StringComparison]::OrdinalIgnoreCase)){$pdfs++}
 }
 if(!$pdfs){throw 'The declared PDF issue has no PDF products.'}
 return [ordered]@{products=$Manifest.products;product_count=$Manifest.products.Count;product_bytes=$total;pdf_count=$pdfs}
}
function Write-Receipt([string]$Path,$Value){$Value|ConvertTo-Json -Depth 24|Set-Content -LiteralPath $Path -Encoding utf8NoBOM}
function Assert-RegenerationDeadline([double]$ElapsedSeconds){
 if(![double]::IsFinite($ElapsedSeconds) -or $ElapsedSeconds -lt 0 -or $ElapsedSeconds -gt 600){throw 'The completed regeneration exceeded its fixed600-second deadline.'}
}
if($FunctionsOnly){return}

$cache=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens')).TrimEnd('\','/')
$root=(Resolve-Path -LiteralPath $Project).Path.TrimEnd('\','/');$output=[IO.Path]::GetFullPath($OutputDirectory).TrimEnd('\','/')
foreach($path in @($root,$output)){if(!$path.StartsWith($cache+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Project and output must be isolated below LOCALAPPDATA/GreenLens.'};Assert-NoReparseAncestors $path}
if($root.Equals($output,[StringComparison]::OrdinalIgnoreCase) -or $root.StartsWith($output+'\',[StringComparison]::OrdinalIgnoreCase) -or $output.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Project and receipt/output directories must be separate.'}
if(Test-Path -LiteralPath $output){throw 'Output directory already exists; no overwrite, retry, or resume is allowed.'}
$destination=Join-Path $output 'regenerated'
$cliPath=(Resolve-Path -LiteralPath $Cli).Path;$typst=(Resolve-Path -LiteralPath (Join-Path $TypstDir 'typst.exe')).Path
$native=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'annual-paired-native-process.cs')).Path
$typstPinPath=Join-Path $PSScriptRoot '../dependencies/typst.json';$typstPin=Read-BoundedJson $typstPinPath 64KB
$typstPinEntry=$typstPin.members['typst.exe']
if(!$typstPinEntry -or $typstPinEntry.sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'Pinned Typst dependency identity is unavailable.'}
$typstHash=File-Hash $typst
if((File-Hash $cliPath) -cne $CliSha256.ToLowerInvariant() -or $typstHash -cne $typstPinEntry.sha256 -or (Get-Item -LiteralPath $typst).Length -ne $typstPinEntry.bytes){throw 'CLI or Typst differs from its declared pinned artifact.'}
$conhost=Join-Path ([Environment]::GetFolderPath('System')) 'conhost.exe';$conhostHash=File-Hash $conhost
foreach($path in $ColdUnavailablePaths){if(Test-Path -LiteralPath $path){throw 'A declared unavailable original path is still available.'}}
$before=Snapshot-Members $root
if(!$before.files.Contains('project.json') -or $before.files['project.json'].sha256 -cne $ExpectedManifestSha256.ToLowerInvariant()){throw 'Saved project differs from the explicitly declared manifest.'}
$projectJson=Read-BoundedJson (Join-Path $root 'project.json') 16MB
$issues=@($projectJson.report_issues.issues|Where-Object {$_.id -ceq $IssueId})
if($issues.Count -ne 1){throw 'Exactly one retained issue must match the declaration.'};$issue=$issues[0]
if($issue.path -cne ('report-issues/'+$IssueId) -or !$issue.include_pdf -or $issue.content_sha256 -cne $ExpectedContentSha256 -or $issue.products_sha256 -cne $ExpectedProductsSha256){throw 'Retained issue does not bind the declared content/products/PDF selection.'}
$scopeObservation=Get-ObservedScopeSelection $root $projectJson $issue.scope_revision_sha256
$issueDir=Portable-Child $root $issue.path
foreach($pair in @(@('issue-content.json','content_file_sha256'),@('issue-products.json','products_file_sha256'),@('issue-runtime.json','receipt_file_sha256'))){if((File-Hash (Portable-Child $issueDir $pair[0])) -cne $issue[$pair[1]]){throw 'Retained issue metadata hash differs from the project binding.'}}
$content=Read-BoundedJson (Portable-Child $issueDir 'issue-content.json') 48MB
if($content.renderer.method -cne 'greenlens.energy-report-issue-renderer/3'){throw 'This single continuation is declared for the new renderer/3 issue only.'}
if($content.scope_revision.revision_sha256 -cne $scopeObservation.revision.revision_sha256){throw 'The retained issue scope differs from its history selection.'}
$products=Read-BoundedJson (Portable-Child $issueDir 'issue-products.json') 256KB
if($products.content_sha256 -cne $ExpectedContentSha256 -or $products.products_sha256 -cne $ExpectedProductsSha256){throw 'Issued product manifest does not match the declared bindings.'}
$productCheck=Assert-Products $issueDir $products
$arguments=@('report-issue','regenerate',$root,$IssueId,$destination,'--typst-dir',(Split-Path -Parent $typst))
$identities=[ordered]@{source_commit_declared=$ExpectedSource.ToLowerInvariant();source_binding='Declared source/build receipt and exact CLI SHA256; source is not inferred from executable bytes';cli_sha256=$CliSha256.ToLowerInvariant();typst_sha256=$typstHash;typst_dependency_sha256=(File-Hash $typstPinPath);supervisor_sha256=(File-Hash $PSCommandPath);native_helper_sha256=(File-Hash $native);scope_history_helper_sha256=(File-Hash $scopeHelper);conhost_sha256=$conhostHash}
New-Item -ItemType Directory -Path $output|Out-Null
$coldRuntime=Join-Path $output 'cold-runtime';$coldRoaming=Join-Path $output 'cold-roaming';$coldTemp=Join-Path $output 'cold-temp'
foreach($path in @($coldRuntime,$coldRoaming,$coldTemp)){New-Item -ItemType Directory -Path $path|Out-Null}
$childEnvironment=[ordered]@{LOCALAPPDATA=$coldRuntime;APPDATA=$coldRoaming;TEMP=$coldTemp;TMP=$coldTemp;GREENLENS_ENERGYPLUS_DIR=(Join-Path $output 'unavailable-energyplus');GREENLENS_RADIANCE=(Join-Path $output 'unavailable-radiance');GREENLENS_TYPST_DIR=(Split-Path -Parent $typst);PATH=([Environment]::GetFolderPath('System')+';'+[Environment]::GetFolderPath('Windows'))}
$parentEnvironment=@{};foreach($key in $childEnvironment.Keys){$parentEnvironment[$key]=[Environment]::GetEnvironmentVariable($key,'Process')}
Write-Receipt (Join-Path $output 'before-members.json') $before
$protocol=[ordered]@{schema_version='greenlens.report-review-cold-regeneration.v1';declared_utc=[DateTime]::UtcNow.ToString('o');identities=$identities;project_id=$projectJson.id;manifest_sha256=$ExpectedManifestSha256.ToLowerInvariant();issue=$issue;arguments=$arguments;cold_unavailable_paths=$ColdUnavailablePaths;child_environment=$childEnvironment;cold_runtime_initial_entries=@(Get-ChildItem -LiteralPath $coldRuntime -Force).Count;attempt_limit=1;deadline_seconds=600;kernel_job_commit_limit_bytes=1GB;acceptance_raw_peak_limit_bytes=2GB;active_process_limit=16;sample_milliseconds=200;maximum_samples=3002;stdout_cap_bytes=1MB;stderr_cap_bytes=1MB;scope='One CLI regeneration only; fresh owned LOCALAPPDATA/APPDATA/TEMP, explicitly absent engine paths, OS-only PATH and explicit pinned Typst. No source hiding, input edit, new issue, simulation, retry, or historical mutation';process_observation='Only pinned CLI/Typst and canonical System32 conhost may be observed. Exited-before-inspection races remain disclosed. Kernel job peak includes all members; no peak normalization or denied-allocation subtraction.';failure_policy='Any substantive failure ends this attempt. Preserve logs, partial outputs and normal CLI scratch; never relaunch automatically.';claim_limit='Exact deterministic output agreement and retained-byte preservation are reproducibility checks, not scientific qualification.'}
$protocol.scope_selection=[ordered]@{revision_sha256=$scopeObservation.revision.revision_sha256;storage_schema=$scopeObservation.storage_schema;member_path=$scopeObservation.member_path;member_bytes=$scopeObservation.member_bytes;member_sha256=$scopeObservation.member_sha256;verification_scope=$scopeObservation.verification_scope}
Write-Receipt (Join-Path $output 'protocol.json') $protocol
$protocolHash=File-Hash (Join-Path $output 'protocol.json')

if(!('GreenLensPairedProcess.NativeProcess' -as [type])){Add-Type -LiteralPath $native}
if(!('GreenLensColdRegeneration.Capture' -as [type])){Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Threading.Tasks;
namespace GreenLensColdRegeneration {
 public sealed class Capture {
  readonly byte[] bytes=new byte[1048576]; readonly object gate=new object(); int retained; long total; bool overflow;
  public long Total { get { lock(gate) return total; } }
  public bool Overflow { get { lock(gate) return overflow; } }
  public byte[] Bytes { get { lock(gate) { var result=new byte[retained]; Array.Copy(bytes,result,retained); return result; } } }
  public async Task Drain(Stream source) {
   var buffer=new byte[65536]; int count;
   while((count=await source.ReadAsync(buffer,0,buffer.Length).ConfigureAwait(false))!=0) {
    lock(gate) { total=checked(total+count); int keep=Math.Min(count,bytes.Length-retained);
     if(keep>0) { Array.Copy(buffer,0,bytes,retained,keep); retained+=keep; }
     if(keep<count) overflow=true;
    }
   }
  }
 }
}
'@}
$proc=$null;$stdout=[GreenLensColdRegeneration.Capture]::new();$stderr=[GreenLensColdRegeneration.Capture]::new();$outTask=$null;$errTask=$null
$seen=@{};$samples=[Collections.Generic.List[object]]::new();$timer=[Diagnostics.Stopwatch]::new();$commandPassed=$false;$typstCount=0
$record=[ordered]@{schema_version=$protocol.schema_version;protocol_sha256=$protocolHash;identities=$identities;attempt=1;completed=$false;failure=$null;cleanup_errors=@();preservation_errors=@();raw_peak_max_bytes=0;sampled_private_max_bytes=0;member_count_max=0;energyplus_observed=$false;cleanup_job_empty=$false;original_members_preserved=$false;products_identical=$false}
# Persist allowance consumption before the only process creation. No resume path.
Write-Receipt (Join-Path $output 'attempt-started.json') @{attempt=1;protocol_sha256=$protocolHash;started_utc=[DateTime]::UtcNow.ToString('o')}
try{
 foreach($key in $childEnvironment.Keys){[Environment]::SetEnvironmentVariable($key,$childEnvironment[$key],'Process')}
 $timer.Start();$proc=[GreenLensPairedProcess.NativeProcess]::StartBounded($cliPath,$arguments,$output,'.',1GB,16);$proc.Input.Close();$record.root_pid=$proc.Id
 $outTask=$stdout.Drain($proc.Output);$errTask=$stderr.Drain($proc.Error)
 while($true){
  $elapsed=$timer.ElapsedMilliseconds
  if($elapsed -gt 600000 -or $samples.Count -ge 3002){throw 'The single regeneration exceeded its fixed deadline/sample bound.'}
  if($stdout.Overflow -or $stderr.Overflow){throw 'A process stream exceeded its fixed1 MiB bound.'}
  if($outTask.IsFaulted -or $errTask.IsFaulted){throw 'A bounded process-output drain failed.'}
  $limits=@($proc.CommittedMemoryLimitSnapshot)
  if($limits.Count -ne 4 -or ($limits[0] -band 0x2208) -ne 0x2208 -or $limits[1] -ne 16 -or $limits[2] -ne 1GB){throw 'Actual kernel Job limits differ from the declared stricter enforcement.'}
  $record.raw_peak_max_bytes=[Math]::Max([long]$record.raw_peak_max_bytes,[long]$limits[3]);if($limits[3] -gt 2GB){throw 'Raw kernel Job peak exceeded2 GiB; no normalization is permitted.'}
  $members=@($proc.MemberIds);$active=$proc.ActiveProcesses;$working=$proc.WorkingSetBytes;[long]$private=0;$observed=[Collections.Generic.List[object]]::new()
  if($active -gt 16 -or $members.Count -gt 16){throw 'Job process count exceeded16.'}
  foreach($member in $members){
   $p=$null;$row=$null
   try{$p=[Diagnostics.Process]::GetProcessById([int]$member);$p.Refresh();$started=$p.StartTime.ToUniversalTime();$key=[string]$member+':'+$started.Ticks;$name=$p.ProcessName;$image=$p.MainModule.FileName;$privateBytes=[long]$p.PrivateMemorySize64;$private+=$privateBytes;$observed.Add(@{pid=$member;name=$name;private_bytes=$privateBytes})
    if(!$seen.ContainsKey($key)){if(!$image){throw 'Live process image identity is unavailable.'};$row=[ordered]@{pid=$member;name=$name;image_path=$image;started_utc=$started.ToString('o');first_observed_utc=[DateTime]::UtcNow.ToString('o');image_sha256=$null;policy=$null};$seen[$key]=$row}
   }catch [ArgumentException]{$observed.Add(@{pid=$member;identity_unavailable='Exited between Job membership and process inspection'})}catch{if($p -and $p.HasExited){$observed.Add(@{pid=$member;identity_unavailable='Exited during process inspection';inspection_error=$_.Exception.Message})}else{throw}}finally{if($p){$p.Dispose()}}
   # Never catch policy failures as an exited-process race. File hashes admit
   # package-redirected aliases of the exact pinned bytes, including DOS/NT names.
   if($row){if($row.name -imatch '^(energyplus|expandobjects|readvarseso)$'){$record.energyplus_observed=$true};$row.image_sha256=File-Hash $row.image_path;$row.policy=Assert-ImageIdentity $row.name $row.image_sha256 $row.image_path ([IO.Path]::GetFileNameWithoutExtension($cliPath)) $identities.cli_sha256 $typstHash $conhost $conhostHash;if($row.policy -ceq 'pinned_typst'){$typstCount++}}
  }
  $record.sampled_private_max_bytes=[Math]::Max([long]$record.sampled_private_max_bytes,$private);$record.member_count_max=[Math]::Max([int]$record.member_count_max,[int]$active)
  $samples.Add(@{elapsed_ms=$elapsed;active_processes=$active;member_ids=$members;members=$observed.ToArray();raw_peak_bytes=$limits[3];sampled_sum_private_bytes=$private;sampled_sum_working_set_bytes=$working})
  if($proc.HasExited -and $proc.JobEmpty){$record.exit_code=$proc.ExitCode;break}
  Start-Sleep -Milliseconds 200
 }
 if($record.exit_code -ne 0){throw 'The single CLI regeneration failed; inspect retained bounded stderr.'}
 if(!$typstCount -or $record.energyplus_observed){throw 'Pinned Typst was not observed, or a forbidden simulation process was observed.'}
 $record.command_seconds=$timer.Elapsed.TotalSeconds
 Assert-RegenerationDeadline $record.command_seconds
 $commandPassed=$true
}catch{$record.failure=$_.Exception.Message}
finally{
 # The child inherited its isolated environment at process creation. Restore
 # every parent value even if process startup or verification failed.
 foreach($key in $childEnvironment.Keys){[Environment]::SetEnvironmentVariable($key,$parentEnvironment[$key],'Process')}
 $record.parent_environment_restored=$true
 if($proc){
  try{if(!$proc.JobEmpty){$record.forced_owned_job_kill=$true;$proc.Kill();$kill=[Diagnostics.Stopwatch]::StartNew();while(!$proc.JobEmpty -and $kill.Elapsed.TotalSeconds -le 5){Start-Sleep -Milliseconds 50}};$record.cleanup_job_empty=$proc.JobEmpty;$record.final_raw_peak_bytes=$proc.PeakJobCommittedBytes;$record.raw_peak_max_bytes=[Math]::Max([long]$record.raw_peak_max_bytes,[long]$record.final_raw_peak_bytes);if(!$record.cleanup_job_empty -or $record.raw_peak_max_bytes -gt 2GB){throw 'Owned process tree remained live or final raw peak exceeded2 GiB.'};foreach($task in @($outTask,$errTask)){if($task){if(!$task.Wait(5000)){throw 'Bounded process output did not reach EOF.'};$null=$task.GetAwaiter().GetResult()}}}catch{$record.cleanup_errors+=@($_.Exception.Message)}finally{$proc.Dispose()}
 }
 [IO.File]::WriteAllBytes((Join-Path $output 'stdout.bin'),$stdout.Bytes);[IO.File]::WriteAllBytes((Join-Path $output 'stderr.bin'),$stderr.Bytes)
 $record.stdout_total_bytes=$stdout.Total;$record.stderr_total_bytes=$stderr.Total;$record.stdout_overflow=$stdout.Overflow;$record.stderr_overflow=$stderr.Overflow
 try{$after=Snapshot-Members $root;Write-Receipt (Join-Path $output 'after-members.json') $after;Assert-SameMembers $before $after;$record.original_members_preserved=$true;$record.original_member_count=$after.member_count;$record.original_member_bytes=$after.total_bytes}catch{$record.preservation_errors+=@($_.Exception.Message)}
 try{foreach($path in @($ColdUnavailablePaths)+@($childEnvironment.GREENLENS_ENERGYPLUS_DIR,$childEnvironment.GREENLENS_RADIANCE)){if(Test-Path -LiteralPath $path){throw 'A declared unavailable path became available.'}};if((File-Hash $cliPath) -cne $identities.cli_sha256 -or (File-Hash $typst) -cne $typstHash -or (File-Hash $native) -cne $identities.native_helper_sha256 -or (File-Hash $scopeHelper) -cne $identities.scope_history_helper_sha256 -or (File-Hash $PSCommandPath) -cne $identities.supervisor_sha256 -or (File-Hash $typstPinPath) -cne $identities.typst_dependency_sha256){throw 'Declared tooling changed during the operation.'}}catch{$record.preservation_errors+=@($_.Exception.Message)}
 $record.samples=$samples.ToArray();$record.members=@($seen.Values|Sort-Object first_observed_utc);$record.observed_typst_processes=$typstCount
}
# Publication is deliberately after cleanup and all preservation checks. A
# successful CLI exit alone never writes a successful result/receipt.
try{
 if(!$commandPassed -or $record.cleanup_errors.Count -or $record.preservation_errors.Count -or $stdout.Overflow -or $stderr.Overflow -or !$record.cleanup_job_empty){throw 'Command, cleanup, bounded-output or preservation checks did not pass.'}
 $response=[Text.UTF8Encoding]::new($false,$true).GetString($stdout.Bytes)|ConvertFrom-Json -AsHashtable -DateKind String -Depth 100
 if($response.issue_id -cne $IssueId -or $response.identity -cne $ExpectedContentSha256){throw 'CLI response does not bind the exact declared issue/content.'}
 $regenerated=Read-BoundedJson (Portable-Child $destination 'issue-products.json') 256KB
 if((File-Hash (Portable-Child $destination 'issue-products.json')) -cne $issue.products_file_sha256){throw 'Regenerated product inventory bytes differ from the retained inventory.'}
 $record.regenerated=Assert-Products $destination $products
 $actual=Snapshot-Members $destination
 if($actual.member_count -ne ($products.products.Count+1)){throw 'Regeneration contains missing or unadvertised additional members.'}
 foreach($p in $products.products){if(!$actual.files.Contains($p.path)){throw 'A regenerated deterministic member is missing.'}}
  $record.verification_seconds=$timer.Elapsed.TotalSeconds
  Assert-RegenerationDeadline $record.verification_seconds
 $record.products_identical=$true;$record.completed=$true
}catch{if(!$record.failure){$record.failure=$_.Exception.Message}else{$record.verification_failure=$_.Exception.Message}}
$record.elapsed_seconds=$timer.Elapsed.TotalSeconds;$record.completed_utc=[DateTime]::UtcNow.ToString('o')
Write-Receipt (Join-Path $output 'receipt.json') $record
if(!$record.completed){throw "Single regeneration incomplete/failed. Retained receipt: $output; $($record.failure)"}
Write-Receipt (Join-Path $output 'result.json') @{schema_version=$protocol.schema_version;completed=$true;issue_id=$IssueId;source=$ExpectedSource.ToLowerInvariant();product_count=$record.regenerated.product_count;product_bytes=$record.regenerated.product_bytes;all_product_hashes_equal=$true;all_prior_project_members_unchanged=$true;job_empty=$record.cleanup_job_empty;raw_peak_bytes=$record.raw_peak_max_bytes;command_seconds=$record.command_seconds;receipt_sha256=(File-Hash (Join-Path $output 'receipt.json'))}
Write-Host "One exact issue regenerated with retained bytes preserved. Receipt: $output"
