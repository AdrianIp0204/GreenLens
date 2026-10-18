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
function New-InertCase($Id,$Run,$Hash){return @{id=$Id;label=('Inert '+$Id);run_id=$Run;record_sha256=$Hash;model_input_sha256=$Hash;physical_input_sha256=$Hash;model=@{fixture='Original inert object, not a thermal model';design=$Id};study=@{calendar=@{fixture='Original inert calendar'}};assessment=@{fixture='No scientific assessment'}}}
$caseA=New-InertCase 'A' $arm.baseline_run_id ('a'*64);$caseB=New-InertCase 'B' $arm.proposed_run_id ('b'*64)
function New-InertFrame($Id){return @{id=$Id;title=('Inert '+$Id);kind='plan';eye_m=@(0,0,10);target_m=@(0,0,0);up=@(0,1,0);vertical_span_m=5;aspect_ratio=1.6;section=$null}}
function New-InertProducts($Drawing,$Frames){
 # Cleanup is limited to flat files in this newly owned external fixture folder.
 if([IO.Path]::GetFullPath($directory) -cne [IO.Path]::GetFullPath((Join-Path $root ('design-reports/'+$reportId)))){throw 'Fixture cleanup path differs.'}
 foreach($file in Get-ChildItem -LiteralPath $directory -File){Remove-Item -LiteralPath $file.FullName}
 $content=@{schema_version='greenlens.design-report-content.v1';content_sha256=$reportId;include_pdf=$true;analytical_scope=$arm.analytical_scope;north_declaration=$null;frames=@((New-InertFrame 'axonometric'),(New-InertFrame 'plan'));sensitivity=$null;proposal_declarations=@();renderer=@{method='greenlens.analytical-design-report-renderer/1';figure_method='greenlens.analytical-paired-drawing/1';typst_executable_sha256=$typstHash};workbench=@{base=@{baseline=(Copy-Value $caseA);proposed=(Copy-Value $caseB)};typed_comparison=@()}}
 if($null -ne $Frames){$content.frames=Copy-Value $Frames}
 if($null -ne $Drawing){$content.drawing=Copy-Value $Drawing;$content.frames=$content.drawing.frames;$content.schema_version='greenlens.design-report-content.v2';$content.renderer.method='greenlens.bound-design-report-renderer/2';$content.renderer.figure_method='greenlens.scoped-paired-drawing/2'}
 $figures=@{schema_version= $(if($null -eq $Drawing){'greenlens.paired-design-figures.v1'}else{'greenlens.paired-design-figures.v2'});method=$content.renderer.figure_method;request=@{schema_version='greenlens.paired-design-figures.v1';frames=$content.frames;scope=$arm.analytical_scope;north_declaration=$null};products=@();changes=@()}
 foreach($name in @('baseline','proposed')){$case=$content.workbench.base[$name];$figures.request[$name]=@{design_id=$case.id;label=$case.label;model=$case.model;study=$case.study;calendar=$case.study.calendar;scope_revision_sha256=$(if($null -ne $Drawing){$Drawing.selection.scope_revision_sha256}else{$null});evidence=@{kind='retained_run';run_id=$case.run_id;record_sha256=$case.record_sha256;physical_input_sha256=$case.physical_input_sha256;assessment=$case.assessment}}}
 foreach($frame in $content.frames){foreach($side in @('A','B','overlay')){$filename='design-'+$frame.id+'-'+$side.ToLowerInvariant()+'.svg';$svg='<svg xmlns="http://www.w3.org/2000/svg"><text>Original inert '+$frame.id+' '+$side+' inventory fixture</text></svg>';[IO.File]::WriteAllText((Join-Path $directory $filename),$svg);$figures.products+=,@{filename=$filename;frame_id=$frame.id;side=$side;svg=$svg;svg_sha256=(File-Hash (Join-Path $directory $filename))}}}
 $json=@{'design-content.json'=$content;'design-figures.json'=$figures;'energy-report.json'=$content.workbench.base;'workbench-report.json'=$content.workbench;'performance.json'=@{fixture='not calculated'};'design-changes.json'=@();'sensitivity.json'=$null;'design-proposals.json'=@();'typed-input-differences.json'=@()}
 if($null -ne $Drawing){$json['design-drawing.json']=$content.drawing;$json['design-scope-revision.json']=$content.drawing.scope_revision}
 foreach($name in $json.Keys){Write-Value (Join-Path $directory $name) $json[$name]}
 foreach($name in @('performance.csv','inputs.csv','design-changes.csv','baseline-model.svg','proposed-model.svg','annual.svg','monthly.svg','design-executive.html','design-technical.html','input-appendix.html','performance.html','design-executive.pdf','design-technical.pdf','input-appendix.pdf','performance.pdf','performance-page-001.png')){[IO.File]::WriteAllText((Join-Path $directory $name),'Original inert inventory fixture, deliberately not a rendered product or accepted simulation record.')}
 return (Seal-InertProducts)
}
function Seal-InertProducts {
 $names=[string[]]@(Get-ChildItem -LiteralPath $directory -File|Where-Object {$_.Name -cne 'design-products.json'}|ForEach-Object {$_.Name});[Array]::Sort($names,[StringComparer]::Ordinal)
 $products=@($names|ForEach-Object {$p=Join-Path $directory $_;@{path=$_;bytes=[long](Get-Item -LiteralPath $p).Length;sha256=(File-Hash $p)}})
 $inventory=@{schema_version='greenlens.design-report-products.v1';content_sha256=$reportId;products_sha256=('e'*64);products=@($products)}
 Write-Value (Join-Path $directory 'design-products.json') $inventory
 return @{id=$reportId;path=('design-reports/'+$reportId);content_sha256=$reportId;products_sha256=('e'*64);include_pdf=$true;manifest_file_sha256=(File-Hash (Join-Path $directory 'design-products.json'));baseline=@{run_id=$arm.baseline_run_id;record_sha256=('a'*64);design_id='A'};proposed=@{run_id=$arm.proposed_run_id;record_sha256=('b'*64);design_id='B'}}
}
function Inspect-Inert($Summary,$Selection){Assert-ObservedDesignProducts $root $Summary ${function:File-Hash} ${function:Read-BoundedJson} ${function:Portable-Child} $Selection $typstHash $manifest}
function Edit-InertJson($Name,[scriptblock]$Edit){$path=Join-Path $directory $Name;$value=Read-BoundedJson $path 48MB;& $Edit $value;Write-Value $path $value;return (Seal-InertProducts)}
function New-InertDrawing($Selection){
 $retained=(Get-ObservedScopeSelection $root $manifest $Selection.scope_revision_sha256).revision
 $frames=@((New-InertFrame 'axonometric'),(New-InertFrame 'plan'))
 for($i=0;$i -lt $Selection.accepted_view_ids.Count;$i++){$view=@($retained.scope.report_views|Where-Object {$_.id -ceq $Selection.accepted_view_ids[$i]})[0];$frame=New-InertFrame ('accepted-'+($i+1));foreach($field in @('eye_m','target_m','up','vertical_span_m','aspect_ratio','section')){$frame[$field]=$view[$field]};$frames+=,$frame}
 $context=$null
 if($Selection.include_reference_context){$context=@{schema_version='greenlens.paired-reference-context.v1';scope_revision_sha256=$retained.revision_sha256;reference_revision_sha256=$retained.reference.manifest.revision_sha256;reference_geometry_sha256=$retained.reference.manifest.geometry_sha256;manifest_sha256=$retained.reference.manifest_sha256;content_sha256=('f'*64);layers=@($frames|ForEach-Object {@{frame_sha256=('e'*64);source_triangle_count=1;visible_triangle_count=0;omitted_visible_triangle_count=0;triangles=@()}})}}
 return @{schema_version='greenlens.design-report-drawing.v1';selection=(Copy-Value $Selection);scope_revision=(Copy-Value $retained);frames=$frames;context=$context}
}
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
 Check 'Actual helper verifies complete inert family, exact declarations and typed renderer' ($result.product_count -eq 31 -and $result.pdf_count -eq 4 -and $result.analytical_scope -ceq $arm.analytical_scope)
 $listedBytes=[long](($result.products.products|Measure-Object -Property bytes -Sum).Sum);$inventoryBytes=(Get-Item -LiteralPath (Join-Path $directory 'design-products.json')).Length
 Check 'Listed product byte receipt semantics remain distinct from inventory and aggregate directory bytes' ($result.product_bytes -eq $listedBytes -and $result.inventory_bytes -eq $inventoryBytes -and $result.directory_bytes -eq $listedBytes+$inventoryBytes)
 Check 'Cold typed product check accepts the same analytical inventory' ((Assert-Products $directory $result.products).product_count -eq 31)
 $ReportKind='ArchitecturalClosure';Reject 'Architectural reader rejects analytical product schema' {Assert-Products $directory $result.products};$ReportKind='AnalyticalDesign'
 $bad=Copy-Value $summary;$bad.baseline.record_sha256='f'*64;Reject 'Resealed report cannot substitute the selected record' {Inspect-Inert $bad $selection}
 $bad=Copy-Value $summary;$bad.path='report-issues/'+$reportId;Reject 'Report-family path substitution rejected' {Inspect-Inert $bad $selection}
 $bad=Copy-Value $summary;$bad.include_pdf='true';Reject 'String PDF selection cannot replace a typed Boolean' {Inspect-Inert $bad $selection}
 $bad=Copy-Value $selection;$bad.north_declaration='Invented north';Reject 'North declaration drift rejected' {Inspect-Inert $summary $bad}
 $bad=Copy-Value $selection;$bad.analytical_scope='Different scope';Reject 'Scope declaration drift rejected' {Inspect-Inert $summary $bad}
 [IO.File]::AppendAllText((Join-Path $directory 'performance.pdf'),'tamper');Reject 'Actual product tamper rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 [IO.File]::WriteAllText((Join-Path $directory 'unadvertised.txt'),'Original extra fixture');Reject 'Unadvertised products rejected' {Inspect-Inert $summary $selection};Remove-Item -LiteralPath (Join-Path $directory 'unadvertised.txt')
 $content=Read-BoundedJson (Join-Path $directory 'design-content.json') 48MB;$content.workbench.base.proposed.record_sha256='f'*64;Write-Value (Join-Path $directory 'design-content.json') $content;$summary=Seal-InertProducts
 Reject 'Resealed canonical content with wrong selected record rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 $content=Read-BoundedJson (Join-Path $directory 'design-content.json') 48MB;$content.renderer.method='different-renderer';Write-Value (Join-Path $directory 'design-content.json') $content;$summary=Seal-InertProducts
 Reject 'Resealed content with unsupported renderer rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 $content=Read-BoundedJson (Join-Path $directory 'design-content.json') 48MB;$content.analytical_scope='';Write-Value (Join-Path $directory 'design-content.json') $content;$summary=Seal-InertProducts
 Reject 'Cold preflight rejects resealed empty scope without needing an arm marker' {Inspect-Inert $summary $null};$summary=New-InertProducts
 $inventory=Read-BoundedJson (Join-Path $directory 'design-products.json') 256KB;[Array]::Reverse($inventory.products);Write-Value (Join-Path $directory 'design-products.json') $inventory;$summary.manifest_file_sha256=File-Hash (Join-Path $directory 'design-products.json')
 Reject 'Resealed unsorted product inventory rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 # Complete-family failures are consistently resealed, so byte hashes alone
 # cannot hide an omitted appendix, figure, table, sensitivity null or PNG.
 foreach($missing in @('design-technical.pdf','input-appendix.html','performance-page-001.png','design-plan-overlay.svg','inputs.csv','design-changes.json','sensitivity.json','design-proposals.json')){
  Remove-Item -LiteralPath (Join-Path $directory $missing);$summary=Seal-InertProducts
  Reject ('Resealed missing required family member rejected: '+$missing) {Inspect-Inert $summary $selection};$summary=New-InertProducts
 }
 foreach($extra in @('extra.pdf','extra.svg','performance-page-003.png')){[IO.File]::WriteAllText((Join-Path $directory $extra),'Original inert extra');$summary=Seal-InertProducts;Reject ('Resealed unexpected/gapped product rejected: '+$extra) {Inspect-Inert $summary $selection};$summary=New-InertProducts}
 $summary=Edit-InertJson 'design-figures.json' {param($v)$v.request.frames[0].aspect_ratio=7};Reject 'Resealed figure frame drift rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 $summary=Edit-InertJson 'design-figures.json' {param($v)$v.products[0].svg='Changed SVG'};Reject 'Resealed SVG data/file mismatch rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 $summary=Edit-InertJson 'energy-report.json' {param($v)$v.proposed.label='Wrong case'};Reject 'Resealed canonical snapshot projection mismatch rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 $summary=Edit-InertJson 'design-content.json' {param($v)$v.schema_version='greenlens.design-report-content.v3'};Reject 'Unknown report codec rejected' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 # V1 permits 2–4 safe uniquely named frames, including custom elevations and
 # sections. Only v2 imposes the exact default/accepted sequence.
 $customIso=New-InertFrame 'Iso_View1';$customIso.kind='axonometric';$customIso.eye_m=@(10,-10,8);$customIso.up=@(0,0,1)
 $customFront=New-InertFrame 'South-Elevation';$customFront.kind='front';$customFront.eye_m=@(0,-10,0);$customFront.up=@(0,0,1);$customFront.section=@{axis='y';position_m=2;keep='greater'}
 $customRight=New-InertFrame 'East_Section';$customRight.kind='right';$customRight.eye_m=@(10,0,0);$customRight.up=@(0,0,1);$customRight.section=@{axis='x';position_m=2;keep='less'}
 $customTop=New-InertFrame 'Roof_Plan'
 foreach($frames in @(@($customIso,$customFront),@($customFront,$customIso,$customRight),@($customTop,$customRight,$customFront,$customIso))){$summary=New-InertProducts -Frames $frames;$customResult=Inspect-Inert $summary $selection;Check ('Historical v1 custom elevation/section family with '+$frames.Count+' frames remains supported') ($customResult.product_count -eq 25+3*$frames.Count)}
 foreach($badId in @('../escape','bad/name','bad\name','bad:name','空間','',('x'*81),"good`n")){$summary=New-InertProducts;$summary=Edit-InertJson 'design-content.json' {param($v)$v.frames[0].id=$badId};Reject ('Unsafe or out-of-bound v1 frame ID rejected: '+$badId) {Inspect-Inert $summary $selection}}
 $summary=New-InertProducts;$summary=Edit-InertJson 'design-content.json' {param($v)$v.frames[1].id=$v.frames[0].id};Reject 'Duplicate v1 frame IDs rejected before deriving product paths' {Inspect-Inert $summary $selection};$summary=New-InertProducts
 $referenceSource=[IO.File]::ReadAllText((Join-Path $PSScriptRoot '../crates/greenlens-core/src/reference/mod.rs'))
 foreach($constant in @(@('MAX_TRIANGLES','1_000_000'),@('TRIANGLES_PER_CHUNK','8192'),@('CHUNK_HEADER_BYTES','16'),@('TRIANGLE_BYTES','80'),@('MAX_NORMALIZED_BYTES','80 * 1024 * 1024'))){Check ('Observer byte admission stays aligned with Rust '+$constant[0]) ($referenceSource.Contains(('pub const '+$constant[0]+': usize = '+$constant[1]+';')))}
 Check 'Reference chunk limits derive exactly from declared Rust constants' ([int][Math]::Ceiling(1000000.0/8192) -eq 123 -and (16+8192*80) -eq 655376)
 # Actual retained bytes for an original inert reference/history envelope.
 # These are deliberately not admitted geometry or accepted scope revisions.
 $assetId='original-inert-reference';$assetPath='references/'+$assetId;$assetDir=Join-Path $root $assetPath;New-Item -ItemType Directory -Path $assetDir|Out-Null
 $chunkPath=Join-Path $assetDir 'chunk-0000.bin';[IO.File]::WriteAllText($chunkPath,'Original inert reference bytes; not decoded geometry.')
 $ref=@{schema_version='greenlens.reference-model.v1';revision_sha256=('1'*64);geometry_sha256=('2'*64);triangle_count=1;chunks=@(@{path='chunk-0000.bin';sha256=(File-Hash $chunkPath);byte_length=(Get-Item -LiteralPath $chunkPath).Length;triangle_count=1})}
 Write-Value (Join-Path $assetDir 'reference.json') $ref;$refHash=File-Hash (Join-Path $assetDir 'reference.json')
 Write-Value (Join-Path $assetDir 'checksums.json') @{schema_version='0.1.0';sha256=@{'reference.json'=$refHash;'chunk-0000.bin'=$ref.chunks[0].sha256}}
 $scopeId='3'*64;$views=@()
 foreach($id in @('Top 空間','Section retained')){$view=New-InertFrame $id;$view.Remove('kind');$view.design_id='B';$view.model_input_sha256=$caseB.model_input_sha256;if($id -ceq 'Section retained'){$view.aspect_ratio=0.75;$view.eye_m=@(0,-10,0);$view.up=@(0,0,1);$view.section=@{axis='y';position_m=2;keep='greater'}};$views+=,$view}
 $revision=@{schema_version='greenlens.scope-revision.v3';revision_sha256=$scopeId;scope=@{report_views=$views;design_model_sha256=@{A=$caseA.model_input_sha256;B=$caseB.model_input_sha256}};designs=@{A=@{model=$caseA.model;calendar=$caseA.study.calendar;study=$caseA.study};B=@{model=$caseB.model;calendar=$caseB.study.calendar;study=$caseB.study}};reference=@{asset_id=$assetId;manifest_sha256=$refHash;manifest=$ref}}
 $other=Copy-Value $revision;$other.revision_sha256='4'*64
 $manifest.architecture=@{assets=@(@{id=$assetId;path=$assetPath;revision_sha256=$ref.revision_sha256;geometry_sha256=$ref.geometry_sha256;manifest_sha256=$refHash});scope_history=@{schema_version='greenlens.scope-history.v1';revisions=@($revision,$other);active_revision_sha256=$other.revision_sha256}}
 $drawingSelection=@{schema_version='greenlens.design-report-drawing-selection.v1';scope_revision_sha256=$scopeId;accepted_view_ids=@('Top 空間','Section retained');include_reference_context=$true}
 $drawingArm=Copy-Value $arm;$drawingArm.schema_version='greenlens.native-analytical-design-report-arm.v2';$drawingArm.drawing_selection=$drawingSelection
 $armedDrawing=Get-ObservedDesignArmSelection $drawingArm $manifest $manifest.id $root
 $drawing=New-InertDrawing $drawingSelection;$summary=New-InertProducts $drawing;$result=Inspect-Inert $summary $armedDrawing
 Check 'V2 exact historical scope two ordered accepted views and neutral context pass structurally' ($result.product_count -eq 39 -and $result.drawing.selection.scope_revision_sha256 -ceq $scopeId -and $result.drawing.selection.accepted_view_ids[0] -ceq 'Top 空間')
 Check 'Cold v2 preflight verifies exact historical source without an arm marker' ((Inspect-Inert $summary $null).drawing.selection.include_reference_context)
 Check 'Observer does not claim Rust geometry semantics' ($result.check.Contains('Rust canonical identities, geometry/context reduction'))
 Reject 'Bound drawing cannot be silently accepted by old analytical-only arm' {Inspect-Inert $summary $selection}
 $summary=New-InertProducts;Reject 'Armed drawing cannot disappear into valid v1 analytical-only report' {Inspect-Inert $summary $armedDrawing};$summary=New-InertProducts $drawing
 $bad=Copy-Value $drawingArm;$bad.schema_version='greenlens.native-analytical-design-report-arm.v1';Reject 'Drawing under old arm codec rejected' {Get-ObservedDesignArmSelection $bad $manifest $manifest.id $root}
 $bad=Copy-Value $drawingArm;$bad.drawing_selection=$null;Reject 'Missing v2 drawing declaration rejected' {Get-ObservedDesignArmSelection $bad $manifest $manifest.id $root}
 foreach($field in @('schema_version','scope_revision_sha256','accepted_view_ids','include_reference_context')){$bad=Copy-Value $drawingArm;$bad.drawing_selection[$field]=switch($field){'schema_version'{'future'}'scope_revision_sha256'{'f'*64}'accepted_view_ids'{@('Top 空間','Top 空間')}'include_reference_context'{'true'}};Reject ('Invalid typed drawing selection rejected: '+$field) {Get-ObservedDesignArmSelection $bad $manifest $manifest.id $root}}
 $bad=Copy-Value $drawingArm;$bad.drawing_selection.accepted_view_ids=@('Absent display name');Reject 'View names cannot substitute exact historical IDs' {Get-ObservedDesignArmSelection $bad $manifest $manifest.id $root}
 $bad=Copy-Value $drawingArm;$bad.drawing_selection.accepted_view_ids=@('Section retained','Top 空間');$wrongOrder=Get-ObservedDesignArmSelection $bad $manifest $manifest.id $root;Reject 'Selected view order drift rejected' {Inspect-Inert $summary $wrongOrder}
 foreach($field in @('eye_m','target_m','up','vertical_span_m','aspect_ratio','section')){$bad=Copy-Value $drawing;$bad.frames[2][$field]=switch($field){'eye_m'{@(1,0,10)}'target_m'{@(1,0,0)}'up'{@(1,0,0)}'vertical_span_m'{9}'aspect_ratio'{9}'section'{@{axis='z';position_m=1;keep='greater'}}};$summary=New-InertProducts $bad;Reject ('Resealed accepted frame field drift rejected: '+$field) {Inspect-Inert $summary $armedDrawing}}
 $summary=New-InertProducts $drawing
 foreach($missing in @('design-drawing.json','design-scope-revision.json','design-accepted-2-overlay.svg')){Remove-Item -LiteralPath (Join-Path $directory $missing);$summary=Seal-InertProducts;Reject ('Resealed v2 required artifact omission rejected: '+$missing) {Inspect-Inert $summary $armedDrawing};$summary=New-InertProducts $drawing}
 $summary=Edit-InertJson 'design-scope-revision.json' {param($v)$v.scope.note='Changed'};Reject 'Resealed complete scope artifact drift rejected' {Inspect-Inert $summary $armedDrawing};$summary=New-InertProducts $drawing
 foreach($field in @('scope_revision_sha256','reference_revision_sha256','reference_geometry_sha256','manifest_sha256')){$bad=Copy-Value $drawing;$bad.context[$field]='9'*64;$summary=New-InertProducts $bad;Reject ('Resealed context binding drift rejected: '+$field) {Inspect-Inert $summary $armedDrawing}}
 $bad=Copy-Value $drawing;$bad.context=$null;$summary=New-InertProducts $bad;Reject 'Requested context cannot disappear' {Inspect-Inert $summary $armedDrawing}
 $summary=New-InertProducts $drawing;[IO.File]::AppendAllText($chunkPath,'tamper');Reject 'Actual historical source bytes checked despite matching descriptors' {Inspect-Inert $summary $armedDrawing};[IO.File]::WriteAllText($chunkPath,'Original inert reference bytes; not decoded geometry.')
 $scopeOnly=Copy-Value $drawingSelection;$scopeOnly.accepted_view_ids=@();$scopeOnly.include_reference_context=$false;$scopeOnlyArm=Copy-Value $drawingArm;$scopeOnlyArm.drawing_selection=$scopeOnly;$scopeOnlySelection=Get-ObservedDesignArmSelection $scopeOnlyArm $manifest $manifest.id $root;$summary=New-InertProducts (New-InertDrawing $scopeOnly)
 Check 'Scope-only v2 with zero accepted views and explicit no context remains supported' ((Inspect-Inert $summary $scopeOnlySelection).product_count -eq 33)
 # Versioned external history uses the actual held-byte reader, not expected paths.
 $historyDir=Join-Path $root 'scope-history';New-Item -ItemType Directory -Path $historyDir|Out-Null;$historyFile=Join-Path $historyDir ($scopeId+'.json');Write-Value $historyFile $revision
 $manifest.architecture.scope_history=@{schema_version='greenlens.scope-history.v2';entries=@(@{revision_sha256=$scopeId;path=('scope-history/'+$scopeId+'.json');bytes=(Get-Item -LiteralPath $historyFile).Length;sha256=(File-Hash $historyFile)});active_revision_sha256=$scopeId}
 $externalSelection=Get-ObservedDesignArmSelection $drawingArm $manifest $manifest.id $root;$summary=New-InertProducts $drawing
 Check 'External scope history exact bytes are supported' ((Inspect-Inert $summary $externalSelection).drawing.scope_storage_schema -ceq 'greenlens.scope-history.v2')
 [IO.File]::AppendAllText($historyFile,'tamper');Reject 'External history tamper cannot be replaced by embedded report scope' {Inspect-Inert $summary $externalSelection};Write-Value $historyFile $revision
 $changed=Copy-Value $revision;$changed.scope.extra='Resealed after arm';Write-Value $historyFile $changed;$manifest.architecture.scope_history.entries[0].bytes=(Get-Item -LiteralPath $historyFile).Length;$manifest.architecture.scope_history.entries[0].sha256=File-Hash $historyFile
 Reject 'Consistently resealed changed scope still differs from the armed snapshot' {Inspect-Inert $summary $externalSelection};Write-Value $historyFile $revision;$manifest.architecture.scope_history.entries[0].bytes=(Get-Item -LiteralPath $historyFile).Length;$manifest.architecture.scope_history.entries[0].sha256=File-Hash $historyFile
 $summary=New-InertProducts
 $nativeFile=Join-Path $PSScriptRoot 'supervise-v08-report-review.ps1';$nativeText=[IO.File]::ReadAllText($nativeFile);$tokens=$null;$errors=$null;$nativeAst=[Management.Automation.Language.Parser]::ParseFile($nativeFile,[ref]$tokens,[ref]$errors)
 Check 'Native collector with separate mode has no parser errors' ($errors.Count -eq 0)
 foreach($name in @('Read-Json','Member-Path','Stable-Value','Json-Identity','Issues','Assert-OldIssues','Assert-NoNewRuns','Assert-ArmSelection','Assert-IssueProducts')){$definition=$nativeAst.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq $name},$true);. ([scriptblock]::Create($definition.Extent.Text))}
 $ReportKind='AnalyticalDesign';Check 'Actual native arm dispatch invokes analytical exact bindings' ((Assert-ArmSelection $arm $manifest $manifest.id).analytical_scope -ceq $arm.analytical_scope)
 $projectRoot=$root;$armSelection=Assert-ArmSelection $drawingArm $manifest $manifest.id;$identities=@{typst_executable_sha256=$typstHash};$summary=New-InertProducts $drawing
 Check 'Actual native v2 arm dispatch resolves the exact historical sidecar' ($armSelection.drawing.scope_member_sha256 -ceq $manifest.architecture.scope_history.entries[0].sha256)
 Check 'Actual native v2 publication dispatch verifies complete bound family' ((Assert-IssueProducts $summary $manifest).product_count -eq 39)
 $summary=New-InertProducts;Reject 'Actual native dispatch rejects dropped drawing before a success receipt' {Assert-IssueProducts $summary $manifest}
 foreach($shape in @(@{name='empty';value=@()},@{name='singleton';value=@('one')},@{name='null';value=$null})){$shapePath=Join-Path $output ($shape.name+'.json');Write-Value $shapePath $shape.value;Assert-ObservedDesignEqual (Read-BoundedJson $shapePath 1024) $shape.value ('cold reader '+$shape.name);Assert-ObservedDesignEqual (Read-Json $shapePath 1024) $shape.value ('native reader '+$shape.name);Check ('Actual collectors preserve root JSON shape: '+$shape.name) $true}
 $before=Copy-Value $manifest;$before.report_issues=@{issues=@(@{id=('1'*64)})};$after=Copy-Value $before;$after.design_reports=@{schema_version='greenlens.design-report-index.v1';reports=@($summary)}
 Assert-OldIssues $before $after;Assert-NoNewRuns $before $after;Check 'New analytical report preserves the separate historical family and runs' $true
 $bad=Copy-Value $after;$bad.report_issues.issues[0].id='2'*64;Reject 'Changed architectural issue rejected during analytical mode' {Assert-OldIssues $before $bad}
 $before=Copy-Value $after;$bad=Copy-Value $after;$bad.design_reports.reports[0].products_sha256='f'*64;Reject 'Historical analytical summary replacement rejected' {Assert-OldIssues $before $bad}
 $coldFile=Join-Path $PSScriptRoot 'regenerate-v08-review-once.ps1';$tokens=$null;$errors=$null;$coldAst=[Management.Automation.Language.Parser]::ParseFile($coldFile,[ref]$tokens,[ref]$errors)
 $preflight=$coldAst.Find({param($node)$node -is [Management.Automation.Language.IfStatementAst] -and $node.Extent.Text.StartsWith("if(`$ReportKind -ceq 'AnalyticalDesign'){") -and $node.Extent.Text.Contains('$designObservation=Assert-ObservedDesignProducts')},$true)
 $projectJson=$manifest;$issue=New-InertProducts $drawing;. ([scriptblock]::Create($preflight.Extent.Text));Check 'Actual cold v2 preflight runs shared history and complete-family checks before launch' ($designObservation.product_count -eq 39)
 $summary=New-InertProducts
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
