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
function Get-ObservedDesignArmSelection($Candidate,$Manifest,[string]$ExpectedProjectId){
 if($Candidate.project_id -cne $ExpectedProjectId -or $Manifest.id -cne $ExpectedProjectId){throw 'Analytical arm and saved manifest must identify the declared document.'}
 if($Candidate.Contains('scope_revision_sha256')){throw 'Analytical report arming must not substitute an architectural scope revision.'}
 if(!$Candidate.baseline_run_id -or !$Candidate.proposed_run_id -or $Candidate.baseline_run_id -ceq $Candidate.proposed_run_id){throw 'Analytical arm requires distinct exact A/B run identities.'}
 $baseline=@($Manifest.energy.runs|Where-Object {$_.id -ceq $Candidate.baseline_run_id})
 $proposed=@($Manifest.energy.runs|Where-Object {$_.id -ceq $Candidate.proposed_run_id})
 if($baseline.Count -ne 1 -or $proposed.Count -ne 1 -or $baseline[0].alternative_id -cne 'A' -or $proposed[0].alternative_id -cne 'B'){throw 'The analytical arm must bind exactly one retained A and B record.'}
 foreach($run in @($baseline[0],$proposed[0])){if($run.record_sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'An armed analytical run lacks its retained record hash.'}}
 Assert-ObservedDesignDeclarations $Candidate
 return [ordered]@{baseline=[ordered]@{run_id=$baseline[0].id;record_sha256=$baseline[0].record_sha256;design_id='A'};proposed=[ordered]@{run_id=$proposed[0].id;record_sha256=$proposed[0].record_sha256;design_id='B'};analytical_scope=$Candidate.analytical_scope;north_declaration=$Candidate.north_declaration}
}
function Assert-ObservedDesignDeclarations($Value){
 if($Value.analytical_scope -isnot [string] -or [string]::IsNullOrWhiteSpace($Value.analytical_scope) -or $Value.analytical_scope -cne $Value.analytical_scope.Trim() -or [Text.Encoding]::UTF8.GetByteCount($Value.analytical_scope) -gt 2048){throw 'Analytical scope requires an exact nonempty trimmed declaration within 2048 UTF-8 bytes.'}
 if(!$Value.Contains('north_declaration') -or ($null -ne $Value.north_declaration -and ($Value.north_declaration -isnot [string] -or [string]::IsNullOrWhiteSpace($Value.north_declaration) -or $Value.north_declaration -cne $Value.north_declaration.Trim() -or [Text.Encoding]::UTF8.GetByteCount($Value.north_declaration) -gt 2048))){throw 'Analytical north must be explicitly null (unknown) or an exact bounded declaration.'}
}
function Assert-ObservedDesignBinding($Actual,$Expected){
 foreach($name in @('run_id','record_sha256','design_id')){if(!$Actual[$name] -or $Actual[$name] -cne $Expected[$name]){throw 'Analytical report differs from its exact selected run/record/design binding.'}}
}
function Assert-ObservedDesignProducts([string]$Root,$Summary,[scriptblock]$Hash,[scriptblock]$Read,[scriptblock]$Child,$Selection,[string]$ExpectedTypstHash){
 if($Summary.id -cnotmatch '^[a-f0-9]{64}$' -or $Summary.id -cne $Summary.content_sha256 -or $Summary.path -cne ('design-reports/'+$Summary.id) -or $Summary.include_pdf -isnot [bool] -or !$Summary.include_pdf -or $Summary.manifest_file_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $Summary.products_sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'Invalid analytical report identity/path/PDF selection.'}
 if($null -ne $Selection){Assert-ObservedDesignBinding $Summary.baseline $Selection.baseline;Assert-ObservedDesignBinding $Summary.proposed $Selection.proposed}
 $directory=& $Child $Root $Summary.path
 $manifestPath=& $Child $directory 'design-products.json'
 if((& $Hash $manifestPath) -cne $Summary.manifest_file_sha256){throw 'Analytical product-manifest bytes differ from the retained index.'}
 $manifest=& $Read $manifestPath 256KB
 if($manifest.schema_version -cne 'greenlens.design-report-products.v1' -or $manifest.products -isnot [array] -or $manifest.products.Count -lt 1 -or $manifest.products.Count -gt 252 -or $manifest.content_sha256 -cne $Summary.content_sha256 -or $manifest.products_sha256 -cne $Summary.products_sha256){throw 'Analytical product inventory differs from the retained content/products identities.'}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase);[long]$total=0;$pdfs=0;$previous=$null
 foreach($p in $manifest.products){
  if($p.path -isnot [string] -or $p.path.Contains('/') -or $p.path -in @('design-products.json','design-render-failure.json') -or !$seen.Add($p.path) -or ($null -ne $previous -and [StringComparer]::Ordinal.Compare($previous,$p.path) -ge 0) -or ($p.bytes -isnot [long] -and $p.bytes -isnot [int]) -or $p.bytes -le 0 -or $p.bytes -gt 64MB -or $p.sha256 -cnotmatch '^[a-f0-9]{64}$'){throw 'Invalid, unsorted or oversized analytical product member.'}
  $path=& $Child $directory $p.path;$item=Get-Item -LiteralPath $path -Force
  if($item.PSIsContainer -or $item.Length -ne $p.bytes -or (& $Hash $path) -cne $p.sha256){throw 'Analytical product bytes differ from their retained inventory.'}
  $total+=$p.bytes;if($total -gt 256MB){throw 'Analytical products exceed the shared aggregate bound.'}
  if($p.path.EndsWith('.pdf',[StringComparison]::OrdinalIgnoreCase)){$pdfs++};$previous=$p.path
 }
 if(!$pdfs -or !$seen.Contains('design-content.json')){throw 'Analytical PDF issue lacks requested PDFs or canonical content.'}
 $actual=@(Get-ChildItem -LiteralPath $directory -Force)
 if($actual.Count -ne $manifest.products.Count+1 -or @($actual|Where-Object {$_.PSIsContainer -or ($_.Name -cne 'design-products.json' -and !$seen.Contains($_.Name))}).Count){throw 'Analytical report directory contains missing or unadvertised products.'}
 $content=& $Read (& $Child $directory 'design-content.json') 48MB
 if($content.schema_version -cne 'greenlens.design-report-content.v1' -or $content.content_sha256 -cne $Summary.content_sha256 -or $content.include_pdf -isnot [bool] -or !$content.include_pdf -or $content.renderer.method -cne 'greenlens.analytical-design-report-renderer/1' -or $content.renderer.typst_executable_sha256 -cne $ExpectedTypstHash){throw 'Analytical content/renderer/Typst differs from the declared report protocol.'}
 Assert-ObservedDesignDeclarations $content
 $a=$content.workbench.base.baseline;$b=$content.workbench.base.proposed
 Assert-ObservedDesignBinding @{run_id=$a.run_id;record_sha256=$a.record_sha256;design_id=$a.id} $Summary.baseline
 Assert-ObservedDesignBinding @{run_id=$b.run_id;record_sha256=$b.record_sha256;design_id=$b.id} $Summary.proposed
 if($null -ne $Selection -and ($content.analytical_scope -cne $Selection.analytical_scope -or $content.north_declaration -cne $Selection.north_declaration)){throw 'Published analytical scope/orientation differs from the explicitly armed declaration.'}
 return [ordered]@{id=$Summary.id;baseline=$Summary.baseline;proposed=$Summary.proposed;analytical_scope=$content.analytical_scope;north_declaration=$content.north_declaration;renderer=$content.renderer;product_count=$manifest.products.Count;product_bytes=$total;pdf_count=$pdfs;manifest_file_sha256=$Summary.manifest_file_sha256;products=$manifest;check='Actual-byte inventory and exact retained bindings only; Rust semantic/arithmetic checks and native display acceptance remain separate'}
}
