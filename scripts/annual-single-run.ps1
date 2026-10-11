#Requires -Version 7.5
# One complete annual CI matrix member through the shared native CLI services.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('A','B')][string]$Alternative,
    [Parameter(Mandatory)][ValidateSet('shipping','reference')][string]$Profile,
    [Parameter(Mandatory)][string]$Cli,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$EngineDirectory = ''
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new annual output directory; prior projects/evidence are never overwritten.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$started = [DateTime]::UtcNow
$watch = [Diagnostics.Stopwatch]::StartNew()
$report = [ordered]@{
    schema_version='greenlens.annual-single-run-evidence.v0.3';alternative=$Alternative;profile=$Profile
    started_utc=$started.ToString('o');pass=$false;completed=$false;full_evidence_validated=$false
    project='project';evidence='annual-evidence.zip';expected_intervals=8760;expected_sensors=24
    replay_executed=$false;accuracy_acceptance=$false;native_desktop_acceptance=$false
    scope='One complete annual native CLI integration run. Convergence, matched-sky science, replay and installed desktop acceptance are separate.'
}
function Invoke-SingleRunCli([string[]]$Arguments, [string]$Name) {
    $stdout = Join-Path $OutputDirectory "$Name.stdout.json"
    & $Cli @Arguments 1> $stdout 2> (Join-Path $OutputDirectory "$Name.stderr.txt")
    if ($LASTEXITCODE -ne 0) { throw "Required native CLI $Name failed with exit $LASTEXITCODE; retained logs. This is a failure, not a skipped run." }
    if ((Get-Item -LiteralPath $stdout).Length -gt 64MB) { throw "CLI $Name output exceeded the bounded metadata limit." }
    return $stdout
}
function Assert-SingleWeather($Binding, $Dataset) {
    # Isolated function scope preserves the caller's CLI/output parameters.
    . "$PSScriptRoot/weather-verification.ps1" -FunctionsOnly
    Assert-ReviewedWeatherBinding $Binding $Dataset
}
function Read-SingleValidatedExport([string]$Archive, [string]$RunId, $CompletedRecord) {
    . "$PSScriptRoot/annual-compare-convergence.ps1" -FunctionsOnly
    $record = Read-ConvergenceRecord $Archive $RunId
    Assert-ConvergenceEqual $record $CompletedRecord 'completed versus validated saved run'
    return $record
}
try {
    $Cli = (Resolve-Path -LiteralPath $Cli).Path
    $report.cli_sha256 = (Get-FileHash -LiteralPath $Cli -Algorithm SHA256).Hash.ToLowerInvariant()
    $versionFile = Invoke-SingleRunCli @('--version') 'cli-version'
    $report.cli_version = (Get-Content -LiteralPath $versionFile -Raw).Trim()
    $repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
    $dataset = (Get-Content -LiteralPath (Join-Path $repository 'dependencies/weather.json') -Raw | ConvertFrom-Json -DateKind String).datasets[0]
    $weather = Join-Path $repository 'examples/weather/pvgis-hong-kong-tmy-2005-2023.epw'
    $source = Join-Path $repository 'examples/weather/pvgis-hong-kong.source.json'
    if (!(Test-Path -LiteralPath $weather -PathType Leaf) -or (Get-FileHash -LiteralPath $weather).Hash.ToLowerInvariant() -cne $dataset.original_sha256) { throw 'Required reviewed original real-weather file is absent or has changed.' }
    if (!(Test-Path -LiteralPath $source -PathType Leaf)) { throw 'Required weather source/rights record is unavailable.' }
    $project = Join-Path $OutputDirectory 'project'
    $asset = Join-Path $OutputDirectory 'weather-import'
    Invoke-SingleRunCli @('example',$project) 'example' | Out-Null
    $weatherFile = Invoke-SingleRunCli @('weather','import',$weather,$asset,'--source-profile','pvgis-tmy','--source-record',$source) 'weather-import'
    $weatherInspection = Get-Content -LiteralPath $weatherFile -Raw | ConvertFrom-Json -DateKind String -Depth 100
    Assert-SingleWeather $weatherInspection.binding $dataset
    Invoke-SingleRunCli @('weather','attach',$project,$asset) 'weather-attached' | Out-Null
    $manifest = Join-Path $project 'project.json'
    $document = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json -DateKind String -Depth 100
    if ($document.alternatives.Count -ne 2 -or @($document.alternatives | Where-Object id -CEQ $Alternative).Count -ne 1) { throw 'Shared synthetic example lacks the requested alternative.' }
    # Match climate-cli-integration.ps1 exactly: original shared A/B example,
    # 1.0 m grid, default daily 08:00–18:00 metrics. Only this new empty project is
    # edited; configure/run revalidate through Rust before any calculation.
    foreach ($entry in $document.alternatives) { $entry.model.sensors.requested_spacing_m = 1.0 }
    $document | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $manifest -Encoding utf8NoBOM
    $study = [ordered]@{
        schema_version='greenlens.study.v0.3'
        calculation=@{mode='annual';weather=$weatherInspection.binding;profile=$Profile}
        metrics=@{da_threshold_lux=300;udi_lower_lux=100;udi_upper_lux=3000;occupancy=@{kind='daily';start_minute=480;end_minute=1080}}
    }
    $studyFile = Join-Path $OutputDirectory 'study.json'
    $study | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $studyFile -Encoding utf8NoBOM
    Invoke-SingleRunCli @('study','configure',$project,$studyFile,'--alternative',$Alternative) 'configured' | Out-Null
    $runArguments = @('study','run',$project,'--alternative',$Alternative)
    if ($EngineDirectory) { $runArguments += @('--engine-dir',[IO.Path]::GetFullPath($EngineDirectory)) }
    Write-Host "Calculating all 8,760 intervals for alternative $Alternative with the $Profile profile."
    $completedFile = Invoke-SingleRunCli $runArguments 'completed'
    $status = Get-Content -LiteralPath $completedFile -Raw | ConvertFrom-Json -DateKind String -Depth 100
    $run = $status.climate_result
    if ($status.state -cne 'completed' -or $null -eq $run -or $run.alternative_id -cne $Alternative -or $run.study.calculation.mode -cne 'annual' -or $run.study.calculation.profile -cne $Profile -or $run.provenance.profile.name -cne $Profile) { throw 'Native job did not complete the requested annual alternative/profile.' }
    if ($run.series.interval_count -ne 8760 -or @($run.series.sensor_ids).Count -ne 24 -or @($run.metrics).Count -ne 24) { throw 'Annual run does not contain exactly 8760 intervals and 24 sensors/metrics.' }
    Assert-SingleWeather $run.study.calculation.weather $dataset
    $report.completed = $true
    $export = Join-Path $OutputDirectory 'annual-evidence.zip'
    Invoke-SingleRunCli @('export',$project,$export,'--run',$run.id) 'validated-export' | Out-Null
    $run = Read-SingleValidatedExport $export $run.id $run
    $zip = [IO.Compression.ZipFile]::OpenRead($export)
    try {
        $entry = $zip.GetEntry("runs/$($run.id)/annual-resources.json")
        if ($null -eq $entry -or $entry.Length -gt 1MB) { throw 'Validated export lacks a bounded native resource record.' }
        $reader = [IO.StreamReader]::new($entry.Open(),[Text.Encoding]::UTF8,$true)
        try { $resources = $reader.ReadToEnd() | ConvertFrom-Json -DateKind String -Depth 100 } finally { $reader.Dispose() }
    } finally { $zip.Dispose() }
    if ((Get-FileHash -LiteralPath $Cli).Hash.ToLowerInvariant() -cne $report.cli_sha256) { throw 'CLI bytes changed during execution; use a frozen executable copy.' }
    $report.full_evidence_validated = $true
    $report.run_id = $run.id
    $report.intervals = $run.series.interval_count
    $report.sensors = @($run.series.sensor_ids).Count
    $report.source_commit = $run.provenance.source_commit
    $report.source_worktree_dirty = $run.provenance.source_worktree_dirty
    $report.engine = $run.provenance.engine
    $report.numerical_profile = $run.provenance.profile
    $report.weather = $run.study.calculation.weather
    $report.physical_input_hash = $run.provenance.physical_input_hash
    $report.physical_result_fingerprint = $run.provenance.physical_result_fingerprint
    $report.resources = $resources
    $report.package_bytes = (Get-Item -LiteralPath $export).Length
    $report.package_sha256 = (Get-FileHash -LiteralPath $export).Hash.ToLowerInvariant()
    $report.convergence_selection = @{alternative=$Alternative;profile=$Profile;project='project';run_id=$run.id}
    $report.hardware = @{cpu=@(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors);physical_memory_bytes=(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory;os=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,OSArchitecture)}
    $report.pass = $true
} catch {
    $report.error = $_.Exception.Message
    throw
} finally {
    $watch.Stop()
    $report.finished_utc = [DateTime]::UtcNow.ToString('o')
    $report.wall_seconds = $watch.Elapsed.TotalSeconds
    $report | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8NoBOM
}
Write-Output $OutputDirectory
