# Finite original synthetic presentation protocol; no simulation commands.
# Run with native PowerShell 7. All data/receipts remain in the supplied external
# LOCALAPPDATA directory. A failed attempt is retained and is never auto-retried.
[CmdletBinding(DefaultParameterSetName='Run')]
param(
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Cli,
 [Parameter(Mandatory,ParameterSetName='Run')][ValidatePattern('^[a-fA-F0-9]{40}$')][string]$CliSourceCommit,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Project,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$BaselineRun,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$ProposedRun,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$TypstDirectory,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$OutputDirectory,
 [Parameter(ParameterSetName='Run')][string]$LegacyProject,
 [Parameter(ParameterSetName='Run')][ValidateCount(0,2)][ValidatePattern('^[a-f0-9]{64}$')][string[]]$LegacyIssueIds=@(),
 [Parameter(Mandatory,ParameterSetName='Functions')][switch]$FunctionsOnly
)

# Reuse the existing inspected JSON, hash, snapshot and finite-budget helpers.
# Its FunctionsOnly branch returns before any process or filesystem mutation.
# Isolate its parameter bindings: ordinary dot-sourcing would overwrite this
# collector's Cli/Project/FunctionsOnly variables with the helper's defaults.
$helperModule=New-Module -Name GreenLensV08ReportCollectorHelpers -ArgumentList (Join-Path $PSScriptRoot 'verify-study-issues.ps1') -ScriptBlock {
 param($helperPath)
 . $helperPath -FunctionsOnly
 Export-ModuleMember -Function Read-IssueJson,Write-IssueJson,Get-IssueHash,Assert-IssueChild,Get-IssueTreeBytes,Get-IssueSnapshot,Assert-IssueSnapshot,Test-IssueJsonEqual,Copy-IssueJson,Assert-IssueExecutionBudgets,Wait-IssueStreamTask
}
Import-Module -ModuleInfo $helperModule -Scope Local

function New-ReportFixtureGlb([string]$Path) {
 $nodes=@(0..80|ForEach-Object {[ordered]@{name=('Synthetic component {0:D3} - original synthetic geometry' -f $_);mesh=0;translation=@($_,0,0)}})
 $json=[ordered]@{
  asset=@{version='2.0';generator='GreenLens original bounded report fixture v1'}
  buffers=@(@{byteLength=36});bufferViews=@(@{buffer=0;byteLength=36})
  accessors=@(@{bufferView=0;componentType=5126;count=3;type='VEC3';min=@(0,0,0);max=@(1,1,0)})
  meshes=@(@{primitives=@(@{attributes=@{POSITION=0}})});nodes=$nodes
  scenes=@(@{nodes=@(0..80)});scene=0
 }
 $utf8=[Text.UTF8Encoding]::new($false)
 $text=ConvertTo-Json -InputObject $json -Depth 20 -Compress
 while(($utf8.GetByteCount($text)%4)-ne 0){$text+=' '}
 $data=$utf8.GetBytes($text);$stream=[IO.MemoryStream]::new();$writer=[IO.BinaryWriter]::new($stream)
 try{
  foreach($value in @([uint32]0x46546c67,[uint32]2,[uint32](28+$data.Length+36),[uint32]$data.Length,[uint32]0x4e4f534a)){$writer.Write($value)}
  $writer.Write($data);$writer.Write([uint32]36);$writer.Write([uint32]0x004e4942)
  foreach($value in @(0,0,0,1,0,0,0,1,0)){$writer.Write([single]$value)}
  $writer.Flush();[IO.File]::WriteAllBytes($Path,$stream.ToArray())
 }finally{$writer.Dispose();$stream.Dispose()}
}
function Assert-ReportTreeBounds([string]$Root) {
 [long]$bytes=0;$count=0
 if((Get-Item -LiteralPath $Root).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Protocol root cannot be a reparse point'}
 foreach($entry in Get-ChildItem -LiteralPath $Root -Recurse -Force){
  if($entry.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Protocol input/output cannot contain reparse points'}
  if($entry.PSIsContainer){continue}
  if($entry.Length -gt 64MB){throw 'Project member exceeds the unchanged 64 MiB entry limit'}
  $bytes+=$entry.Length;$count++
  if($count -gt 5000 -or $bytes -gt 256MB){throw 'Complete project exceeds the unchanged archive bounds; no history is pruned'}
 }
 return [ordered]@{entries=$count;bytes=$bytes;maximum_entries=5000;maximum_entry_bytes=64MB;maximum_uncompressed_bytes=256MB}
}
function New-ReportFixtureScope($Manifest,$Context,$Document) {
 if($Manifest.occurrences.Count -ne 81){throw 'The declared fixture requires exactly 81 admitted occurrences'}
 $mappings=[Collections.Generic.List[object]]::new();$assumptions=[Collections.Generic.List[object]]::new()
 for($i=0;$i -lt 81;$i++){
  $id=$Manifest.occurrences[$i].id
  $status=if($i -eq 80){'unresolved'}elseif($i -eq 79){'excluded'}else{'context_only'}
  $mappings.Add([ordered]@{occurrence_id=$id;status=$status;targets=@();rationale="Original explicit report disposition row $i; this triangle is not a thermal zone"})
  $assumptions.Add([ordered]@{id=('source-{0:D3}' -f $i);link=@{kind='reference_occurrence';occurrence_id=$id};field='measured dimension';status=$(if($i -eq 80){'missing'}else{'synthetic'});value=$(if($i -eq 80){$null}else{'1'});units='m';source='Original synthetic geometry';rationale='Readability fixture, not measured building information';affected_scope='synthetic reference'})
 }
 $system=$Document.energy.designs.B.model.zones[0].cooling_system
 if($null -eq $system){throw 'The finite fixture requires a conditioned first proposed zone'}
 foreach($declaration in @(@('conflict-one','3.6'),@('conflict-two','3.7'))){
  $assumptions.Add([ordered]@{id=$declaration[0];link=@{kind='thermal_object';design_id='B';object_id=$system.id};field='/gross_rated_cop';status='assumed';value=$declaration[1];units='1';source='Deliberately contradictory original source declarations';rationale='Must be disclosed after the original first-80 boundary; does not edit the applied COP';affected_scope=$Document.energy.designs.B.model.zones[0].id})
 }
 $hashes=[ordered]@{};foreach($id in @('A','B')){$hashes[$id]=$Context.designs[$id].model_input_sha256}
 return [ordered]@{
  schema_version='greenlens.architectural-scope.v2';title='Original 81-row report completeness fixture'
  purpose='Report completeness, pagination and immutable evidence handling using genuine retained simulations. No architectural equivalence or scientific qualification is inferred.'
  scope_kind='representative_study';reference_revision_sha256=$Manifest.revision_sha256;reference_geometry_sha256=$Manifest.geometry_sha256;reference_options_sha256=$Context.reference_options_sha256
  design_model_sha256=$hashes;mappings=@($mappings.ToArray());assumptions=@($assumptions.ToArray());reconciliations=@()
  included_scope=@('The unchanged accepted A/B thermal models and their selected retained annual results.','Eighty-one original synthetic triangles test report register completeness only.')
  excluded_scope=@('No private architecture, source-equivalence claim or measured physical validation.','Source disposition row 79 is explicitly excluded; row 80 remains unresolved.')
  boundary_consequences='Thermal geometry, boundaries, schedules, weather, system and service assumptions remain exactly those of the retained selected runs. The unrelated synthetic display triangles do not change simulation inclusion.'
  coverage=@{status='unknown';reason='The original report fixture is not a building source; no valid whole-building area denominator exists.'};whole_building_review=$null;report_views=@()
 }
}
function Assert-ReportFixtureProducts($Inspected,[string]$IssueRoot) {
 $c=Read-IssueJson (Join-Path $IssueRoot 'presentation-coverage.json')
 $m=Read-IssueJson (Join-Path $IssueRoot 'scope-mappings.json')
 $a=Read-IssueJson (Join-Path $IssueRoot 'scope-assumptions.json')
 if($Inspected.content.renderer.method -cne 'greenlens.energy-report-issue-renderer/2'){throw 'New issue did not use the separately versioned renderer/2'}
 if(!$Inspected.summary.include_pdf){throw 'Native PDF output was not included'}
 if($m.rows.Count -ne 81 -or $m.rows[80].status -cne 'unresolved' -or $m.rows[79].status -cne 'excluded'){throw 'Lossless mapping artifact lost the declared late exceptions'}
 if($a.rows.Count -ne 83 -or $a.rows[80].status -cne 'missing' -or $a.rows[81].id -cne 'conflict-one' -or $a.rows[82].id -cne 'conflict-two'){throw 'Lossless assumption artifact lost the late missing/conflicting declarations'}
 if($c.mappings.total_rows -ne 81 -or $c.mappings.displayed_rows -ne 80 -or $c.mappings.omitted_rows -ne 1 -or $c.mappings.status_counts.unresolved -ne 1 -or $c.mappings.status_counts.excluded -ne 1 -or $c.mappings.status_counts.'context only' -ne 79){throw 'All-row mapping totals differ from the declared fixture'}
 if($c.mappings.displayed_ids[0] -cne $m.rows[80].occurrence_id -or $c.assumptions.displayed_ids[0] -cne 'source-080' -or $c.assumptions.total_rows -ne 83 -or $c.provenance.status_counts.conflicting -ne 1){throw 'Late review-critical rows are not reflected in bounded presentation'}
 foreach($kind in @('mappings','assumptions','provenance','reconciliations')){
  $t=$c[$kind];$total=0;foreach($n in $t.status_counts.Values){$total+=$n}
  if($total -ne $t.total_rows -or $t.displayed_rows+$t.omitted_rows -ne $t.total_rows -or $t.review_required_displayed+$t.review_required_omitted -ne $t.review_required_total){throw "Incomplete register accounting: $kind"}
 }
 $rows=$Inspected.content.scope_revision.provenance.rows
 $conflicts=@($rows|Where-Object status -CEQ 'conflicting')
 if($conflicts.Count -ne 1 -or $c.provenance.displayed_ids[0] -cne $conflicts[0].id){throw 'Conflicting applied-field provenance is not disclosed first'}
 $appendix=[IO.File]::ReadAllText((Join-Path $IssueRoot 'input-appendix.html'))
 $executive=[IO.File]::ReadAllText((Join-Path $IssueRoot 'executive.html'))
 foreach($text in @('Displayed 80 of 81','1 omitted','unresolved: 1','conflicting: 1','scope-mappings.json','source-080','conflict-one')){if(!$appendix.Contains($text)){throw "HTML appendix lacks explicit complete-register disclosure: $text"}}
 foreach($text in @('Documentation: INCOMPLETE','conflicting','unresolved rows','Comparison eligibility','Unassessed')){if(!$executive.Contains($text)){throw "Executive HTML lacks material disclosure: $text"}}
 if($null -eq $Inspected.summary.sensitivity_session_id -and !$executive.Contains('Exact selected-pair sensitivity: MISSING')){throw 'Absent exact-pair sensitivity is not disclosed'}
 $products=[Collections.Generic.List[object]]::new()
 foreach($p in $Inspected.products.products){
  $file=Join-Path $IssueRoot $p.path;Assert-IssueChild $IssueRoot $file
  if((Get-Item -LiteralPath $file).Length -ne $p.bytes -or (Get-IssueHash $file) -cne $p.sha256){throw "Generated product differs from its immutable manifest: $($p.path)"}
  $products.Add([ordered]@{path=$p.path;bytes=$p.bytes;sha256=$p.sha256})
 }
 return [ordered]@{coverage=$c;products=@($products.ToArray());complete_artifact_counts=@{mappings=81;assumptions=83};html_disclosures_passed=$true;pdf_visual_review='Not performed by this collector; every new page/image requires separate review.'}
}
if($FunctionsOnly){return}
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
if(!$IsWindows -or $PSVersionTable.PSVersion.Major -lt 7){throw 'Use native Windows PowerShell 7'}
$localBase=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
foreach($name in @('Cli','Project','TypstDirectory','OutputDirectory')){Set-Variable -Name $name -Value ([IO.Path]::GetFullPath((Get-Variable $name -ValueOnly)))}
Assert-IssueChild $localBase $Project;Assert-IssueChild $localBase $OutputDirectory
if($OutputDirectory -ieq $Project -or $OutputDirectory.StartsWith($Project+'\',[StringComparison]::OrdinalIgnoreCase) -or $Project.StartsWith($OutputDirectory+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Source and collector output must be independent'}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new output directory. A previously started protocol is retained and cannot be automatically retried.'}
if([bool]$LegacyProject -ne ($LegacyIssueIds.Count -gt 0)){throw 'Supply both LegacyProject and one or two explicit LegacyIssueIds, or neither'}
if(@($LegacyIssueIds|Select-Object -Unique).Count -ne $LegacyIssueIds.Count){throw 'Duplicate legacy issue IDs are not permitted'}
if($LegacyProject){
 $LegacyProject=[IO.Path]::GetFullPath($LegacyProject);Assert-IssueChild $localBase $LegacyProject
 if($LegacyProject -ieq $Project){throw 'The pre-issue source and legacy issued source are distinct declared documents'}
 if($OutputDirectory -ieq $LegacyProject -or $OutputDirectory.StartsWith($LegacyProject+'\',[StringComparison]::OrdinalIgnoreCase) -or $LegacyProject.StartsWith($OutputDirectory+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Legacy source and collector output must be independent'}
 $null=Assert-ReportTreeBounds $LegacyProject
}
$source=Read-IssueJson (Join-Path $Project 'project.json')
if($source.Contains('report_issues') -and $null -ne $source.report_issues -and $source.report_issues.issues.Count){throw 'The primary source must be pre-issue; never prune an issued document for this fixture'}
foreach($id in @('A','B')){if(!$source.energy.designs.Contains($id)){throw 'Declared A/B accepted designs are required'}}
$selected=@();foreach($pair in @(@($BaselineRun,'A'),@($ProposedRun,'B'))){$run=@($source.energy.runs|Where-Object id -CEQ $pair[0]);if($run.Count -ne 1 -or $run[0].alternative_id -cne $pair[1]){throw 'Select one genuine retained baseline A and proposed B'};$selected+=@($run[0])}
$initialBounds=Assert-ReportTreeBounds $Project
$original=Get-IssueSnapshot $Project;$legacyOriginal=if($LegacyProject){Get-IssueSnapshot $LegacyProject}else{$null}
$logs=Join-Path $OutputDirectory 'commands';New-Item -ItemType Directory -Path $logs -Force|Out-Null
$cliHash=Get-IssueHash $Cli;$collectorHash=Get-IssueHash $PSCommandPath;$typstHash=Get-IssueHash (Join-Path $TypstDirectory 'typst.exe')
$frozenCli=Join-Path $OutputDirectory ('greenlens-'+$cliHash+'.exe');Copy-Item -LiteralPath $Cli -Destination $frozenCli
$work=Join-Path $OutputDirectory 'report fixture study';$fixture=Join-Path $OutputDirectory 'original-81-occurrences.glb'
New-ReportFixtureGlb $fixture
$declaration=[ordered]@{
 schema_version='greenlens.v08-report-fixture-protocol.v1';declared_utc=[DateTime]::UtcNow.ToString('o');cli_source_commit_declared=$CliSourceCommit.ToLowerInvariant();cli_sha256=$cliHash;collector_sha256=$collectorHash;typst_sha256=$typstHash
 source_manifest_sha256=$original['project.json'];baseline=$selected[0];proposed=$selected[1];source_project=$Project;legacy_project=$LegacyProject;legacy_issue_ids=$LegacyIssueIds
 fixture_sha256=Get-IssueHash $fixture;fixture_bytes=(Get-Item -LiteralPath $fixture).Length;expected=@{occurrences=81;mappings=81;assumptions=83;unresolved=1;excluded=1;conflicting_applied_fields=1}
 new_issue_attempts=1;legacy_regeneration_attempts_per_selected_issue=1;maximum_seconds_per_command=600;maximum_process_tree_working_set_bytes=2GB;maximum_new_scratch_bytes=2GB;maximum_processes=16;maximum_energyplus_processes=0;maximum_cli_log_bytes=64MB
 source_bounds=$initialBounds;scope='Synthetic report completeness/readability and existing evidence integration only. No new simulation, scientific qualification, architectural equivalence, installed interaction or visual PDF acceptance.'
}
Write-IssueJson (Join-Path $OutputDirectory 'declared-protocol.json') $declaration
Write-IssueJson (Join-Path $OutputDirectory 'original-member-hashes.json') $original
if($LegacyProject){Write-IssueJson (Join-Path $OutputDirectory 'legacy-original-member-hashes.json') $legacyOriginal}
$state=[ordered]@{new_issue_attempt_started=$false;legacy_regeneration_attempts=@();completed=$false;commands=@()};$statePath=Join-Path $OutputDirectory 'collector-state.json';Write-IssueJson $statePath $state
function Get-ReportScratch {
 $set=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
 foreach($name in @('jobs','report-jobs','report-issue-staging')){$path=Join-Path $localBase $name;if(Test-Path -LiteralPath $path){foreach($dir in Get-ChildItem -LiteralPath $path -Directory){$null=$set.Add($dir.FullName)}}}
 return ,$set
}
function Invoke-ReportCli([string]$Name,[string[]]$Arguments,[switch]$AllowTypst) {
 $verb=$Arguments[0];$sub=if($Arguments.Count -gt 1){$Arguments[1]}else{''}
 if(!(($verb -in @('copy','capacity')) -or ($verb -eq 'energy' -and $sub -in @('inspect','compare')) -or ($verb -eq 'reference' -and $sub -in @('import','inspect','scope-context','scope-apply','scope-assess','scope-review','scope-accept')) -or ($verb -eq 'report-issue' -and $sub -in @('create','inspect','regenerate')))){throw 'CLI operation is outside this no-engine protocol'}
 if($AllowTypst -and !($verb -eq 'report-issue' -and $sub -in @('create','regenerate'))){throw 'Only explicit report generation may start Typst'}
 if((Get-IssueHash $frozenCli) -cne $cliHash -or (Get-IssueHash (Join-Path $TypstDirectory 'typst.exe')) -cne $typstHash){throw 'Declared tool bytes changed'}
 $attempt=Join-Path $logs $Name;if(Test-Path -LiteralPath $attempt){throw 'No command attempt is silently repeated'};New-Item -ItemType Directory -Path $attempt|Out-Null
 $request=@{arguments=$Arguments;deadline_seconds=600;allowed_engines=0;allow_typst=[bool]$AllowTypst;started_utc=[DateTime]::UtcNow.ToString('o')};Write-IssueJson (Join-Path $attempt 'request.json') $request
 $start=[Diagnostics.ProcessStartInfo]::new($frozenCli);$start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.WorkingDirectory=$OutputDirectory;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
 foreach($arg in $Arguments){$start.ArgumentList.Add($arg)}
 $start.Environment['GREENLENS_ENERGYPLUS_DIR']=Join-Path $OutputDirectory 'engine-not-available'
 $stdout=[IO.File]::Create((Join-Path $attempt 'stdout.json'));$stderr=[IO.File]::Create((Join-Path $attempt 'stderr.txt'));$p=[Diagnostics.Process]::new();$p.StartInfo=$start
 $known=[Collections.Generic.HashSet[int]]::new();$births=@{};$children=@{};$scratchBefore=Get-ReportScratch;$watch=[Diagnostics.Stopwatch]::StartNew();[long]$peakMemory=0;[long]$peakScratch=0;$peakCount=0;$failure=$null;$exitCode=$null;$started=$false
 try{
  if(!$p.Start()){throw 'CLI did not start'};$started=$true;$null=$known.Add($p.Id)
  $outTask=$p.StandardOutput.BaseStream.CopyToAsync($stdout);$errTask=$p.StandardError.BaseStream.CopyToAsync($stderr)
  do{
   $table=@(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name,CreationDate)
   foreach($row in $table){$key=[string]$row.ProcessId;if($known.Contains([int]$row.ProcessId)){if($births.ContainsKey($key) -and $births[$key] -ne $row.CreationDate){$null=$known.Remove([int]$row.ProcessId)}elseif(!$births.ContainsKey($key)){$births[$key]=$row.CreationDate}}}
   do{$added=$false;foreach($row in $table){if($known.Contains([int]$row.ParentProcessId) -and $known.Add([int]$row.ProcessId)){$births[[string]$row.ProcessId]=$row.CreationDate;$added=$true}}}while($added)
   [long]$memory=0;$alive=0;$engineCount=0
   foreach($row in $table){if($known.Contains([int]$row.ProcessId)){
    try{$child=[Diagnostics.Process]::GetProcessById([int]$row.ProcessId);$memory+=$child.WorkingSet64;$alive++;$child.Dispose()}catch [ArgumentException]{}
    if($row.Name -ieq 'energyplus.exe'){$engineCount++}
    if($row.ProcessId -ne $p.Id){$children[[string]$row.ProcessId]=@{name=$row.Name;creation_utc=$row.CreationDate.ToUniversalTime().ToString('o')};if(!$AllowTypst -or $row.Name -ine 'typst.exe'){throw 'An undeclared child process started; no engines are permitted'}}
   }}
   [long]$scratch=0;foreach($path in (Get-ReportScratch)){if(!$scratchBefore.Contains($path)){$scratch+=Get-IssueTreeBytes $path -Live}}
   $peakMemory=[Math]::Max($peakMemory,$memory);$peakScratch=[Math]::Max($peakScratch,$scratch);$peakCount=[Math]::Max($peakCount,$alive)
   Assert-IssueExecutionBudgets $watch.Elapsed.TotalSeconds 600 $memory $scratch $alive $engineCount 0 0
   if($stdout.Length -gt 64MB -or $stderr.Length -gt 64MB){throw 'Collector log exceeded its 64 MiB bound'}
   if(!$p.HasExited){Start-Sleep -Milliseconds 400}
  }while(!$p.HasExited)
  $p.WaitForExit()
  foreach($task in @($outTask,$errTask)){
   $remaining=[int][Math]::Max(0,[Math]::Min(10000,600000-$watch.Elapsed.TotalMilliseconds))
   if(!$task.Wait($remaining)){throw 'CLI stream drain exceeded its remaining command deadline'}
   Wait-IssueStreamTask $task
  }
  if($watch.Elapsed.TotalSeconds -gt 600){throw 'Declared command deadline exceeded during stream drain'}
  $exitCode=$p.ExitCode
 }catch{
  $failure=$_.Exception.Message
  if($started -and !$p.HasExited){$p.Kill($true);$null=$p.WaitForExit(10000)}
  foreach($row in @(Get-CimInstance Win32_Process -Property ProcessId,CreationDate -ErrorAction SilentlyContinue)){if($known.Contains([int]$row.ProcessId) -and $births.ContainsKey([string]$row.ProcessId) -and $births[[string]$row.ProcessId] -eq $row.CreationDate){try{$child=[Diagnostics.Process]::GetProcessById([int]$row.ProcessId);if(!$child.HasExited){$child.Kill($true)};$child.Dispose()}catch [ArgumentException]{}}}
 }finally{
  $watch.Stop();$stdout.Dispose();$stderr.Dispose();$p.Dispose()
  $receipt=@{request=$request;exit_code=$exitCode;wall_seconds=$watch.Elapsed.TotalSeconds;peak_observed_working_set_bytes=$peakMemory;peak_observed_new_scratch_bytes=$peakScratch;peak_process_count=$peakCount;observed_children=@($children.Values);sampling='Windows process table and working sets sampled approximately every 400 ms; this is not an absolute instrumented peak or native cancellation observation.';failure=$failure;finished_utc=[DateTime]::UtcNow.ToString('o')}
  Write-IssueJson (Join-Path $attempt 'receipt.json') $receipt;$state.commands+=@(@{name=$Name;receipt_sha256=Get-IssueHash (Join-Path $attempt 'receipt.json')});Write-IssueJson $statePath $state
 }
 if($failure -or $exitCode -ne 0){throw "Report collector stopped at $Name; retained receipt contains failure and resources"}
 return Read-IssueJson (Join-Path $attempt 'stdout.json')
}
try{
 $null=Invoke-ReportCli 'complete-save-as' @('copy',$Project,$work)
 $retained=Copy-IssueJson $original;$retained.Remove('project.json')|Out-Null;Assert-IssueSnapshot $work $retained
 $doc=Read-IssueJson (Join-Path $work 'project.json')
 foreach($pair in @(@($BaselineRun,'A'),@($ProposedRun,'B'))){
  $inspected=Invoke-ReportCli ('verify-run-'+$pair[1]) @('energy','inspect',$work,$pair[0]);$run=$inspected.run
  if($run.series.interval_count -ne 52560 -or $run.series.interval_minutes -ne 10 -or $run.provenance.severe_count -ne 0){throw 'Selected genuine run does not meet the declared complete annual shape'}
  if(!(Test-IssueJsonEqual $run.model $doc.energy.designs[$pair[1]].model) -or !(Test-IssueJsonEqual $run.study $doc.energy.designs[$pair[1]].study)){throw 'Accepted inputs differ from selected retained run; do not rebind a historical run'}
 }
 $comparison=Invoke-ReportCli 'verify-compatible-pair' @('energy','compare',$work,$BaselineRun,$ProposedRun)
 Write-IssueJson (Join-Path $OutputDirectory 'canonical-comparison.json') $comparison
 if($doc.Contains('architecture') -and $null -ne $doc.architecture -and $doc.architecture.Contains('scope') -and $null -ne $doc.architecture.scope -and (!$doc.architecture.Contains('scope_history') -or $null -eq $doc.architecture.scope_history)){
  $legacyScope=Join-Path $OutputDirectory 'adopted-legacy-scope.json';Write-IssueJson $legacyScope $doc.architecture.scope
  $null=Invoke-ReportCli 'adopt-old-scope' @('reference','scope-apply',$work,$legacyScope)
 }
 $null=Invoke-ReportCli 'reference-capacity' @('capacity',$work,'--operation','import-reference')
 $null=Invoke-ReportCli 'import-original-fixture' @('reference','import',$work,$fixture)
 $manifest=Invoke-ReportCli 'inspect-original-reference' @('reference','inspect',$work)
 $context=Invoke-ReportCli 'scope-context' @('reference','scope-context',$work)
 $doc=Read-IssueJson (Join-Path $work 'project.json');$scope=New-ReportFixtureScope $manifest $context $doc;$scopePath=Join-Path $OutputDirectory 'report-fixture-scope.json';Write-IssueJson $scopePath $scope
 $null=Invoke-ReportCli 'assess-fixture-scope' @('reference','scope-assess',$work,$scopePath)
 $history=if($doc.architecture.Contains('scope_history')){$doc.architecture.scope_history}else{$null}
 if($null -eq $history){$null=Invoke-ReportCli 'accept-initial-fixture-scope' @('reference','scope-apply',$work,$scopePath)}else{
  $parent=$history.active_revision_sha256;$plan=Invoke-ReportCli 'review-fixture-scope' @('reference','scope-review',$work,'--parent',$parent,'--candidate',$scopePath)
  if(!(Test-IssueJsonEqual $plan.proposed_scope $scope)){throw 'Rust proposed a different candidate; stop for review rather than accepting it silently'}
  $decisions=@();foreach($row in $plan.rows){
   if($row.kind -eq 'input' -and $row.change -ne 'unchanged'){throw 'This report fixture must not change any accepted scientific input'}
   if($row.requires_review){$resolution=if($row.kind -in @('assumption','mapping','provenance')){'unresolved'}else{'reviewed'};$decisions+=@(@{row_id=$row.id;resolution=$resolution;rationale='Explicit original report fixture: new unrelated triangles, mapping dispositions and deliberately contradictory source declarations. Preserve previous history; no architecture equivalence, applied-input change or source qualification is accepted.'})}
  }
  $decisionsPath=Join-Path $OutputDirectory 'review-decisions.json';Write-IssueJson $decisionsPath $decisions
  $null=Invoke-ReportCli 'accept-reviewed-fixture-scope' @('reference','scope-accept',$work,'--parent',$parent,'--plan',$plan.plan_sha256,'--scope',$scopePath,'--decisions',$decisionsPath)
 }
 $doc=Read-IssueJson (Join-Path $work 'project.json');$scopeId=$doc.architecture.scope_history.active_revision_sha256
 if(!(Test-IssueJsonEqual $doc.energy $source.energy)){throw 'Report preparation changed retained energy inputs or evidence indexes'}
 $null=Invoke-ReportCli 'issue-capacity' @('capacity',$work,'--operation','report-issue')
 $state.new_issue_attempt_started=$true;Write-IssueJson $statePath $state
 $created=Invoke-ReportCli 'issue-once' @('report-issue','create',$work,'--a',$BaselineRun,'--b',$ProposedRun,'--scope',$scopeId,'--typst-dir',$TypstDirectory) -AllowTypst
 $issueId=$created.issue.summary.id;$inspected=Invoke-ReportCli 'inspect-new-issue' @('report-issue','inspect',$work,$issueId)
 if($inspected.summary.baseline.run_id -cne $BaselineRun -or $inspected.summary.proposed.run_id -cne $ProposedRun -or $inspected.summary.scope_revision_sha256 -cne $scopeId){throw 'Issued scope/pair binding differs from the declared protocol'}
 $issueRoot=Join-Path $work $inspected.summary.path;Assert-IssueChild $work $issueRoot
 $checks=Assert-ReportFixtureProducts $inspected $issueRoot;Write-IssueJson (Join-Path $OutputDirectory 'complete-register-and-product-checks.json') $checks
 Assert-IssueSnapshot $work $retained -AllowAdditional
 $finalBounds=Assert-ReportTreeBounds $work
 $legacyResults=@()
 foreach($id in $LegacyIssueIds){
  $old=Invoke-ReportCli ('inspect-legacy-'+$id) @('report-issue','inspect',$LegacyProject,$id)
  if($old.content.renderer.method -cne 'greenlens.energy-report-issue-renderer/1' -or !$old.summary.include_pdf){throw 'Select an actual preserved renderer/1 PDF issue'}
  $state.legacy_regeneration_attempts+=@($id);Write-IssueJson $statePath $state
  $dest=Join-Path $OutputDirectory ('legacy-regenerated-'+$id)
  $null=Invoke-ReportCli ('regenerate-legacy-'+$id) @('report-issue','regenerate',$LegacyProject,$id,$dest,'--typst-dir',$TypstDirectory) -AllowTypst
  foreach($product in $old.products.products){$file=Join-Path $dest $product.path;Assert-IssueChild $dest $file;if((Get-IssueHash $file) -cne $product.sha256 -or (Get-Item -LiteralPath $file).Length -ne $product.bytes){throw 'Legacy regenerated deterministic product differs from retained issue'}}
  $legacyResults+=@(@{id=$id;content_sha256=$old.summary.content_sha256;products_sha256=$old.summary.products_sha256;products=$old.products.products;all_deterministic_product_bytes_equal=$true})
 }
 $state.completed=$true;Write-IssueJson $statePath $state
 Write-IssueJson (Join-Path $OutputDirectory 'result.json') (@{schema_version='greenlens.v08-report-fixture-result.v1';cli_sha256=$cliHash;cli_source_commit_declared=$CliSourceCommit.ToLowerInvariant();collector_sha256=$collectorHash;fixture_sha256=Get-IssueHash $fixture;scope_revision_sha256=$scopeId;issue=$inspected.summary;products=$checks.products;legacy_regeneration=$legacyResults;final_project_bounds=$finalBounds;new_simulations_permitted=0;native_interaction_observed=$false;pdf_visual_review='Pending separate page-by-page review';qualification='No physical qualification or private-case acceptance';finished_utc=[DateTime]::UtcNow.ToString('o')})
}finally{
 Assert-IssueSnapshot $Project $original
 if($LegacyProject){Assert-IssueSnapshot $LegacyProject $legacyOriginal}
}
Write-Output (ConvertTo-Json -InputObject @{output=$OutputDirectory;completed=$state.completed;issue=$issueId;new_simulations_permitted=0} -Depth 8)
