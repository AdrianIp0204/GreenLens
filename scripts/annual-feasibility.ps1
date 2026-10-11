#Requires -Version 7.2
# Native feasibility experiment, not release acceptance or a production adapter.
# Only original synthetic weather is generated here. It is not climatic data.
param(
    [string]$EngineRoot = (Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'),
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 annual 空間'),
    [ValidateRange(1,8760)][int]$Hours = 24,
    [ValidateRange(1,4)][int]$Basis = 2,
    [ValidateRange(1,8)][int]$Bounces = 3,
    [ValidateRange(16,65536)][int]$Samples = 1024,
    [string]$SceneFile = '',
    [string]$SensorsFile = '',
    [string]$WeatherFile = '',
    [ValidateRange(1,7200)][int]$ProcessTimeoutSeconds = 600,
    [switch]$FunctionsOnly
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/verify-matrix-multiplication.ps1"
$culture = [Globalization.CultureInfo]::InvariantCulture
$utf8 = [Text.UTF8Encoding]::new($false)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path
$records = [Collections.Generic.List[object]]::new()
if (-not ('GreenLensFeasibility.Matrix' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Globalization;
namespace GreenLensFeasibility {
    public sealed class Matrix {
        public int rows, columns, components;
        public double[] values = Array.Empty<double>();
        public static Matrix Read(string path) {
            var result = new Matrix();
            using var reader = new StreamReader(path);
            string line;
            bool ascii = false;
            while ((line = reader.ReadLine()) != null && line.Length != 0) {
                if (line.StartsWith("NROWS=")) result.rows = int.Parse(line.Substring(6), CultureInfo.InvariantCulture);
                if (line.StartsWith("NCOLS=")) result.columns = int.Parse(line.Substring(6), CultureInfo.InvariantCulture);
                if (line.StartsWith("NCOMP=")) result.components = int.Parse(line.Substring(6), CultureInfo.InvariantCulture);
                if (line == "FORMAT=ascii") ascii = true;
            }
            if (!ascii || result.rows < 1 || result.columns < 1 || result.components != 3) throw new InvalidDataException("Invalid ASCII RGB matrix header");
            result.values = new double[checked(result.rows*result.columns*result.components)];
            int index = 0;
            while ((line = reader.ReadLine()) != null) {
                foreach (string token in line.Split((char[])null, StringSplitOptions.RemoveEmptyEntries)) {
                    double value = double.Parse(token, CultureInfo.InvariantCulture);
                    if (index == result.values.Length || !double.IsFinite(value) || value < 0) throw new InvalidDataException("Excess or invalid matrix value");
                    result.values[index++] = value;
                }
            }
            if (index != result.values.Length) throw new InvalidDataException("Truncated matrix values");
            return result;
        }
    }
}
'@
}
function Write-Input([string]$Name, [string]$Content) {
    [IO.File]::WriteAllText((Join-Path $OutputDirectory $Name), $Content.Replace("`r`n","`n"), $utf8)
}
function Invoke-Engine([string]$Name, [string[]]$Arguments, [string]$Output, [string]$InputText = '', [bool]$ExpectedFailure = $false) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = Join-Path $EngineRoot "bin/$Name.exe"
    $info.WorkingDirectory = $OutputDirectory
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $info.Environment['RAYPATH'] = '.;' + (Join-Path $EngineRoot 'lib')
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    $started = [DateTime]::UtcNow.ToString('o')
    $timer = [Diagnostics.Stopwatch]::StartNew()
    if (!$process.Start()) { throw "Cannot start $Name" }
    $stream = [IO.File]::Create((Join-Path $OutputDirectory $Output))
    $peak = 0L
    $cancelled = $false
    try {
        $outputTask = $process.StandardOutput.BaseStream.CopyToAsync($stream)
        $errorTask = $process.StandardError.ReadToEndAsync()
        $process.StandardInput.Write($InputText)
        $process.StandardInput.Close()
        while (!$process.WaitForExit(100)) {
            $process.Refresh()
            $peak = [Math]::Max($peak, $process.PeakWorkingSet64)
            if ($timer.Elapsed.TotalSeconds -gt $ProcessTimeoutSeconds) {
                $cancelled = $true
                $process.Kill($true)
                $process.WaitForExit()
                break
            }
        }
        $null = $outputTask.GetAwaiter().GetResult()
        $stderr = $errorTask.GetAwaiter().GetResult()
        $timer.Stop()
        $record = [ordered]@{
            executable="$Name.exe"; arguments=$Arguments; exit_code=$process.ExitCode;
            stderr=$stderr; started_utc=$started; finished_utc=[DateTime]::UtcNow.ToString('o');
            wall_seconds=$timer.Elapsed.TotalSeconds; sampled_peak_working_set_bytes=$(if ($peak -gt 0) {$peak} else {$null});
            output_bytes=$stream.Length; timeout_killed_tree=$cancelled
        }
        $records.Add($record)
        $records | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'processes.json') -Encoding utf8
        Write-Host "$Name $($process.ExitCode): $([Math]::Round($timer.Elapsed.TotalSeconds,3)) s, $($stream.Length) bytes, sampled peak $peak bytes"
        if ($ExpectedFailure) {
            if ($process.ExitCode -eq 0) { throw "$Name unexpectedly accepted the deliberately malformed input" }
        } elseif ($process.ExitCode -ne 0 -or $cancelled) { throw "$Name failed: $stderr" }
    } finally { $stream.Dispose(); $process.Dispose() }
}
function Read-Matrix([string]$Name) {
    return [GreenLensFeasibility.Matrix]::Read((Join-Path $OutputDirectory $Name))
}
if ($FunctionsOnly) { return }

# A black, unglazed aperture fixture; transport is isolated from measured weather.
$scene = "void plastic black`n0`n0`n5 0 0 0 0 0`n"
$polygons = @(
    @(0,0,0, 4,0,0, 4,6,0, 0,6,0), @(0,0,3, 0,6,3, 4,6,3, 4,0,3),
    @(0,0,0, 0,6,0, 0,6,3, 0,0,3), @(4,0,0, 4,0,3, 4,6,3, 4,6,0),
    @(0,6,0, 4,6,0, 4,6,3, 0,6,3), @(0,0,0, 0,0,0.9, 4,0,0.9, 4,0,0),
    @(0,0,2.4, 0,0,3, 4,0,3, 4,0,2.4), @(0,0,0.9, 0,0,2.4, 1.1,0,2.4, 1.1,0,0.9),
    @(2.9,0,0.9, 2.9,0,2.4, 4,0,2.4, 4,0,0.9)
)
for ($index=0; $index -lt $polygons.Count; $index++) {
    $scene += "black polygon wall$index`n0`n0`n12 " + (($polygons[$index] | ForEach-Object { $_.ToString($culture) }) -join ' ') + "`n"
}
if ($SceneFile) { $scene = [IO.File]::ReadAllText($SceneFile) }
if ($scene -match '(?m)^\s*!') { throw 'No shell directives are allowed in the experiment scene' }
Write-Input 'scene.rad' $scene
$sensors = "2 1 0.8 0 0 1`n2 3 0.8 0 0 1`n2 5 0.8 0 0 1`n"
if ($SensorsFile) { $sensors = [IO.File]::ReadAllText($SensorsFile) }
$sensorCount = @($sensors.Trim() -split '\r?\n').Length
Write-Input 'sensors.pts' $sensors
Write-Input 'receiver.rad' @'
void glow sky_glow
0
0
4 1 1 1 0
sky_glow source upper_sky
0
0
4 0 0 1 180
sky_glow source lower_ground
0
0
4 0 0 -1 180
'@

# Canonical hourly EPW input: hour-end 1..24, minute60; engine samples midpoint.
$weather = [Text.StringBuilder]::new()
$null = $weather.Append("LOCATION,Synthetic,None,None,GreenLens,000000,22.3,114.17,8,10`nDESIGN CONDITIONS,0`nTYPICAL/EXTREME PERIODS,0`nGROUND TEMPERATURES,0`nHOLIDAYS/DAYLIGHT SAVINGS,No,0,0,0`nCOMMENTS 1,Original non-climatic feasibility fixture`nCOMMENTS 2,Hourly means represented at midpoint; no real weather claim`nDATA PERIODS,1,1,Synthetic,Monday,1/1,12/31`n")
$start = [DateTime]::new(2001,1,1,0,0,0)
for ($hourIndex=0; $hourIndex -lt $Hours; $hourIndex++) {
    $date = $start.AddHours($hourIndex)
    $dni = if ($date.Hour -ge 8 -and $date.Hour -lt 16) { 600 } else { 0 }
    $dhi = if ($date.Hour -ge 7 -and $date.Hour -lt 17) { 100 } else { 0 }
    $fields = @(2001,$date.Month,$date.Day,($date.Hour+1),60,'?9?9?9?9',25,15,60,101325,0,0,300,($dni+$dhi),$dni,$dhi,0,0,0,0,180,1,5,5,20,77777,9,999999999,20,0.1,0,88,0.2,0,0)
    $null = $weather.Append(($fields -join ',')).Append("`n")
}
if ($WeatherFile) { Write-Input 'weather.epw' ([IO.File]::ReadAllText($WeatherFile)) }
else { Write-Input 'weather.epw' $weather.ToString() }

Invoke-Engine 'rcontrib' @('-version') 'engine-version.txt'
Invoke-Engine 'gendaymtx' @('-O0','-m',"$Basis",'-c','1','1','1','-g','0.2','0.2','0.2','-s','weather.epw') 'diffuse-ground.mtx'
# Separate invocation is essential: -s would also zero the exported solar radiance.
Invoke-Engine 'gendaymtx' @('-O0','-n','-D','suns.rad','-M','sun-modifiers.txt','weather.epw') 'sun-generation.txt'
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'diffuse.oct'
Invoke-Engine 'oconv' @('-f','scene.rad','suns.rad') 'solar.oct'
$options = @('-I+','-n','1','-faa','-y',"$sensorCount",'-ab',"$Bounces",'-ad',"$Samples",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','0.00001','-lr','12','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
Invoke-Engine 'rcontrib' ($options + @('-V-','-e',"MF:$Basis",'-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow','diffuse.oct')) 'coefficients.mtx' $sensors
Invoke-Engine 'dctimestep' @('-oa','coefficients.mtx','diffuse-ground.mtx') 'diffuse-rgb.mtx'
Invoke-Engine 'rcontrib' ($options + @('-V+','-M','sun-modifiers.txt','solar.oct')) 'solar-rgb.mtx' $sensors

$sky = Read-Matrix 'diffuse-ground.mtx'
$coefficients = Read-Matrix 'coefficients.mtx'
$diffuse = Read-Matrix 'diffuse-rgb.mtx'
$solar = Read-Matrix 'solar-rgb.mtx'
$modifiers = @([IO.File]::ReadAllLines((Join-Path $OutputDirectory 'sun-modifiers.txt')) | Where-Object { $_ })
if ($coefficients.rows -ne $sensorCount -or $coefficients.columns -ne $sky.rows -or $diffuse.rows -ne $sensorCount -or $diffuse.columns -ne $sky.columns -or $solar.rows -ne $sensorCount -or $solar.columns -ne $modifiers.Length) { throw 'Matrix dimensions/order contract failed' }
$mapping = [int[]]::new($modifiers.Length)
for ($index=0; $index -lt $modifiers.Length; $index++) {
    if ($modifiers[$index] -notmatch '^solar(\d+)$') { throw 'Invalid solar modifier identifier' }
    $minute = [int]$Matches[1]
    if (($minute-30)%60 -ne 0) { throw 'Sun does not map to a canonical hourly midpoint' }
    $mapping[$index] = [int](($minute-30)/60)
    if ($mapping[$index] -lt 0 -or $mapping[$index] -ge $sky.columns -or ($index -gt 0 -and $mapping[$index] -le $mapping[$index-1])) { throw 'Nonmonotonic or out-of-range solar mapping' }
}
$multiplication = Assert-IndependentRgbProduct -Coefficients $coefficients -Sky $sky -Product $diffuse -MaximumColumns 48 -EvidencePath (Join-Path $OutputDirectory 'independent-multiplication.json')
$total = [double[]]$diffuse.values.Clone()
for ($sensor=0; $sensor -lt $sensorCount; $sensor++) {
    for ($sun=0; $sun -lt $mapping.Length; $sun++) {
        for ($component=0; $component -lt 3; $component++) { $total[($sensor*$sky.columns+$mapping[$sun])*3+$component] += $solar.values[($sensor*$mapping.Length+$sun)*3+$component] }
    }
}
$lux = [double[]]::new($sensorCount*$sky.columns)
for ($index=0; $index -lt $lux.Length; $index++) { $lux[$index] = 179*(0.265074126*$total[$index*3]+0.670114631*$total[$index*3+1]+0.064811243*$total[$index*3+2]) }
Write-Input 'malformed.mtx' "#?RADIANCE`nNROWS=2`nNCOLS=2`nNCOMP=3`nFORMAT=ascii`n`n1 2 3`n"
Invoke-Engine 'dctimestep' @('-oa','coefficients.mtx','malformed.mtx') 'expected-failure.txt' '' $true
$inventory = [ordered]@{}
foreach ($name in @('gendaymtx','rcontrib','dctimestep','oconv')) { $inventory[$name] = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $EngineRoot "bin/$name.exe")).Hash.ToLowerInvariant() }
$result = [ordered]@{
    schema_version='1'; evidence_kind='native feasibility only'; created_utc=[DateTime]::UtcNow.ToString('o');
    engine_version=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'engine-version.txt')).Trim(); executable_sha256=$inventory;
    path_has_spaces=$OutputDirectory.Contains(' '); path_has_non_ascii=[bool]($OutputDirectory -match '[^\x00-\x7F]');
    weather_kind=$(if ($WeatherFile) {'supplied EPW, interpretation reviewed separately'} else {'original non-climatic synthetic'});
    time_policy='hourly EPW interval-end 1..24; representative hour-0.5; no -i horizon shift; retain all columns';
    basis_reinhart_mf=$Basis; sky_rows=$sky.rows; hours=$sky.columns; sensors=$sensorCount; sun_columns=$mapping.Length;
    ordering='sensor-major RGB result; sky patch-major / time columns; solar minute ID -> canonical midpoint index';
    method='diffuse+uniform luminous-ground coefficients; separate explicit suns with black exterior ground and unchanged scene reflectances; no coarse direct sky component';
    profile=@{ambient_bounces=$Bounces;ambient_divisions=$Samples;ambient_accuracy=0;ambient_weight=0.00001;deterministic_sampling=$true;workers=1};
    independent_multiplication_max_absolute_rgb_error=$multiplication.maximum_absolute_rgb_error;
    independent_multiplication=$multiplication;
    illuminance_min_lux=($lux | Measure-Object -Minimum).Minimum; illuminance_max_lux=($lux | Measure-Object -Maximum).Maximum;
    scratch_bytes=(Get-ChildItem -LiteralPath $OutputDirectory -File | Measure-Object Length -Sum).Sum;
    processes=$records
}
$result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'feasibility.json') -Encoding utf8
$result | ConvertTo-Json -Depth 12
