#Requires -Version 7.5
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Cli,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new verification destination.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
# Use the real provider checks, including four expected nonzero child exits.
# The outer shell must observe success only after those rejections are verified.
$global:LASTEXITCODE = 57
$weatherOutput = Join-Path $OutputDirectory 'weather'
& "$PSScriptRoot/weather-verification.ps1" -Cli $Cli -OutputDirectory $weatherOutput
if ($LASTEXITCODE -ne 0) { throw "Successful weather verification leaked child exit $LASTEXITCODE." }
$report = Get-Content -LiteralPath (Join-Path $weatherOutput 'summary.json') -Raw | ConvertFrom-Json -DateKind String
if (!$report.passed -or $report.intervals -ne 8760 -or $report.checks.Count -ne 11) { throw 'Required real provider checks did not finish.' }
$missingCliRejected = $false
$missingOutput = Join-Path $OutputDirectory 'missing-cli'
try {
    & "$PSScriptRoot/weather-verification.ps1" -Cli (Join-Path $OutputDirectory 'absent-greenlens.exe') -OutputDirectory $missingOutput
} catch { $missingCliRejected = $true }
if (!$missingCliRejected -or (Test-Path -LiteralPath (Join-Path $missingOutput 'summary.json'))) { throw 'Unavailable real CLI was incorrectly accepted.' }
@{
    schema_version='greenlens.weather-exit-status-test.v1'
    passed=$true
    completed_utc=[DateTime]::UtcNow.ToString('o')
    cli_sha256=(Get-FileHash -LiteralPath $Cli).Hash.ToLowerInvariant()
    checks=@('real provider/rejection suite returns success', 'previous nonzero shell status cleared only on success', 'missing CLI still throws and creates no success record')
    weather_evidence='weather/summary.json'
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'exit-status.json') -Encoding utf8NoBOM
$global:LASTEXITCODE = 0
