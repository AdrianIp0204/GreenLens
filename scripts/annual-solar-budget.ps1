#Requires -Version 7.2
# Bounded native runtime experiment. Extrapolations are estimates, not full-year
# measurements. Other engine activity must be recorded in its host context.
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 solar128 resource 空間'),
    [int]$SolarCount = 128,
    [int]$SolarSamples = 8192,
    [int]$SolarBounces = 8
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
foreach($file in @('scene.rad','sensors.pts')) { Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file) }
$inputSensors=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'sensors.pts'))
$sensorCount=@($inputSensors.Trim() -split '\r?\n').Count
$sources=[regex]::Matches([IO.File]::ReadAllText((Join-Path $SourceDirectory 'all-suns.rad')),'(?s)void light (solar\d+)\s+0\s+0\s+3\s+(\S+)\s+.*?(?=\s*void light|\z)')
$positive=@($sources | Where-Object { [double]::Parse($_.Groups[2].Value,$culture) -gt 0 })
if($positive.Count -lt $SolarCount) { throw 'Requested benchmark count exceeds positive hourly solar sources' }
$selected=$positive[0..($SolarCount-1)]
Write-Input 'suns.rad' (($selected | ForEach-Object Value) -join "`n")
Write-Input 'modifiers.txt' ((($selected | ForEach-Object { $_.Groups[1].Value }) -join "`n")+"`n")
$concurrent=@(Get-Process rtrace,rcontrib -ErrorAction SilentlyContinue | Select-Object ProcessName,Id)
$declaration=@{schema_version='greenlens.annual-solar-budget.v1';positive_full_year_source_count=$positive.Count;all_full_year_source_count=$sources.Count;selected_positive_sources=$SolarCount;sensors=$sensorCount;ambient_divisions=$SolarSamples;ambient_bounces=$SolarBounces;limit_weight=1e-9;concurrent_engine_processes=$concurrent;declared_utc=[DateTime]::UtcNow.ToString('o');status='exploratory, not a frozen profile or full-year measurement'}
$declaration | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'oconv' @('-f','scene.rad','suns.rad') 'solar.oct'
$options=@('-I+','-n','1','-faa','-y',"$sensorCount",'-ab',"$SolarBounces",'-ad',"$SolarSamples",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-V+','-M','modifiers.txt','solar.oct')
Invoke-Engine 'rcontrib' $options 'solar.mtx' $inputSensors
$matrix=Read-Matrix 'solar.mtx'
if($matrix.rows -ne $sensorCount -or $matrix.columns -ne $SolarCount) { throw 'Budget run output shape mismatch' }
$trace=$records[-1]
$result=@{declaration=$declaration;measured_process=$trace;shape=@($matrix.rows,$matrix.columns,3);scratch_bytes=(Get-ChildItem -LiteralPath $OutputDirectory -File | Measure-Object Length -Sum).Sum;estimated_full_year_solar_seconds_same_sensor_count=$trace.wall_seconds*$positive.Count/$SolarCount;estimate_caveat='Simple positive-source count scaling from one bounded subset; solar direction/occlusion affects cost, host was shared, and this is not a measured annual duration.'}
$result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'resource-evidence.json') -Encoding utf8
$result | ConvertTo-Json -Depth 8 | Write-Host
