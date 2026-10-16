# Finite synthetic v0.7 protocol. Run with PowerShell 7 on Windows.
# This collector does not assert native interaction or scientific qualification.
[CmdletBinding(DefaultParameterSetName='Run')]
param(
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Cli,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Project,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$OutputDirectory,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$EngineDirectory,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$TypstDirectory,
 [Parameter(ParameterSetName='Run')][ValidateSet('All','Prepare','Run','Reports','Cold','Replay')][string]$Phase='All',
 # Optional positively identified original weather/conversion/diagnostic scratch.
 # Every path is local, declared before execution, safety-checked and restored.
 [Parameter(ParameterSetName='Run')][string[]]$AdditionalUnavailablePaths=@(),
 # An explicit software-only repair may change the frozen CLI. Old receipts and
 # completed scientific evidence remain; production/replay attempt counters do not reset.
 [Parameter(ParameterSetName='Run')][switch]$AdoptRepairedCli,
 [Parameter(Mandatory,ParameterSetName='Functions')][switch]$FunctionsOnly
)

function Wait-IssueStreamTask([Threading.Tasks.Task]$Task) {
 # CopyToAsync may return Task<VoidTaskResult>; PowerShell otherwise emits its
 # internal result object and changes the enclosing CLI response into an array.
 $null=$Task.GetAwaiter().GetResult()
}
function Read-IssueJson([string]$Path) {
 return Get-Content -LiteralPath $Path -Raw -Encoding utf8 | ConvertFrom-Json -AsHashtable -Depth 100
}
function Write-IssueJson([string]$Path,$Value) {
 $temporary=$Path+'.'+[guid]::NewGuid().ToString('N')+'.tmp'
 [IO.File]::WriteAllText($temporary,(ConvertTo-Json -InputObject $Value -Depth 100)+"`n",[Text.UTF8Encoding]::new($false))
 [IO.File]::Move($temporary,$Path,$true)
}
function Get-IssueHash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Assert-IssueChild([string]$Root,[string]$Path) {
 $parent=[IO.Path]::GetFullPath($Root).TrimEnd([IO.Path]::DirectorySeparatorChar)+[IO.Path]::DirectorySeparatorChar
 $child=[IO.Path]::GetFullPath($Path)
 if(!$child.StartsWith($parent,[StringComparison]::OrdinalIgnoreCase)){throw 'Path is outside the declared owned directory'}
}
function Get-IssueTreeBytes([string]$Path,[switch]$Live) {
 [long]$bytes=0
 if(Test-Path -LiteralPath $Path){
  if($Live){
   $samplingErrors=@()
   foreach($file in Get-ChildItem -LiteralPath $Path -File -Recurse -ErrorAction SilentlyContinue -ErrorVariable +samplingErrors){$bytes+=$file.Length}
   foreach($failure in $samplingErrors){if($failure.CategoryInfo.Category -ne 'ObjectNotFound'){throw $failure}}
  }else{foreach($file in Get-ChildItem -LiteralPath $Path -File -Recurse -ErrorAction Stop){$bytes+=$file.Length}}
 }
 return $bytes
}
function Get-IssueSnapshot([string]$Root,[switch]$WithoutManifest) {
 $result=[ordered]@{}
 if(!$WithoutManifest){$result['project.json']=Get-IssueHash (Join-Path $Root 'project.json')}
 foreach($name in @('runs','weather','energy-assessments','references','attachments','report-issues')){
  $path=Join-Path $Root $name
  if(Test-Path -LiteralPath $path){foreach($file in Get-ChildItem -LiteralPath $path -File -Recurse | Sort-Object FullName){
   $relative=[IO.Path]::GetRelativePath($Root,$file.FullName).Replace('\','/')
   if($file.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Evidence must not contain a reparse-point file'}
   $result[$relative]=Get-IssueHash $file.FullName
  }}
 }
 return $result
}
function Assert-IssueSnapshot([string]$Root,[Collections.IDictionary]$Expected,[switch]$AllowAdditional) {
 foreach($name in $Expected.Keys){
  $path=Join-Path $Root $name
  Assert-IssueChild $Root $path
  if(!(Test-Path -LiteralPath $path -PathType Leaf) -or (Get-IssueHash $path) -cne $Expected[$name]){throw "Retained evidence changed: $name"}
 }
 if(!$AllowAdditional){
  $actual=Get-IssueSnapshot $Root -WithoutManifest:(!$Expected.Contains('project.json'))
  if($actual.Count -ne $Expected.Count){throw 'Retained evidence member set changed'}
 }
}
function Test-IssueJsonEqual($A,$B) {
 # Comparison only. Rust remains authoritative for model/physical identities.
 if($null -eq $A -or $null -eq $B){return $null -eq $A -and $null -eq $B}
 if($A -is [Collections.IDictionary] -and $B -is [Collections.IDictionary]){
  if($A.Count -ne $B.Count){return $false}
  foreach($key in $A.Keys){if(!$B.Contains($key) -or !(Test-IssueJsonEqual $A[$key] $B[$key])){return $false}}
  return $true
 }
 if($A -is [array] -and $B -is [array]){
  if($A.Count -ne $B.Count){return $false}
  for($i=0;$i -lt $A.Count;$i++){if(!(Test-IssueJsonEqual $A[$i] $B[$i])){return $false}}
  return $true
 }
 if($A -is [string] -or $B -is [string]){return $A -is [string] -and $B -is [string] -and $A -ceq $B}
 return $A -eq $B
}
function Copy-IssueJson($Value) { return ConvertTo-Json -InputObject $Value -Depth 100 | ConvertFrom-Json -AsHashtable -Depth 100 }
function Assert-IssueExecutionBudgets([double]$Elapsed,[int]$MaximumSeconds,[long]$Memory,[long]$Scratch,[int]$Processes,[int]$EngineCount,[int]$AllowedEngines,[double]$EngineAge) {
 if($MaximumSeconds -notin @(60,600,900) -or $AllowedEngines -lt 0 -or $AllowedEngines -gt 2){throw 'Unrecognized finite collector budget'}
 if($Elapsed -gt $MaximumSeconds){throw 'Declared command deadline exceeded'}
 if($EngineAge -gt 300){throw 'An EnergyPlus process exceeded 300 seconds from its Windows creation time'}
 if($Memory -gt 2GB -or $Scratch -gt 2GB -or $Processes -gt 16){throw 'Declared working-set, scratch or process-count guard exceeded'}
 if($EngineCount -gt $AllowedEngines){throw 'An undeclared EnergyPlus process started'}
}
function Assert-IssueCopEdit($Before,$After) {
 $expected=Copy-IssueJson $Before
 if($expected.zones.Count -ne 4){throw 'Declared protocol requires its four-zone retained model'}
 foreach($zone in $expected.zones){
  if($null -eq $zone.cooling_system -or $zone.cooling_system.gross_rated_cop -ne 3.6){throw 'Declared original proposed COP must be exactly 3.6 in every zone'}
  $zone.cooling_system.gross_rated_cop=3.8
 }
 if(!(Test-IssueJsonEqual $expected $After)){throw 'Proposed model differs beyond the four declared COP edits'}
}
function New-IssueReviewDecisions($Plan) {
 $decisions=[Collections.Generic.List[object]]::new()
 foreach($row in $Plan.rows){
  if($row.kind -eq 'input' -and $row.change -ne 'unchanged'){
   if($row.change -ne 'changed' -or $row.after.field_path -notmatch '/gross_rated_cop$' -or $row.before.applied_value -ne 3.6 -or $row.after.applied_value -ne 3.8 -or $row.after.design_id -cne 'B'){
    throw "Carry-forward contains an undeclared input change: $($row.key)"
   }
  }
  if($row.kind -in @('reference','mapping','reconciliation','report_view') -and $row.change -ne 'unchanged'){
   throw "COP-only protocol cannot silently accept a geometry/reference change: $($row.kind) $($row.key)"
  }
  if(!$row.requires_review){continue}
  $resolution='reviewed'
  $rationale='Declared synthetic workflow review: exact retained reference and unchanged geometry were compared by Rust. Retain existing documentation only; this is not source verification or scientific qualification.'
  if($row.kind -eq 'input'){
   $rationale='Explicit protocol edit: proposed gross rated cooling COP 3.6 to 3.8 only. This is an illustrative synthetic efficiency assumption, not a sourced product or a claim of physical qualification.'
  } elseif($row.kind -eq 'assumption' -and $row.change -ne 'unchanged'){
   $resolution='unresolved'
   $rationale='The previously declared source value remains in S1 and the review row. Its applicability to the new applied input is unresolved; preserve the proposed missing-source disclosure and do not invent replacement provenance.'
  }
  $decisions.Add([ordered]@{row_id=$row.id;resolution=$resolution;rationale=$rationale})
 }
 return ,@($decisions.ToArray())
}
function Restore-IssuePaths($Moves) {
 $failures=[Collections.Generic.List[string]]::new()
 for($i=$Moves.Count-1;$i -ge 0;$i--){
  try{
   if(Test-Path -LiteralPath $Moves[$i].original){throw 'Original path was recreated externally; hidden evidence preserved'}
   Move-Item -LiteralPath $Moves[$i].hidden -Destination $Moves[$i].original -ErrorAction Stop
  }catch{$failures.Add("$($Moves[$i].original): $($_.Exception.Message)")}
 }
 if($failures.Count){throw ('Cold path restoration needs intervention: '+($failures -join '; '))}
}
if($FunctionsOnly){return}
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
if(!$IsWindows -or $PSVersionTable.PSVersion.Major -lt 7){throw 'Use native Windows PowerShell 7'}
$localBase=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
foreach($variable in @('Cli','Project','OutputDirectory','EngineDirectory','TypstDirectory')){
 Set-Variable -Name $variable -Value ([IO.Path]::GetFullPath((Get-Variable $variable -ValueOnly)))
}
Assert-IssueChild $localBase $OutputDirectory
Assert-IssueChild $localBase $Project
if($OutputDirectory.StartsWith($Project+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $Project.StartsWith($OutputDirectory+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $Project -ieq $OutputDirectory){throw 'Source and protocol output must be independent directories'}
$AdditionalUnavailablePaths=@($AdditionalUnavailablePaths|ForEach-Object {[IO.Path]::GetFullPath($_)})
foreach($path in $AdditionalUnavailablePaths){
 Assert-IssueChild $localBase $path
 foreach($protected in @($OutputDirectory,$EngineDirectory,$TypstDirectory,$Cli)){
  if($protected -ieq $path -or $protected.StartsWith($path+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $path.StartsWith($OutputDirectory+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Additional cold paths must not contain protected protocol outputs or installed tools'}
 }
}
$protocolPath=Join-Path $PSScriptRoot '../docs/v0.7-issue-protocol.md'
$A1='f523dbb1-b134-4a1e-88db-f7d24a732f8c'; $B1='0a8f9f8d-7415-4011-8957-8bc85c708d8d'
$sourceManifest='74302544b5c5c43080a2f5af6abb40c46b5cc99469b4a0fcd47aa54d83057dfd'
$statePath=Join-Path $OutputDirectory 'collector-state.json'
$declarationPath=Join-Path $OutputDirectory 'declared-protocol.json'
$work=Join-Path $OutputDirectory 'working study 空間'
$cold=Join-Path $OutputDirectory 'cold portable study 空間'
$package=Join-Path $OutputDirectory 'complete-issued-study.zip'
$logs=Join-Path $OutputDirectory 'commands'
$inputCliHash=Get-IssueHash $Cli
if(!(Test-Path -LiteralPath $statePath)){
 if(Test-Path -LiteralPath $OutputDirectory){throw 'New protocol requires a new output directory; resume only with its retained state'}
 if((Get-IssueHash (Join-Path $Project 'project.json')) -cne $sourceManifest){throw 'Source does not match the predeclared seven-run project'}
 $source=Read-IssueJson (Join-Path $Project 'project.json')
 if($source.energy.runs.Count -ne 7){throw 'Declared source requires exactly seven retained energy runs'}
 foreach($selection in @(@($A1,'85f2beded5c6278cf3e783eca939c930b2e4a6348c3c1ef253c82b9899dfac44'),@($B1,'5c1f9539145886f041ed5eef18e1c62bda37cd26435361a19afdea833fa7481b'))){
  $selected=@($source.energy.runs|Where-Object id -CEQ $selection[0])
  if($selected.Count -ne 1 -or $selected[0].record_sha256 -cne $selection[1]){throw 'Source run does not match the declared exact identity'}
 }
 if(@($source.architecture.assets|Where-Object { $_.manifest_sha256 -ceq 'a7a41da2a7a6bf3d8bae7888eed4300c421e38dfa88311b592676e10841f1789' -and $_.geometry_sha256 -ceq 'f605bf71ea682dee28d10bfccc087e0efef7b8e065f04a035ffadc51fb113f8e' }).Count -ne 1){throw 'Declared reference asset is missing'}
 New-Item -ItemType Directory -Path $logs -Force|Out-Null
 $sourceBytes=Get-IssueSnapshot $Project
 Write-IssueJson (Join-Path $OutputDirectory 'original-member-hashes.json') $sourceBytes
 $declaration=[ordered]@{
  schema_version='greenlens.study-issues-protocol-receipt.v1';declared_utc=[DateTime]::UtcNow.ToString('o');protocol_sha256=Get-IssueHash $protocolPath
  source_manifest_sha256=$sourceManifest;source_project=$Project;output_directory=$OutputDirectory;baseline_run=$A1;original_proposed_run=$B1
  additional_unavailable_paths=$AdditionalUnavailablePaths
  initial_cli_sha256=$inputCliHash;collector_sha256=Get-IssueHash $PSCommandPath;engine_directory=$EngineDirectory;typst_directory=$TypstDirectory
  initial_energyplus_executable_sha256=Get-IssueHash (Join-Path $EngineDirectory 'energyplus.exe');initial_typst_executable_sha256=Get-IssueHash (Join-Path $TypstDirectory 'typst.exe')
  hardware=[ordered]@{logical_processors=[Environment]::ProcessorCount;os_description=[Runtime.InteropServices.RuntimeInformation]::OSDescription;process_architecture=[Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString();total_physical_memory_bytes=[long](Get-CimInstance Win32_ComputerSystem -Property TotalPhysicalMemory).TotalPhysicalMemory}
  scientific_claim='Workflow, immutable evidence and deterministic reporting only. No physical qualification, native interaction or private architectural acceptance.'
  production_run_budget=1;replay_run_budget=2;maximum_engine_process_seconds=300;maximum_command_seconds=600;replay_command_seconds=900
  process_tree_working_set_bytes=2GB;scratch_bytes=2GB;maximum_process_tree_count=16;maximum_archive_entries=5000;maximum_entry_bytes=64MB;maximum_archive_bytes=256MB
  edit=[ordered]@{design='B';field='all zones /cooling_system/gross_rated_cop';before=3.6;after=3.8;other_inputs_unchanged=$true}
  phases=@('Prepare','Run','Reports','Cold','Replay');review_policy='Only COP field changes may be accepted. Unchanged geometry is retained after exact Rust comparison. Changed source declarations remain unresolved; no fabricated provenance.'
 }
 Write-IssueJson $declarationPath $declaration
 $state=[ordered]@{schema_version='greenlens.study-issues-collector-state.v1';cli_sha256=$inputCliHash;production_attempt_started=$false;replay_attempt_started=$false;completed_phases=@();repairs=@()}
 Write-IssueJson $statePath $state
}else{
 $declaration=Read-IssueJson $declarationPath; $state=Read-IssueJson $statePath
 if($declaration.source_project -cne $Project -or $declaration.engine_directory -cne $EngineDirectory -or $declaration.typst_directory -cne $TypstDirectory -or $declaration.protocol_sha256 -cne (Get-IssueHash $protocolPath) -or !(Test-IssueJsonEqual $declaration.additional_unavailable_paths $AdditionalUnavailablePaths)){throw 'Resume arguments or finite protocol differ from the original declaration'}
 if($state.cli_sha256 -cne $inputCliHash){
  if(!$AdoptRepairedCli){throw 'Frozen CLI differs. Software-only repair requires explicit -AdoptRepairedCli; engine attempt counters will remain unchanged'}
  $state.repairs+=@([ordered]@{utc=[DateTime]::UtcNow.ToString('o');previous_cli_sha256=$state.cli_sha256;cli_sha256=$inputCliHash;collector_sha256=Get-IssueHash $PSCommandPath;reason='Explicit software repair; no permission to repeat completed or failed engine attempts'})
  $state.cli_sha256=$inputCliHash; Write-IssueJson $statePath $state
 }
 Assert-IssueSnapshot $Project (Read-IssueJson (Join-Path $OutputDirectory 'original-member-hashes.json'))
}
if($declaration.initial_energyplus_executable_sha256 -cne (Get-IssueHash (Join-Path $EngineDirectory 'energyplus.exe')) -or $declaration.initial_typst_executable_sha256 -cne (Get-IssueHash (Join-Path $TypstDirectory 'typst.exe'))){throw 'Pinned executable bytes changed since this protocol was declared'}
$frozenCli=Join-Path $OutputDirectory "greenlens-$($state.cli_sha256).exe"
if(!(Test-Path -LiteralPath $frozenCli)){Copy-Item -LiteralPath $Cli -Destination $frozenCli}
if((Get-IssueHash $frozenCli) -cne $state.cli_sha256){throw 'Frozen collector CLI was modified'}

function Save-IssueState { Write-IssueJson $statePath $state }
function Get-StudyDocument { return Read-IssueJson (Join-Path $work 'project.json') }
function Get-IssueScratchInventory {
 $result=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
 foreach($name in @('jobs','report-jobs','report-issue-staging','issued-project-replay','issued-project-inspection','issued-project-export')){
  $path=Join-Path $localBase $name
  if(Test-Path -LiteralPath $path){foreach($dir in Get-ChildItem -LiteralPath $path -Directory){$null=$result.Add($dir.FullName)}}
 }
 return ,$result
}
function Invoke-IssueCli([string]$Name,[string[]]$Arguments,[int]$Seconds=600,[int]$AllowedEngines=0,[switch]$ExpectedFailure,[hashtable]$Environment=@{}) {
 if((Get-IssueHash $frozenCli) -cne $state.cli_sha256){throw 'Frozen CLI changed'}
 $attempt=Join-Path $logs ($Name+'-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff')+'-'+[guid]::NewGuid().ToString('N').Substring(0,6))
 New-Item -ItemType Directory -Path $attempt|Out-Null
 $request=[ordered]@{name=$Name;cli_sha256=$state.cli_sha256;collector_sha256=Get-IssueHash $PSCommandPath;arguments=$Arguments;environment_overrides=$Environment;started_utc=[DateTime]::UtcNow.ToString('o');deadline_seconds=$Seconds;allowed_energyplus_processes=$AllowedEngines}
 Write-IssueJson (Join-Path $attempt 'request.json') $request
 $start=[Diagnostics.ProcessStartInfo]::new($frozenCli);$start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.WorkingDirectory=$OutputDirectory
 $start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
 foreach($argument in $Arguments){$start.ArgumentList.Add($argument)}
 foreach($key in $Environment.Keys){$start.Environment[$key]=[string]$Environment[$key]}
 $stdout=[IO.File]::Create((Join-Path $attempt 'stdout.json'));$stderr=[IO.File]::Create((Join-Path $attempt 'stderr.txt'))
 $process=[Diagnostics.Process]::new();$process.StartInfo=$start
 $known=[Collections.Generic.HashSet[int]]::new();$births=@{};$engines=@{};$scratchBefore=Get-IssueScratchInventory
 $watch=[Diagnostics.Stopwatch]::StartNew();[long]$peakMemory=0;[long]$peakScratch=0;$maxProcesses=0;$violation=$null;$exitCode=$null;$started=$false
 try{
  if(!$process.Start()){throw 'CLI failed to start'};$started=$true;$null=$known.Add($process.Id)
  $outTask=$process.StandardOutput.BaseStream.CopyToAsync($stdout);$errTask=$process.StandardError.BaseStream.CopyToAsync($stderr)
  do{
   $table=@(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name,ExecutablePath,CreationDate -ErrorAction Stop)
   # Retain process creation times so a recycled PID can never authorize a kill
   # or be mistaken for a descendant of this bounded command.
   foreach($child in $table){
    $key=[string]$child.ProcessId
    if($known.Contains([int]$child.ProcessId)){
     if($births.Contains($key) -and $births[$key] -ne $child.CreationDate){$null=$known.Remove([int]$child.ProcessId)}
     elseif(!$births.Contains($key)){$births[$key]=$child.CreationDate}
    }
   }
   do{$added=$false;foreach($child in $table){if($known.Contains([int]$child.ParentProcessId) -and $known.Add([int]$child.ProcessId)){$births[[string]$child.ProcessId]=$child.CreationDate;$added=$true}}}while($added)
   [long]$memory=0;$alive=0;[double]$oldestEngine=0
   foreach($child in $table){if($known.Contains([int]$child.ProcessId)){
    try{$observed=[Diagnostics.Process]::GetProcessById([int]$child.ProcessId);$observed.Refresh();$memory+=$observed.WorkingSet64;$observed.Dispose();$alive++}catch [ArgumentException]{}
    if($child.Name -ieq 'energyplus.exe'){
     $key=[string]$child.ProcessId
     if(!$engines.Contains($key)){$engines[$key]=[ordered]@{pid=[int]$child.ProcessId;first_observed_seconds=$watch.Elapsed.TotalSeconds;last_observed_seconds=$watch.Elapsed.TotalSeconds;executable=$child.ExecutablePath}}
     $engines[$key].last_observed_seconds=$watch.Elapsed.TotalSeconds
     $oldestEngine=[Math]::Max($oldestEngine,([DateTime]::UtcNow-$child.CreationDate.ToUniversalTime()).TotalSeconds)
    }
   }}
   $peakMemory=[Math]::Max($peakMemory,$memory);$maxProcesses=[Math]::Max($maxProcesses,$alive)
   [long]$scratch=0
   foreach($path in (Get-IssueScratchInventory)){if(!$scratchBefore.Contains($path)){$scratch+=Get-IssueTreeBytes $path -Live}}
   $peakScratch=[Math]::Max($peakScratch,$scratch)
   Assert-IssueExecutionBudgets $watch.Elapsed.TotalSeconds $Seconds $memory $scratch $alive $engines.Count $AllowedEngines $oldestEngine
   if($stdout.Length -gt 64MB -or $stderr.Length -gt 64MB){$violation='Collector output exceeded its bounded log size'}
   if($violation){throw $violation}
   if(!$process.HasExited){Start-Sleep -Milliseconds 400}
  }while(!$process.HasExited)
  $process.WaitForExit();Wait-IssueStreamTask $outTask;Wait-IssueStreamTask $errTask;$exitCode=$process.ExitCode
 }catch{
  if(!$violation){$violation=$_.Exception.Message}
  if($started -and !$process.HasExited){$process.Kill($true);$process.WaitForExit(10000)|Out-Null}
  # Known descendant handles are a fallback if the root exited before its child.
  foreach($entry in @(Get-CimInstance Win32_Process -Property ProcessId,CreationDate -ErrorAction SilentlyContinue)){
   if($known.Contains([int]$entry.ProcessId) -and $births.Contains([string]$entry.ProcessId) -and $births[[string]$entry.ProcessId] -eq $entry.CreationDate){
    try{$child=[Diagnostics.Process]::GetProcessById([int]$entry.ProcessId);if(!$child.HasExited){$child.Kill($true)};$child.Dispose()}catch [ArgumentException]{}
   }
  }
 }finally{
  $watch.Stop();$stdout.Dispose();$stderr.Dispose();$process.Dispose()
  $receipt=[ordered]@{request=$request;exit_code=$exitCode;wall_seconds=$watch.Elapsed.TotalSeconds;peak_observed_process_tree_working_set_bytes=$peakMemory;peak_observed_scratch_bytes=$peakScratch;peak_process_count=$maxProcesses;observed_energyplus_processes=@($engines.Values);sampling='Windows process-tree working-set and new scratch directories sampled approximately every 400 ms; sampled peaks are not an instrumented absolute maximum';failure=$violation;finished_utc=[DateTime]::UtcNow.ToString('o')}
  Write-IssueJson (Join-Path $attempt 'receipt.json') $receipt
 }
 if($violation){throw "Protocol stage $Name stopped: $violation. Retained receipt: $attempt"}
 if($ExpectedFailure){if($exitCode -eq 0){throw "Expected dependency failure did not occur: $Name"};return $null}
 if($exitCode -ne 0){throw "Protocol stage $Name exited $exitCode; inspect $attempt"}
 return Read-IssueJson (Join-Path $attempt 'stdout.json')
}
function Assert-RetainedOriginals {
 $expected=Read-IssueJson (Join-Path $OutputDirectory 'original-member-hashes.json')
 $expected.Remove('project.json')|Out-Null
 Assert-IssueSnapshot $work $expected -AllowAdditional
 if($state.Contains('r1_members')){Assert-IssueSnapshot $work $state.r1_members -AllowAdditional}
}
function Get-MatchingIssue([string]$Scope,[string]$B) {
 $doc=Get-StudyDocument
 if(!$doc.Contains('report_issues') -or $null -eq $doc.report_issues){return $null}
 $found=@($doc.report_issues.issues|Where-Object { $_.scope_revision_sha256 -ceq $Scope -and $_.baseline.run_id -ceq $A1 -and $_.proposed.run_id -ceq $B })
 if($found.Count -gt 1){throw 'More than one issued product matches the declared pair/scope; select explicitly before continuing'}
 if($found.Count){return $found[0]};return $null
}
function Ensure-ProtocolIssue([string]$Label,[string]$Scope,[string]$B) {
 $found=Get-MatchingIssue $Scope $B
 if($null -eq $found){$created=Invoke-IssueCli "issue-$Label" @('report-issue','create',$work,'--a',$A1,'--b',$B,'--scope',$Scope,'--typst-dir',$TypstDirectory);$found=$created.issue.summary}
 $inspected=Invoke-IssueCli "inspect-$Label" @('report-issue','inspect',$work,$found.id)
 if($inspected.summary.scope_revision_sha256 -cne $Scope -or $inspected.summary.baseline.run_id -cne $A1 -or $inspected.summary.proposed.run_id -cne $B){throw 'Issue binding differs from its declared scope/pair'}
 if($null -ne $inspected.summary.sensitivity_session_id){throw 'Predeclared absent exact-pair sensitivity unexpectedly acquired a binding; stop for evidence review'}
 if(!$inspected.summary.include_pdf -or $inspected.products.products.Count -gt 252){throw 'Issue does not meet its declared product family/bounds'}
 return $inspected.summary.id
}
function Find-CompletedB2 {
 $doc=Get-StudyDocument;$matches=[Collections.Generic.List[object]]::new()
 $model=Read-IssueJson (Join-Path $OutputDirectory 'B2-model.json');$study=Read-IssueJson (Join-Path $OutputDirectory 'B2-study.json')
 foreach($summary in $doc.energy.runs|Where-Object alternative_id -CEQ 'B'){
  $path=Join-Path $work ($summary.path+'/results.json');$record=Read-IssueJson $path
  if((Test-IssueJsonEqual $record.model $model) -and (Test-IssueJsonEqual $record.study $study)){$matches.Add($summary)}
 }
 if($matches.Count -gt 1){throw 'Multiple matching B2 runs violate the finite one-run protocol; inspect retained attempts'}
 if($matches.Count -eq 0){return $null}
 $selected=Invoke-IssueCli 'verify-retained-B2' @('energy','inspect',$work,$matches[0].id)
 if($selected.run.series.interval_count -ne 52560 -or $selected.run.series.interval_minutes -ne 10 -or $selected.run.provenance.severe_count -ne 0){throw 'B2 does not meet required output/diagnostic shape'}
 foreach($processReceipt in $selected.run.provenance.processes){
  if($processReceipt.elapsed_ms -gt 300000 -or !$processReceipt.process_tree_empty -or $processReceipt.sampled_peak_runner_plus_job_working_set_bytes -gt 2GB -or $processReceipt.peak_observed_scratch_bytes -gt 2GB){throw 'Retained native process receipt exceeds the declared budget or leaves a process tree'}
 }
 return $matches[0].id
}

$requested=if($Phase -eq 'All'){@('Prepare','Run','Reports','Cold','Replay')}else{@($Phase)}
try{
 foreach($step in $requested){
  if($step -in $state.completed_phases){continue}
  switch($step){
   'Prepare' {
    if(!(Test-Path -LiteralPath (Join-Path $work 'project.json'))){$null=Invoke-IssueCli 'save-as-working' @('copy',$Project,$work)}
    Assert-RetainedOriginals
    $doc=Get-StudyDocument
    if(!$state.Contains('s1')){
     $history=if($doc.architecture.Contains('scope_history')){$doc.architecture.scope_history}else{$null}
     if($null -eq $history){
      if($null -eq $doc.architecture.scope){throw 'Declared legacy scope is missing'}
      $scopePath=Join-Path $OutputDirectory 'S1-legacy-scope.json';Write-IssueJson $scopePath $doc.architecture.scope
      $null=Invoke-IssueCli 'adopt-S1' @('reference','scope-apply',$work,$scopePath)
      $doc=Get-StudyDocument;$history=$doc.architecture.scope_history
     }
     if($history.revisions.Count -ne 1){throw 'S1 adoption requires exactly one explicit retained initial revision'}
     $state.s1=$history.active_revision_sha256;Save-IssueState
    }
    $state.r1=Ensure-ProtocolIssue 'R1' $state.s1 $B1
    $r1Root=Join-Path $work ('report-issues/'+$state.r1);$r1Files=[ordered]@{}
    foreach($file in Get-ChildItem -LiteralPath $r1Root -File -Recurse){$r1Files[[IO.Path]::GetRelativePath($work,$file.FullName).Replace('\','/')]=Get-IssueHash $file.FullName}
    $state.r1_members=$r1Files;Save-IssueState
    $doc=Get-StudyDocument;$initial=Read-IssueJson (Join-Path $Project 'project.json');$model=Copy-IssueJson $initial.energy.designs.B.model
    foreach($zone in $model.zones){$zone.cooling_system.gross_rated_cop=3.8}
    Assert-IssueCopEdit $initial.energy.designs.B.model $model
    Write-IssueJson (Join-Path $OutputDirectory 'B2-model.json') $model
    Write-IssueJson (Join-Path $OutputDirectory 'B2-study.json') $initial.energy.designs.B.study
    if(!(Test-IssueJsonEqual $doc.energy.designs.B.model $model)){
     if(!(Test-IssueJsonEqual $doc.energy.designs.B.model $initial.energy.designs.B.model)){throw 'Current design differs from both declared B1 and B2'}
     $null=Invoke-IssueCli 'configure-B2' @('energy','configure',$work,(Join-Path $OutputDirectory 'B2-model.json'),(Join-Path $OutputDirectory 'B2-study.json'),'--alternative','B')
    }
    if(!$state.Contains('s2')){
     $doc=Get-StudyDocument;$children=@($doc.architecture.scope_history.revisions|Where-Object parent_revision_sha256 -CEQ $state.s1)
     if($children.Count -gt 1){throw 'Unexpected additional revision children'}
     if($children.Count -eq 1){$state.s2=$children[0].revision_sha256}else{
      $plan=Invoke-IssueCli 'prepare-S2' @('reference','scope-review',$work,'--parent',$state.s1)
      Write-IssueJson (Join-Path $OutputDirectory 'S2-plan.json') $plan
      Write-IssueJson (Join-Path $OutputDirectory 'S2-scope.json') $plan.proposed_scope
      $decisions=New-IssueReviewDecisions $plan
      Write-IssueJson (Join-Path $OutputDirectory 'S2-decisions.json') $decisions
      $history=Invoke-IssueCli 'accept-S2' @('reference','scope-accept',$work,'--parent',$state.s1,'--plan',$plan.plan_sha256,'--scope',(Join-Path $OutputDirectory 'S2-scope.json'),'--decisions',(Join-Path $OutputDirectory 'S2-decisions.json'))
      $state.s2=$history.active_revision_sha256
     }
     Save-IssueState
    }
    $null=Invoke-IssueCli 'preflight-B2' @('energy','preflight',$work,'--alternative','B')
    Assert-RetainedOriginals
   }
   'Run' {
    if('Prepare' -notin $state.completed_phases){throw 'Complete Prepare before Run'}
    $existing=Find-CompletedB2
    if($null -ne $existing){$state.b2=$existing;Save-IssueState}else{
     if($state.production_attempt_started){throw 'The one declared production attempt was already started without a retained completed B2. Preserve diagnostics; recover completed pending evidence or record the blocker, do not rerun'}
     $null=Invoke-IssueCli 'engine-identity' @('energy','doctor','--engine-dir',$EngineDirectory) 60
     $state.production_attempt_started=$true;$state.production_attempt_utc=[DateTime]::UtcNow.ToString('o');Save-IssueState
     $null=Invoke-IssueCli 'run-B2-once' @('energy','run',$work,'--alternative','B','--engine-dir',$EngineDirectory) 600 1
     $state.b2=Find-CompletedB2
     if($null -eq $state.b2){throw 'Successful process did not retain the exact declared B2'}
     Save-IssueState
    }
    Assert-RetainedOriginals
   }
   'Reports' {
    if('Run' -notin $state.completed_phases){throw 'Complete Run before Reports'}
    $state.r2=Ensure-ProtocolIssue 'R2' $state.s2 $state.b2;Save-IssueState
    $null=Invoke-IssueCli 'historical-R1-after-R2' @('report-issue','inspect',$work,$state.r1)
    $null=Invoke-IssueCli 'canonical-R2-comparison' @('energy','compare',$work,$A1,$state.b2)
    Assert-RetainedOriginals
    Write-IssueJson (Join-Path $OutputDirectory 'issued-study-members.json') (Get-IssueSnapshot $work)
   }
   'Cold' {
    if('Reports' -notin $state.completed_phases){throw 'Complete Reports before Cold'}
    if(!(Test-Path -LiteralPath (Join-Path $cold 'project.json'))){$null=Invoke-IssueCli 'save-as-cold' @('copy',$work,$cold)}
    $expected=Read-IssueJson (Join-Path $OutputDirectory 'issued-study-members.json')
    # Save As gives the new document a new identity; all compact evidence must remain identical.
    $expected.Remove('project.json')|Out-Null;Assert-IssueSnapshot $cold $expected
    $coldBefore=Get-IssueSnapshot $cold;$moves=[Collections.Generic.List[object]]::new();$doc=Get-StudyDocument
    try{
     foreach($target in @($Project,$work)+$AdditionalUnavailablePaths+@($doc.energy.runs|ForEach-Object {Join-Path $localBase ('jobs/'+[guid]::Parse($_.id).ToString())})){
      $resolved=[IO.Path]::GetFullPath($target);Assert-IssueChild $localBase $resolved
      if(Test-Path -LiteralPath $resolved){
       $hidden=$resolved+'.unavailable-'+[guid]::NewGuid().ToString('N')
       if((Split-Path -Parent $resolved) -cne (Split-Path -Parent $hidden) -or (Test-Path -LiteralPath $hidden)){throw 'Unsafe cold path destination'}
       Move-Item -LiteralPath $resolved -Destination $hidden
       $moves.Add([ordered]@{original=$resolved;hidden=$hidden});Write-IssueJson (Join-Path $OutputDirectory 'cold-path-recovery.json') @($moves.ToArray())
      }
     }
     $environment=@{GREENLENS_ENERGYPLUS_DIR=(Join-Path $OutputDirectory 'unavailable-energyplus')}
     $null=Invoke-IssueCli 'cold-missing-engine' @('energy','doctor') 60 0 -ExpectedFailure -Environment $environment
     $null=Invoke-IssueCli 'cold-open' @('open',$cold) -Environment $environment
     foreach($label in @('r1','r2')){
      $null=Invoke-IssueCli "cold-inspect-$label" @('report-issue','inspect',$cold,$state[$label]) -Environment $environment
      $destination=Join-Path $OutputDirectory ('regenerated-'+$label+'-'+[guid]::NewGuid().ToString('N'))
      $null=Invoke-IssueCli "cold-regenerate-$label" @('report-issue','regenerate',$cold,$state[$label],$destination,'--typst-dir',$TypstDirectory) 600 0 -Environment $environment
      $state['regenerated_'+$label]=$destination;Save-IssueState
     }
     Assert-IssueSnapshot $cold $coldBefore
     Write-IssueJson (Join-Path $OutputDirectory 'cold-verification.json') ([ordered]@{engine_discovery_failed=$true;original_project_and_working_copy_hidden=$true;known_retained_job_scratch_hidden=$true;complete_retained_member_hashes_unchanged=$true;regeneration_deterministic_products_equal=$true;native_interaction_observed=$false;paths_hidden=@($moves.ToArray());external_path_coverage='Only the exact listed paths were hidden. Portable run provenance deliberately contains relative arguments, not original machine paths. Unknown former collector scratch paths were not inferred; complete ZIP extraction/replay is a separate closure check.';cli_sha256=$state.cli_sha256})
    }finally{Restore-IssuePaths $moves}
   }
   'Replay' {
    if('Cold' -notin $state.completed_phases){throw 'Complete Cold before Replay'}
    if(!(Test-Path -LiteralPath $package)){$null=Invoke-IssueCli 'complete-export' @('report-issue','export',$cold,$package)}
    $inspection=Invoke-IssueCli 'complete-package-inspect' @('report-issue','inspect-package',$package)
    if($inspection.energy_runs -ne 8 -or $inspection.issue_ids.Count -ne 2 -or $inspection.scope_revision_ids.Count -ne 2 -or $inspection.simulations_started -ne 0){throw 'Complete package lost or added declared history'}
    if($state.replay_attempt_started -and !$state.Contains('replay_completed')){throw 'The declared two-case replay attempt already started. Preserve its outputs; do not automatically repeat either case'}
    if(!$state.Contains('replay_completed')){
     $state.replay_attempt_started=$true;$state.replay_attempt_utc=[DateTime]::UtcNow.ToString('o');Save-IssueState
     $result=Invoke-IssueCli 'replay-R2-only' @('report-issue','replay',$package,$state.r2,'--engine-dir',$EngineDirectory) 900 2
     if($result.state -ne 'completed' -or $result.runs.Count -ne 2 -or $result.runs[0].run_id -cne $A1 -or $result.runs[1].run_id -cne $state.b2){throw 'Replay did not verify exactly the declared pair'}
     $state.replay_completed=$true;Save-IssueState
    }
    Write-IssueJson (Join-Path $OutputDirectory 'delivery-inventory.json') ([ordered]@{cli_sha256=$state.cli_sha256;source_manifest_sha256=$sourceManifest;s1=$state.s1;s2=$state.s2;r1=$state.r1;r2=$state.r2;baseline=$A1;original_proposed=$B1;new_proposed=$state.b2;package_sha256=Get-IssueHash $package;package_bytes=(Get-Item -LiteralPath $package).Length;working_project_bytes=Get-IssueTreeBytes $work;cold_project_bytes=Get-IssueTreeBytes $cold;native_acceptance='Not established by this headless collector';report_visual_review='Separate required observation; not established here';scientific_qualification='No qualification inferred'})
   }
  }
  $state.completed_phases+=@($step);Save-IssueState
 }
}finally{
 if(Test-Path -LiteralPath (Join-Path $Project 'project.json')){Assert-IssueSnapshot $Project (Read-IssueJson (Join-Path $OutputDirectory 'original-member-hashes.json'))}
}
Write-Output (ConvertTo-Json -InputObject ([ordered]@{output=$OutputDirectory;completed_phases=$state.completed_phases;cli_sha256=$state.cli_sha256;scope='Headless protocol only; see retained individual receipts, inherited failures and native acceptance ledger'}) -Depth 8)
