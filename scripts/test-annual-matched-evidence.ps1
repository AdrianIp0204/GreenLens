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
$transport=Join-Path $root 'transport';$null=New-Item -ItemType Directory -Path $transport
$files=[ordered]@{}
foreach($name in (Get-MatchedTransportNames 'coefficient')){[IO.File]::WriteAllText((Join-Path $transport $name),'synthetic');$files[$name]=Get-MatchedHash (Join-Path $transport $name)}
$seal=@{schema_version='greenlens.research-transport-integrity.v1';kind='coefficient';captured_after_execution=$true;files=$files}
$sealPath=Join-Path $transport 'transport-integrity.json';$seal|ConvertTo-Json -Depth 5|Set-Content -LiteralPath $sealPath
Assert-MatchedTransport $transport 'coefficient';$checks.Add('complete explicit transport seal accepted')
$files.Remove('coefficients.mtx');$seal|ConvertTo-Json -Depth 5|Set-Content -LiteralPath $sealPath
Assert-Rejected {Assert-MatchedTransport $transport 'coefficient'} 'file set' 'omitted coefficient checksum cannot bypass transport validation'
$files['coefficients.mtx']=Get-MatchedHash (Join-Path $transport 'coefficients.mtx');$seal|ConvertTo-Json -Depth 5|Set-Content -LiteralPath $sealPath
[IO.File]::WriteAllText((Join-Path $transport 'coefficients.mtx'),'changed')
Assert-Rejected {Assert-MatchedTransport $transport 'coefficient'} 'changed after sealing' 'changed coefficient values rejected'
$oldMethod='perez-epw-dewpoint3-reinhart-patch-indirect-finite-direct/2';$newMethod='perez-epw-dewpoint3-reinhart-explicit-indirect-finite-direct/3'
$physical=[ordered]@{scene_sha256=('a'*64);sensor_sha256=('b'*64);sky_sha256=('c'*64);sun_sha256=('d'*64);engine=[ordered]@{version='pinned';adapter_version='radiance-climate-direct-corrected/2';method_version=$oldMethod;fingerprint=('e'*64);executable_sha256=@{rtrace=('f'*64)};library_sha256=@{rayinit=('1'*64)}};ab=9;ad=262144;lw=1e-9;lr=16;finite_samples=4096;mf=2;interval_indices=@(10,20)}
$new=$physical|ConvertTo-Json -Depth 10|ConvertFrom-Json -AsHashtable
$new.engine.adapter_version='radiance-climate-explicit-indirect-finite-direct/3';$new.engine.method_version=$newMethod;$new.engine.fingerprint=('2'*64)
Assert-MatchedPhysicalReference $physical $new;$checks.Add('method-only provenance difference may reuse unchanged physical reference')
Assert-Test ($physical.engine.method_version -ceq $oldMethod -and $physical.engine.fingerprint -ceq ('e'*64)) 'historical reference identity remains immutable'
foreach($mutation in @({param($x)$x.sky_sha256='changed'},{param($x)$x.ad=131072},{param($x)$x.engine.library_sha256.rayinit='changed'},{param($x)$x.engine.executable_sha256.rtrace='changed'},{param($x)$x.interval_indices=@(20,10)})){
    $different=$new|ConvertTo-Json -Depth 10|ConvertFrom-Json -AsHashtable;& $mutation $different
    Assert-Rejected {Assert-MatchedPhysicalReference $physical $different} 'physical inputs' 'physical input, argument, binary, library or interval-order change prevents reuse'
}
$recipe=@{schema_version='greenlens.annual-recipe.v3';method_version=$newMethod;engine=$new.engine}
Assert-Test ((Get-MatchedMethodTuple $recipe).StartsWith('greenlens.annual-recipe.v3|')) 'matched harness recognizes exact method3 contract'
$recipe.schema_version='greenlens.annual-recipe.v2'
Assert-Rejected {Get-MatchedMethodTuple $recipe} 'method tuple' 'mixed annual method tuple rejected'
$report=@{scope='Synthetic evidence-handling regression, not scientific acceptance';checks=$checks;pass=$true}
$report|ConvertTo-Json -Depth 4|Set-Content -LiteralPath (Join-Path $root 'result.json') -Encoding utf8
Write-Host "Matched evidence regression: $($checks.Count) checks passed."
