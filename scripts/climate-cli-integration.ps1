#Requires -Version 7.5
# Native real-engine integration, with explicit whole-year mode. No optional engine skip.
param(
    [ValidateSet('WeatherHour','Annual')][string]$Mode = 'WeatherHour',
    [ValidateSet('shipping','reference')][string]$Profile = 'shipping',
    [int]$IntervalIndex = 1908,
    [string]$Cli = '',
    [string]$OutputDirectory = '',
    [switch]$Replay
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/dev-env.ps1" -SkipVisualStudio
if (!$Cli) { $Cli = Join-Path $env:CARGO_TARGET_DIR 'debug/greenlens.exe' }
if (!(Test-Path -LiteralPath $Cli -PathType Leaf)) { throw 'Build the native CLI before running required integration.' }
if (!$OutputDirectory) { $OutputDirectory = Join-Path $env:LOCALAPPDATA ('GreenLens/verification/v0.3 CLI 空間 ' + [guid]::NewGuid().ToString('N')) }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new output directory; earlier evidence is never overwritten.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$projectDirectory = Join-Path $OutputDirectory 'Original project'
$copyDirectory = Join-Path $OutputDirectory 'Portable copy'
$assetDirectory = Join-Path $OutputDirectory 'Weather import'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$weatherFile = Join-Path $OutputDirectory 'Input weather.epw'
Copy-Item -LiteralPath (Join-Path $repositoryRoot 'examples/weather/pvgis-hong-kong-tmy-2005-2023.epw') -Destination $weatherFile
$sourceRecord = Join-Path $repositoryRoot 'examples/weather/pvgis-hong-kong.source.json'
function Invoke-Cli([string[]]$Arguments, [string]$OutputName) {
    & $Cli @Arguments | Out-File -LiteralPath (Join-Path $OutputDirectory $OutputName) -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw "Required CLI integration failed: $($Arguments[0]); retained $OutputName" }
}
Invoke-Cli @('example',$projectDirectory) 'example.json'
Invoke-Cli @('weather','import',$weatherFile,$assetDirectory,'--source-profile','pvgis-tmy','--source-record',$sourceRecord) 'weather-import.json'
Invoke-Cli @('weather','attach',$projectDirectory,$assetDirectory) 'weather-attached.json'
$manifestPath = Join-Path $projectDirectory 'project.json'
$project = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -DateKind String
# This is the documented annual example: 24 sensors, distinct from legacy 77.
foreach ($alternative in $project.alternatives) { $alternative.model.sensors.requested_spacing_m = 1.0 }
$project | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
$calculation = @{mode=$(if ($Mode -eq 'Annual') {'annual'} else {'weather_hour'});weather=$project.climate.weather[0].binding;profile=$Profile}
if ($Mode -eq 'WeatherHour') { $calculation.interval_index = $IntervalIndex }
$study = @{schema_version='greenlens.study.v0.3';calculation=$calculation;metrics=@{da_threshold_lux=300;udi_lower_lux=100;udi_upper_lux=3000;occupancy=@{kind='daily';start_minute=480;end_minute=1080}}}
$studyPath = Join-Path $OutputDirectory 'study.json'
$study | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $studyPath -Encoding utf8NoBOM
$started = [DateTime]::UtcNow
$watch = [Diagnostics.Stopwatch]::StartNew()
$completed = @()
foreach ($alternative in @('A','B')) {
    Invoke-Cli @('study','configure',$projectDirectory,$studyPath,'--alternative',$alternative) "configured-$alternative.json"
    Invoke-Cli @('study','run',$projectDirectory,'--alternative',$alternative) "completed-$alternative.json"
    $status = Get-Content -LiteralPath (Join-Path $OutputDirectory "completed-$alternative.json") -Raw | ConvertFrom-Json -DateKind String
    $run = $status.climate_result
    if ($status.state -ne 'completed' -or $run.series.sensor_ids.Count -ne 24 -or $run.series.interval_count -ne $(if ($Mode -eq 'Annual') {8760} else {1})) { throw 'Completed result has incorrect state or exact dimensions.' }
    $completed += $run
}
$watch.Stop()
Invoke-Cli @('copy',$projectDirectory,$copyDirectory) 'copied-project.json'
$runHashes = @{}
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $projectDirectory 'runs') -File -Recurse) {
    $relative = [IO.Path]::GetRelativePath($projectDirectory,$file.FullName)
    $hash = (Get-FileHash -LiteralPath $file.FullName).Hash
    if ((Get-FileHash -LiteralPath (Join-Path $copyDirectory $relative)).Hash -ne $hash) { throw 'Copied immutable evidence differs.' }
    $runHashes[$relative.Replace('\','/')] = $hash.ToLowerInvariant()
}
# Only owned paths under this new verification directory are made unavailable.
$safeOutput = [IO.Path]::GetFullPath($OutputDirectory).TrimEnd('\') + '\'
foreach ($owned in @($projectDirectory,$weatherFile,$assetDirectory)) {
    $resolved = (Get-Item -LiteralPath $owned).FullName
    if (!$resolved.StartsWith($safeOutput,[StringComparison]::OrdinalIgnoreCase)) { throw 'Portability target escaped owned verification directory.' }
    Move-Item -LiteralPath $resolved -Destination ($resolved + '.unavailable')
}
$scratchRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens/jobs')).TrimEnd('\') + '\'
foreach ($run in $completed) {
    $runId = [guid]::Parse($run.id).ToString()
    $scratch = [IO.Path]::GetFullPath((Join-Path $scratchRoot $runId))
    if (!$scratch.StartsWith($scratchRoot,[StringComparison]::OrdinalIgnoreCase)) { throw 'Scratch target escaped owned job directory.' }
    if (Test-Path -LiteralPath $scratch) { Move-Item -LiteralPath $scratch -Destination ($scratch + '.portability-unavailable') }
}
$export = Join-Path $OutputDirectory 'climate-evidence.zip'
foreach ($run in $completed) {
    Invoke-Cli @('result',$copyDirectory,$run.id,'--hour',"$IntervalIndex") ("inspected-hour-" + $run.alternative_id + '.json')
    Invoke-Cli @('result',$copyDirectory,$run.id,'--sensor',$run.series.sensor_ids[0]) ("inspected-sensor-" + $run.alternative_id + '.json')
}
Invoke-Cli @('study','compare',$copyDirectory,$completed[0].id,$completed[1].id,'--interval',"$IntervalIndex") 'hourly-comparison.json'
if ($Mode -eq 'Annual') { Invoke-Cli @('study','compare',$copyDirectory,$completed[0].id,$completed[1].id,'--field','da') 'da-comparison.json' }
Invoke-Cli @('export',$copyDirectory,$export,'--run',$completed[0].id,'--run',$completed[1].id) 'export.txt'
if ($Replay) { Invoke-Cli @('replay',$export) 'replay.json' }
$result = [ordered]@{
    schema_version='greenlens.climate-cli-integration.v0.3';mode=$Mode;profile=$Profile;started_utc=$started.ToString('o');finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$watch.Elapsed.TotalSeconds
    cli_sha256=(Get-FileHash -LiteralPath $Cli -Algorithm SHA256).Hash.ToLowerInvariant();project='Portable copy';evidence='climate-evidence.zip';package_bytes=(Get-Item -LiteralPath $export).Length
    portability=@{byte_identical_run_files=$runHashes.Count;original_project_unavailable=$true;original_weather_path_unavailable=$true;original_scratch_unavailable=$true;run_sha256=$runHashes}
    hardware=@{cpu=@(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors);physical_memory_bytes=(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory;os=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,OSArchitecture)}
    runs=@($completed | ForEach-Object { @{id=$_.id;alternative=$_.alternative_id;sensors=$_.series.sensor_ids.Count;intervals=$_.series.interval_count;source_commit=$_.provenance.source_commit;source_dirty=$_.provenance.source_worktree_dirty;engine=$_.provenance.engine;profile=$_.provenance.profile;resources=(Get-Content -LiteralPath (Join-Path $copyDirectory ("runs/" + $_.id + '/annual-resources.json')) -Raw | ConvertFrom-Json -DateKind String)} })
    replay_executed=[bool]$Replay;native_desktop_acceptance=$false;accuracy_acceptance=$false;scope='Real native shared-service A/B integration; scientific convergence and installed desktop are separate required checks.'
}
$result | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8NoBOM
Write-Output $OutputDirectory
