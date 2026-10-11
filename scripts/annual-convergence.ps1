#Requires -Version 7.2
# Full-year numerical convergence, not independent physical validation.
# Runs both complete alternatives at both profiles; no sampled-hour extrapolation.
param(
    [string]$Cli = '',
    [string]$OutputDirectory = '',
    [switch]$ReplayShipping
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/dev-env.ps1" -SkipVisualStudio
if (!$Cli) { $Cli = Join-Path $env:CARGO_TARGET_DIR 'debug/greenlens.exe' }
if (!(Test-Path -LiteralPath $Cli -PathType Leaf)) { throw 'Build the required native CLI first.' }
if (!$OutputDirectory) { $OutputDirectory = Join-Path $env:LOCALAPPDATA ('GreenLens/verification/v0.3 full-year convergence ' + [guid]::NewGuid().ToString('N')) }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Use a new verification directory; earlier results are retained.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$started = [DateTime]::UtcNow
$watch = [Diagnostics.Stopwatch]::StartNew()
foreach ($profile in @('shipping','reference')) {
    $arguments = @{Cli=$Cli;Mode='Annual';Profile=$profile;OutputDirectory=(Join-Path $OutputDirectory $profile)}
    if ($profile -eq 'shipping' -and $ReplayShipping) { $arguments.Replay = $true }
    & "$PSScriptRoot/climate-cli-integration.ps1" @arguments | Out-Null
}
$checks = @()
$failed = @()
$metricNames = @('da_percent','cda_percent','udi_percent','underlit_percent','overlit_percent')
foreach ($alternative in @('A','B')) {
    $shipping = (Get-Content -LiteralPath (Join-Path $OutputDirectory "shipping/completed-$alternative.json") -Raw | ConvertFrom-Json -DateKind String).climate_result
    $reference = (Get-Content -LiteralPath (Join-Path $OutputDirectory "reference/completed-$alternative.json") -Raw | ConvertFrom-Json -DateKind String).climate_result
    if ($shipping.series.interval_count -ne 8760 -or $reference.series.interval_count -ne 8760) { throw 'A full year from both profiles is mandatory.' }
    if ($shipping.series.sensor_ids.Count -ne 24 -or $reference.series.sensor_ids.Count -ne 24) { throw 'The predeclared annual example has exactly 24 sensors.' }
    if (($shipping.series.sensor_ids -join '|') -ne ($reference.series.sensor_ids -join '|')) { throw 'Sensor identity/order mismatch.' }
    if (($shipping.study.metrics | ConvertTo-Json -Depth 20 -Compress) -ne ($reference.study.metrics | ConvertTo-Json -Depth 20 -Compress)) { throw 'Metric conventions differ.' }
    if (($shipping.study.calculation.weather | ConvertTo-Json -Compress) -ne ($reference.study.calculation.weather | ConvertTo-Json -Compress)) { throw 'Weather identity differs.' }
    if (($shipping.model | ConvertTo-Json -Depth 30 -Compress) -ne ($reference.model | ConvertTo-Json -Depth 30 -Compress)) { throw 'Geometry/material/sensor inputs differ between profiles.' }
    if ($shipping.metrics.Count -ne 24 -or $reference.metrics.Count -ne 24) { throw 'Missing exact metric count.' }
    for ($sensor=0; $sensor -lt 24; $sensor++) {
        foreach ($metric in $metricNames) {
            $value = [double]$shipping.metrics[$sensor].$metric
            $target = [double]$reference.metrics[$sensor].$metric
            if (![double]::IsFinite($value) -or ![double]::IsFinite($target)) { throw 'Nonfinite metric.' }
            $error = [Math]::Abs($value-$target)
            $check = [ordered]@{alternative=$alternative;sensor_id=$shipping.series.sensor_ids[$sensor];metric=$metric;shipping=$value;reference=$target;absolute_difference_pp=$error;tolerance_pp=2;pass=($error -le 2)}
            $checks += $check
            if (!$check.pass) { $failed += $check }
        }
        foreach ($record in @($shipping,$reference)) {
            $metric = $record.metrics[$sensor]
            $partitionError = [Math]::Abs($metric.udi_percent+$metric.underlit_percent+$metric.overlit_percent-100)
            if ($partitionError -gt 1e-9) { throw 'UDI partition fails unchanged 1e-9 percentage-point threshold.' }
        }
    }
}
$watch.Stop()
$summary = [ordered]@{
    schema_version='greenlens.annual-convergence-evidence.v0.3'
    started_utc=$started.ToString('o');finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$watch.Elapsed.TotalSeconds
    cli_sha256=(Get-FileHash -LiteralPath $Cli -Algorithm SHA256).Hash.ToLowerInvariant()
    scope='Complete real-weather A/B, both numerical profiles; numerical convergence, not independent physical or measured-building validation.'
    alternatives=2;profiles=2;intervals_per_run=8760;sensors_per_run=24;checks=$checks;failures=$failed.Count;pass=($failed.Count -eq 0)
    shipping_evidence='shipping/summary.json';reference_evidence='reference/summary.json';shipping_replay_executed=[bool]$ReplayShipping
    native_desktop_acceptance=$false
}
$summary | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'convergence.json') -Encoding utf8NoBOM
if ($failed.Count) { throw "Annual convergence failed $($failed.Count) frozen per-sensor checks; all results retained in $OutputDirectory" }
Write-Output $OutputDirectory
