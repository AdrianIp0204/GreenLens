#Requires -Version 7.5
# Synthetic orchestration regressions only. No engine or scientific acceptance.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/weather-verification.ps1" -FunctionsOnly
. "$PSScriptRoot/annual-compare-convergence.ps1" -FunctionsOnly
$checks = [Collections.Generic.List[string]]::new()
function Assert-Test([bool]$Condition, [string]$Name) {
    if (!$Condition) { throw "Script regression failed: $Name" }
    $checks.Add($Name)
}
function Assert-Rejected([scriptblock]$Action, [string]$Expected, [string]$Name) {
    $caught = $null
    try { & $Action | Out-Null } catch { $caught = $_.Exception.Message }
    if (!$caught -or $caught -notmatch $Expected) { throw "$Name expected rejection '$Expected', got '$caught'." }
    $checks.Add($Name)
}
function Copy-Synthetic($Value) { return ($Value | ConvertTo-Json -Depth 100 | ConvertFrom-Json -DateKind String) }
function New-SyntheticRecords([int]$Count = 2) {
    $records = @{}
    foreach ($profile in @('shipping','reference')) {
        foreach ($alternative in @('A','B')) {
            $record = [ordered]@{
                id=[guid]::NewGuid().ToString();alternative_id=$alternative
                model=@{room=@{width=4};overhang=($alternative -eq 'B')}
                validated=@{sensor_hash=('c'*64)}
                study=@{calculation=@{mode='annual';profile=$profile;weather=@{original_sha256=('a'*64);normalized_sha256=('b'*64);time_axis_sha256=('c'*64);interpretation='synthetic-test-only'}};metrics=@{threshold=300;occupancy='synthetic-test-only'}}
                series=@{interval_count=8760;sensor_ids=@(0..($Count-1) | ForEach-Object { "sensor-$_" })}
                metrics=@(0..($Count-1) | ForEach-Object { @{da_percent=50.0;cda_percent=60.0;udi_percent=50.0;underlit_percent=25.0;overlit_percent=25.0} })
                provenance=@{source_commit=('d'*40);source_worktree_dirty=$false;state='completed';units='lx';profile=@{name=$profile;sampling=$(if ($profile -eq 'shipping') {1} else {2})};comparison_fingerprint=$profile;engine=@{version='synthetic-test-only'};adapter_version='synthetic-test-only';application_version='synthetic-test-only'}
            }
            $records["$profile-$alternative"] = Copy-Synthetic $record
        }
    }
    return $records
}
$dataset = (Get-Content -LiteralPath (Join-Path $PSScriptRoot '../dependencies/weather.json') -Raw | ConvertFrom-Json).datasets[0]
$binding = [pscustomobject]@{original_sha256=$dataset.original_sha256;normalized_sha256=$dataset.normalized_sha256;time_axis_sha256=$dataset.time_axis_sha256;interpretation=$dataset.interpretation}
Assert-ReviewedWeatherBinding $binding $dataset
$checks.Add('exact reviewed original/normalized/time-axis hashes accepted')
foreach ($field in @('original_sha256','normalized_sha256','time_axis_sha256')) {
    $changed = Copy-Synthetic $binding
    $changed.$field = '0'*64
    Assert-Rejected { Assert-ReviewedWeatherBinding $changed $dataset } $field "changed $field rejected"
}
$missing = Copy-Synthetic $binding
$missing.PSObject.Properties.Remove('normalized_sha256')
Assert-Rejected { Assert-ReviewedWeatherBinding $missing $dataset } 'normalized_sha256' 'missing normalized identity rejected'
$badInventory = Copy-Synthetic $dataset
$badInventory.time_axis_sha256 = ''
Assert-Rejected { Assert-ReviewedWeatherBinding $binding $badInventory } 'inventory' 'missing reviewed expected identity rejected'

$records = New-SyntheticRecords 24
$result = Compare-AnnualConvergence $records 24
Assert-Test ($result.pass -and $result.checks.Count -eq 240 -and $result.partition_checks.Count -eq 96) 'required 24-sensor example checks all 240 metrics and 96 partitions'
$records = New-SyntheticRecords 2
$records['shipping-A'].metrics[0].da_percent = 52
$result = Compare-AnnualConvergence $records 2
Assert-Test ($result.pass -and $result.checks.Count -eq 20 -and $result.partition_checks.Count -eq 8) 'configurable exact sensor count and inclusive 2pp threshold'
$records['shipping-A'].metrics[0].da_percent = 52.000000001
$records['shipping-B'].metrics[1].cda_percent = 62.1
$records['shipping-B'].metrics[0].udi_percent = 53
$result = Compare-AnnualConvergence $records 2
Assert-Test (!$result.pass -and $result.failures -eq 4 -and $result.checks.Count -eq 20 -and $result.partition_checks.Count -eq 8) 'all three metric failures plus UDI partition retained without early exit'

$records = New-SyntheticRecords
$records['reference-A'].metrics[0].udi_percent = 50.0000000005
Assert-Test (Compare-AnnualConvergence $records 2).pass 'UDI partition within unchanged 1e-9pp accepted'
$records['reference-A'].metrics[0].udi_percent = 50.000000002
Assert-Test (!(Compare-AnnualConvergence $records 2).pass) 'UDI partition beyond 1e-9pp fails'

$records = New-SyntheticRecords
$records.Remove('reference-B')
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'four' 'missing matrix member rejected'
$records = New-SyntheticRecords
$records['reference-A'].series.interval_count = 8759
Assert-Rejected { Compare-AnnualConvergence $records 2 } '8760' 'truncated year rejected'
$records = New-SyntheticRecords
$records['reference-A'].series.sensor_ids = @('sensor-1','sensor-0')
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'sensor identity' 'sensor ordering mismatch rejected'
$records = New-SyntheticRecords
$records['reference-A'].validated.sensor_hash = 'f'*64
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'world-space' 'same IDs with different world sensors rejected'
$records = New-SyntheticRecords
$records['reference-A'].model.room.width = 5
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'complete model' 'different geometry between profiles rejected'
$records = New-SyntheticRecords
$records['reference-A'].study.calculation.weather.time_axis_sha256 = 'f'*64
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'weather' 'different time policy rejected'
$records = New-SyntheticRecords
$records['reference-B'].provenance.source_commit = 'f'*40
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'source commit' 'mixed source builds rejected'
$records = New-SyntheticRecords
$records['shipping-B'].provenance.profile.sampling = 999
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'complete numerical' 'mixed settings within profile rejected'
$records = New-SyntheticRecords
$records['shipping-B'].provenance.comparison_fingerprint = 'incompatible'
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'comparison conditions' 'incompatible A/B rejected'
$records = New-SyntheticRecords
$records['reference-B'].metrics[0].da_percent = $null
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'Missing' 'null metric not silently converted to zero'
$records['reference-B'].metrics[0].da_percent = '50'
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'Missing' 'numeric string rejected'
$records['reference-B'].metrics[0].da_percent = [double]::NaN
Assert-Rejected { Compare-AnnualConvergence $records 2 } 'nonfinite' 'NaN metric rejected'

$selection = [pscustomobject]@{schema_version='greenlens.annual-convergence-inputs.v0.3';runs=@()}
foreach ($profile in @('shipping','reference')) { foreach ($alternative in @('A','B')) { $selection.runs += [pscustomobject]@{profile=$profile;alternative=$alternative;run_id=[guid]::NewGuid().ToString();project="relative $profile $alternative"} } }
Assert-ConvergenceSelection $selection
$checks.Add('four explicit selections accepted')
$duplicate = Copy-Synthetic $selection
$duplicate.runs[3].profile = 'shipping'
Assert-Rejected { Assert-ConvergenceSelection $duplicate } 'Duplicate' 'duplicate slot rejected'
$duplicate = Copy-Synthetic $selection
$duplicate.runs[3].run_id = $duplicate.runs[0].run_id
Assert-Rejected { Assert-ConvergenceSelection $duplicate } 'distinct' 'reused run ID rejected'
$missing = Copy-Synthetic $selection
$missing.runs = @($missing.runs[0..2])
Assert-Rejected { Assert-ConvergenceSelection $missing } 'four' 'incomplete selection rejected'

# Exercise the wrapper, ZIP reader and retained failure report with an explicit
# mock CLI. These files are synthetic test data, never real-engine evidence.
$testRoot = Join-Path $env:LOCALAPPDATA ('GreenLens/script-tests/convergence synthetic ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
'Synthetic script regression fixtures. No Radiance execution, physical simulation or release acceptance.' | Set-Content -LiteralPath (Join-Path $testRoot 'README.txt')
$mockCli = Join-Path $testRoot 'mock-cli.ps1'
@'
if ($args.Count -eq 1 -and $args[0] -eq '--version') { 'synthetic-script-mock-only'; exit 0 }
if ($args.Count -ne 5 -or $args[0] -ne 'export' -or $args[3] -ne '--run') { throw 'Mock forbids all commands except version and explicit export.' }
$zip = [IO.Compression.ZipFile]::Open($args[2], [IO.Compression.ZipArchiveMode]::Create)
try {
    $entry = $zip.CreateEntry("runs/$($args[4])/results.json")
    $writer = [IO.StreamWriter]::new($entry.Open(), [Text.UTF8Encoding]::new($false))
    try { $writer.Write([IO.File]::ReadAllText((Join-Path $args[1] 'results.json'))) } finally { $writer.Dispose() }
} finally { $zip.Dispose() }
'Synthetic mock export; no Rust/engine validation.'
exit 0
'@ | Set-Content -LiteralPath $mockCli -Encoding utf8NoBOM
$records = New-SyntheticRecords
$selection = [pscustomobject]@{schema_version='greenlens.annual-convergence-inputs.v0.3';runs=@()}
foreach ($profile in @('shipping','reference')) {
    foreach ($alternative in @('A','B')) {
        $key = "$profile-$alternative"
        $project = Join-Path $testRoot $key
        New-Item -ItemType Directory -Path $project | Out-Null
        $records[$key] | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $project 'results.json') -Encoding utf8NoBOM
        $selection.runs += [pscustomobject]@{profile=$profile;alternative=$alternative;run_id=$records[$key].id;project=$key}
    }
}
$inputFile = Join-Path $testRoot 'selection.json'
$selection | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $inputFile -Encoding utf8NoBOM
$passedDirectory = Join-Path $testRoot 'passed'
& "$PSScriptRoot/annual-compare-convergence.ps1" -InputsFile $inputFile -Cli $mockCli -OutputDirectory $passedDirectory -ExpectedSensorCount 2 | Out-Null
$wrapper = Get-Content -LiteralPath (Join-Path $passedDirectory 'convergence.json') -Raw | ConvertFrom-Json
Assert-Test ($wrapper.pass -and $wrapper.metric_check_count -eq 20 -and $wrapper.validated_runs.Count -eq 4 -and !$wrapper.simulation_executed -and $wrapper.cli_version -eq 'synthetic-script-mock-only') 'mock wrapper reads exact four archive records without simulation commands'
$selection.runs[3].profile = 'shipping'
$selection | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $inputFile -Encoding utf8NoBOM
$failedDirectory = Join-Path $testRoot 'duplicate'
Assert-Rejected { & "$PSScriptRoot/annual-compare-convergence.ps1" -InputsFile $inputFile -Cli $mockCli -OutputDirectory $failedDirectory -ExpectedSensorCount 2 } 'Duplicate' 'wrapper rejects duplicate before export'
$wrapper = Get-Content -LiteralPath (Join-Path $failedDirectory 'convergence.json') -Raw | ConvertFrom-Json
Assert-Test (!$wrapper.pass -and $wrapper.failure_details[0].kind -eq 'input_or_evidence_validation' -and $wrapper.validated_runs.Count -eq 0) 'duplicate failure report retained with no successful evidence claim'
$selection.runs[3].profile = 'reference'
$selection | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $inputFile -Encoding utf8NoBOM
$records['shipping-A'].metrics[0].da_percent = 53
$records['shipping-B'].metrics[1].cda_percent = 63
foreach ($key in @('shipping-A','shipping-B')) { $records[$key] | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $testRoot "$key/results.json") -Encoding utf8NoBOM }
$failedDirectory = Join-Path $testRoot 'threshold'
Assert-Rejected { & "$PSScriptRoot/annual-compare-convergence.ps1" -InputsFile $inputFile -Cli $mockCli -OutputDirectory $failedDirectory -ExpectedSensorCount 2 } 'failed 2 checks' 'wrapper rejects multiple metric failures'
$wrapper = Get-Content -LiteralPath (Join-Path $failedDirectory 'convergence.json') -Raw | ConvertFrom-Json
Assert-Test (!$wrapper.pass -and $wrapper.failures -eq 2 -and $wrapper.checks.Count -eq 20 -and $wrapper.partition_checks.Count -eq 8) 'failed wrapper retains every comparison and partition'
Assert-Rejected { & "$PSScriptRoot/annual-compare-convergence.ps1" -InputsFile $inputFile -Cli $mockCli -OutputDirectory $failedDirectory -ExpectedSensorCount 2 } 'new output' 'existing result directory preserved'
[ordered]@{schema_version='greenlens.script-regression.v0.3';passed=$true;checks=$checks.ToArray();count=$checks.Count;synthetic_only=$true;engine_executed=$false;scientific_acceptance=$false} | ConvertTo-Json -Depth 5
