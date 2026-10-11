#Requires -Version 7.2
param(
    [Parameter(Mandatory)][string[]]$EvidenceDirectories,
    [Parameter(Mandatory)][string]$OutputFile
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-matched-evidence.ps1"
if(Test-Path -LiteralPath $OutputFile){throw 'Use a new aggregate file to preserve prior evidence.'}
$result=Join-MatchedEvidence $EvidenceDirectories
$result|ConvertTo-Json -Depth 25|Set-Content -LiteralPath $OutputFile -Encoding utf8
if(!$result.pass){throw 'Required matched-sky tolerance failed; all declared intervals and failures are retained in the aggregate.'}
