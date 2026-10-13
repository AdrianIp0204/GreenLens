param([string]$OutputDirectory)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$source=Join-Path $PSScriptRoot 'energy-qualification.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($source,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Launcher parse error'}
$functions=@($ast.FindAll({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq 'Assert-EnergySameValue'},$true))
if($functions.Count -ne 1){throw 'Require the single owned preflight function'}
. ([scriptblock]::Create($functions[0].Extent.Text))
$snapshot=Join-Path $PSScriptRoot '../examples/energy/revision1-inputs.json'
$historical=Get-Content -LiteralPath $snapshot -Raw|ConvertFrom-Json -AsHashtable
$checks=[Collections.Generic.List[string]]::new()
function Reject([scriptblock]$Body,[string]$Name){$failed=$false;try{& $Body}catch{$failed=$true};if(!$failed){throw "Mutation accepted: $Name"};$checks.Add($Name)}
foreach($alternative in @('A','B')){
 $old=$historical.cases[$alternative].model
 $expected=$old|ConvertTo-Json -Depth 100|ConvertFrom-Json -AsHashtable
 foreach($zone in $expected.zones){$zone.cooling_system.cooling_deadband_k=0.0}
 $actual=$expected|ConvertTo-Json -Depth 100|ConvertFrom-Json -AsHashtable
 Assert-EnergySameValue $actual $expected 'model';$checks.Add("$alternative exact revised model")
 Reject {Assert-EnergySameValue $old $expected 'model'} "$alternative unchanged cutout"
 $actual.zones[0].cooling_system.gross_rated_cop+=.01
 Reject {Assert-EnergySameValue $actual $expected 'model'} "$alternative extra physical edit"
}
Reject {Assert-EnergySameValue @{x=0;y=1} @{x=0} 'extra'} 'extra object key'
Reject {Assert-EnergySameValue @{x=0} @{x=0;y=1} 'missing'} 'missing object key'
Reject {Assert-EnergySameValue @(1,2) @(1) 'length'} 'array length'
Reject {Assert-EnergySameValue 'A' 'a' 'string'} 'case-sensitive strings'
Reject {Assert-EnergySameValue '0' 0 'typed'} 'numeric string'
Reject {Assert-EnergySameValue $true 1 'typed'} 'numeric Boolean'
Reject {Assert-EnergySameValue $null 0 'typed'} 'null value'
Reject {Assert-EnergySameValue ([double]::NaN) 0 'finite'} 'nonfinite number'
$result=[ordered]@{schema='greenlens.energy-input-preflight-guards.v1';native_processes=0;source_sha256=(Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant();test_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();snapshot_sha256=(Get-FileHash -LiteralPath $snapshot).Hash.ToLowerInvariant();passed=$checks.Count;checks=@($checks.ToArray())}
if($OutputDirectory){if(Test-Path -LiteralPath $OutputDirectory){throw 'Fresh test directory required'};New-Item -ItemType Directory -Path $OutputDirectory|Out-Null;$result|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'tests.json') -Encoding utf8}
$result|ConvertTo-Json -Depth 10
