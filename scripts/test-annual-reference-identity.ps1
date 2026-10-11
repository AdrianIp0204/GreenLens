#Requires -Version 7.2
# Synthetic identity/coverage checks only; no native/scientific acceptance claim.
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-reference-identity.ps1"
$script:passed=0
function Assert-True([bool]$Condition,[string]$Message){if(!$Condition){throw $Message};$script:passed++}
function Reject([scriptblock]$Action,[string]$Pattern){
 $failure=$null;try{& $Action|Out-Null}catch{$failure=$_.Exception.Message}
 if(!$failure -or $failure -notmatch $Pattern){throw "Expected failure /$Pattern/, observed: $failure"};$script:passed++
}
function Clone($Value){ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject $Value -Depth 50) -AsHashtable -Depth 50}
function Declaration([string]$Alternative='A',[int[]]$Indices=@(1904)){
 $hashes=[ordered]@{}
 foreach($file in @('scene.rad','sensors.pts','identical-sky.mtx','suns.rad','weather.epw','source-recipe.json','model.json','study.json','weather-asset.json')){$hashes[$file]='a'*64}
 if($Alternative -eq 'B'){foreach($file in @('scene.rad','source-recipe.json','model.json')){$hashes[$file]='b'*64}}
 return [ordered]@{
  schema_version='greenlens.controlled-reference.v1';declared_utc='2026-10-11T00:00:00Z';alternative=$Alternative
  declared_interval_indices=(Get-ReferenceFixedIntervals);executed_interval_indices=$Indices
  normalized_model=@{room=@{width_m=4;depth_m=6};overhang=$(if($Alternative -eq 'B'){@{projection_m=.6}}else{$null})}
  input_sha256=$hashes;engine_sha256=@{'rcontrib.exe'=('c'*64)};weather_binding=@{original_sha256=('d'*64)}
  ambient_divisions=8192;ambient_bounces=9;ray_depth=-16;ambient_weight=1e-9;source_scale=100
  initial_rungs=@(64,128,256);full_arguments=@('-I+','-ab','9','-lr','-16');protocol_sha256=('e'*64)
  harness_sha256=('f'*64);aggregation_sha256=('1'*64);finite_kernel_sha256=('2'*64);process_helper_sha256=('3'*64)
  future_scientific_setting=@{enabled=$true;tuple=@(4,5)}
 }
}
$one=[ordered]@{z=@{b=2;a=1};a=@(3,@{x=$false;y=$null});empty=@();singleton=@(7)}
$two=[pscustomobject]@{singleton=@(7);empty=@();a=@(3,[pscustomobject]@{y=$null;x=$false});z=[pscustomobject]@{a=1;b=2}}
Assert-True ((ConvertTo-ReferenceCanonicalJson $one) -ceq (ConvertTo-ReferenceCanonicalJson $two)) 'Object key ordering changed semantic identity.'
Assert-True ((ConvertTo-ReferenceCanonicalJson @()) -ceq '[]') 'Empty arrays lost shape.'
Assert-True ((ConvertTo-ReferenceCanonicalJson @(7)) -ceq '[7]') 'Singleton arrays lost shape.'
Assert-True ((ConvertTo-ReferenceCanonicalJson @('a','b')) -cne (ConvertTo-ReferenceCanonicalJson @('b','a'))) 'Array order was discarded.'
$a=Declaration;$aShard=Clone $a;$aShard.declared_utc='2026-10-11T02:00:00Z';$aShard.executed_interval_indices=@(8512,1908)
$b=Declaration B @(4116,4112)
$identity=Assert-ReferenceShardIdentity @($a,$aShard,$b)
Assert-True ($identity.declarations.Count -eq 3 -and $identity.alternative_identities.Count -eq 2) 'Valid per-alternative and shard differences rejected.'
Assert-True ($identity.common_identity.Contains('future_scientific_setting')) 'Unknown scientific field was discarded.'
Assert-True ($identity.declarations[1].executed_interval_indices[0] -eq 8512) 'Returned declaration did not preserve execution order.'
$reordered=[ordered]@{};$keys=@($a.Keys);[array]::Reverse($keys);foreach($key in $keys){$reordered[$key]=$a[$key]}
Assert-True ((Assert-ReferenceShardIdentity @([pscustomobject]$a,[pscustomobject]$reordered)).declarations.Count -eq 2) 'PSCustomObject/key-only differences rejected.'
foreach($field in @('ambient_divisions','ambient_bounces','source_scale','ray_depth','ambient_weight')){
 $changed=Clone $b;$changed[$field]=123;Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'identity differs'
}
foreach($field in @('protocol_sha256','harness_sha256','aggregation_sha256','finite_kernel_sha256','process_helper_sha256')){
 $changed=Clone $b;$changed[$field]='4'*64;Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'identity differs'
}
$changed=Clone $b;$changed.engine_sha256['rcontrib.exe']='5'*64;Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'identity differs'
$changed=Clone $b;$changed.weather_binding.original_sha256='6'*64;Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'identity differs'
foreach($file in @('sensors.pts','identical-sky.mtx','suns.rad','weather.epw','study.json','weather-asset.json')){
 $changed=Clone $b;$changed.input_sha256[$file]='7'*64;Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'identity differs'
}
foreach($file in @('scene.rad','source-recipe.json','model.json')){
 $changed=Clone $aShard;$changed.input_sha256[$file]='8'*64;Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'within alternative'
}
$changed=Clone $aShard;$changed.normalized_model.room.width_m=5;Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'within alternative'
$changed=Clone $b;$changed.future_scientific_setting.tuple=@(5,4);Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'identity differs'
$changed=Clone $b;$null=$changed.Remove('future_scientific_setting');Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'identity differs'
$changed=Clone $b;$changed.initial_rungs=@(128,64,256);Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'identity differs'
$changed=Clone $b;$changed.full_arguments=@('-ab','9','-I+','-lr','-16');Reject {Assert-ReferenceShardIdentity @($a,$changed)} 'identity differs'
$changed=Clone $a;$changed.declared_interval_indices=@(8512,1904,1908,1912,4112,4116,4120,6320,6324,6328,8504,8508);Reject {Assert-ReferenceShardIdentity @($changed)} 'fixed twelve-case plan'
$changed=Clone $a;$changed.executed_interval_indices=@(1904,1904);Reject {Assert-ReferenceShardIdentity @($changed)} 'unique subset'
$changed=Clone $a;$changed.executed_interval_indices=@(0);Reject {Assert-ReferenceShardIdentity @($changed)} 'unique subset'
$changed=Clone $a;$changed.executed_interval_indices=@();Reject {Assert-ReferenceShardIdentity @($changed)} 'nonempty array'
$changed=Clone $a;$changed.executed_interval_indices=@('1904');Reject {Assert-ReferenceShardIdentity @($changed)} 'integer JSON number'
$changed=Clone $a;$changed.input_sha256['scene.rad']='broken';Reject {Assert-ReferenceShardIdentity @($changed)} 'malformed'
$changed=Clone $a;$changed.alternative='a';Reject {Assert-ReferenceShardIdentity @($changed)} 'exactly A or B'
Reject {Assert-ReferenceShardIdentity @()} 'At least one'
$cases=@(foreach($alternative in @('A','B')){foreach($index in (Get-ReferenceFixedIntervals)){[ordered]@{alternative=$alternative;interval_index=$index;qualified=$true}}})
$coverage=Test-ReferenceCaseCoverage $cases -RequireComplete
Assert-True ($coverage.complete -and $coverage.completed_case_count -eq 24 -and $coverage.missing_case_keys.Count -eq 0) 'Exact full coverage did not pass.'
$partial=Test-ReferenceCaseCoverage @($cases[0])
Assert-True (!$partial.complete -and $partial.missing_case_keys.Count -eq 23 -and $partial.missing_case_keys[0] -ceq 'A:1908') 'Partial snapshot missing keys are wrong.'
Assert-True (!(Test-ReferenceCaseCoverage @()).complete) 'Empty snapshot became complete.'
Reject {Test-ReferenceCaseCoverage @($cases[0]) -RequireComplete} 'coverage is incomplete'
Reject {Test-ReferenceCaseCoverage ($cases+@($cases[0])) -RequireComplete} 'Duplicate completed'
Reject {Test-ReferenceCaseCoverage @(@{alternative='A';interval_index=0})} 'outside the declared plan'
Reject {Test-ReferenceCaseCoverage @(@{alternative='C';interval_index=1904})} 'exactly A or B'
Reject {Test-ReferenceCaseCoverage @(@{alternative='A';interval_index=1904.5})} 'outside the supported year'
Reject {Test-ReferenceCaseCoverage @(@{alternative='A';interval_index='1904'})} 'integer JSON number'
$failed=Clone $cases;$failed[0].qualified=$false
Assert-True ((Test-ReferenceCaseCoverage $failed).complete) 'Coverage must not pretend to perform numerical qualification.'
Assert-True ($null -eq $coverage.PSObject.Properties['qualified']) 'Coverage helper must not claim numerical qualification.'
[ordered]@{schema_version='greenlens.reference-identity-tests.v1';synthetic_only=$true;checks_passed=$script:passed;passed=$true}|ConvertTo-Json
