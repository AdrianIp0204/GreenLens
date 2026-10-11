#Requires -Version 7.5
# Native compatibility verification, not physical accuracy or release acceptance.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Cli,
    [Parameter(Mandatory)][string]$BuildRecord,
    [Parameter(Mandatory)][string]$SourceProject,
    [Parameter(Mandatory)][string]$LegacyPackage,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$EngineDirectory = ''
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$Cli = (Resolve-Path -LiteralPath $Cli).Path
$SourceProject = (Resolve-Path -LiteralPath $SourceProject).Path
$LegacyPackage = (Resolve-Path -LiteralPath $LegacyPackage).Path
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new output directory; earlier evidence is preserved.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$report = [ordered]@{
    schema_version = 'greenlens.historical-method-compatibility.v0.3'
    started_utc = [DateTime]::UtcNow.ToString('o'); passed = $false
    scope = 'Real native historical method-2 and published v0.1 static inspection/export/replay. Same-engine integration only; no method-2 physical-accuracy or method-3 acceptance claim.'
    scientific_accuracy_acceptance = $false; installed_desktop_acceptance = $false
    verification_script_sha256 = (Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
    commands = @(
        'greenlens copy <historical-project> <new-copy>',
        'greenlens result <new-copy> <A-id> --hour 1908',
        'greenlens study compare <new-copy> <A-id> <B-id> --field cda',
        'greenlens export <new-copy> <new-A.zip> --run <A-id>',
        'greenlens replay <new-A.zip>',
        'greenlens replay <published-v0.1.zip>'
    )
}
function Inventory([string]$Directory) {
    $entries = @{}
    foreach ($file in Get-ChildItem -LiteralPath $Directory -File -Recurse | Sort-Object FullName) {
        $relative = [IO.Path]::GetRelativePath($Directory, $file.FullName).Replace('\', '/')
        # The persistent lock-file's presence does not mean its OS lock is held.
        # The Rust copy/export services acquire the authoritative writer lock.
        if ($relative -eq '.greenlens.lock') { continue }
        $entries[$relative] = @{ bytes = $file.Length; sha256 = (Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant() }
    }
    return $entries
}
function Same-Inventory($Expected, $Actual, [switch]$EvidenceOnly) {
    $names = @($Expected.Keys | Where-Object { !$EvidenceOnly -or $_ -ne 'project.json' } | Sort-Object)
    $actualNames = @($Actual.Keys | Where-Object { !$EvidenceOnly -or $_ -ne 'project.json' } | Sort-Object)
    if (($names -join "`n") -cne ($actualNames -join "`n")) { throw 'Retained file inventory changed.' }
    foreach ($name in $names) {
        if ($Expected[$name].bytes -ne $Actual[$name].bytes -or $Expected[$name].sha256 -cne $Actual[$name].sha256) { throw "Retained bytes changed: $name" }
    }
}
function Invoke-Cli([string]$Name, [string[]]$Arguments) {
    & $Cli @Arguments 1> (Join-Path $OutputDirectory "$Name.json") 2> (Join-Path $OutputDirectory "$Name.stderr.txt")
    if ($LASTEXITCODE -ne 0) { throw "Required CLI operation $Name failed with exit $LASTEXITCODE; logs retained." }
}
try {
    $build = Get-Content -LiteralPath $BuildRecord -Raw | ConvertFrom-Json -DateKind String
    $cliHash = (Get-FileHash -LiteralPath $Cli).Hash.ToLowerInvariant()
    if ($build.cli_sha256 -cne $cliHash -or $build.source_worktree_dirty -isnot [bool] -or $build.source_worktree_dirty) { throw 'CLI must match an explicitly clean frozen build record.' }
    $report.cli = @{ sha256 = $cliHash; bytes = (Get-Item -LiteralPath $Cli).Length; source_commit = $build.source_commit; source_worktree_dirty = $build.source_worktree_dirty; build_command = $build.build_command; built_utc = $build.built_utc }
    $original = Inventory $SourceProject
    $manifest = Get-Content -LiteralPath (Join-Path $SourceProject 'project.json') -Raw | ConvertFrom-Json -Depth 100
    $a = @($manifest.climate.runs | Where-Object alternative_id -CEQ 'A')
    $b = @($manifest.climate.runs | Where-Object alternative_id -CEQ 'B')
    if ($a.Count -ne 1 -or $b.Count -ne 1) { throw 'This bounded verification requires exactly one historical A and B.' }
    $records = @($a[0], $b[0]) | ForEach-Object {
        $record = Get-Content -LiteralPath (Join-Path $SourceProject $_.path 'results.json') -Raw | ConvertFrom-Json -Depth 100
        if ($record.provenance.engine.method_version -cne 'perez-epw-dewpoint3-reinhart-patch-indirect-finite-direct/2' -or $record.series.interval_count -ne 8760 -or @($record.series.sensor_ids).Count -ne 24) { throw 'Expected actual method-2 annual 24-sensor history.' }
        @{ id = $record.id; alternative = $record.alternative_id; method = $record.provenance.engine.method_version; original_source_commit = $record.provenance.source_commit; original_source_worktree_dirty = $record.provenance.source_worktree_dirty; physical_result_fingerprint = $record.provenance.physical_result_fingerprint }
    }
    $copy = Join-Path $OutputDirectory 'Portable historical copy'
    Invoke-Cli 'copy' @('copy', $SourceProject, $copy)
    Same-Inventory $original (Inventory $SourceProject)
    Same-Inventory $original (Inventory $copy) -EvidenceOnly
    $report.copy = @{ source_files = $original.Count; immutable_evidence_files = $original.Count - 1; historical_runs = $records; byte_identical_evidence = $true }
    Invoke-Cli 'hour1908' @('result', $copy, $a[0].id, '--hour', '1908')
    Invoke-Cli 'cda-comparison' @('study', 'compare', $copy, $a[0].id, $b[0].id, '--field', 'cda')
    $package = Join-Path $OutputDirectory 'Historical method2 A.zip'
    Invoke-Cli 'export' @('export', $copy, $package, '--run', $a[0].id)
    $report.selected_export = @{ run_id = $a[0].id; sha256 = (Get-FileHash -LiteralPath $package).Hash.ToLowerInvariant(); bytes = (Get-Item -LiteralPath $package).Length }
    $annualArguments = @{ Package = $package; Cli = $Cli; OutputDirectory = (Join-Path $OutputDirectory 'annual-replay'); ExpectedAlternatives = @('A'); ExpectedSensorCount = 24 }
    if ($EngineDirectory) { $annualArguments.EngineDirectory = $EngineDirectory }
    & (Join-Path $PSScriptRoot 'annual-replay.ps1') @annualArguments | Out-Null
    $annual = Get-Content -LiteralPath (Join-Path $OutputDirectory 'annual-replay/summary.json') -Raw | ConvertFrom-Json -Depth 100 -DateKind String
    if (!$annual.pass) { throw 'Historical annual replay did not pass.' }
    $report.annual_replay = @{ pass = $annual.pass; started_utc = $annual.started_utc; finished_utc = $annual.finished_utc; wall_seconds = $annual.wall_seconds; checks = $annual.checks; package_unchanged = $annual.package_unchanged; cli_unchanged = $annual.cli_unchanged }
    $staticHash = (Get-FileHash -LiteralPath $LegacyPackage).Hash.ToLowerInvariant()
    if ($staticHash -cne 'c53dcc4094630f3bac5c3f1f76b7e3c452f015c0e819d8713e4744f3e0aedcec') { throw 'Expected the byte-identical published synthetic v0.1 fixture.' }
    $staticArguments = @('replay', $LegacyPackage)
    if ($EngineDirectory) { $staticArguments += @('--engine-dir', $EngineDirectory) }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    Invoke-Cli 'static-replay' $staticArguments
    $watch.Stop()
    $static = Get-Content -LiteralPath (Join-Path $OutputDirectory 'static-replay.json') -Raw | ConvertFrom-Json -Depth 100
    if (!$static.passed -or @($static.checks).Count -ne 2 -or @($static.checks | Where-Object { !$_.passed -or $_.sensors -ne 77 }).Count -ne 0) { throw 'Published static replay did not return both complete 77-sensor runs.' }
    if ((Get-FileHash -LiteralPath $LegacyPackage).Hash.ToLowerInvariant() -cne $staticHash) { throw 'Static fixture changed.' }
    $report.static_replay = @{ package_sha256 = $staticHash; package_unchanged = $true; wall_seconds = $watch.Elapsed.TotalSeconds; checks = $static.checks; tolerance = 'max(0.1 lx, 0.1% of reference)' }
    Same-Inventory $original (Inventory $SourceProject)
    Same-Inventory $original (Inventory $copy) -EvidenceOnly
    if ((Get-FileHash -LiteralPath $Cli).Hash.ToLowerInvariant() -cne $cliHash) { throw 'Frozen CLI changed.' }
    $report.source_project_unchanged = $true
    $report.copied_immutable_evidence_unchanged = $true
    $report.cli_unchanged = $true
    $report.passed = $true
} catch {
    $report.error = $_.Exception.Message
    throw
} finally {
    $report.finished_utc = [DateTime]::UtcNow.ToString('o')
    $report | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8NoBOM
}
Write-Output $OutputDirectory
