param([Parameter(Mandatory)][string]$FixtureDirectory,[string]$EngineRoot=(Join-Path $env:LOCALAPPDATA 'GreenLens/engines/energyplus-26.2.0/package/EnergyPlus-26.2.0-4bd7a1f26f-Windows-x86_64'),[switch]$Execute)
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
$OutputDirectory=[IO.Path]::GetFullPath($FixtureDirectory)
$declaration=Get-Content -LiteralPath (Join-Path $OutputDirectory 'fixture-declaration.json') -Raw|ConvertFrom-Json -AsHashtable
$dependency=Get-Content -LiteralPath (Join-Path $PSScriptRoot '../dependencies/energyplus.json') -Raw|ConvertFrom-Json -AsHashtable
foreach($name in $declaration.input_sha256.Keys){if((Get-FileHash -LiteralPath (Join-Path $OutputDirectory $name)).Hash.ToLowerInvariant() -cne $declaration.input_sha256[$name]){throw "Input differs: $name"}}
foreach($name in $dependency.critical_files.Keys){if((Get-FileHash -LiteralPath (Join-Path $EngineRoot $name)).Hash.ToLowerInvariant() -cne $dependency.critical_files[$name].sha256){throw "Engine differs: $name"}}
foreach($existing in @('execution-declaration.json','processes.json','eplusout.err','stdout.txt')){if(Test-Path -LiteralPath (Join-Path $OutputDirectory $existing)){throw 'Use a fresh generated fixture; attempts are immutable'}}
$arguments=@('--jobs','1','--weather','weather.epw','--output-directory','.','--output-prefix','eplus','model.epJSON')
$sources=[ordered]@{}
foreach($path in @($PSCommandPath,(Join-Path $PSScriptRoot 'energy-gate0-fixture.py'),(Join-Path $PSScriptRoot 'annual-paired-process.ps1'),(Join-Path $PSScriptRoot 'annual-paired-native-process.cs'))){$sources[[IO.Path]::GetFileName($path)]=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()}
$execution=[ordered]@{schema='greenlens.energy-gate0-execution.v1';declared_utc=[DateTime]::UtcNow.ToString('o');execute=[bool]$Execute;fixture_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'fixture-declaration.json')).Hash.ToLowerInvariant();dependency_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot '../dependencies/energyplus.json')).Hash.ToLowerInvariant();source_sha256=$sources;arguments=$arguments;limits=$declaration.native_limits;native_process_count=1}
$execution|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'execution-declaration.json') -Encoding utf8
if(!$Execute){$execution|ConvertTo-Json -Depth 12;return}
$records=[Collections.Generic.List[object]]::new()
. (Join-Path $PSScriptRoot 'annual-paired-process.ps1')
try{
 Invoke-PairedProcess -Executable (Join-Path $EngineRoot 'energyplus.exe') -Name energyplus -Arguments $arguments -Output 'stdout.txt' -TimeoutSeconds $declaration.native_limits.timeout_seconds -MemoryLimitBytes $declaration.native_limits.memory_bytes -ScratchLimitBytes $declaration.native_limits.scratch_bytes -FileLimitBytes $declaration.native_limits.file_bytes
 foreach($name in $declaration.input_sha256.Keys){if((Get-FileHash -LiteralPath (Join-Path $OutputDirectory $name)).Hash.ToLowerInvariant() -cne $declaration.input_sha256[$name]){throw "Input changed: $name"}}
 foreach($name in $dependency.critical_files.Keys){if((Get-FileHash -LiteralPath (Join-Path $EngineRoot $name)).Hash.ToLowerInvariant() -cne $dependency.critical_files[$name].sha256){throw "Engine changed: $name"}}
}catch{
 [ordered]@{schema='greenlens.energy-gate0-failure.v1';finished_utc=[DateTime]::UtcNow.ToString('o');message=$_.Exception.Message;execution_completed=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $OutputDirectory 'failure.json') -Encoding utf8
 throw
}
