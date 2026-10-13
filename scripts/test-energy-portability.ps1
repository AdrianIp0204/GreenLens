# Pure restoration regression; no engine or original project is touched.
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
. "$PSScriptRoot/energy-portability.ps1" -FunctionsOnly
$root=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/portability restore '+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root|Out-Null
$moves=[Collections.Generic.List[object]]::new()
foreach($name in @('project','job1','job2')){
 $original=Join-Path $root $name; $hidden=$original+'.hidden'
 New-Item -ItemType Directory -Path $hidden|Out-Null
 Set-Content -LiteralPath (Join-Path $hidden 'identity.txt') -Value $name
 $moves.Add(@{original=$original;hidden=$hidden})
}
New-Item -ItemType Directory -Path $moves[1].original|Out-Null
Set-Content -LiteralPath (Join-Path $moves[1].original 'new.txt') -Value 'external change must survive'
$failed=$false
try{Restore-EnergyVerificationPaths $moves}catch{$failed=$_.Exception.Message.Contains('restore conflicts')}
if(!$failed){throw 'Expected explicit conflict'}
foreach($index in @(0,2)){
 if(!(Test-Path -LiteralPath (Join-Path $moves[$index].original 'identity.txt')) -or (Test-Path -LiteralPath $moves[$index].hidden)){throw 'Independent evidence was not restored'}
}
if(!(Test-Path -LiteralPath (Join-Path $moves[1].hidden 'identity.txt')) -or !(Test-Path -LiteralPath (Join-Path $moves[1].original 'new.txt'))){throw 'Conflict overwrote new or retained evidence'}
[ordered]@{schema='greenlens.energy-portability-restore-test.v1';conflict_reported=$true;independent_paths_restored=$true;conflicted_evidence_preserved=$true}|ConvertTo-Json
