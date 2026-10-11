# No engine, network or repository mutation. Every source fixture is isolated.
$ErrorActionPreference='Stop'
. "$PSScriptRoot/prepare-reference-sources.ps1"
$root=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/pilot-source-tests-'+[Guid]::NewGuid().ToString('N'))
$names=@('scripts/annual-controlled-reference.ps1','scripts/annual-reference-process.ps1','scripts/annual-reference-aggregation.cs','scripts/annual-matched-corrected.ps1','scripts/annual-feasibility.ps1','scripts/annual-matched-evidence.ps1','scripts/verify-matrix-multiplication.ps1','docs/v0.3-reference-protocol.md','dependencies/radiance.json')
$utf8=[Text.UTF8Encoding]::new($false);$checks=0
function Fixture([string]$Label,[string]$Content,[string]$Expected){
 $dir=Join-Path $root $Label;$pins=[ordered]@{}
 foreach($name in $names){$path=Join-Path $dir $name;$null=New-Item -ItemType Directory -Path (Split-Path $path -Parent) -Force;[IO.File]::WriteAllText($path,$Content,$utf8);$pins[$name]=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($utf8.GetBytes($Expected))).ToLowerInvariant()}
 return @{root=$dir;pins=($pins|ConvertTo-Json|ConvertFrom-Json)}
}
function Reject($f){$rejected=$false;try{$null=Prepare-ReferenceSources $f.root $f.pins}catch{$rejected=$true};if(!$rejected){throw 'Expected rejection'};$script:checks++}
foreach($case in @(@('unchanged-lf',"a`nb`n","a`nb`n",$false),@('unchanged-crlf',"a`r`nb`r`n","a`r`nb`r`n",$false),@('to-lf',"a`r`nb`r`n","a`nb`n",$true),@('to-crlf',"a`nb`n","a`r`nb`r`n",$true))){$f=Fixture $case[0] $case[1] $case[2];$r=Prepare-ReferenceSources $f.root $f.pins;if(@($r.files|Where-Object {$_.changed -ne $case[3]}).Count -or $r.files.Count -ne 9){throw 'Incorrect preparation receipt'};foreach($name in $names){if([IO.File]::ReadAllText((Join-Path $f.root $name)) -cne $case[2]){throw 'Prepared bytes differ'}};$checks++}
$f=Fixture 'changed-code' "changed`n" "expected`n";Reject $f
$f=Fixture 'lone-cr' "a`rb`n" "a`nb`n";Reject $f
$f=Fixture 'invalid-utf8' "a`n" "b`n";[IO.File]::WriteAllBytes((Join-Path $f.root $names[0]),[byte[]]@(255,254,255));Reject $f
$f=Fixture 'unknown-path' "a`n" "a`n";$f.pins|Add-Member extra ('0'*64);Reject $f
$f=Fixture 'missing-pin' "a`n" "a`n";$f.pins.PSObject.Properties.Remove($names[0]);Reject $f
$f=Fixture 'no-partial-rewrite' "a`n" "a`r`n";[IO.File]::WriteAllText((Join-Path $f.root $names[-1]),"different`n",$utf8);Reject $f;if([IO.File]::ReadAllText((Join-Path $f.root $names[0])) -cne "a`n"){throw 'Earlier source changed despite later rejection'};$checks++
$f=Fixture 'data-untouched' "a`n" "a`r`n";$data=Join-Path $f.root 'numeric.mtx';[IO.File]::WriteAllText($data,"1 2 3`n",$utf8);$null=Prepare-ReferenceSources $f.root $f.pins;if([IO.File]::ReadAllText($data) -cne "1 2 3`n"){throw 'Unlisted data changed'};$checks++
@{checks=$checks;passed=$true;engine_executed=$false;repository_modified=$false;fixture_directory=$root}|ConvertTo-Json
