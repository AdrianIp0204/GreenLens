#Requires -Version 7.5
# Comparison only: validate and read four saved runs; never run/replay Radiance.
[CmdletBinding()]
param(
    [string]$InputsFile,
    [string]$Cli,
    [string]$OutputDirectory,
    [ValidateRange(1,256)][int]$ExpectedSensorCount = 24,
    [switch]$FunctionsOnly
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

function ConvertTo-ConvergenceCanonical($Value) {
    if ($null -eq $Value) { return $null }
    if ($Value -is [string] -or $Value -is [ValueType]) { return $Value }
    if ($Value -is [Collections.IDictionary]) {
        $ordered = [ordered]@{}
        foreach ($key in ($Value.Keys | Sort-Object -CaseSensitive)) { $ordered[$key] = ConvertTo-ConvergenceCanonical $Value[$key] }
        return $ordered
    }
    if ($Value -is [pscustomobject]) {
        $ordered = [ordered]@{}
        foreach ($key in ($Value.PSObject.Properties.Name | Sort-Object -CaseSensitive)) { $ordered[$key] = ConvertTo-ConvergenceCanonical $Value.$key }
        return $ordered
    }
    if ($Value -is [Collections.IEnumerable] -and $Value -isnot [string]) {
        return ,@($Value | ForEach-Object { ConvertTo-ConvergenceCanonical $_ })
    }
    return $Value
}
function Assert-ConvergenceEqual($Left, $Right, [string]$Label) {
    $a = ConvertTo-Json -InputObject (ConvertTo-ConvergenceCanonical $Left) -Depth 100 -Compress
    $b = ConvertTo-Json -InputObject (ConvertTo-ConvergenceCanonical $Right) -Depth 100 -Compress
    if ($a -cne $b) { throw "Convergence identity mismatch: $Label." }
}
function Assert-ConvergenceSelection($InputManifest) {
    if ($InputManifest.schema_version -cne 'greenlens.annual-convergence-inputs.v0.3' -or @($InputManifest.runs).Count -ne 4) { throw 'Select exactly four saved annual runs using greenlens.annual-convergence-inputs.v0.3.' }
    $keys = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $ids = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($entry in $InputManifest.runs) {
        if ($entry.alternative -cnotin @('A','B') -or $entry.profile -cnotin @('shipping','reference')) { throw 'Unsupported alternative/profile in convergence selection.' }
        if (!$keys.Add("$($entry.profile)-$($entry.alternative)")) { throw 'Duplicate alternative/profile; the convergence matrix is incomplete.' }
        $id = [guid]::Empty
        if ($entry.run_id -isnot [string] -or ![guid]::TryParseExact($entry.run_id,'D',[ref]$id) -or $id.ToString() -cne $entry.run_id -or !$ids.Add($entry.run_id)) { throw 'Run IDs must be distinct canonical lowercase UUIDs.' }
        if ($entry.project -isnot [string] -or [string]::IsNullOrWhiteSpace($entry.project) -or $entry.project.Length -gt 4096) { throw 'Every selected run requires a bounded project directory.' }
    }
}
function Get-ConvergenceMetric($Metric, [string]$Name) {
    $value = $Metric.$Name
    if ($null -eq $value -or $value -is [string] -or $value -is [bool] -or ![double]::IsFinite([double]$value) -or [double]$value -lt 0 -or [double]$value -gt 100) { throw "Missing, nonfinite or out-of-range metric: $Name." }
    return [double]$value
}
function Compare-AnnualConvergence([Collections.IDictionary]$Records, [int]$SensorCount) {
    if ($SensorCount -lt 1 -or $SensorCount -gt 256 -or $Records.Count -ne 4) { throw 'Expected four bounded annual records.' }
    $baseline = $Records['shipping-A']
    $source = $baseline.provenance.source_commit
    if ($source -isnot [string] -or $source -cnotmatch '^[0-9a-f]{40}$' -or $baseline.provenance.source_worktree_dirty -isnot [bool]) { throw 'Embedded source commit/dirty identity is required.' }
    foreach ($profile in @('shipping','reference')) {
        foreach ($alternative in @('A','B')) {
            $record = $Records["$profile-$alternative"]
            if ($null -eq $record -or $record.alternative_id -cne $alternative -or $record.study.calculation.mode -cne 'annual' -or $record.study.calculation.profile -cne $profile -or $record.provenance.profile.name -cne $profile) { throw 'Selected record does not match its annual alternative/profile slot.' }
            if ($record.provenance.state -cne 'completed' -or $record.provenance.units -cne 'lx' -or $record.series.interval_count -ne 8760) { throw 'Every run must be completed with all 8760 hourly illuminance intervals.' }
            if (@($record.series.sensor_ids).Count -ne $SensorCount -or @($record.metrics).Count -ne $SensorCount -or @($record.series.sensor_ids | Select-Object -Unique).Count -ne $SensorCount) { throw 'Exact sensor/metric count and unique sensor IDs are required.' }
            Assert-ConvergenceEqual $record.series.sensor_ids $baseline.series.sensor_ids 'sensor identity/order'
            Assert-ConvergenceEqual $record.validated.sensor_hash $baseline.validated.sensor_hash 'world-space sensors'
            Assert-ConvergenceEqual $record.study.calculation.weather $baseline.study.calculation.weather 'weather and time interpretation'
            Assert-ConvergenceEqual $record.study.metrics $baseline.study.metrics 'occupancy and thresholds'
            Assert-ConvergenceEqual $record.provenance.engine $baseline.provenance.engine 'engine and method identity'
            Assert-ConvergenceEqual $record.provenance.adapter_version $baseline.provenance.adapter_version 'adapter version'
            Assert-ConvergenceEqual $record.provenance.application_version $baseline.provenance.application_version 'application version'
            Assert-ConvergenceEqual $record.provenance.source_commit $source 'embedded source commit'
            Assert-ConvergenceEqual $record.provenance.source_worktree_dirty $baseline.provenance.source_worktree_dirty 'embedded source dirty state'
        }
        Assert-ConvergenceEqual $Records["$profile-A"].provenance.profile $Records["$profile-B"].provenance.profile "$profile complete numerical settings"
        Assert-ConvergenceEqual $Records["$profile-A"].provenance.comparison_fingerprint $Records["$profile-B"].provenance.comparison_fingerprint "$profile A/B comparison conditions"
    }
    $checks = [Collections.Generic.List[object]]::new()
    $partitions = [Collections.Generic.List[object]]::new()
    $failures = [Collections.Generic.List[object]]::new()
    foreach ($alternative in @('A','B')) {
        $shipping = $Records["shipping-$alternative"]
        $reference = $Records["reference-$alternative"]
        Assert-ConvergenceEqual $shipping.model $reference.model "$alternative complete model between profiles"
        for ($sensor=0; $sensor -lt $SensorCount; $sensor++) {
            foreach ($metric in @('da_percent','cda_percent','udi_percent','underlit_percent','overlit_percent')) {
                $value = Get-ConvergenceMetric $shipping.metrics[$sensor] $metric
                $target = Get-ConvergenceMetric $reference.metrics[$sensor] $metric
                $errorPp = [Math]::Abs($value-$target)
                $check = [ordered]@{alternative=$alternative;sensor_id=$shipping.series.sensor_ids[$sensor];metric=$metric;shipping=$value;reference=$target;absolute_difference_pp=$errorPp;tolerance_pp=2;pass=($errorPp -le 2)}
                $checks.Add($check)
                if (!$check.pass) { $failures.Add([ordered]@{kind='metric_convergence';check=$check}) }
            }
            foreach ($profile in @('shipping','reference')) {
                $metric = $Records["$profile-$alternative"].metrics[$sensor]
                $sum = (Get-ConvergenceMetric $metric 'udi_percent') + (Get-ConvergenceMetric $metric 'underlit_percent') + (Get-ConvergenceMetric $metric 'overlit_percent')
                $errorPp = [Math]::Abs($sum-100)
                $check = [ordered]@{alternative=$alternative;profile=$profile;sensor_id=$shipping.series.sensor_ids[$sensor];sum_percent=$sum;absolute_difference_pp=$errorPp;tolerance_pp=1e-9;pass=($errorPp -le 1e-9)}
                $partitions.Add($check)
                if (!$check.pass) { $failures.Add([ordered]@{kind='udi_partition';check=$check}) }
            }
        }
    }
    return [ordered]@{checks=$checks.ToArray();partition_checks=$partitions.ToArray();metric_check_count=$checks.Count;partition_check_count=$partitions.Count;failure_details=$failures.ToArray();failures=$failures.Count;pass=($failures.Count -eq 0)}
}
function Read-ConvergenceRecord([string]$Archive, [string]$RunId) {
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        if ($zip.Entries.Count -gt 5000) { throw 'Validated archive exceeds the shared member limit.' }
        $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        [long]$total = 0
        foreach ($entry in $zip.Entries) {
            if (!$names.Add($entry.FullName) -or $entry.FullName -match '(^/|\\|:|(^|/)\.\.?(/|$))' -or $entry.Length -gt 64MB) { throw 'Invalid, duplicate or oversized archive member.' }
            $total += $entry.Length
        }
        if ($total -gt 256MB) { throw 'Validated archive exceeds the shared total size limit.' }
        $entry = $zip.GetEntry("runs/$RunId/results.json")
        if ($null -eq $entry) { throw 'Explicit selected run is absent from its validated export.' }
        $reader = [IO.StreamReader]::new($entry.Open(), [Text.Encoding]::UTF8, $true)
        try { return ($reader.ReadToEnd() | ConvertFrom-Json -DateKind String -Depth 100) }
        finally { $reader.Dispose() }
    } finally { $zip.Dispose() }
}
# Synthetic script regression only; no validation/acceptance artifact is written.
if ($FunctionsOnly) { return }
if (!$OutputDirectory) { $OutputDirectory = Join-Path $env:LOCALAPPDATA ('GreenLens/verification/v0.3 convergence comparison ' + [guid]::NewGuid().ToString('N')) }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new output directory; prior evidence is never overwritten.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$started = [DateTime]::UtcNow
$watch = [Diagnostics.Stopwatch]::StartNew()
$validated = [Collections.Generic.List[object]]::new()
$report = [ordered]@{
    schema_version='greenlens.annual-convergence-evidence.v0.3';comparison_only=$true
    started_utc=$started.ToString('o');source_selection_sha256=$null;cli_sha256=$null
    cli_source_identity='Not inferred from checkout; executable hash/version recorded, embedded simulation source identity retained per validated run.'
    alternatives=2;profiles=2;intervals_per_run=8760;sensors_per_run=$ExpectedSensorCount
    simulation_executed=$false;shipping_replay_executed=$false;native_desktop_acceptance=$false
    scope='Saved complete-year evidence numerical convergence; not independent physical validation, replay or native desktop acceptance.'
    checks=@();partition_checks=@();metric_check_count=0;partition_check_count=0;failure_details=@();failures=0;pass=$false
}
try {
    if (!$InputsFile -or !$Cli) { throw 'Supply -InputsFile and the trusted native -Cli executable.' }
    $Cli = (Resolve-Path -LiteralPath $Cli).Path
    $InputsFile = (Resolve-Path -LiteralPath $InputsFile).Path
    if ((Get-Item -LiteralPath $InputsFile).Length -gt 64KB) { throw 'Convergence input selection exceeds 64 KiB.' }
    $report.cli_sha256 = (Get-FileHash -LiteralPath $Cli).Hash.ToLowerInvariant()
    $report.source_selection_sha256 = (Get-FileHash -LiteralPath $InputsFile).Hash.ToLowerInvariant()
    $selection = Get-Content -LiteralPath $InputsFile -Raw | ConvertFrom-Json -DateKind String -Depth 20
    Assert-ConvergenceSelection $selection
    $version = & $Cli --version 2> (Join-Path $OutputDirectory 'cli-version.stderr.txt')
    if ($LASTEXITCODE -ne 0) { throw 'Required CLI version query failed.' }
    $report.cli_version = ($version -join "`n").Trim()
    $records = @{}
    foreach ($entry in $selection.runs) {
        $key = "$($entry.profile)-$($entry.alternative)"
        $project = [IO.Path]::GetFullPath($entry.project, (Split-Path -Parent $InputsFile))
        if (!(Test-Path -LiteralPath $project -PathType Container)) { throw "Required project is unavailable for $key." }
        $archive = Join-Path $OutputDirectory "$key.zip"
        # The existing Rust export service reopens and validates full retained
        # weather, raw RGB/lux chunks, metrics, logs, recipe and checksums. A fast
        # `result` metadata query alone cannot establish this acceptance.
        & $Cli export $project $archive --run $entry.run_id 1> (Join-Path $OutputDirectory "$key.stdout.txt") 2> (Join-Path $OutputDirectory "$key.stderr.txt")
        if ($LASTEXITCODE -ne 0) { throw "Full Rust evidence validation/export failed for $key; logs retained." }
        $record = Read-ConvergenceRecord $archive $entry.run_id
        if ($record.id -cne $entry.run_id) { throw "Validated export record ID differs for $key." }
        $records[$key] = $record
        $validated.Add([ordered]@{alternative=$entry.alternative;profile=$entry.profile;run_id=$record.id;archive="$key.zip";archive_sha256=(Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant();archive_bytes=(Get-Item -LiteralPath $archive).Length;intervals=$record.series.interval_count;sensors=@($record.series.sensor_ids).Count;source_commit=$record.provenance.source_commit;source_worktree_dirty=$record.provenance.source_worktree_dirty;engine=$record.provenance.engine;numerical_profile=$record.provenance.profile;physical_input_hash=$record.provenance.physical_input_hash;physical_result_fingerprint=$record.provenance.physical_result_fingerprint})
    }
    $comparison = Compare-AnnualConvergence $records $ExpectedSensorCount
    foreach ($key in $comparison.Keys) { $report[$key] = $comparison[$key] }
    if (!$report.pass) { throw "Annual convergence failed $($report.failures) checks; every comparison and partition is retained." }
} catch {
    $report.pass = $false
    $report.error = $_.Exception.Message
    if ($report.failures -eq 0) {
        $report.failures = 1
        $report.failure_details = @([ordered]@{kind='input_or_evidence_validation';message=$report.error})
    }
    throw
} finally {
    $watch.Stop()
    $report.finished_utc = [DateTime]::UtcNow.ToString('o')
    $report.wall_seconds = $watch.Elapsed.TotalSeconds
    $report.validated_runs = $validated.ToArray()
    $report | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'convergence.json') -Encoding utf8NoBOM
}
Write-Output $OutputDirectory
