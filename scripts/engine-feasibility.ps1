#Requires -Version 7.2
param(
    [string]$EngineRoot = (Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'),
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/verification/Gate 0 空間')
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$records = [System.Collections.Generic.List[object]]::new()
function Invoke-Radiance([string]$Name, [string[]]$Arguments, [string]$Output, [string]$InputText = '') {
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
    if (!$process.Start()) { throw "Cannot start $Name" }
    $stream = [IO.File]::Create((Join-Path $OutputDirectory $Output))
    try {
        $outputTask = $process.StandardOutput.BaseStream.CopyToAsync($stream)
        $errorTask = $process.StandardError.ReadToEndAsync()
        $process.StandardInput.Write($InputText)
        $process.StandardInput.Close()
        $process.WaitForExit()
        $null = $outputTask.GetAwaiter().GetResult()
        $stderr = $errorTask.GetAwaiter().GetResult()
        $records.Add([ordered]@{executable="$Name.exe"; arguments=$Arguments; exit_code=$process.ExitCode; stderr=$stderr; started_utc=$started; finished_utc=[DateTime]::UtcNow.ToString('o')})
        if ($process.ExitCode -ne 0) { throw "$Name exited $($process.ExitCode): $stderr" }
    } finally { $stream.Dispose(); $process.Dispose() }
}
$irradiance = (10000.0 / 179.0).ToString('R', [Globalization.CultureInfo]::InvariantCulture)
Invoke-Radiance 'rtrace' @('-version') 'version.txt'
Invoke-Radiance 'gensky' @('-ang','45','0','-c','-B',$irradiance,'-g','0.2') 'sky.rad'
$dome = @'

skyfunc glow sky_glow
0
0
4 1 1 1 0
sky_glow source sky
0
0
4 0 0 1 180
skyfunc glow ground_glow
0
0
4 1 1 1 0
ground_glow source ground
0
0
4 0 0 -1 180
'@
[IO.File]::AppendAllText((Join-Path $OutputDirectory 'sky.rad'), $dome + "`n", [Text.UTF8Encoding]::new($false))
Invoke-Radiance 'oconv' @('-f','sky.rad') 'scene.oct'
$traceArgs = @('-I+','-h','-faa','-ov','-n','1','-ab','1','-ad','32768','-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','0.000001','-lr','12','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','scene.oct')
Invoke-Radiance 'rtrace' $traceArgs 'raw.rgb' "0 0 0.8 0 0 1`n"
$values = ([IO.File]::ReadAllText((Join-Path $OutputDirectory 'raw.rgb')).Trim() -split '\s+') | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) }
if ($values.Count -ne 3 -or ($values | Where-Object { ![double]::IsFinite($_) -or $_ -lt 0 }).Count -gt 0) { throw 'Malformed/nonfinite/negative irradiance output' }
$lux = 179 * (0.265074126*$values[0] + 0.670114631*$values[1] + 0.064811243*$values[2])
$relativeError = [Math]::Abs($lux - 10000) / 10000
$result = [ordered]@{schema_version='1'; verified_utc=[DateTime]::UtcNow.ToString('o'); platform='native Windows'; working_path_contains_spaces=$OutputDirectory.Contains(' '); working_path_contains_non_ascii=([bool](@($OutputDirectory.ToCharArray() | Where-Object { [int]$_ -gt 127 }).Count)); engine_version=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'version.txt')).Trim(); executable_hashes=@{}; raypath='.;<engine>/lib'; rgb=$values; illuminance_lux=$lux; configured_illuminance_lux=10000; relative_error=$relativeError; processes=$records}
foreach ($name in @('gensky','oconv','rtrace')) { $result.executable_hashes[$name] = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $EngineRoot "bin/$name.exe")).Hash.ToLowerInvariant() }
$result | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'feasibility.json') -Encoding utf8
$result | ConvertTo-Json -Depth 10
if ($relativeError -gt 0.01) { throw "Exterior illuminance error $relativeError exceeds 1%. Gate 0 finite execution succeeded; Gate 1 exterior normalization needs investigation." }
