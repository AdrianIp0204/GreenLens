#Requires -Version 7.2
# Seal completed, trusted local research transport before cache reuse.
# This is an after-execution integrity record, not an execution signature.
param(
 [Parameter(Mandatory)][string]$Directory,
 [Parameter(Mandatory)][ValidateSet('coefficient','solar')][string]$Kind
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-matched-evidence.ps1"
$destination=Join-Path $Directory 'transport-integrity.json'
if(Test-Path -LiteralPath $destination){throw 'Do not replace an existing transport integrity seal.'}
$processes=Get-Content -LiteralPath (Join-Path $Directory 'processes.json') -Raw|ConvertFrom-Json
if($processes.Count -ne 3 -or @($processes|Where-Object{$_.exit_code -ne 0 -or $_.timeout_killed_tree}).Count){throw 'Transport process sequence did not complete successfully'}
$names=Get-MatchedTransportNames $Kind
$hashes=[ordered]@{}
foreach($name in $names){$hashes[$name]=Get-MatchedHash (Join-Path $Directory $name)}
$record=[ordered]@{schema_version='greenlens.research-transport-integrity.v1';kind=$Kind;sealed_utc=[DateTime]::UtcNow.ToString('o');captured_after_execution=$true;files=$hashes}
$text=($record|ConvertTo-Json -Depth 5).Replace("`r`n","`n")+"`n"
[IO.File]::WriteAllText($destination,$text,[Text.UTF8Encoding]::new($false))
