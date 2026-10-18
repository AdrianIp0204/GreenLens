#Requires -Version 7.5
# Original inert synthetic inventories only. No app, CLI, renderer, engine,
# native process or accepted simulation evidence is created by these checks.
[CmdletBinding()]
param([string]$OutputDirectory=(Join-Path $env:LOCALAPPDATA ('GreenLens\verification\analytical-observer-source-'+[Guid]::NewGuid().ToString('N'))))
$ErrorActionPreference='Stop'
$output=[IO.Path]::GetFullPath($OutputDirectory);$cache=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
if(!$output.StartsWith($cache+'\',[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $output)){throw 'Use a new external source-check receipt directory.'}
New-Item -ItemType Directory -Path $output|Out-Null
. (Join-Path $PSScriptRoot 'regenerate-v08-review-once.ps1') -FunctionsOnly
$ReportKind='AnalyticalDesign';$checks=[Collections.Generic.List[object]]::new();$failure=$null
function Check([string]$Name,[bool]$Value){if(!$Value){throw ('Failed: '+$Name)};$checks.Add(@{name=$Name;passed=$true})}
function Reject([string]$Name,[scriptblock]$Action){$caught=$false;try{& $Action|Out-Null}catch{$caught=$true};Check $Name $caught}
function Copy-Value($Value){ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject $Value -Depth 100 -Compress) -AsHashtable -DateKind String -Depth 100 -NoEnumerate}
function Write-Value([string]$Path,$Value){[IO.File]::WriteAllText($Path,(ConvertTo-Json -InputObject $Value -Depth 100 -Compress),[Text.UTF8Encoding]::new($false))}
$reportId='c'*64;$typstHash='d'*64
$manifest=@{id='original-synthetic-analytical-observer';runs=@();climate=$null;energy=@{runs=@(@{id='00000000-0000-4000-8000-000000000001';alternative_id='A';record_sha256=('a'*64)},@{id='00000000-0000-4000-8000-000000000002';alternative_id='B';record_sha256=('b'*64)})}}
$arm=@{project_id=$manifest.id;baseline_run_id=$manifest.energy.runs[0].id;proposed_run_id=$manifest.energy.runs[1].id;analytical_scope='Original inert fixture scope; not accepted simulation evidence.';north_declaration=$null}
$root=Join-Path $output 'inert-fixture';$directory=Join-Path $root ('design-reports/'+$reportId);New-Item -ItemType Directory -Path $directory -Force|Out-Null
function New-InertProducts {
 $content=@{schema_version='greenlens.design-report-content.v1';content_sha256=$reportId;include_pdf=$true;analytical_scope=$arm.analytical_scope;north_declaration=$null;renderer=@{method='greenlens.analytical-design-report-renderer/1';typst_executable_sha256=$typstHash};workbench=@{base=@{baseline=@{id='A';run_id=$arm.baseline_run_id;record_sha256=('a'*64)};proposed=@{id='B';run_id=$arm.proposed_run_id;record_sha256=('b'*64)}}}}
 Write-Value (Join-Path $directory 'design-content.json') $content
 [IO.File]::WriteAllText((Join-Path $directory 'paired.svg'),'<svg xmlns="http://www.w3.org/2000/svg"><text>Original inert inventory fixture</text></svg>')
 [IO.File]::WriteAllText((Join-Path $directory 'report.pdf'),'Original inert inventory test bytes, deliberately not a rendered PDF or accepted report.')
 return (Seal-InertProducts)
}
function Seal-InertProducts {
 $products=@('design-content.json','paired.svg','report.pdf')|ForEach-Object {$p=Join-Path $directory $_;@{path=$_;bytes=[long](Get-Item -LiteralPath $p).Length;sha256=(File-Hash $p)}}
 $inventory=@{schema_version='greenlens.design-report-products.v1';content_sha256=$reportId;products_sha256=('e'*64);products=@($products)}
 Write-Value (Join-Path $directory 'design-products.json') $inventory
 return @{id=$reportId;path=('design-reports/'+$reportId);content_sha256=$reportId;products_sha256=('e'*64);include_pdf=$true;manifest_file_sha256=(File-Hash (Join-Path $directory 'design-products.json'));baseline=@{run_id=$arm.baseline_run_id;record_sha256=('a'*64);design_id='A'};proposed=@{run_id=$arm.proposed_run_id;record_sha256=('b'*64);design_id='B'}}
}
function Inspect-Inert($Summary,$Selection){Assert-ObservedDesignProducts $root $Summary ${function:File-Hash} ${function:Read-BoundedJson} ${function:Portable-Child} $Selection $typstHash}
try{
 Check 'Absent analytical index is an empty inventory, not an architectural fallback' (@(Get-ObservedDesignReports $manifest).Count -eq 0)
 $bad=Copy-Value $manifest;$bad.design_reports=@{schema_version='other';reports=@()};Reject 'Unknown analytical index codec rejected' {Get-ObservedDesignReports $bad}
 $bad.design_reports=@{schema_version='greenlens.design-report-index.v1';reports=@{id=$reportId}};Reject 'Scalar analytical report list rejected' {Get-ObservedDesignReports $bad}
 $bad.design_reports.reports=@(@{id=$reportId},@{id=$reportId});Reject 'Duplicate analytical report ID rejected' {Get-ObservedDesignReports $bad}
 $bad.design_reports.reports=@(1..33|ForEach-Object {@{id=('{0:x64}' -f $_)}});Reject 'Analytical index cap retained' {Get-ObservedDesignReports $bad}
 $selection=Get-ObservedDesignArmSelection $arm $manifest $manifest.id
 Check 'Exact A/B records and explicit unknown north retained' ($selection.baseline.design_id -ceq 'A' -and $selection.proposed.design_id -ceq 'B' -and $null -eq $selection.north_declaration)
 foreach($field in @('project_id','baseline_run_id','proposed_run_id')){$bad=Copy-Value $arm;$bad[$field]='different';Reject ('Missing or different arm identity rejected: '+$field) {Get-ObservedDesignArmSelection $bad $manifest $manifest.id}}
 $bad=Copy-Value $manifest;$bad.energy.runs[0].alternative_id='B';Reject 'Swapped design role rejected' {Get-ObservedDesignArmSelection $arm $bad $manifest.id}
 $bad=Copy-Value $manifest;$bad.energy.runs+=@($bad.energy.runs[0]);Reject 'Duplicate selected run rejected' {Get-ObservedDesignArmSelection $arm $bad $manifest.id}
 $bad=Copy-Value $manifest;$bad.energy.runs[0].record_sha256='invalid';Reject 'Missing valid record hash rejected' {Get-ObservedDesignArmSelection $arm $bad $manifest.id}
 $bad=Copy-Value $arm;$bad.scope_revision_sha256='f'*64;Reject 'Architectural revision is not accepted as analytical scope' {Get-ObservedDesignArmSelection $bad $manifest $manifest.id}
 foreach($value in @('',('x'*2049),('界'*683),' untrimmed ')){$bad=Copy-Value $arm;$bad.analytical_scope=$value;Reject 'Empty oversized multibyte or untrimmed scope rejected' {Get-ObservedDesignArmSelection $bad $manifest $manifest.id}}
 $bad=Copy-Value $arm;$bad.analytical_scope=('x'*2048);Check 'Exact 2048-byte scope remains accepted' ((Get-ObservedDesignArmSelection $bad $manifest $manifest.id).analytical_scope.Length -eq 2048)
 $bad=Copy-Value $arm;$bad.Remove('north_declaration');Reject 'Missing north declaration differs from explicit unknown' {Get-ObservedDesignArmSelection $bad $manifest $manifest.id}
 $bad=Copy-Value $arm;$bad.north_declaration='';Reject 'Empty north is not silently normalized to unknown' {Get-ObservedDesignArmSelection $bad $manifest $manifest.id}
 $summary=New-InertProducts;$result=Inspect-Inert $summary $selection
 Check 'Actual helper verifies inert members, exact declarations and typed renderer' ($result.product_count -eq 3 -and $result.pdf_count -eq 1 -and $result.analytical_scope -ceq $arm.analytical_scope)
 Check 'Cold typed product check accepts the same analytical inventory' ((Assert-Products $directory $result.products).product_count -eq 3)
 $ReportKind='ArchitecturalClosure';Reject 'Architectural reader rejects analytical product schema' {Assert-Products $directory $result.products};$ReportKind='AnalyticalDesign'
 $bad=Copy-Value $summary;$bad.baseline.record_sha256='f'*64;Reject 'Resealed report cannot substitute the selected record' {Inspect-Inert $bad $selection}
 $bad=Copy-Value $summary;$bad.path='report-issues/'+$reportId;Reject 'Report-family path substitution rejected' {Inspect-Inert $bad $selection}
 $bad=Copy-Value $summary;$bad.include_pdf='true';Reject 'String PDF selection cannot replace a typed Boolean' {Inspect-Inert $bad $selection}
 $bad=Copy-Value $selection;$bad.north_declaration='Invented north';Reject 'North declaration drift rejected' {Inspect-Inert $summary $bad}
 $bad=Copy-Value $selection;$bad.analytical_scope='Different scope';Reject 'Scope declaration drift rejected' {Inspect-Inert $summary $bad}
 [IO.File]::AppendAllText((Join-Path $directory 'report.pdf'),'tamper');Reject 'Actual product tamper rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 [IO.File]::WriteAllText((Join-Path $directory 'unadvertised.txt'),'Original extra fixture');Reject 'Unadvertised products rejected' {Inspect-Inert $summary $selection};Remove-Item -LiteralPath (Join-Path $directory 'unadvertised.txt')
 $content=Read-BoundedJson (Join-Path $directory 'design-content.json') 48MB;$content.workbench.base.proposed.record_sha256='f'*64;Write-Value (Join-Path $directory 'design-content.json') $content;$summary=Seal-InertProducts
 Reject 'Resealed canonical content with wrong selected record rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 $content=Read-BoundedJson (Join-Path $directory 'design-content.json') 48MB;$content.renderer.method='different-renderer';Write-Value (Join-Path $directory 'design-content.json') $content;$summary=Seal-InertProducts
 Reject 'Resealed content with unsupported renderer rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 $content=Read-BoundedJson (Join-Path $directory 'design-content.json') 48MB;$content.analytical_scope='';Write-Value (Join-Path $directory 'design-content.json') $content;$summary=Seal-InertProducts
 Reject 'Cold preflight rejects resealed empty scope without needing an arm marker' {Inspect-Inert $summary $null};$summary=New-InertProducts
 $inventory=Read-BoundedJson (Join-Path $directory 'design-products.json') 256KB;[Array]::Reverse($inventory.products);Write-Value (Join-Path $directory 'design-products.json') $inventory;$summary.manifest_file_sha256=File-Hash (Join-Path $directory 'design-products.json')
 Reject 'Resealed unsorted product inventory rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 $nativeFile=Join-Path $PSScriptRoot 'supervise-v08-report-review.ps1';$nativeText=[IO.File]::ReadAllText($nativeFile);$tokens=$null;$errors=$null;$nativeAst=[Management.Automation.Language.Parser]::ParseFile($nativeFile,[ref]$tokens,[ref]$errors)
 Check 'Native collector with separate mode has no parser errors' ($errors.Count -eq 0)
 foreach($name in @('Stable-Value','Json-Identity','Issues','Assert-OldIssues','Assert-NoNewRuns','Assert-ArmSelection')){$definition=$nativeAst.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq $name},$true);. ([scriptblock]::Create($definition.Extent.Text))}
 $ReportKind='AnalyticalDesign';Check 'Actual native arm dispatch invokes analytical exact bindings' ((Assert-ArmSelection $arm $manifest $manifest.id).analytical_scope -ceq $arm.analytical_scope)
 $before=Copy-Value $manifest;$before.report_issues=@{issues=@(@{id=('1'*64)})};$after=Copy-Value $before;$after.design_reports=@{schema_version='greenlens.design-report-index.v1';reports=@($summary)}
 Assert-OldIssues $before $after;Assert-NoNewRuns $before $after;Check 'New analytical report preserves the separate historical family and runs' $true
 $bad=Copy-Value $after;$bad.report_issues.issues[0].id='2'*64;Reject 'Changed architectural issue rejected during analytical mode' {Assert-OldIssues $before $bad}
 $before=Copy-Value $after;$bad=Copy-Value $after;$bad.design_reports.reports[0].products_sha256='f'*64;Reject 'Historical analytical summary replacement rejected' {Assert-OldIssues $before $bad}
 $coldFile=Join-Path $PSScriptRoot 'regenerate-v08-review-once.ps1';$tokens=$null;$errors=$null;$coldAst=[Management.Automation.Language.Parser]::ParseFile($coldFile,[ref]$tokens,[ref]$errors)
 $selectionAssignment=$coldAst.Find({param($node)$node -is [Management.Automation.Language.AssignmentStatementAst] -and $node.Left.Extent.Text -ceq '$issues'},$true)
 $projectJson=$after;$IssueId=$reportId;. ([scriptblock]::Create($selectionAssignment.Extent.Text))
 Check 'Actual cold selection preserves one analytical summary as an array' ($issues -is [array] -and $issues.Count -eq 1 -and $issues[0].id -ceq $reportId)
 $IssueId='f'*64;. ([scriptblock]::Create($selectionAssignment.Extent.Text));Check 'Actual cold selection preserves zero matches as an empty array' ($issues -is [array] -and $issues.Count -eq 0)
 $ReportKind='ArchitecturalClosure';$IssueId='1'*64;. ([scriptblock]::Create($selectionAssignment.Extent.Text));Check 'Actual historical cold selection still preserves a singleton array' ($issues -is [array] -and $issues.Count -eq 1 -and $issues[0].id -ceq $IssueId)
 $ReportKind='AnalyticalDesign';$IssueId=$reportId;$ExpectedContentSha256=$reportId;$ExpectedProductsSha256='e'*64
 $responseCheck=$coldAst.Find({param($node)$node -is [Management.Automation.Language.IfStatementAst] -and $node.Extent.Text.StartsWith("if(`$ReportKind -ceq 'AnalyticalDesign'){") -and $node.Extent.Text.Contains('$response.report_id')},$true)
 Check 'Actual cold response validator found without invoking CLI' ($null -ne $responseCheck)
 $response=@{report_id=$reportId;content_sha256=$reportId;products_sha256=('e'*64)};. ([scriptblock]::Create($responseCheck.Extent.Text));Check 'Actual analytical response verifies all three exact identities' $true
 foreach($field in @('report_id','content_sha256','products_sha256')){$old=$response[$field];$response[$field]='f'*64;Reject ('Actual cold response rejects identity drift: '+$field) {. ([scriptblock]::Create($responseCheck.Extent.Text))};$response[$field]=$old}
 $ReportKind='ArchitecturalClosure';$response=@{issue_id=$reportId;identity=$reportId};. ([scriptblock]::Create($responseCheck.Extent.Text));Check 'Actual historical response identity semantics preserved' $true;$ReportKind='AnalyticalDesign'
 foreach($script in @('supervise-v08-report-review.ps1','regenerate-v08-review-once.ps1')){
  $text=[IO.File]::ReadAllText((Join-Path $PSScriptRoot $script));$tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseInput($text,[ref]$tokens,[ref]$errors)
  Check ($script+' preserves one observed bounded launch path') ($errors.Count -eq 0 -and @($ast.FindAll({param($node)$node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Member.Value -ceq 'StartBounded'},$true)).Count -eq 1 -and $text.Contains('Get-ObservedJobSample $session') -and $text.Contains('Close-ObservedJobSession $session') -and $text.Contains('Write-ObservedJobReceipt $session'))
  Check ($script+' preserves explicit default, separate type and fixed limits') ($text.Contains("`$ReportKind='ArchitecturalClosure'") -and $text.Contains("'AnalyticalDesign'") -and $text.Contains('kernel_job_commit_limit_bytes=1GB') -and $text.Contains('acceptance_raw_peak_limit_bytes=2GB') -and $text.Contains('active_process_limit=16') -and $text.Contains('600000'))
  Check ($script+' binds new shared helper source bytes') ($text.Contains('design_report_helper_sha256=(File-Hash $designHelper)') -and $text.Contains('(File-Hash $designHelper) -cne $identities.design_report_helper_sha256'))
 }
}catch{$failure=$_.Exception.Message}
finally{
 $receipt=[ordered]@{schema_version='greenlens.analytical-observer-source-checks.v1';helper_sha256=(File-Hash (Join-Path $PSScriptRoot 'observed-design-report.ps1'));test_source_sha256=(File-Hash $PSCommandPath);native_or_renderer_executions=0;accepted_simulation_records_created=0;passed_count=$checks.Count;failure=$failure;completed_utc=[DateTime]::UtcNow.ToString('o');checks=$checks.ToArray()}
 Write-Value (Join-Path $output 'checks.json') $receipt
}
if($failure){throw "Analytical observer source checks failed; receipt: $output; $failure"}
Write-Host "$($checks.Count) analytical observer source checks passed; receipt: $output"
