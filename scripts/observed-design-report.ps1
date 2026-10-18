# Original GreenLens source, Apache-2.0. Pure bounded analytical-report binding
# checks shared by the existing native/cold collectors. No process launch.
function Get-ObservedDesignReports($Manifest){
 if(!$Manifest.Contains('design_reports') -or $null -eq $Manifest.design_reports){return @()}
 $index=$Manifest.design_reports
 if($index.schema_version -cne 'greenlens.design-report-index.v1' -or $index.reports -isnot [array] -or $index.reports.Count -gt 32 -or [Text.Encoding]::UTF8.GetByteCount((ConvertTo-Json -InputObject $index -Compress -Depth 100)) -gt 256KB){throw 'Invalid bounded analytical design-report index.'}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($report in $index.reports){if($report.id -cnotmatch '^[a-f0-9]{64}$' -or !$seen.Add($report.id)){throw 'Invalid or duplicate analytical report identity.'}}
 return @($index.reports)
}
function ConvertTo-ObservedDesignStable($Value){
 if($null -eq $Value){return $null}
 if($Value -is [Collections.IDictionary]){$r=[ordered]@{};$keys=[string[]]@($Value.Keys);[Array]::Sort($keys,[StringComparer]::Ordinal);foreach($k in $keys){$r[$k]=ConvertTo-ObservedDesignStable $Value[$k]};return $r}
 if($Value -is [Collections.IEnumerable] -and $Value -isnot [string]){$r=[Collections.Generic.List[object]]::new();foreach($v in $Value){$r.Add((ConvertTo-ObservedDesignStable $v))};return ,$r.ToArray()}
 return $Value
}
function Get-ObservedDesignFingerprint($Value){
 # Observer structural fingerprint only, never a replacement Rust identity.
 $text=ConvertTo-Json -InputObject (ConvertTo-ObservedDesignStable $Value) -Compress -Depth 100
 return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($text))).ToLowerInvariant()
}
function Assert-ObservedDesignEqual($Actual,$Expected,[string]$What){if((Get-ObservedDesignFingerprint $Actual) -cne (Get-ObservedDesignFingerprint $Expected)){throw "Observed $What differs from its exact retained fields."}}
function Get-ObservedDesignDrawingSelection([string]$Root,$Manifest,$Selection){
 if($Selection -isnot [Collections.IDictionary] -or $Selection.Count -ne 4 -or $Selection.schema_version -cne 'greenlens.design-report-drawing-selection.v1' -or $Selection.scope_revision_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $Selection.accepted_view_ids -isnot [array] -or $Selection.accepted_view_ids.Count -gt 2 -or $Selection.include_reference_context -isnot [bool]){throw 'Invalid explicit bounded drawing selection.'}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($id in $Selection.accepted_view_ids){if($id -isnot [string] -or [string]::IsNullOrWhiteSpace($id) -or [Text.Encoding]::UTF8.GetByteCount($id) -gt 128 -or !$seen.Add($id)){throw 'Invalid or duplicate exact accepted view identity.'}}
 $observed=Get-ObservedScopeSelection $Root $Manifest $Selection.scope_revision_sha256
 $revision=$observed.revision
 if($revision.schema_version -cnotin @('greenlens.scope-revision.v1','greenlens.scope-revision.v2','greenlens.scope-revision.v3')){throw 'Unsupported selected historical scope codec.'}
 $views=@();if($revision.scope.Contains('report_views')){$views=$revision.scope.report_views}
 if($views -isnot [array] -or $views.Count -gt 8){throw 'Invalid bounded accepted view inventory.'}
 foreach($id in $Selection.accepted_view_ids){if(@($views|Where-Object {$_.id -ceq $id}).Count -ne 1){throw 'An exact selected view is missing or ambiguous in the selected historical scope.'}}
 return [ordered]@{selection=$Selection;scope_observer_sha256=(Get-ObservedDesignFingerprint $revision);scope_storage_schema=$observed.storage_schema;scope_member_sha256=$observed.member_sha256}
}
function Get-ObservedDesignArmSelection($Candidate,$Manifest,[string]$ExpectedProjectId,[string]$Root){
 if($Candidate.project_id -cne $ExpectedProjectId -or $Manifest.id -cne $ExpectedProjectId){throw 'Analytical arm and saved manifest must identify the declared document.'}
 if($Candidate.Contains('scope_revision_sha256')){throw 'Analytical report arming must not substitute an architectural scope revision.'}
 if(!$Candidate.baseline_run_id -or !$Candidate.proposed_run_id -or $Candidate.baseline_run_id -ceq $Candidate.proposed_run_id){throw 'Analytical arm requires distinct exact A/B run identities.'}
 $baseline=@($Manifest.energy.runs|Where-Object {$_.id -ceq $Candidate.baseline_run_id})
 $proposed=@($Manifest.energy.runs|Where-Object {$_.id -ceq $Candidate.proposed_run_id})
 if($baseline.Count -ne 1 -or $proposed.Count -ne 1 -or $baseline[0].alternative_id -cne 'A' -or $proposed[0].alternative_id -cne 'B'){throw 'The analytical arm must bind exactly one retained A and B record.'}
 foreach($run in @($baseline[0],$proposed[0])){if($run.record_sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'An armed analytical run lacks its retained record hash.'}}
 Assert-ObservedDesignDeclarations $Candidate
 $drawing=$null
 if($Candidate.schema_version -ceq 'greenlens.native-analytical-design-report-arm.v2'){
  if(!$Candidate.Contains('drawing_selection') -or $null -eq $Candidate.drawing_selection){throw 'Drawing arm v2 requires an explicit drawing selection; use v1 for analytical-only reporting.'}
  $drawing=Get-ObservedDesignDrawingSelection $Root $Manifest $Candidate.drawing_selection
 }elseif(($Candidate.Contains('schema_version') -and $Candidate.schema_version -cne 'greenlens.native-analytical-design-report-arm.v1') -or $Candidate.Contains('drawing_selection')){throw 'Unknown analytical arm or drawing supplied under the analytical-only v1 contract.'}
 return [ordered]@{baseline=[ordered]@{run_id=$baseline[0].id;record_sha256=$baseline[0].record_sha256;design_id='A'};proposed=[ordered]@{run_id=$proposed[0].id;record_sha256=$proposed[0].record_sha256;design_id='B'};analytical_scope=$Candidate.analytical_scope;north_declaration=$Candidate.north_declaration;drawing=$drawing}
}
function Assert-ObservedDesignDeclarations($Value){
 if($Value.analytical_scope -isnot [string] -or [string]::IsNullOrWhiteSpace($Value.analytical_scope) -or $Value.analytical_scope -cne $Value.analytical_scope.Trim() -or [Text.Encoding]::UTF8.GetByteCount($Value.analytical_scope) -gt 2048){throw 'Analytical scope requires an exact nonempty trimmed declaration within 2048 UTF-8 bytes.'}
 if(!$Value.Contains('north_declaration') -or ($null -ne $Value.north_declaration -and ($Value.north_declaration -isnot [string] -or [string]::IsNullOrWhiteSpace($Value.north_declaration) -or $Value.north_declaration -cne $Value.north_declaration.Trim() -or [Text.Encoding]::UTF8.GetByteCount($Value.north_declaration) -gt 2048))){throw 'Analytical north must be explicitly null (unknown) or an exact bounded declaration.'}
}
function Assert-ObservedDesignBinding($Actual,$Expected){
 foreach($name in @('run_id','record_sha256','design_id')){if(!$Actual[$name] -or $Actual[$name] -cne $Expected[$name]){throw 'Analytical report differs from its exact selected run/record/design binding.'}}
}
function Assert-ObservedDesignDrawing([string]$Root,$Project,$Content,$Armed,[scriptblock]$Hash,[scriptblock]$Read,[scriptblock]$Child){
 $drawing=$Content.drawing
 if($drawing -isnot [Collections.IDictionary] -or $drawing.schema_version -cne 'greenlens.design-report-drawing.v1'){throw 'Missing or unsupported retained drawing.'}
 $binding=Get-ObservedDesignDrawingSelection $Root $Project $drawing.selection
 if($null -ne $Armed){Assert-ObservedDesignEqual $binding $Armed 'armed drawing/scope selection'}
 if((Get-ObservedDesignFingerprint $drawing.scope_revision) -cne $binding.scope_observer_sha256){throw 'Embedded drawing scope differs from actual selected history.'}
 $revision=$drawing.scope_revision;$views=$drawing.selection.accepted_view_ids
 Assert-ObservedDesignEqual $drawing.frames $Content.frames 'drawing frames'
 if($Content.frames.Count -ne 2+$views.Count){throw 'Drawing frames do not preserve every selected accepted view.'}
 foreach($case in @($Content.workbench.base.baseline,$Content.workbench.base.proposed)){
  $snapshot=$revision.designs[$case.id]
  if($null -eq $snapshot -or $revision.scope.design_model_sha256[$case.id] -cne $case.model_input_sha256){throw 'Scope does not bind the exact selected report model.'}
  Assert-ObservedDesignEqual $snapshot.model $case.model 'scope/model snapshot'
  Assert-ObservedDesignEqual $snapshot.calendar $case.study.calendar 'scope/calendar snapshot'
  if($null -ne $snapshot.study){Assert-ObservedDesignEqual $snapshot.study $case.study 'scope/study snapshot'}
 }
 for($i=0;$i -lt $views.Count;$i++){
  $view=@($revision.scope.report_views|Where-Object {$_.id -ceq $views[$i]})[0];$frame=$Content.frames[$i+2]
  $case=@($Content.workbench.base.baseline,$Content.workbench.base.proposed)|Where-Object {$_.id -ceq $view.design_id}
  if($null -eq $case -or $case.model_input_sha256 -cne $view.model_input_sha256 -or $frame.id -cne ('accepted-'+($i+1))){throw 'Accepted view design/model/order differs from the retained pair.'}
  foreach($field in @('eye_m','target_m','up','vertical_span_m','aspect_ratio','section')){Assert-ObservedDesignEqual $frame[$field] $view[$field] ('accepted frame '+$field)}
 }
 $reference=$revision.reference
 if($Project.architecture.assets -isnot [array] -or $Project.architecture.assets.Count -gt 8){throw 'Invalid bounded reference asset inventory.'}
 $assets=@($Project.architecture.assets|Where-Object {$_.id -ceq $reference.asset_id})
 if($assets.Count -ne 1){throw 'Exact historical reference asset is missing or ambiguous.'};$asset=$assets[0]
 if($asset.path -cne ('references/'+$asset.id) -or $asset.manifest_sha256 -cne $reference.manifest_sha256 -or $asset.revision_sha256 -cne $reference.manifest.revision_sha256 -or $asset.geometry_sha256 -cne $reference.manifest.geometry_sha256){throw 'Reference descriptor differs from retained scope.'}
 $assetRoot=& $Child $Root $asset.path;$manifestPath=& $Child $assetRoot 'reference.json'
 if((& $Hash $manifestPath) -cne $reference.manifest_sha256){throw 'Retained reference manifest bytes differ.'}
 $manifest=& $Read $manifestPath 16MB
 Assert-ObservedDesignEqual $manifest $reference.manifest 'reference manifest'
 # Derived from reference/mod.rs MAX_TRIANGLES / TRIANGLES_PER_CHUNK and
 # CHUNK_HEADER_BYTES + TRIANGLES_PER_CHUNK * TRIANGLE_BYTES. Source checks
 # fence these constants; they are byte admission, not a geometry decoder.
 $maxReferenceChunks=[int][Math]::Ceiling(1000000.0/8192);$maxReferenceChunkBytes=16+8192*80
 if((& $Hash $manifestPath) -cne $reference.manifest_sha256 -or $manifest.schema_version -cne 'greenlens.reference-model.v1' -or $manifest.chunks -isnot [array] -or $manifest.chunks.Count -gt $maxReferenceChunks){throw 'Changed or unbounded reference manifest.'}
 $seal=& $Read (& $Child $assetRoot 'checksums.json') 64KB
 if($seal.schema_version -cne '0.1.0' -or $seal.sha256.Count -ne $manifest.chunks.Count+1 -or $seal.sha256['reference.json'] -cne $reference.manifest_sha256){throw 'Reference seal differs from its bound manifest.'}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal);[long]$total=0
 foreach($chunk in $manifest.chunks){
  if($chunk.path -isnot [string] -or !$seen.Add($chunk.path) -or $chunk.sha256 -cnotmatch '^[a-f0-9]{64}$' -or $chunk.byte_length -le 0 -or $chunk.byte_length -gt $maxReferenceChunkBytes -or $seal.sha256[$chunk.path] -cne $chunk.sha256){throw 'Invalid bounded reference chunk descriptor.'}
  $total+=$chunk.byte_length;if($total -gt 80MB){throw 'Reference normalized bytes exceed admission.'}
  $path=& $Child $assetRoot $chunk.path;if((Get-Item -LiteralPath $path).Length -ne $chunk.byte_length -or (& $Hash $path) -cne $chunk.sha256){throw 'Actual retained reference chunk differs.'}
 }
 $context=$drawing.context
 if($drawing.selection.include_reference_context -ne ($null -ne $context)){throw 'Requested neutral reference context disappeared or was added silently.'}
 if($null -ne $context){
  if($context.schema_version -cne 'greenlens.paired-reference-context.v1' -or $context.scope_revision_sha256 -cne $revision.revision_sha256 -or $context.reference_revision_sha256 -cne $manifest.revision_sha256 -or $context.reference_geometry_sha256 -cne $manifest.geometry_sha256 -or $context.manifest_sha256 -cne $reference.manifest_sha256 -or $context.content_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $context.layers -isnot [array] -or $context.layers.Count -ne $Content.frames.Count){throw 'Neutral context binding/shape differs from actual retained source and frames.'}
  foreach($layer in $context.layers){if($layer.frame_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $layer.triangles -isnot [array] -or $layer.triangles.Count -gt 8000 -or $layer.source_triangle_count -ne $manifest.triangle_count -or $layer.visible_triangle_count -gt $layer.source_triangle_count -or $layer.omitted_visible_triangle_count+$layer.triangles.Count -ne $layer.visible_triangle_count){throw 'Invalid bounded context layer inventory.'}}
 }
 return $binding
}
function Assert-ObservedDesignFamily([string]$Directory,$Content,$Manifest,[scriptblock]$Hash,[scriptblock]$Read,[scriptblock]$Child){
 $required=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($name in @('design-content.json','energy-report.json','workbench-report.json','performance.json','performance.csv','inputs.csv','design-figures.json','design-changes.json','design-changes.csv','sensitivity.json','design-proposals.json','typed-input-differences.json','baseline-model.svg','proposed-model.svg','annual.svg','monthly.svg','design-executive.html','design-technical.html','input-appendix.html','performance.html','design-executive.pdf','design-technical.pdf','input-appendix.pdf','performance.pdf')){[void]$required.Add($name)}
 $v2=$null -ne $Content.drawing
 if($Content.frames -isnot [array] -or $Content.frames.Count -lt 2 -or $Content.frames.Count -gt 4){throw 'Missing bounded matched frames.'}
 if($v2 -and ($Content.frames[0].id -cne 'axonometric' -or $Content.frames[1].id -cne 'plan')){throw 'Bound drawing default frames are missing or reordered.'}
 $ids=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($frame in $Content.frames){if($frame.id -isnot [string] -or $frame.id -cnotmatch '^[A-Za-z0-9_-]{1,80}\z' -or !$ids.Add($frame.id) -or ($v2 -and $frame.id -cnotmatch '^(axonometric|plan|accepted-[12])$')){throw 'Invalid or duplicate figure frame identity.'};foreach($side in @('a','b','overlay')){[void]$required.Add(('design-'+$frame.id+'-'+$side+'.svg'))}}
 if($null -ne $Content.drawing){[void]$required.Add('design-drawing.json');[void]$required.Add('design-scope-revision.json')}
 $pages=@($Manifest.products|Where-Object {$_.path -cmatch '^performance-page-[0-9]{3}\.png$'})
 if($pages.Count -lt 1 -or $pages.Count -gt 128){throw 'Missing or unbounded performance PNG family.'}
 for($i=0;$i -lt $pages.Count;$i++){if($pages[$i].path -cne ('performance-page-{0:d3}.png' -f ($i+1))){throw 'Performance PNG sequence has a gap.'};[void]$required.Add($pages[$i].path)}
 if($required.Count -ne $Manifest.products.Count){throw 'Supported renderer product family is incomplete or has extras.'}
 foreach($p in $Manifest.products){if(!$required.Contains($p.path)){throw 'Unexpected product outside supported renderer family.'}}
 $figures=& $Read (& $Child $Directory 'design-figures.json') 48MB
 $schema=if($v2){'greenlens.paired-design-figures.v2'}else{'greenlens.paired-design-figures.v1'}
 if($figures.schema_version -cne $schema -or $figures.method -cne $Content.renderer.figure_method -or $figures.request.schema_version -cne 'greenlens.paired-design-figures.v1' -or $figures.products -isnot [array] -or $figures.products.Count -ne 3*$Content.frames.Count){throw 'Unsupported or incomplete exact figure family.'}
 Assert-ObservedDesignEqual $figures.request.frames $Content.frames 'figure request frames'
 Assert-ObservedDesignEqual $figures.request.scope $Content.analytical_scope 'figure scope declaration'
 Assert-ObservedDesignEqual $figures.request.north_declaration $Content.north_declaration 'figure orientation declaration'
 foreach($name in @('baseline','proposed')){
  $case=$Content.workbench.base[$name];$figure=$figures.request[$name];$side=if($name -ceq 'baseline'){'A'}else{'B'}
  $scope=if($v2){$Content.drawing.selection.scope_revision_sha256}else{$null}
  if($figure.design_id -cne $side -or $figure.scope_revision_sha256 -cne $scope -or $figure.evidence.kind -cne 'retained_run'){throw 'Figure does not bind its retained design/scope.'}
  foreach($field in @('model','study','label')){Assert-ObservedDesignEqual $figure[$field] $case[$field] ('figure '+$field)}
  Assert-ObservedDesignEqual $figure.calendar $case.study.calendar 'figure calendar'
  foreach($field in @('run_id','record_sha256','physical_input_sha256','assessment')){Assert-ObservedDesignEqual $figure.evidence[$field] $case[$field] ('figure evidence '+$field)}
 }
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($figure in $figures.products){
  if($figure.side -cnotin @('A','B','overlay') -or !$ids.Contains($figure.frame_id) -or $figure.filename -cne ('design-'+$figure.frame_id+'-'+$figure.side.ToLowerInvariant()+'.svg') -or !$seen.Add($figure.filename) -or $figure.svg -isnot [string] -or [Text.Encoding]::UTF8.GetByteCount($figure.svg) -gt 8MB){throw 'Invalid or repeated owned figure output.'}
  $digest=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($figure.svg))).ToLowerInvariant()
  if($digest -cne $figure.svg_sha256 -or (& $Hash (& $Child $Directory $figure.filename)) -cne $digest){throw 'Figure product bytes differ from exact retained figure data.'}
 }
 foreach($entry in @(@('energy-report.json',$Content.workbench.base),@('workbench-report.json',$Content.workbench),@('design-changes.json',$figures.changes),@('sensitivity.json',$Content.sensitivity),@('design-proposals.json',$Content.proposal_declarations),@('typed-input-differences.json',$Content.workbench.typed_comparison))){Assert-ObservedDesignEqual (& $Read (& $Child $Directory $entry[0]) 48MB) $entry[1] $entry[0]}
 if($v2){Assert-ObservedDesignEqual (& $Read (& $Child $Directory 'design-drawing.json') 40MB) $Content.drawing 'complete drawing artifact';Assert-ObservedDesignEqual (& $Read (& $Child $Directory 'design-scope-revision.json') 32MB) $Content.drawing.scope_revision 'complete scope artifact'}
}
function Assert-ObservedDesignProducts([string]$Root,$Summary,[scriptblock]$Hash,[scriptblock]$Read,[scriptblock]$Child,$Selection,[string]$ExpectedTypstHash,$Project){
 if($Summary.id -cnotmatch '^[a-f0-9]{64}$' -or $Summary.id -cne $Summary.content_sha256 -or $Summary.path -cne ('design-reports/'+$Summary.id) -or $Summary.include_pdf -isnot [bool] -or !$Summary.include_pdf -or $Summary.manifest_file_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $Summary.products_sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'Invalid analytical report identity/path/PDF selection.'}
 if($null -ne $Selection){Assert-ObservedDesignBinding $Summary.baseline $Selection.baseline;Assert-ObservedDesignBinding $Summary.proposed $Selection.proposed}
 $directory=& $Child $Root $Summary.path
 $manifestPath=& $Child $directory 'design-products.json'
 if((& $Hash $manifestPath) -cne $Summary.manifest_file_sha256){throw 'Analytical product-manifest bytes differ from the retained index.'}
 $manifest=& $Read $manifestPath 256KB
 if($manifest.schema_version -cne 'greenlens.design-report-products.v1' -or $manifest.products -isnot [array] -or $manifest.products.Count -lt 1 -or $manifest.products.Count -gt 252 -or $manifest.content_sha256 -cne $Summary.content_sha256 -or $manifest.products_sha256 -cne $Summary.products_sha256){throw 'Analytical product inventory differs from the retained content/products identities.'}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase);[long]$inventoryBytes=(Get-Item -LiteralPath $manifestPath).Length;[long]$productBytes=0;[long]$total=$inventoryBytes;$pdfs=0;$previous=$null
 foreach($p in $manifest.products){
  if($p.path -isnot [string] -or $p.path.Contains('/') -or $p.path -in @('design-products.json','design-render-failure.json') -or !$seen.Add($p.path) -or ($null -ne $previous -and [StringComparer]::Ordinal.Compare($previous,$p.path) -ge 0) -or ($p.bytes -isnot [long] -and $p.bytes -isnot [int]) -or $p.bytes -le 0 -or $p.bytes -gt 64MB -or $p.sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'Invalid, unsorted or oversized analytical product member.'}
  $path=& $Child $directory $p.path;$item=Get-Item -LiteralPath $path -Force
  if($item.PSIsContainer -or $item.Length -ne $p.bytes -or (& $Hash $path) -cne $p.sha256){throw 'Analytical product bytes differ from their retained inventory.'}
  $productBytes+=$p.bytes;$total+=$p.bytes;if($total -gt 256MB){throw 'Analytical products exceed the shared aggregate bound.'}
  if($p.path.EndsWith('.pdf',[StringComparison]::OrdinalIgnoreCase)){$pdfs++};$previous=$p.path
 }
 if(!$pdfs -or !$seen.Contains('design-content.json')){throw 'Analytical PDF issue lacks requested PDFs or canonical content.'}
 $actual=@(Get-ChildItem -LiteralPath $directory -Force)
 if($actual.Count -ne $manifest.products.Count+1 -or @($actual|Where-Object {$_.PSIsContainer -or ($_.Name -cne 'design-products.json' -and !$seen.Contains($_.Name))}).Count){throw 'Analytical report directory contains missing or unadvertised products.'}
 $content=& $Read (& $Child $directory 'design-content.json') 48MB
 $v2=$content.schema_version -ceq 'greenlens.design-report-content.v2'
 $method=if($v2){'greenlens.bound-design-report-renderer/2'}else{'greenlens.analytical-design-report-renderer/1'};$figureMethod=if($v2){'greenlens.scoped-paired-drawing/2'}else{'greenlens.analytical-paired-drawing/1'}
 if($content.schema_version -cnotin @('greenlens.design-report-content.v1','greenlens.design-report-content.v2') -or $v2 -ne ($null -ne $content.drawing) -or $content.content_sha256 -cne $Summary.content_sha256 -or $content.include_pdf -isnot [bool] -or !$content.include_pdf -or $content.renderer.method -cne $method -or $content.renderer.figure_method -cne $figureMethod -or $content.renderer.typst_executable_sha256 -cne $ExpectedTypstHash){throw 'Content/renderer/Typst or drawing presence differs from the supported report contract.'}
 Assert-ObservedDesignDeclarations $content
 $a=$content.workbench.base.baseline;$b=$content.workbench.base.proposed
 Assert-ObservedDesignBinding @{run_id=$a.run_id;record_sha256=$a.record_sha256;design_id=$a.id} $Summary.baseline
 Assert-ObservedDesignBinding @{run_id=$b.run_id;record_sha256=$b.record_sha256;design_id=$b.id} $Summary.proposed
 if($null -ne $Selection -and ($content.analytical_scope -cne $Selection.analytical_scope -or $content.north_declaration -cne $Selection.north_declaration)){throw 'Published analytical scope/orientation differs from the explicitly armed declaration.'}
 $drawing=$null
 if($null -ne $Selection -and ($v2 -ne ($null -ne $Selection.drawing))){throw 'The requested drawing was dropped or an unarmed drawing was added.'}
 if($v2){$armed=if($null -ne $Selection){$Selection.drawing}else{$null};$drawing=Assert-ObservedDesignDrawing $Root $Project $content $armed $Hash $Read $Child}
 Assert-ObservedDesignFamily $directory $content $manifest $Hash $Read $Child
 return [ordered]@{id=$Summary.id;baseline=$Summary.baseline;proposed=$Summary.proposed;analytical_scope=$content.analytical_scope;north_declaration=$content.north_declaration;drawing=$drawing;renderer=$content.renderer;product_count=$manifest.products.Count;product_bytes=$productBytes;inventory_bytes=$inventoryBytes;directory_bytes=$total;pdf_count=$pdfs;manifest_file_sha256=$Summary.manifest_file_sha256;products=$manifest;check='Actual-byte complete family, exact retained fields and reference chunks only; Rust canonical identities, geometry/context reduction, semantics/arithmetic and native display acceptance remain separate'}
}
