#Requires -Version 7.5
# Synthetic wrapper contract regression only; no Rust or Radiance execution.
[CmdletBinding()]
param()
$ErrorActionPreference='Stop'
$testRoot=Join-Path $env:LOCALAPPDATA ('GreenLens/script-tests/annual replay synthetic ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
'Synthetic mock CLI and metadata only. No engine, physical accuracy or complete Rust evidence validation is claimed.' | Set-Content (Join-Path $testRoot 'README.txt')
$mockCli=Join-Path $testRoot 'mock-cli.ps1'
@'
$ErrorActionPreference='Stop'
$config=Get-Content (Join-Path $PSScriptRoot 'config.json') -Raw | ConvertFrom-Json
($args | ConvertTo-Json -Compress) | Add-Content (Join-Path $PSScriptRoot 'commands.jsonl')
if ($args[0] -eq '--version') { 'synthetic-replay-mock-only'; exit 0 }
if ($args[0] -ne 'replay' -or $args.Count -ne 4 -or $args[2] -ne '--engine-dir') { throw 'Mock rejects any invocation except explicit package replay and engine path.' }
if ($config.scenario -eq 'missing-engine') { [Console]::Error.WriteLine('Synthetic missing required engine'); exit 3 }
if ($config.scenario -eq 'truncated-json') { '{"passed":'; exit 0 }
$checks=@(foreach ($run in $config.runs) {
    @{run_id=$run.id;passed=$true;intervals=8760;sensors=24;values_checked=210240;max_absolute_error_lux=0;max_tolerance_ratio=0;tolerance='max(0.5 lx, 0.5% of reference)';resources=@{schema_version='greenlens.annual-resources.v1';measurement_policy='synthetic-wrapper-test-only';total_wall_ms=1;peak_engine_working_set_bytes=10;peak_observed_runner_plus_engine_working_set_bytes=20;peak_scratch_bytes=30;max_scratch_bytes=2147483648;max_matrix_bytes=536870912;max_observed_working_set_bytes=2147483648;processes=@(1..5 | ForEach-Object {@{synthetic_test_only=$true}})}}
})
switch ($config.scenario) {
    'partial-year' { $checks[0].intervals=8759 }
    'partial-values' { $checks[0].values_checked=210239 }
    'duplicate-check' { $checks=@($checks[0],$checks[0]) }
    'unknown-check' { $checks[0].run_id='00000000-0000-4000-8000-000000000999' }
    'failed-check' { $checks[0].passed=$false }
    'over-tolerance' { $checks[0].max_tolerance_ratio=1.000001 }
    'on-tolerance' { $checks[0].max_tolerance_ratio=1; $checks[0].max_absolute_error_lux=0.5 }
    'changed-tolerance' { $checks[0].tolerance='max(5 lx, 5% of reference)' }
    'nonfinite-error' { $checks[0].max_absolute_error_lux='NaN' }
    'missing-telemetry' { $checks[0].Remove('resources') }
    'resource-exceeded' { $checks[0].resources.peak_scratch_bytes=2147483649 }
    'inflated-caps' { $checks[0].resources.max_scratch_bytes=4294967296 }
    'changed-package' { [IO.File]::AppendAllText($args[1],'changed') }
    'changed-cli' { [IO.File]::AppendAllText($PSCommandPath,"`n# changed") }
}
@{passed=$true;checks=$checks} | ConvertTo-Json -Depth 30
exit 0
'@ | Set-Content -LiteralPath $mockCli -Encoding utf8NoBOM
function New-MockPackage([string]$Path,[object[]]$Runs,[string]$Scenario) {
    $zip=[IO.Compression.ZipFile]::Open($Path,[IO.Compression.ZipArchiveMode]::Create)
    try {
        $summaries=@($Runs | ForEach-Object {@{id=$_.id;alternative_id=$_.alternative;path="runs/$($_.id)"}})
        $project=@{runs=@();climate=@{runs=$summaries}}
        if ($Scenario -eq 'mixed-static') { $project.runs=@(@{id='static'}) }
        $entries=@(@{name='project.json';value=$project})
        foreach ($run in $Runs) {
            $profile=if($Scenario -eq 'wrong-profile'){'reference'}else{'shipping'}
            $entries+=@{name="runs/$($run.id)/results.json";value=@{id=$run.id;alternative_id=$run.alternative;study=@{calculation=@{mode='annual';profile=$profile;weather=@{synthetic=$true}}};series=@{interval_count=8760;sensor_ids=@(0..23 | ForEach-Object {"s$_"})};provenance=@{profile=@{name=$profile};source_commit=('a'*40);source_worktree_dirty=$false;engine=@{synthetic=$true};physical_result_fingerprint=('b'*64)}}}
        }
        if ($Scenario -eq 'unsafe-path') { $entries+=@{name='../outside.json';value=@{}} }
        foreach ($entry in $entries) {
            $writer=[IO.StreamWriter]::new($zip.CreateEntry($entry.name).Open(),[Text.UTF8Encoding]::new($false))
            try { $writer.Write(($entry.value | ConvertTo-Json -Depth 30)) } finally { $writer.Dispose() }
        }
    } finally { $zip.Dispose() }
}
$a=@{id='00000000-0000-4000-8000-000000000001';alternative='A'}
$b=@{id='00000000-0000-4000-8000-000000000002';alternative='B'}
$cases=@(
    @{name='single-pass';runs=@($a)},@{name='pair-pass';runs=@($a,$b)},@{name='on-tolerance';runs=@($a)},
    @{name='missing-engine';error='exit 3'},@{name='partial-year';error='truncated'},@{name='partial-values';error='truncated'},
    @{name='duplicate-check';runs=@($a,$b);error='duplicate'},@{name='unknown-check';error='unknown'},@{name='failed-check';error='failed'},
    @{name='over-tolerance';error='tolerance'},@{name='changed-tolerance';error='tolerance'},@{name='nonfinite-error';error='finite'},
    @{name='missing-telemetry';error='telemetry'},@{name='resource-exceeded';error='resource limit'},@{name='inflated-caps';error='resource caps'},
    @{name='wrong-profile';error='profile/shape';preflight=$true},@{name='mixed-static';error='mixed static';preflight=$true},
    @{name='unsafe-path';error='Unsafe';preflight=$true},@{name='truncated-json';error='JSON'},
    @{name='changed-package';error='bytes changed'},@{name='changed-cli';error='bytes changed'}
)
$checks=[Collections.Generic.List[string]]::new()
foreach ($case in $cases) {
    $runs=if($case.runs){$case.runs}else{@($a)}
    @{scenario=$case.name;runs=@($runs)} | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $testRoot 'config.json') -Encoding utf8NoBOM
    $archive=Join-Path $testRoot "$($case.name).zip"
    New-MockPackage $archive @($runs) $case.name
    $destination=Join-Path $testRoot $case.name
    $caught=$null
    try { & "$PSScriptRoot/annual-replay.ps1" -Package $archive -Cli $mockCli -OutputDirectory $destination -ExpectedAlternatives @($runs | ForEach-Object alternative) -EngineDirectory (Join-Path $testRoot 'synthetic engine 空間') | Out-Null } catch { $caught=$_.Exception.Message }
    $summary=Get-Content (Join-Path $destination 'summary.json') -Raw | ConvertFrom-Json -DateKind String -Depth 100
    if ($case.error) {
        if (!$caught -or $caught -notmatch $case.error -or $summary.pass) { throw "Synthetic $($case.name) did not fail safely: $caught" }
        if ($case.preflight -and $summary.replay_started) { throw 'Unsafe/wrong selection invoked the CLI before rejecting.' }
    } elseif ($caught -or !$summary.pass -or $summary.checks.Count -ne @($runs).Count -or !$summary.package_unchanged -or !$summary.cli_unchanged -or $summary.scientific_accuracy_acceptance -or $summary.native_desktop_acceptance) { throw "Synthetic $($case.name) orchestration failed: $caught" }
    $checks.Add($case.name)
}
$before=(Get-FileHash (Join-Path $testRoot 'single-pass/summary.json')).Hash
$caught=$null
try { & "$PSScriptRoot/annual-replay.ps1" -Package (Join-Path $testRoot 'single-pass.zip') -Cli $mockCli -OutputDirectory (Join-Path $testRoot 'single-pass') | Out-Null } catch { $caught=$_.Exception.Message }
if (!$caught -or $caught -notmatch 'new annual replay' -or (Get-FileHash (Join-Path $testRoot 'single-pass/summary.json')).Hash -cne $before) { throw 'Existing replay evidence was overwritten.' }
$checks.Add('existing-evidence-preserved')
[ordered]@{schema_version='greenlens.script-regression.v0.3';passed=$true;count=$checks.Count;checks=$checks.ToArray();synthetic_only=$true;engine_executed=$false;scientific_acceptance=$false} | ConvertTo-Json -Depth 5
