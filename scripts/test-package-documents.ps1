#Requires -Version 7.2
# Documentation-only regression: no compilation, engine, installer or CLI execution.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/package-artifacts.ps1" -FunctionsOnly
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$fixtureRoot = Join-Path $env:LOCALAPPDATA ('GreenLens/verification/package documents ' + [Guid]::NewGuid().ToString('N') + ' 空間')
$destination = Join-Path $fixtureRoot 'package'
Copy-PackageDocuments -Source $sourceRoot -Destination $destination
$paths = @(Get-PackageDocumentPaths)
foreach ($required in @('docs/v0.3-reference-protocol.md','docs/v0.3-engine-investigation.md','docs/v0.3-engine-investigation.json','scripts/annual-validation-plan.json','dependencies/weather.json')) {
    if ($required -notin $paths) { throw "Required reference document omitted: $required" }
}
foreach ($path in $paths) {
    if ((Get-FileHash -LiteralPath (Join-Path $sourceRoot $path)).Hash -cne (Get-FileHash -LiteralPath (Join-Path $destination $path)).Hash) {
        throw "Copied documentation bytes changed: $path"
    }
}
if (@(Get-ChildItem -LiteralPath $destination -Recurse -File | Where-Object Extension -in '.exe','.dll','.zip','.oct','.mtx').Count) {
    throw 'Documentation selection unexpectedly includes a binary or large evidence container.'
}
$linkCount = Assert-PackageDocumentLinks $destination
# Deleting one known file within this fresh private fixture must fail link closure.
$protocol = Join-Path $destination 'docs/v0.3-reference-protocol.md'
Remove-Item -LiteralPath $protocol
$rejected = $false
try { Assert-PackageDocumentLinks $destination | Out-Null } catch { $rejected = $true }
if (!$rejected) { throw 'A missing linked reference protocol was silently accepted.' }
Copy-Item -LiteralPath (Join-Path $sourceRoot 'docs/v0.3-reference-protocol.md') -Destination $protocol
# An escaping link must fail even when its target exists outside the package.
$guide = Join-Path $destination 'docs/v0.3-usage.md'
$original = [IO.File]::ReadAllBytes($guide)
[IO.File]::WriteAllText((Join-Path $fixtureRoot 'outside-package.md'), 'Original synthetic packaging fixture.')
[IO.File]::AppendAllText($guide, "`n[Escaping fixture](../../outside-package.md)`n")
$rejected = $false
try { Assert-PackageDocumentLinks $destination | Out-Null } catch { $rejected = $true }
[IO.File]::WriteAllBytes($guide, $original)
if (!$rejected) { throw 'A path outside the documentation package was silently accepted.' }
[IO.File]::AppendAllText($guide, "`n[Absolute fixture](<$guide>)`n")
$rejected = $false
try { Assert-PackageDocumentLinks $destination | Out-Null } catch { $rejected = $true }
[IO.File]::WriteAllBytes($guide, $original)
if (!$rejected) { throw 'An absolute machine path was silently accepted.' }
$null = Assert-PackageDocumentLinks $destination
$bytes = (Get-ChildItem -LiteralPath $destination -Recurse -File | Measure-Object Length -Sum).Sum
[ordered]@{schema_version='greenlens.package-document-check.v1';passed=$true;source_files=$paths.Count;inline_local_links=$linkCount;retained_bytes=$bytes;missing_link_rejected=$true;outside_path_rejected=$true;absolute_path_rejected=$true;runtime_or_engine_executed=$false} |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination 'document-check.json') -Encoding utf8
Write-Host "Documentation checks passed: $($paths.Count) exact-byte files, $linkCount local links, $bytes bytes. Retained $destination"
