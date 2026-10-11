#Requires -Version 7.2
# Extract the complete predeclared matched plan from trusted native job scratch.
# This prepares transport only; it makes no numerical acceptance claim.
param(
 [Parameter(Mandatory)][string]$CandidateDirectory,
 [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new investigation directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
. "$PSScriptRoot/annual-matched-evidence.ps1"
$recipe=Get-Content -LiteralPath (Join-Path $CandidateDirectory 'annual-recipe.json') -Raw|ConvertFrom-Json
if($recipe.interval_indices.Count -ne 8760 -or ($recipe.interval_indices -join ',') -ne ((0..8759) -join ',')){throw 'Requires ordered full-year native scratch'}
foreach($pair in @(@('annual-scene.rad','scene.rad'),@('annual-sensors.pts','sensors.pts'),@('annual-receiver.rad','receiver.rad'))){Copy-Item -LiteralPath (Join-Path $CandidateDirectory $pair[0]) -Destination (Join-Path $OutputDirectory $pair[1])}
$plan=Get-Content -LiteralPath "$PSScriptRoot/annual-validation-plan.json" -Raw|ConvertFrom-Json
$weather=@([IO.File]::ReadAllLines((Join-Path $CandidateDirectory 'annual-weather.epw'))|Select-Object -Skip 8)
function Condition([int]$Index){$f=$weather[$Index].Split(',');$dni=[double]::Parse($f[14],$culture);$dhi=[double]::Parse($f[15],$culture);if($dhi -lt 50){return 'low_diffuse'};if($dni/$dhi -ge 3){return 'clear'};if($dni/$dhi -ge .5){return 'mixed'};return 'overcast'}
$indices=[Collections.Generic.List[int]]::new()
foreach($interval in $plan.fixed_intervals){$date=[DateTime]::new(2001,$interval.month,$interval.day);$indices.Add(($date.DayOfYear-1)*24+$interval.hour_ending-1)}
$classes=@($indices|ForEach-Object{Condition $_})
foreach($class in @('clear','mixed','overcast')){if($classes -notcontains $class){for($i=0;$i -lt 8760;$i++){if((Condition $i) -eq $class -and $i%24 -ge 8 -and $i%24 -le 16){$indices.Add($i);break}}}}
$suns=@{}
foreach($match in [regex]::Matches([IO.File]::ReadAllText((Join-Path $CandidateDirectory 'annual-suns.rad')),'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')){$suns[[int](([int]$match.Groups[1].Value-30)/60)]=$match.Value}
$text=[Text.StringBuilder]::new();$modifiers=[Text.StringBuilder]::new()
foreach($index in $indices){if(!$suns.ContainsKey($index)){throw "Declared interval $index has no source"};$null=$text.Append($suns[$index]).Append("`n");$null=$modifiers.Append("solar$($index*60+30)`n");Write-Input "hour-$index-sun.rad" $suns[$index]}
Write-Input 'selected-suns.rad' $text.ToString();Write-Input 'selected-modifiers.txt' $modifiers.ToString()
$sky=[GreenLensFeasibility.Matrix]::Read((Join-Path $CandidateDirectory 'annual-sky.mtx'))
if($sky.columns -ne 8760 -or $sky.rows -ne (144*$recipe.profile.sky_basis_mf*$recipe.profile.sky_basis_mf+2)){throw 'Sky basis mismatch'}
$matrix=[Text.StringBuilder]::new("#?RADIANCE`nNROWS=$($sky.rows)`nNCOLS=$($indices.Count)`nNCOMP=3`nFORMAT=ascii`n`n")
for($row=0;$row -lt $sky.rows;$row++){foreach($index in $indices){$offset=($row*8760+$index)*3;for($k=0;$k -lt 3;$k++){$null=$matrix.Append($sky.values[$offset+$k].ToString('R',$culture)).Append(' ')}};$null=$matrix.Append("`n")}
Write-Input 'selected-skies.mtx' $matrix.ToString()
$declaration=[ordered]@{scope='Transport preparation only; reference comparison pending';declared_utc=[DateTime]::UtcNow.ToString('o');intervals=@($indices|ForEach-Object{@{index=$_;condition=(Condition $_)}});source_recipe_sha256=(Get-MatchedHash (Join-Path $CandidateDirectory 'annual-recipe.json'));scene_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'scene.rad'));sensor_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'sensors.pts'));sky_sha256=(Get-MatchedHash (Join-Path $CandidateDirectory 'annual-sky.mtx'));sun_sha256=(Get-MatchedHash (Join-Path $CandidateDirectory 'annual-suns.rad'));sky_basis_mf=$recipe.profile.sky_basis_mf;source_engine=$recipe.engine}
$declaration|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
