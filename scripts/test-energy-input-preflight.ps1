param([string]$OutputDirectory)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$source=Join-Path $PSScriptRoot 'energy-qualification.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($source,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Launcher parse error'}
foreach($name in @('Assert-EnergySameValue','Get-EnergyExpectedInput')){
 $functions=@($ast.FindAll({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq $name},$true))
 if($functions.Count -ne 1){throw "Require the single owned function: $name"}
 . ([scriptblock]::Create($functions[0].Extent.Text))
}
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
foreach($revision in @(2,3)){
 foreach($alternative in @('A','B')){
  foreach($minutes in @(10,5)){
   $expected=$historical.cases[$alternative]|ConvertTo-Json -Depth 100|ConvertFrom-Json -AsHashtable
   foreach($zone in $expected.model.zones){
    $zone.cooling_system.cooling_deadband_k=0.0
    if($revision -eq 3 -and $alternative -ceq 'B'){$zone.cooling_system.supply_fan.total_efficiency=.6}
   }
   $expected.study.zone_timesteps_per_hour=if($minutes -eq 10){6}else{12}
   $actual=Get-EnergyExpectedInput $historical $alternative $minutes $revision
   Assert-EnergySameValue $actual $expected 'complete expected input'
   $checks.Add("revision$revision $alternative$minutes full model and study")
   $actual.model.zones[0].cooling_system.supply_fan.total_efficiency=if($revision -eq 3 -and $alternative -ceq 'B'){.5}else{.6}
   Reject {Assert-EnergySameValue $actual $expected 'fan'} "revision$revision $alternative$minutes omitted or extra fan change"
   $actual=Get-EnergyExpectedInput $historical $alternative $minutes $revision
   $actual.model.zones[0].cooling_system.supply_fan.pressure_rise_pa+=1
   Reject {Assert-EnergySameValue $actual $expected 'pressure'} "revision$revision $alternative$minutes undeclared pressure change"
  }
 }
}
foreach($case in @(@('C',10,3),@('a',10,3),@('A',15,3),@('A',10,1),@('A',10,4))){
 Reject {Get-EnergyExpectedInput $historical $case[0] $case[1] $case[2]|Out-Null} "undeclared tuple $($case -join '/')"
}
foreach($mutation in @('cutout','fan','zone-count')){
 $changed=$historical|ConvertTo-Json -Depth 100|ConvertFrom-Json -AsHashtable
 if($mutation -eq 'cutout'){$changed.cases.B.model.zones[0].cooling_system.cooling_deadband_k=.3}
 elseif($mutation -eq 'fan'){$changed.cases.B.model.zones[0].cooling_system.supply_fan.total_efficiency=.6}
 else{$changed.cases.B.model.zones=@($changed.cases.B.model.zones[0..2])}
 Reject {Get-EnergyExpectedInput $changed 'B' 10 3|Out-Null} "changed historical $mutation"
}
$unchanged=Get-Content -LiteralPath $snapshot -Raw|ConvertFrom-Json -AsHashtable
Assert-EnergySameValue $historical $unchanged 'historical source unchanged';$checks.Add('all constructions preserve historical input')
$result=[ordered]@{schema='greenlens.energy-input-preflight-guards.v1';native_processes=0;source_sha256=(Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant();test_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();snapshot_sha256=(Get-FileHash -LiteralPath $snapshot).Hash.ToLowerInvariant();passed=$checks.Count;checks=@($checks.ToArray())}
if($OutputDirectory){if(Test-Path -LiteralPath $OutputDirectory){throw 'Fresh test directory required'};New-Item -ItemType Directory -Path $OutputDirectory|Out-Null;$result|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'tests.json') -Encoding utf8}
$result|ConvertTo-Json -Depth 10
