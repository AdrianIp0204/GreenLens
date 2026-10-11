#Requires -Version 7.2
# Real native generator consistency; this is not independent physical validation.
param(
    [Parameter(Mandatory)][string]$WeatherFile,
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 weather context 空間'),
    [int[]]$IntervalIndices = @(0,1,8,23,24,32,1908,8759)
)
$ErrorActionPreference='Stop'
$weather = [IO.File]::ReadAllText($WeatherFile)
$sourceWeatherPath = [IO.Path]::GetFullPath($WeatherFile)
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly
Copy-Item -LiteralPath $sourceWeatherPath -Destination (Join-Path $OutputDirectory 'annual.epw')
$lines=$weather.TrimEnd("`r","`n") -split '\r?\n'
if($lines.Count -ne 8768) { throw 'Expected canonical full-year EPW' }
Invoke-Engine 'gendaymtx' @('-O0','-m','2','-c','1','1','1','-g','0.2','0.2','0.2','-s','annual.epw') 'annual.mtx'
Invoke-Engine 'gendaymtx' @('-O0','-n','-D','annual-suns.rad','-M','annual-modifiers.txt','annual.epw') 'solar-generation.txt'
$annual=Read-Matrix 'annual.mtx'
$annualSuns=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'annual-suns.rad'))
$checks=[Collections.Generic.List[object]]::new()
foreach($intervalIndex in $IntervalIndices) {
    $start=[Math]::Max(0,$intervalIndex-2)
    $first=$lines[$start+8].Split(','); $last=$lines[$intervalIndex+8].Split(',')
    $context=($lines[0..6] -join "`n")+"`nDATA PERIODS,1,1,GreenLens context,Monday,$($first[1])/$($first[2]),$($last[1])/$($last[2])`n"
    $context+=($lines[($start+8)..($intervalIndex+8)] -join "`n")+"`n"
    Write-Input 'selected.epw' $context
    Invoke-Engine 'gendaymtx' @('-O0','-m','2','-c','1','1','1','-g','0.2','0.2','0.2','-s','selected.epw') "selected-$intervalIndex.mtx"
    Invoke-Engine 'gendaymtx' @('-O0','-n','-D',"suns-$intervalIndex.rad",'-M',"modifiers-$intervalIndex.txt",'selected.epw') "sun-generation-$intervalIndex.txt"
    $selected=Read-Matrix "selected-$intervalIndex.mtx"
    $maxError=0.0
    for($patch=0;$patch -lt $annual.rows;$patch++) {
        for($channel=0;$channel -lt 3;$channel++) {
            $expected=$annual.values[($patch*$annual.columns+$intervalIndex)*3+$channel]
            $actual=$selected.values[($patch*$selected.columns+$selected.columns-1)*3+$channel]
            $maxError=[Math]::Max($maxError,[Math]::Abs($actual-$expected))
        }
    }
    $solarName="solar$($intervalIndex*60+30)"
    $pattern="(?s)void light $solarName\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)"
    $expectedSun=[regex]::Match($annualSuns,$pattern).Value.Trim()
    $actualSun=[regex]::Match([IO.File]::ReadAllText((Join-Path $OutputDirectory "suns-$intervalIndex.rad")),$pattern).Value.Trim()
    $check=@{index=$intervalIndex;hour_ending=[int]$last[3];context_indices=@($start..$intervalIndex);sky_max_absolute_rgb_radiance_error=$maxError;solar_source_exact_match=($expectedSun -eq $actualSun);has_solar_source=($expectedSun.Length -gt 0);passed=($maxError -eq 0 -and $expectedSun -eq $actualSun)}
    $checks.Add($check)
    $check | ConvertTo-Json -Compress | Write-Host
}
$result=@{schema_version='greenlens.annual-weather-context-check.v1';weather_sha256=(Get-FileHash -LiteralPath $sourceWeatherPath).Hash.ToLowerInvariant();annual_shape=@($annual.rows,$annual.columns,3);checks=$checks;all_passed=(@($checks | Where-Object passed -eq $false).Count -eq 0);method='Pinned native gendaymtx full-year columns versus preceding-two-record selected context; equality of parsed numeric sky and exact requested solar source. Integration consistency only.'}
$result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'weather-context.json') -Encoding utf8
if(!$result.all_passed) { throw 'Selected context does not reproduce annual sky/source' }
