#Requires -Version 7.5
param([string]$OutputDirectory='')
# Every source below is an original synthetic fixture. No engine or real checkout is mutated.
$ErrorActionPreference='Stop'
. "$PSScriptRoot/prepare-sensitivity-sources.ps1"
$helperHash=(Get-FileHash -LiteralPath "$PSScriptRoot/prepare-sensitivity-sources.ps1").Hash.ToLowerInvariant()
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/sensitivity source guards '+[guid]::NewGuid().ToString('N')+' 空間')}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Fresh test directory required.'}
$names=@('scripts/annual-reference-sensitivity.ps1','scripts/annual-controlled-reference.ps1','scripts/annual-reference-process.ps1','scripts/annual-reference-aggregation.cs','scripts/annual-matched-corrected.ps1','scripts/annual-feasibility.ps1','scripts/annual-matched-evidence.ps1','scripts/verify-matrix-multiplication.ps1','scripts/annual-collect-reference.ps1','scripts/annual-reference-identity.ps1','scripts/annual-reference-recovery.ps1','docs/v0.3-reference-protocol.md','dependencies/radiance.json','examples/synthetic-a.json','examples/synthetic-b.json')
$utf8=[Text.UTF8Encoding]::new($false)
$checks=[Collections.Generic.List[object]]::new()
function Pass([string]$Name){$checks.Add([ordered]@{name=$Name;passed=$true})}
function Fixture([string]$Label,[string]$Content,[string]$Expected){
 $dir=Join-Path $OutputDirectory $Label;$pins=[ordered]@{}
 foreach($name in $names){$path=Join-Path $dir $name;$null=New-Item -ItemType Directory -Path (Split-Path $path -Parent) -Force;[IO.File]::WriteAllText($path,$Content,$utf8);$pins[$name]=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($utf8.GetBytes($Expected))).ToLowerInvariant()}
 return @{root=$dir;pins=($pins|ConvertTo-Json|ConvertFrom-Json)}
}
function Reject([string]$Name,$Fixture){$failed=$false;try{$null=Prepare-SensitivitySources $Fixture.root $Fixture.pins}catch{$failed=$true};if(!$failed){throw "Expected rejection: $Name"};Pass $Name}
foreach($case in @(@('unchanged-lf',"alpha 空間`nbeta`n","alpha 空間`nbeta`n",$false),@('unchanged-crlf',"alpha 空間`r`nbeta`r`n","alpha 空間`r`nbeta`r`n",$false),@('to-lf',"alpha 空間`r`nbeta`r`n","alpha 空間`nbeta`n",$true),@('to-crlf',"alpha 空間`nbeta`n","alpha 空間`r`nbeta`r`n",$true))){
 $f=Fixture $case[0] $case[1] $case[2];$r=Prepare-SensitivitySources $f.root $f.pins
 if($r.files.Count -ne 15 -or @($r.files|Where-Object{$_.changed -ne $case[3]}).Count){throw 'Incorrect preparation receipt'}
 foreach($name in $names){if([IO.File]::ReadAllText((Join-Path $f.root $name)) -cne $case[2]){throw 'Prepared Unicode bytes differ'}}
 Pass $case[0]
}
$f=Fixture 'changed-code' "changed`n" "expected`n";Reject 'non-newline source change' $f
$f=Fixture 'lone-cr' "a`rb`n" "a`nb`n";Reject 'lone CR is not normalized' $f
$f=Fixture 'invalid-utf8' "a`n" "b`n";[IO.File]::WriteAllBytes((Join-Path $f.root $names[0]),[byte[]]@(255,254,255));Reject 'invalid UTF8' $f
$f=Fixture 'extra-pin' "a`n" "a`n";$f.pins|Add-Member '../outside.txt' ('0'*64);Reject 'unlisted path pin' $f
$f=Fixture 'missing-pin' "a`n" "a`n";$f.pins.PSObject.Properties.Remove($names[0]);Reject 'missing source pin' $f
$f=Fixture 'bad-digest' "a`n" "a`n";$f.pins.($names[0])='invalid';Reject 'malformed source digest' $f
$f=Fixture 'directory-source' "a`n" "a`n";$p=Join-Path $f.root $names[0];Remove-Item -LiteralPath $p;$null=New-Item -ItemType Directory -Path $p;Reject 'directory source' $f
$f=Fixture 'oversized-source' "a`n" "a`n";$stream=[IO.File]::OpenWrite((Join-Path $f.root $names[0]));try{$stream.SetLength(2097153)}finally{$stream.Dispose()};Reject 'source beyond 2MiB cap' $f
$f=Fixture 'validate-before-write' "a`n" "a`r`n";[IO.File]::WriteAllText((Join-Path $f.root $names[-1]),"different`n",$utf8);Reject 'later invalid source rejects whole plan' $f
if([IO.File]::ReadAllText((Join-Path $f.root $names[0])) -cne "a`n"){throw 'Partial rewrite occurred'};Pass 'all validation precedes any rewrite'
$f=Fixture 'unlisted-data' "a`n" "a`r`n";$p=Join-Path $f.root 'numeric.mtx';[IO.File]::WriteAllText($p,"1 2 3`n",$utf8);$hash=(Get-FileHash -LiteralPath $p).Hash;$null=Prepare-SensitivitySources $f.root $f.pins;if((Get-FileHash -LiteralPath $p).Hash -cne $hash){throw 'Unlisted numeric bytes changed'};Pass 'numeric data untouched'
if((Get-FileHash -LiteralPath "$PSScriptRoot/prepare-sensitivity-sources.ps1").Hash.ToLowerInvariant() -cne $helperHash){throw 'Source helper changed during test'}
$result=[ordered]@{schema_version='greenlens.sensitivity-source-preparation-tests.v1';scope='Original synthetic source-only newline guard tests; no engine, network, archive-code execution or real checkout mutation';source_sha256=$helperHash;passed=$true;checks=$checks}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'tests.json'),($result|ConvertTo-Json -Depth 10)+"`n",$utf8)
[ordered]@{passed=$true;checks=$checks.Count;output=$OutputDirectory}|ConvertTo-Json
$global:LASTEXITCODE=0
