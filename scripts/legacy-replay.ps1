#Requires -Version 7.2
# Required legacy compatibility: no annual executable may be needed for replay.
param([string]$Cli='',[string]$EngineDirectory='',[string]$OutputDirectory='',[string]$StaticEngineDirectory='')
$ErrorActionPreference='Stop'
if(!$Cli){. "$PSScriptRoot/dev-env.ps1" -SkipVisualStudio;$Cli=Join-Path $env:CARGO_TARGET_DIR 'debug/greenlens.exe'}
. "$PSScriptRoot/legacy-engine-fixture.ps1"
if(!$EngineDirectory){$EngineDirectory=Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'}
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/Legacy replay 空間 '+[guid]::NewGuid().ToString('N'))}
if(!$StaticEngineDirectory){$StaticEngineDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/engines/verification/radiance-6.0.2-static-only-'+[guid]::NewGuid().ToString('N'))}
if(!(Test-Path -LiteralPath $Cli -PathType Leaf)){throw 'Build the native CLI before required legacy replay.'}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Choose a new verification directory; previous evidence is preserved.'}
$fixture=Get-Content -LiteralPath (Join-Path $PSScriptRoot '../tests/fixtures/legacy-static-v0.1.json') -Raw|ConvertFrom-Json
$package=Join-Path $PSScriptRoot ('../tests/fixtures/'+$fixture.file)
$before=(Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant()
if($before -ne $fixture.sha256 -or (Get-Item -LiteralPath $package).Length -ne $fixture.bytes){throw 'Historical fixture differs from its immutable release digest.'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$engine=[IO.Path]::GetFullPath($StaticEngineDirectory)
$engineFixture=New-LegacyEngineFixture -Source $EngineDirectory -Destination $engine
$engineFixture|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'engine-fixture.json') -Encoding utf8NoBOM
# Replay discovers and verifies these engine bytes against its pinned identity.
# Keep the executable stable while other work builds the shared Cargo target.
$frozenCli=Join-Path $OutputDirectory 'greenlens.exe'
Copy-Item -LiteralPath $Cli -Destination $frozenCli
$started=[DateTime]::UtcNow
$watch=[Diagnostics.Stopwatch]::StartNew()
& $frozenCli replay $package --engine-dir $engine 2> (Join-Path $OutputDirectory 'replay.stderr.log') | Out-File -LiteralPath (Join-Path $OutputDirectory 'replay.json') -Encoding utf8
$exitCode=$LASTEXITCODE
$watch.Stop()
$report=if($exitCode -eq 0){Get-Content -LiteralPath (Join-Path $OutputDirectory 'replay.json') -Raw|ConvertFrom-Json}else{$null}
$unchanged=(Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant() -eq $before
$result=[ordered]@{
    schema_version='greenlens.legacy-replay-verification.v1';started_utc=$started.ToString('o');finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$watch.Elapsed.TotalSeconds
    cli_sha256=(Get-FileHash -LiteralPath $frozenCli).Hash.ToLowerInvariant();fixture_sha256=$before;fixture_unchanged=$unchanged
    engine_executables=$engineFixture.engine_executables;engine_fixture=$engineFixture
    annual_executables_present=$engineFixture.annual_executables_present;exit_code=$exitCode;replay=$report
    scope='Native same-engine replay of original v0.1 evidence using only legacy executables; not annual accuracy or installed-desktop acceptance'
}
$result|ConvertTo-Json -Depth 30|Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8NoBOM
if($exitCode -ne 0){throw 'Required legacy replay failed; diagnostics retained.'}
if(!$unchanged){throw 'Historical evidence was modified.'}
if(!$report.passed -or @($report.checks).Count -ne $fixture.expected_runs){throw 'Legacy replay did not verify every retained run.'}
foreach($check in $report.checks){if(!$check.passed -or $check.sensors -ne $fixture.expected_sensors_per_run){throw 'Legacy replay shape or tolerance failed.'}}
Write-Output $OutputDirectory
$global:LASTEXITCODE=0
