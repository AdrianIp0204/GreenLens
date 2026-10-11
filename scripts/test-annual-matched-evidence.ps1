#Requires -Version 7.2
# Synthetic evidence-handling checks only; these do not establish engine accuracy.
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-matched-evidence.ps1"
$checks=[Collections.Generic.List[string]]::new()
function Assert-Test([bool]$Value,[string]$Name){if(!$Value){throw "Failed: $Name"};$checks.Add($Name)}
function Assert-Rejected([scriptblock]$Action,[string]$Pattern,[string]$Name){$errorText='';try{& $Action|Out-Null}catch{$errorText=$_.Exception.Message};if($errorText -notmatch $Pattern){throw "Failed: $Name ($errorText)"};$checks.Add($Name)}
$root=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/matched evidence regression '+[guid]::NewGuid().ToString('N'))
$null=New-Item -ItemType Directory -Path $root
$source=[IO.File]::ReadAllText("$PSScriptRoot/annual-matched-corrected.ps1")
$kernel=Get-MatchedKernelHash $source
Assert-Test ($kernel -cmatch '^[0-9a-f]{64}$') 'numerical kernel exists in production research harness'
Assert-Test ((Get-MatchedKernelHash ($source.Replace("`r`n","`n"))) -ceq $kernel) 'kernel identity ignores text line-ending transport only'
Assert-Test ((Get-MatchedKernelHash ($source.Replace('Math.PI/360','Math.PI/180'))) -cne $kernel) 'finite-disc mathematical change invalidates reference cache'
Assert-Test (((Select-MatchedIntervals @(10,20,30) @(30,10)) -join ',') -ceq '10,30') 'shard retains predeclared ordering'
Assert-Rejected {Select-MatchedIntervals @(10,20) @(10,10)} 'Duplicate requested' 'duplicate shard interval rejected'
Assert-Rejected {Select-MatchedIntervals @(10,20) @(30)} 'predeclared' 'undeclared interval rejected'
function New-Fixture([string]$Name,[int]$Index,[double]$Candidate=100.0){
    $directory=Join-Path $root $Name;$null=New-Item -ItemType Directory -Path $directory
    Copy-Item -LiteralPath "$PSScriptRoot/annual-matched-corrected.ps1" -Destination (Join-Path $directory 'harness-source.ps1')
    @{harness_sha256=(Get-MatchedHash (Join-Path $directory 'harness-source.ps1'))}|ConvertTo-Json|Set-Content (Join-Path $directory 'harness-source.json')
    [IO.File]::WriteAllText((Join-Path $directory 'sensors.pts'),"0 0 .8 0 0 1`n")
    $reference=@{interval_indices=@(10,20);sensor_sha256=(Get-MatchedHash (Join-Path $directory 'sensors.pts'))}
    $declaration=@{reference_identity_sha256=(Get-MatchedReferenceHash $reference);candidate_recipe_sha256=('b'*64);candidate_rgb_sha256=('c'*64);reference=$reference}
    $difference=[Math]::Abs($Candidate-100);$pass=$difference -le 5
    $check=@{interval_index=$Index;pass=$pass;failure_count=$(if($pass){0}else{1});sensors=@(@{sensor_index=0;candidate_lux=$Candidate;reference_lux=100.0;absolute_error_lux=$difference;allowed_error_lux=5.0;passed=$pass})}
    foreach($kind in @('reference','direct','finite')){[IO.File]::WriteAllText((Join-Path $directory "hour-$Index-$kind.rgb"),'1 1 1');$check["${kind}_sha256"]=Get-MatchedHash (Join-Path $directory "hour-$Index-$kind.rgb")}
    @{declaration=$declaration;checks=@($check);completed=$false;pass=$false}|ConvertTo-Json -Depth 12|Set-Content (Join-Path $directory 'matched-corrected.json')
    return $directory
}
$first=New-Fixture 'first' 10
$second=New-Fixture 'second' 20 110.0
$result=Join-MatchedEvidence @($first,$second)
Assert-Test ($result.completed -and !$result.pass -and $result.checks.Count -eq 2 -and $result.checks[1].failure_count -eq 1) 'aggregate retains a failed sensor and all declared intervals from partial serial records'
Assert-Rejected {Join-MatchedEvidence @($first)} 'Incomplete matched' 'incomplete reference coverage cannot pass'
Assert-Rejected {Join-MatchedEvidence @($first,$first)} 'Duplicate matched' 'overlapping shards cannot silently replace a failed result'
$bad=New-Fixture 'bad' 20
Add-Content -LiteralPath (Join-Path $bad 'hour-20-reference.rgb') '2 2 2'
Assert-Rejected {Join-MatchedEvidence @($first,$bad)} 'checksum mismatch' 'mutated cached raw reference rejected'
$badPass=New-Fixture 'bad-pass' 20 110.0
$path=Join-Path $badPass 'matched-corrected.json';$record=Get-Content $path -Raw|ConvertFrom-Json;$record.checks[0].sensors[0].passed=$true;$record|ConvertTo-Json -Depth 12|Set-Content $path
Assert-Rejected {Join-MatchedEvidence @($first,$badPass)} 'arithmetic or fixed tolerance' 'forged per-sensor success rejected'
$badSource=New-Fixture 'bad-source' 20
Add-Content -LiteralPath (Join-Path $badSource 'harness-source.ps1') '# changed'
Assert-Rejected {Assert-MatchedSource $badSource $kernel} 'source checksum mismatch' 'mutated retained harness rejected'
$badIdentity=New-Fixture 'bad-identity' 20
$path=Join-Path $badIdentity 'matched-corrected.json';$record=Get-Content $path -Raw|ConvertFrom-Json;$record.declaration.reference.interval_indices=@(10,20,30);$record|ConvertTo-Json -Depth 12|Set-Content $path
Assert-Rejected {Join-MatchedEvidence @($first,$badIdentity)} 'reference identity checksum' 'changed full plan with stale reference digest rejected'
$report=@{scope='Synthetic evidence-handling regression, not scientific acceptance';checks=$checks;pass=$true}
$report|ConvertTo-Json -Depth 4|Set-Content -LiteralPath (Join-Path $root 'result.json') -Encoding utf8
Write-Host "Matched evidence regression: $($checks.Count) checks passed."
