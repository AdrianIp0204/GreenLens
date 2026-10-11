#Requires -Version 7.5
# Synthetic command-orchestration regression. Does not execute Radiance/Rust.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$testRoot = Join-Path $env:LOCALAPPDATA ('GreenLens/script-tests/single annual synthetic ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
'Synthetic mock CLI tests only. No physical results or engine acceptance.' | Set-Content -LiteralPath (Join-Path $testRoot 'README.txt')
$mockCli = Join-Path $testRoot 'mock-cli.ps1'
$configPath = Join-Path $testRoot 'config.json'
$configuration = @{repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'));scenario='pass'}
@'
$ErrorActionPreference = 'Stop'
$config = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'config.json') -Raw | ConvertFrom-Json
$dataset = (Get-Content -LiteralPath (Join-Path $config.repository 'dependencies/weather.json') -Raw | ConvertFrom-Json).datasets[0]
$binding = @{original_sha256=$dataset.original_sha256;normalized_sha256=$dataset.normalized_sha256;time_axis_sha256=$dataset.time_axis_sha256;interpretation=$dataset.interpretation}
($args | ConvertTo-Json -Compress) | Add-Content -LiteralPath (Join-Path $PSScriptRoot 'commands.jsonl')
switch ($args[0]) {
    '--version' { 'synthetic-script-mock-only'; exit 0 }
    'example' {
        New-Item -ItemType Directory -Path $args[1] | Out-Null
        @{alternatives=@(@{id='A';model=@{sensors=@{requested_spacing_m=0.5}}},@{id='B';model=@{sensors=@{requested_spacing_m=0.5}}})} | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $args[1] 'project.json')
        '{}'; exit 0
    }
    'weather' {
        if ($args[1] -eq 'import') {
            New-Item -ItemType Directory -Path $args[3] | Out-Null
            @{binding=$binding} | ConvertTo-Json -Depth 10; exit 0
        }
        if ($args[1] -eq 'attach') { '{}'; exit 0 }
        throw 'Unexpected weather command'
    }
    'study' {
        $project = $args[2]
        if ($args[1] -eq 'configure') {
            Copy-Item -LiteralPath $args[3] -Destination (Join-Path $project 'study.json')
            '{}'; exit 0
        }
        if ($args[1] -ne 'run') { throw 'Unexpected study command' }
        if ($config.scenario -eq 'missing-engine') { [Console]::Error.WriteLine('Missing required engine (synthetic rejection).'); exit 3 }
        $study = Get-Content -LiteralPath (Join-Path $project 'study.json') -Raw | ConvertFrom-Json
        $document = Get-Content -LiteralPath (Join-Path $project 'project.json') -Raw | ConvertFrom-Json
        if (@($document.alternatives | Where-Object { $_.model.sensors.requested_spacing_m -ne 1.0 }).Count) { throw 'Grid differs from documented1.0m setup' }
        if ($study.calculation.mode -ne 'annual' -or $study.metrics.occupancy.start_minute -ne 480 -or $study.metrics.occupancy.end_minute -ne 1080) { throw 'Study differs from declared annual example' }
        $record = @{id='00000000-0000-4000-8000-000000000123';alternative_id=$args[4];study=$study;model=@{synthetic_test_only=$true};series=@{interval_count=$(if($config.scenario -eq 'truncated'){8759}else{8760});sensor_ids=@(0..23 | ForEach-Object {"test-$_"})};metrics=@(0..23 | ForEach-Object {@{da_percent=50;cda_percent=60;udi_percent=50;underlit_percent=25;overlit_percent=25}});provenance=@{source_commit=('a'*40);source_worktree_dirty=$false;engine=@{synthetic_test_only=$true};profile=@{name=$study.calculation.profile};physical_input_hash=('b'*64);physical_result_fingerprint=('c'*64)}}
        $record | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $project 'results.json')
        @{state='completed';climate_result=$record} | ConvertTo-Json -Depth 100; exit 0
    }
    'export' {
        if ($args.Count -ne 5 -or $args[3] -ne '--run') { throw 'Expected one explicit selected run' }
        $record = Get-Content -LiteralPath (Join-Path $args[1] 'results.json') -Raw | ConvertFrom-Json
        if ($config.scenario -eq 'changed-export') { $record.model.synthetic_test_only=$false }
        $zip = [IO.Compression.ZipFile]::Open($args[2],[IO.Compression.ZipArchiveMode]::Create)
        try {
            foreach ($item in @(@{name="runs/$($args[4])/results.json";text=($record | ConvertTo-Json -Depth 100)},@{name="runs/$($args[4])/annual-resources.json";text='{"synthetic_test_only":true}'})) {
                $writer = [IO.StreamWriter]::new($zip.CreateEntry($item.name).Open(),[Text.UTF8Encoding]::new($false))
                try { $writer.Write($item.text) } finally { $writer.Dispose() }
            }
        } finally { $zip.Dispose() }
        'Synthetic mock export'; exit 0
    }
    default { throw 'Mock forbids unsupported commands, including replay.' }
}
'@ | Set-Content -LiteralPath $mockCli -Encoding utf8NoBOM
$checks = [Collections.Generic.List[string]]::new()
foreach ($scenario in @('pass','missing-engine','truncated','changed-export')) {
    $configuration.scenario = $scenario
    $configuration | ConvertTo-Json | Set-Content -LiteralPath $configPath -Encoding utf8NoBOM
    $destination = Join-Path $testRoot $scenario
    $caught = $null
    try { & "$PSScriptRoot/annual-single-run.ps1" -Alternative B -Profile reference -Cli $mockCli -OutputDirectory $destination | Out-Null } catch { $caught=$_.Exception.Message }
    $summary = Get-Content -LiteralPath (Join-Path $destination 'summary.json') -Raw | ConvertFrom-Json
    if ($scenario -eq 'pass') {
        if ($caught -or !$summary.pass -or !$summary.full_evidence_validated -or $summary.intervals -ne 8760 -or $summary.sensors -ne 24 -or $summary.alternative -ne 'B' -or $summary.profile -ne 'reference' -or $summary.convergence_selection.project -ne 'project' -or $summary.accuracy_acceptance -or $summary.replay_executed) { throw "Synthetic successful orchestration failed: $caught" }
        $checks.Add('one explicit B/reference full-year request,24 sensors, validated export and relative comparator selection')
    } else {
        $expected = @{ 'missing-engine'='exit 3';truncated='exactly 8760';'changed-export'='completed versus validated' }[$scenario]
        if (!$caught -or $caught -notmatch $expected -or $summary.pass -or $summary.full_evidence_validated) { throw "Synthetic $scenario did not fail safely: $caught" }
        $checks.Add("$scenario failure retained without a successful evidence claim")
    }
}
$commands = @(Get-Content -LiteralPath (Join-Path $testRoot 'commands.jsonl') | ForEach-Object { ,($_ | ConvertFrom-Json) })
$runCalls = @($commands | Where-Object { $_[0] -eq 'study' -and $_[1] -eq 'run' })
if ($runCalls.Count -ne 4 -or @($runCalls | Where-Object { $_[4] -ne 'B' }).Count) { throw 'Single-run script issued extra or wrong-alternative calculations.' }
$checks.Add('exactly one study run per invocation and no replay command')
$before = (Get-FileHash -LiteralPath (Join-Path $testRoot 'pass/summary.json')).Hash
$caught=$null
try { & "$PSScriptRoot/annual-single-run.ps1" -Alternative A -Profile shipping -Cli $mockCli -OutputDirectory (Join-Path $testRoot 'pass') | Out-Null } catch { $caught=$_.Exception.Message }
if (!$caught -or $caught -notmatch 'new annual' -or (Get-FileHash -LiteralPath (Join-Path $testRoot 'pass/summary.json')).Hash -ne $before) { throw 'Existing annual output was not preserved.' }
$checks.Add('existing result directory rejected and bytes preserved')
[ordered]@{schema_version='greenlens.script-regression.v0.3';passed=$true;count=$checks.Count;checks=$checks.ToArray();synthetic_only=$true;engine_executed=$false;scientific_acceptance=$false} | ConvertTo-Json -Depth 5
