#Requires -Version 7.5
param([Parameter(Mandatory)][string[]]$CaseDirectories,[Parameter(Mandatory)][string]$BaselineDirectory,[Parameter(Mandatory)][string]$EndpointDirectory,[Parameter(Mandatory)][string]$TrustedSourceRoot,[Parameter(Mandatory)][string]$OutputFile)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/permutation-helpers.ps1"
if(Test-Path -LiteralPath $OutputFile){throw 'Fresh immutable aggregate output required.'}
if($CaseDirectories.Count-ne4){throw 'Exactly four diagnostic directories required.'}
$records=@{}
foreach($directory in $CaseDirectories){
 $declaration=Read-PermutationJson (Join-Path $directory 'declaration.json')
 $id=$declaration.recipe.case.id;$null=Get-PermutationCase $id;if($records.ContainsKey($id)){throw 'Duplicate diagnostic case.'}
 $record=& "$PSScriptRoot/run-permutation-case.ps1" -Mode Read -Case $id -BaselineDirectory $BaselineDirectory -EndpointDirectory $EndpointDirectory -OutputDirectory $directory -TrustedSourceRoot $TrustedSourceRoot
 if(!$record.completed){throw 'Incomplete diagnostic case.'};$records[$id]=$record
}
$checks=Get-PermutationComparisons $records
$failed=@($checks|Where-Object{-not$_.check.passed})
$result=[ordered]@{schema_version='greenlens.permutation-investigation-collection.v1';recorded_utc=[DateTime]::UtcNow.ToString('o');scope='Four fixed A4116 streams, both declared seeds; no averaging, seed selection, candidate comparison or reference-family qualification';check_counts=@{ladder=192;halves=96;depth=48;cross_seed=48;total=384};cases=@(Get-PermutationCases|ForEach-Object{$records[$_.id]});checks=$checks;failure_count=$failed.Count;failed_checks=$failed;completed=$true;investigation_passed=($failed.Count-eq0);reference_family_qualified=$false;release_accepted=$false}
[IO.File]::WriteAllText($OutputFile,($result|ConvertTo-Json -Depth 100)+"`n",[Text.UTF8Encoding]::new($false))
if($failed.Count-ne0){throw 'Fixed permutation investigation failed; every384 comparison retained, no reference qualification.'}
$result
