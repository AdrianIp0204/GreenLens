#Requires -Version 7.5
param([Parameter(Mandatory)][string[]]$CaseDirectories,[Parameter(Mandatory)][string]$BaselineDirectory,[Parameter(Mandatory)][string]$EndpointDirectory,[Parameter(Mandatory)][string]$TrustedSourceRoot,[Parameter(Mandatory)][string]$OutputFile)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/depth-order-helpers.ps1"
if(Test-Path -LiteralPath $OutputFile){throw 'Fresh immutable aggregate output required.'}
if($CaseDirectories.Count-ne4){throw 'Exactly four diagnostic directories required.'}
$records=@{}
foreach($directory in $CaseDirectories){
 $declaration=Get-Content (Join-Path $directory 'declaration.json') -Raw|ConvertFrom-Json -Depth 100 -DateKind String
 $id=$declaration.recipe.case.id;$null=Get-DepthCase $id;if($records.ContainsKey($id)){throw 'Duplicate diagnostic case.'}
 $record=& "$PSScriptRoot/run-depth-order-case.ps1" -Mode Read -Case $id -BaselineDirectory $BaselineDirectory -EndpointDirectory $EndpointDirectory -OutputDirectory $directory -TrustedSourceRoot $TrustedSourceRoot
 if(!$record.completed){throw 'Incomplete diagnostic case.'};$records[$id]=$record
}
$endpoints=& {param($source,$base,$end). (Join-Path $source 'scripts/annual-collect-reference.ps1') -FunctionsOnly;Initialize-ReferenceCollector;$b=Read-ReferenceBaseline $base;$e=Read-ReferenceSensitivity $end $b.contexts[0];@{baseline=$b;endpoint=$e}} $TrustedSourceRoot $BaselineDirectory $EndpointDirectory
$effects=Get-DepthEffects $endpoints.baseline.cases[0].reference $records['ab10-original'].mean $records['lr24-original'].mean $endpoints.endpoint.variants[0].mean $records['baseline-reversed'].mean $records['depth-reversed'].mean
$result=[ordered]@{schema_version='greenlens.depth-order-collection.v1';recorded_utc=[DateTime]::UtcNow.ToString('o');scope='All four fixed diagnostic streams re-read from raw bins; descriptive estimator effects only';original_baseline=@{declaration_sha256=$endpoints.baseline.declaration_sha256;report_sha256=$endpoints.baseline.report_sha256;processes_sha256=$endpoints.baseline.processes_sha256};failed_depth_endpoint=@{declaration_sha256=$endpoints.endpoint.declaration_sha256;report_sha256=$endpoints.endpoint.report_sha256;processes_sha256=$endpoints.endpoint.processes_sha256};cases=@(Get-DepthOrderCases|ForEach-Object{$records[$_.id]});effects=$effects;completed=$true;reference_family_qualified=$false;release_accepted=$false}
[IO.File]::WriteAllText($OutputFile,($result|ConvertTo-Json -Depth 100)+"`n",[Text.UTF8Encoding]::new($false))
$result
