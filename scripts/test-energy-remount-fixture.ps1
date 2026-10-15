# Pure admission/reuse/inventory guards: no CLI, engine or real project.
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
. "$PSScriptRoot/prepare-energy-remount-fixture.ps1" -FunctionsOnly
$checks=0
function Assert-Rejected([scriptblock]$Action){
 $failed=$false;try{&$Action|Out-Null}catch{$failed=$true}
 if(!$failed){throw 'Expected a rejected synthetic fixture'};$script:checks++
}
function Copy-Value($Value){return $Value|ConvertTo-Json -Depth 30|ConvertFrom-Json -AsHashtable -DateKind String}
$cases=@('A10','B10','A5','B5');$index=@();$ten=@{energy=@{runs=@()}};$five=@{energy=@{runs=@()}}
foreach($number in 1..4){
 $case=$cases[$number-1];$minutes=if($number -le 2){10}else{5};$alternative=$case.Substring(0,1);$id=('10000000-0000-4000-8000-{0:D12}' -f $number)
 $index+=@{case=$case;minutes=$minutes;project="project-$minutes";alternative=$alternative;id=$id}
 $run=@{id=$id;alternative_id=$alternative;path="runs/$id";physical_input_hash=([string]$number)*64;record_sha256=('a'*64)}
 if($number -le 2){$ten.energy.runs+=,$run}else{$five.energy.runs+=,$run}
}
$selected=Assert-RemountProductionIndex $index $ten $five
if(($selected.Keys -join ',') -cne 'A10,B10,A5,B5'){throw 'Valid index order changed'};$checks++
$reordered=@($index[1],$index[0],$index[2],$index[3]);Assert-Rejected {Assert-RemountProductionIndex $reordered $ten $five}
$bad=Copy-Value $index;$bad[2].id=$bad[0].id;Assert-Rejected {Assert-RemountProductionIndex $bad $ten $five}
$bad=Copy-Value $index;$bad[0].project='../outside';Assert-Rejected {Assert-RemountProductionIndex $bad $ten $five}
$bad=Copy-Value $index;$bad[0].minutes=5;Assert-Rejected {Assert-RemountProductionIndex $bad $ten $five}
$bad=Copy-Value $ten;$bad.energy.runs+=,$bad.energy.runs[0];Assert-Rejected {Assert-RemountProductionIndex $index $bad $five}
$bad=Copy-Value $five;$bad.energy.runs[0].path='runs/elsewhere';Assert-Rejected {Assert-RemountProductionIndex $index $ten $bad}
$plan=@{schema_version='greenlens.energy-sensitivity-plan.v1';execution_authorized=$false;slots=@();declaration=@{slots=@()}}
foreach($case in $cases){$run=$selected[$case];$plan.slots+=@{slot_id=$case;reuse=@{run_id=$run.id;record_sha256=$run.record_sha256}};$plan.declaration.slots+=@{id=$case;physical_input_sha256=$run.physical_input_hash;zone_timesteps_per_hour=$(if($case -cin @('A10','B10')){6}else{12})}}
Assert-RemountReusePlan $plan $selected;$checks++
$bad=Copy-Value $plan;$bad.slots[3].reuse=$null;Assert-Rejected {Assert-RemountReusePlan $bad $selected}
$bad=Copy-Value $plan;$bad.slots[1].reuse.run_id=$selected.A10.id;Assert-Rejected {Assert-RemountReusePlan $bad $selected}
$bad=Copy-Value $plan;$bad.slots[2].reuse.record_sha256='b'*64;Assert-Rejected {Assert-RemountReusePlan $bad $selected}
$bad=Copy-Value $plan;$bad.declaration.slots[0].physical_input_sha256='f'*64;Assert-Rejected {Assert-RemountReusePlan $bad $selected}
$bad=Copy-Value $plan;$bad.execution_authorized=$true;Assert-Rejected {Assert-RemountReusePlan $bad $selected}
$bad=Copy-Value $plan;$bad.slots=@($bad.slots[0],$bad.slots[1],$bad.slots[2]);Assert-Rejected {Assert-RemountReusePlan $bad $selected}
$bad=Copy-Value $plan;$bad.declaration.slots[3].zone_timesteps_per_hour=6;Assert-Rejected {Assert-RemountReusePlan $bad $selected}
foreach($minutes in @(10,5)){
 $summary=$selected["A$minutes"];$steps=60/$minutes;$intervals=8760*$steps
 $record=@{id=$summary.id;alternative_id='A';provenance=@{physical_input_hash=$summary.physical_input_hash};study=@{zone_timesteps_per_hour=$steps};series=@{interval_minutes=$minutes;interval_count=$intervals};totals=@{interval_count=$intervals;interval_seconds=60*$minutes}}
 Assert-RemountRunShape $record $summary $minutes;$checks++
 $bad=Copy-Value $record;$bad.series.interval_count--;Assert-Rejected {Assert-RemountRunShape $bad $summary $minutes}
 $bad=Copy-Value $record;$bad.study.zone_timesteps_per_hour=4;Assert-Rejected {Assert-RemountRunShape $bad $summary $minutes}
 $bad=Copy-Value $record;$bad.totals.interval_seconds=60;Assert-Rejected {Assert-RemountRunShape $bad $summary $minutes}
}
$root=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/remount-guard-'+[guid]::NewGuid().ToString('N'));New-Item -ItemType Directory -Path $root|Out-Null
$data=Join-Path $root 'evidence.bin';[IO.File]::WriteAllBytes($data,[byte[]]@(1,2,3));$before=Get-RemountInventory $root
Assert-RemountSameInventory $before (Get-RemountInventory $root);$checks++
[IO.File]::WriteAllBytes($data,[byte[]]@(1,2,4));Assert-Rejected {Assert-RemountSameInventory $before (Get-RemountInventory $root)}
[IO.File]::WriteAllBytes($data,[byte[]]@(1,2,3));[IO.File]::WriteAllText((Join-Path $root 'extra.txt'),'unexpected');Assert-Rejected {Assert-RemountSameInventory $before (Get-RemountInventory $root)}
# Sparse oversized file tests the length guard before any large hash/read.
$large=Join-Path $root 'large.bin';$stream=[IO.File]::Create($large);try{$stream.SetLength(64MB+1)}finally{$stream.Dispose()};Assert-Rejected {Get-RemountInventory $root}
# Exercise the real redirected native process/stream path, not a null command
# mock. This child only emits original synthetic JSON and a diagnostic line.
$OutputDirectory=Join-Path $root 'supervised child';New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$frozenCli=(Get-Process -Id $PID).Path;$cliHash=(Get-FileHash -LiteralPath $frozenCli).Hash.ToLowerInvariant();$commands=[Collections.Generic.List[object]]::new();$overall=[Diagnostics.Stopwatch]::StartNew()
$child=Join-Path $OutputDirectory 'original child fixture.ps1'
@'
[Console]::Error.WriteLine('synthetic child diagnostic')
[Console]::Out.WriteLine('{"kind":"original-synthetic-json","values":[1,2,3]}')
'@|Set-Content -LiteralPath $child -Encoding utf8NoBOM
$returned=@(Invoke-FixtureCli 'successful-child' @('-NoProfile','-File',$child))
@($returned|ForEach-Object {$_.GetType().FullName})|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $OutputDirectory 'returned-types.json') -Encoding utf8NoBOM
if($returned.Count -ne 1 -or $returned[0] -isnot [Collections.IDictionary]){throw 'The supervised child must return exactly one parsed JSON dictionary; async completion values must not leak'}
if($returned[0].kind -cne 'original-synthetic-json' -or ($returned[0].values -join ',') -cne '1,2,3'){throw 'Child JSON changed during supervision'};$checks++
if((Get-Content -LiteralPath (Join-Path $OutputDirectory 'successful-child.stderr.txt') -Raw).Trim() -cne 'synthetic child diagnostic' -or $commands.Count -ne 1 -or $commands[0].exit_code -ne 0){throw 'Child diagnostics/receipt were not retained'};$checks++
$failure=Join-Path $OutputDirectory 'failing child fixture.ps1'
"[Console]::Error.WriteLine('deliberate fixture exit 7'); exit 7"|Set-Content -LiteralPath $failure -Encoding utf8NoBOM
Assert-Rejected {Invoke-FixtureCli 'failed-child' @('-NoProfile','-File',$failure)}
if($commands.Count -ne 2 -or $commands[1].exit_code -ne 7 -or !$commands[1].failure){throw 'Native child failure was not recorded'};$checks++
[ordered]@{schema='greenlens.energy-remount-guards.v1';checks_passed=$checks;scope='Original synthetic guard fixtures only; no engine, CLI, annual arithmetic, geometry or native desktop acceptance.'}|ConvertTo-Json
