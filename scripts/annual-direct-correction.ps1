#Requires -Version 7.2
# Exploratory direct-corrected patch transport. This does not change production
# settings or waive any failed matched-sky check. Uses trusted local research.
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [string]$OutputDirectory=(Join-Path $env:LOCALAPPDATA 'GreenLens/verification/v0.3 corrected patch prototype 空間'),
    [int]$Interval=1912,
    [int]$DirectSamples=65536,
    [int]$Basis=2,
    [string]$CoefficientFile='',
    [string]$DirectCoefficientFile='',
    [switch]$SolarBasisOnly
)
$ErrorActionPreference='Stop'
$solarBasis=$Basis
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
foreach($file in @('scene.rad','sensors.pts','receiver.rad')) { Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file) }
if(!$CoefficientFile){$CoefficientFile=Join-Path $SourceDirectory 'coefficients.mtx'}
Copy-Item -LiteralPath $CoefficientFile -Destination (Join-Path $OutputDirectory 'coefficients.mtx')
$inputSensors=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'sensors.pts'))
$sensorLines=@($inputSensors.Trim() -split '\r?\n');$sensorCount=$sensorLines.Count
$fullWeather=[IO.File]::ReadAllLines((Join-Path $SourceDirectory 'weather.epw'))
$contextStart=[Math]::Max(0,$Interval-2)
$weather=($fullWeather[0..7]+$fullWeather[($contextStart+8)..($Interval+8)]) -join "`n"
Write-Input 'weather.epw' ($weather+"`n")
$declaration=@{purpose='Exploratory total patch minus direct patch plus explicit finite-disc direct; not a shipping decision';interval=$Interval;mf=$solarBasis;solar_basis_only=[bool]$SolarBasisOnly;direct_ad=$DirectSamples;full_coefficients_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'coefficients.mtx')).Hash.ToLowerInvariant();direct_samples=@(1024,4096);declared_utc=[DateTime]::UtcNow.ToString('o')}
$declaration|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'gendaymtx' @('-O0','-m',"$solarBasis",'-c','1','1','1','-g','.2','.2','.2','weather.epw') 'total-sky.mtx'
Invoke-Engine 'gendaymtx' @('-O0','-m',"$solarBasis",'-c','1','1','1','-g','.2','.2','.2','-s','weather.epw') 'diffuse-sky.mtx'
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'transmission.oct'
$shared=@('-I+','-n','1','-faa','-ab','1','-ad',"$DirectSamples",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
if($DirectCoefficientFile) {
    Copy-Item -LiteralPath $DirectCoefficientFile -Destination (Join-Path $OutputDirectory 'direct-coefficients.mtx')
} else {
    Invoke-Engine 'rcontrib' ($shared+@('-y',"$sensorCount",'-V-','-e',"MF:$solarBasis",'-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow','transmission.oct')) 'direct-coefficients.mtx' $inputSensors
}
$declaration.direct_coefficients_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'direct-coefficients.mtx')).Hash.ToLowerInvariant()
$declaration|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'dctimestep' @('-oa','coefficients.mtx','total-sky.mtx') 'total-rgb.mtx'
Invoke-Engine 'dctimestep' @('-oa','coefficients.mtx','diffuse-sky.mtx') 'diffuse-rgb.mtx'
Invoke-Engine 'dctimestep' @('-oa','direct-coefficients.mtx','total-sky.mtx') 'direct-total-rgb.mtx'
Invoke-Engine 'dctimestep' @('-oa','direct-coefficients.mtx','diffuse-sky.mtx') 'direct-diffuse-rgb.mtx'
$total=Read-Matrix 'total-rgb.mtx';$diffuse=Read-Matrix 'diffuse-rgb.mtx';$directTotal=Read-Matrix 'direct-total-rgb.mtx';$directDiffuse=Read-Matrix 'direct-diffuse-rgb.mtx'
if($SolarBasisOnly){
    Copy-Item -LiteralPath (Join-Path $SourceDirectory 'diffuse-rgb.mtx') -Destination (Join-Path $OutputDirectory 'original-diffuse.mtx')
    $originalDiffuse=Read-Matrix 'original-diffuse.mtx'
    $originalDeclaration=Get-Content -LiteralPath (Join-Path $SourceDirectory 'declaration.json') -Raw|ConvertFrom-Json
    $originalColumn=[Array]::IndexOf(@($originalDeclaration.intervals.index),[long]$Interval)
    if($originalColumn -lt 0){throw 'Original diffuse interval mapping is missing'}
}
$sourceText=[IO.File]::ReadAllText((Join-Path $SourceDirectory "hour-$Interval.rad"))
$solarText=$sourceText.Substring($sourceText.IndexOf('void light solar'))
Write-Input 'sun.rad' $solarText
Invoke-Engine 'oconv' @('-f','scene.rad','sun.rad') 'centroid.oct'
$rayOptions=@('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0','-st','0','-ss','0','-u-')
Invoke-Engine 'rtrace' (@('-I+')+$rayOptions+@('centroid.oct')) 'centroid.rgb' $inputSensors
function RgbTokens([string]$File) { return ,@(([IO.File]::ReadAllText((Join-Path $OutputDirectory $File))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { [double]::Parse($_,$culture) }) }
$centroid=RgbTokens 'centroid.rgb'
$reference=@(([IO.File]::ReadAllText((Join-Path $SourceDirectory "hour-$Interval.rgb"))).Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { [double]::Parse($_,$culture) })
$tokens=$solarText.Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries)
$radiance=[double]::Parse($tokens[6],$culture)
$center=@($tokens[15..17] | ForEach-Object { [double]::Parse($_,$culture) })
$length=[Math]::Sqrt($center[0]*$center[0]+$center[1]*$center[1]+$center[2]*$center[2]);$center=@($center|ForEach-Object { $_/$length })
$radius=[double]::Parse($tokens[18],$culture)*[Math]::PI/360
$omega=2*[Math]::PI*(1-[Math]::Cos($radius))
$u=@($center[1],(-$center[0]),0.0);$length=[Math]::Sqrt($u[0]*$u[0]+$u[1]*$u[1])
if($length -lt 1e-9) { $u=@(0.0,1.0,0.0) } else { $u=@($u|ForEach-Object { $_/$length }) }
$v=@(($center[1]*$u[2]-$center[2]*$u[1]),($center[2]*$u[0]-$center[0]*$u[2]),($center[0]*$u[1]-$center[1]*$u[0]))
$finiteBySide=@{}
foreach($side in @(32,64)) {
    $directions=[Collections.Generic.List[object]]::new()
    for($ring=0;$ring -lt $side;$ring++) {
        $mu=1-(1-[Math]::Cos($radius))*($ring+.5)/$side;$radial=[Math]::Sqrt(1-$mu*$mu)
        for($az=0;$az -lt $side;$az++) {
            $phi=2*[Math]::PI*($az+.5)/$side
            $direction=@(0,1,2|ForEach-Object { $mu*$center[$_]+$radial*([Math]::Cos($phi)*$u[$_]+[Math]::Sin($phi)*$v[$_]) })
            $directions.Add($direction)
        }
    }
    $rays=[Text.StringBuilder]::new()
    foreach($line in $sensorLines) {
        $position=$line.Split([char[]]' ',[StringSplitOptions]::RemoveEmptyEntries)[0..2]
        foreach($direction in $directions) { $null=$rays.Append(($position+@($direction|ForEach-Object { $_.ToString('R',$culture) })) -join ' ').Append("`n") }
    }
    Invoke-Engine 'rtrace' ($rayOptions+@('transmission.oct')) "finite-$side.rgb" $rays.ToString()
    $rgb=RgbTokens "finite-$side.rgb";$finite=[double[]]::new($sensorCount)
    if($rgb.Count -ne $sensorCount*$side*$side*3) { throw 'Finite direct output count mismatch' }
    for($sensor=0;$sensor -lt $sensorCount;$sensor++) {
        for($ray=0;$ray -lt $directions.Count;$ray++) { $finite[$sensor]+=$rgb[($sensor*$directions.Count+$ray)*3]*[Math]::Max(0.0,$directions[$ray][2]) }
        $finite[$sensor]*=179*$radiance*$omega/$directions.Count
    }
    $finiteBySide[$side]=$finite
}
$checks=@();$column=$Interval-$contextStart
for($sensor=0;$sensor -lt $sensorCount;$sensor++) {
    $offset=($sensor*$total.columns+$column)*3
    $base=$diffuse.values[$offset]
    if($SolarBasisOnly){$base=$originalDiffuse.values[($sensor*$originalDiffuse.columns+$originalColumn)*3]}
    $candidate=179*($base+$total.values[$offset]-$diffuse.values[$offset]-$directTotal.values[$offset]+$directDiffuse.values[$offset])+$finiteBySide[32][$sensor]
    $referenceCorrected=179*($reference[$sensor*3]-$centroid[$sensor*3])+$finiteBySide[64][$sensor]
    $error=[Math]::Abs($candidate-$referenceCorrected);$allowed=[Math]::Max(5.0,.05*$referenceCorrected)
    $checks+=@{sensor=$sensor;candidate_lux=$candidate;reference_corrected_lux=$referenceCorrected;reference_original_lux=179*$reference[$sensor*3];finite_direct_shipping_lux=$finiteBySide[32][$sensor];finite_direct_reference_lux=$finiteBySide[64][$sensor];centroid_direct_lux=179*$centroid[$sensor*3];patch_direct_solar_lux=179*($directTotal.values[$offset]-$directDiffuse.values[$offset]);absolute_error_lux=$error;allowed_error_lux=$allowed;passed=($error -le $allowed)}
}
$result=@{declaration=$declaration;checks=$checks;pass=(@($checks|Where-Object { !$_.passed }).Count -eq 0);maximum_error_over_tolerance=($checks|ForEach-Object { $_.absolute_error_lux/$_.allowed_error_lux }|Measure-Object -Maximum).Maximum}
$result|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'correction.json') -Encoding utf8
$result|ConvertTo-Json -Depth 8|Write-Host
