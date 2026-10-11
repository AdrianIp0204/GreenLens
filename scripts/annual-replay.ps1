#Requires -Version 7.5
# Complete annual evidence replay through the trusted native CLI, not validation
# of arbitrary package commands. This wrapper never extracts or executes files.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Package,
    [Parameter(Mandatory)][string]$Cli,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateSet('A','B')][string[]]$ExpectedAlternatives = @('A','B'),
    [ValidateSet('shipping','reference')][string]$ExpectedProfile = 'shipping',
    [ValidateRange(1,256)][int]$ExpectedSensorCount = 24,
    [string]$EngineDirectory = ''
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new annual replay output directory; existing evidence is preserved.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$started = [DateTime]::UtcNow
$watch = [Diagnostics.Stopwatch]::StartNew()
$report = [ordered]@{
    schema_version='greenlens.annual-replay-evidence.v0.3';started_utc=$started.ToString('o');pass=$false
    expected_alternatives=@($ExpectedAlternatives);expected_profile=$ExpectedProfile;expected_intervals=8760;expected_sensors=$ExpectedSensorCount
    replay_started=$false;exit_code=$null;checks=@();expected_runs=@();package_unchanged=$false;cli_unchanged=$false
    scientific_accuracy_acceptance=$false;native_desktop_acceptance=$false
    scope='Complete interval/sensor same-engine replay integration. Not independent physical validation, convergence or installed desktop acceptance.'
}
function Read-ReplayJsonEntry($Zip,[string]$Name) {
    $entry=$Zip.GetEntry($Name)
    if (!$entry -or $entry.Length -gt 64MB) { throw "Required bounded package metadata is absent: $Name" }
    $reader=[IO.StreamReader]::new($entry.Open(),[Text.Encoding]::UTF8,$true)
    try { return $reader.ReadToEnd() | ConvertFrom-Json -DateKind String -Depth 100 } finally { $reader.Dispose() }
}
function Read-ReplaySelection([string]$Archive) {
    $zip=[IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        if ($zip.Entries.Count -gt 5000) { throw 'Evidence exceeds the shared 5000-member limit.' }
        $names=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        [long]$total=0
        foreach ($entry in $zip.Entries) {
            if (!$names.Add($entry.FullName) -or $entry.FullName -match '(^/|\\|:|(^|/)\.\.?(/|$))' -or $entry.Length -gt 64MB) { throw 'Unsafe, duplicate or oversized archive member.' }
            $total+=$entry.Length
        }
        if ($total -gt 256MB) { throw 'Evidence exceeds the shared 256 MiB uncompressed limit.' }
        $project=Read-ReplayJsonEntry $zip 'project.json'
        $selected=@($project.climate.runs)
        if (@($project.runs).Count -ne 0 -or $selected.Count -ne $ExpectedAlternatives.Count) { throw 'Select only the explicitly expected annual runs; mixed static or extra runs are unsupported by this bounded wrapper.' }
        $ids=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
        $alternatives=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
        $records=@()
        foreach ($summary in $selected) {
            $id=[guid]::Parse($summary.id).ToString()
            if ($id -cne $summary.id -or !$ids.Add($id) -or !$alternatives.Add($summary.alternative_id) -or $summary.alternative_id -cnotin $ExpectedAlternatives -or $summary.path -cne "runs/$id") { throw 'Annual selection has a duplicate, unexpected alternative or invalid run path.' }
            $record=Read-ReplayJsonEntry $zip "runs/$id/results.json"
            if ($record.id -cne $id -or $record.alternative_id -cne $summary.alternative_id -or $record.study.calculation.mode -cne 'annual' -or $record.study.calculation.profile -cne $ExpectedProfile -or $record.provenance.profile.name -cne $ExpectedProfile -or $record.series.interval_count -ne 8760 -or @($record.series.sensor_ids).Count -ne $ExpectedSensorCount) { throw 'Selected evidence does not match the declared annual profile/shape.' }
            $records+=@{id=$id;alternative=$record.alternative_id;intervals=8760;sensors=$ExpectedSensorCount;source_commit=$record.provenance.source_commit;source_worktree_dirty=$record.provenance.source_worktree_dirty;engine=$record.provenance.engine;profile=$record.provenance.profile;weather=$record.study.calculation.weather;physical_result_fingerprint=$record.provenance.physical_result_fingerprint}
        }
        return ,$records
    } finally { $zip.Dispose() }
}
function Require-ReplayNumber($Value,[string]$Field) {
    if ($null -eq $Value -or $Value -is [bool] -or $Value -is [string] -or $Value -isnot [ValueType] -or ![double]::IsFinite([double]$Value) -or $Value -lt 0) { throw "Replay $Field must be a finite nonnegative number." }
}
try {
    if ($ExpectedAlternatives.Count -lt 1 -or $ExpectedAlternatives.Count -gt 2 -or @($ExpectedAlternatives | Select-Object -Unique).Count -ne $ExpectedAlternatives.Count) { throw 'Declare one or two distinct expected alternatives.' }
    $Cli=(Resolve-Path -LiteralPath $Cli).Path
    $Package=(Resolve-Path -LiteralPath $Package).Path
    $report.cli_sha256=(Get-FileHash -LiteralPath $Cli).Hash.ToLowerInvariant()
    $report.package_sha256=(Get-FileHash -LiteralPath $Package).Hash.ToLowerInvariant()
    $report.package_bytes=(Get-Item -LiteralPath $Package).Length
    $report.expected_runs=Read-ReplaySelection $Package
    & $Cli --version 1> (Join-Path $OutputDirectory 'cli-version.txt') 2> (Join-Path $OutputDirectory 'cli-version.stderr.txt')
    if ($LASTEXITCODE -ne 0) { throw 'Required CLI version query failed.' }
    $report.cli_version=(Get-Content -LiteralPath (Join-Path $OutputDirectory 'cli-version.txt') -Raw).Trim()
    $arguments=@('replay',$Package)
    if ($EngineDirectory) { $arguments+=@('--engine-dir',[IO.Path]::GetFullPath($EngineDirectory)) }
    $report.replay_started=$true
    & $Cli @arguments 1> (Join-Path $OutputDirectory 'replay.json') 2> (Join-Path $OutputDirectory 'replay.stderr.txt')
    $report.exit_code=$LASTEXITCODE
    if ($report.exit_code -ne 0) { throw "Required annual replay failed with exit $($report.exit_code); logs retained. Missing engines and partial runs are failures, not skips." }
    $replayFile=Join-Path $OutputDirectory 'replay.json'
    if ((Get-Item -LiteralPath $replayFile).Length -gt 64MB) { throw 'Replay response exceeds the bounded metadata limit.' }
    $result=Get-Content -LiteralPath $replayFile -Raw | ConvertFrom-Json -DateKind String -Depth 100
    $report.checks=@($result.checks)
    if ($result.passed -isnot [bool] -or !$result.passed -or $report.checks.Count -ne $report.expected_runs.Count) { throw 'Replay did not report exactly every expected completed annual run.' }
    $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($check in $report.checks) {
        $expected=@($report.expected_runs | Where-Object id -CEQ $check.run_id)
        if ($expected.Count -ne 1 -or !$seen.Add($check.run_id) -or $check.passed -isnot [bool] -or !$check.passed -or $check.intervals -ne 8760 -or $check.sensors -ne $ExpectedSensorCount -or $check.values_checked -ne (8760*$ExpectedSensorCount)) { throw 'Replay contains a duplicate, unknown, failed or truncated interval/sensor check.' }
        foreach ($field in @('intervals','sensors','values_checked','max_absolute_error_lux','max_tolerance_ratio')) { Require-ReplayNumber $check.$field $field }
        if ($check.tolerance -cne 'max(0.5 lx, 0.5% of reference)' -or $check.max_tolerance_ratio -gt 1) { throw 'Annual replay exceeded or changed the required integration tolerance.' }
        $resources=$check.resources
        if ($resources.schema_version -cne 'greenlens.annual-resources.v1' -or !$resources.measurement_policy -or @($resources.processes).Count -lt 5) { throw 'Required replay resource telemetry is unavailable; use the current compiled CLI.' }
        foreach ($field in @('total_wall_ms','peak_engine_working_set_bytes','peak_observed_runner_plus_engine_working_set_bytes','peak_scratch_bytes','max_scratch_bytes','max_matrix_bytes','max_observed_working_set_bytes')) { Require-ReplayNumber $resources.$field "resources.$field" }
        if ($resources.max_scratch_bytes -ne 2GB -or $resources.max_matrix_bytes -ne 512MB -or $resources.max_observed_working_set_bytes -ne 2GB) { throw 'Replay resource caps differ from the frozen v1 resource contract.' }
        if ($resources.peak_scratch_bytes -gt $resources.max_scratch_bytes -or $resources.peak_observed_runner_plus_engine_working_set_bytes -gt $resources.max_observed_working_set_bytes) { throw 'Replay reported a resource limit violation.' }
    }
    $report.package_unchanged=(Get-FileHash -LiteralPath $Package).Hash.ToLowerInvariant() -ceq $report.package_sha256
    $report.cli_unchanged=(Get-FileHash -LiteralPath $Cli).Hash.ToLowerInvariant() -ceq $report.cli_sha256
    if (!$report.package_unchanged -or !$report.cli_unchanged) { throw 'Package or CLI bytes changed during replay.' }
    $report.hardware=@{cpu=@(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors);physical_memory_bytes=(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory;os=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,OSArchitecture)}
    $report.pass=$true
} catch {
    $report.error=$_.Exception.Message
    throw
} finally {
    $watch.Stop()
    $report.finished_utc=[DateTime]::UtcNow.ToString('o')
    $report.wall_seconds=$watch.Elapsed.TotalSeconds
    $report | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8NoBOM
}
Write-Output $OutputDirectory
