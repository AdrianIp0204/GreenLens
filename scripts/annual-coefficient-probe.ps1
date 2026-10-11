#Requires -Version 7.2
# Shared-geometry coefficient sampling experiment; not a production profile.
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [int]$CoefficientSamples=262144,
    [int]$CoefficientBounces=8,
    [int]$CoefficientBasis=2
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
foreach($file in @('scene.rad','sensors.pts','receiver.rad')) {
    Copy-Item -LiteralPath (Join-Path $SourceDirectory $file) -Destination (Join-Path $OutputDirectory $file)
}
$sensors=[IO.File]::ReadAllText((Join-Path $OutputDirectory 'sensors.pts'))
$sensorCount=@($sensors.Trim() -split '\r?\n').Count
$declaration=@{status='Exploratory coefficient sampling; not release acceptance';sensor_count=$sensorCount;mf=$CoefficientBasis;ab=$CoefficientBounces;ad=$CoefficientSamples;direct_ab=1;direct_ad=$CoefficientSamples;weight=1e-9;declared_utc=[DateTime]::UtcNow.ToString('o')}
$declaration|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'coefficients.oct'
$shared=@('-I+','-n','1','-faa','-ab',"$CoefficientBounces",'-ad',"$CoefficientSamples",'-as','0','-aa','0','-ar','256','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
$coeff=@('-y',"$sensorCount",'-V-','-e',"MF:$CoefficientBasis",'-f','reinhart.cal','-b','rbin','-bn','Nrbins','-m','sky_glow','coefficients.oct')
Invoke-Engine 'rcontrib' ($shared+$coeff) 'coefficients.mtx' $sensors
$shared[[Array]::IndexOf($shared,'-ab')+1]='1'
Invoke-Engine 'rcontrib' ($shared+$coeff) 'direct-coefficients.mtx' $sensors
foreach($name in @('coefficients.mtx','direct-coefficients.mtx')) {
    $matrix=Read-Matrix $name
    if($matrix.rows -ne $sensorCount -or $matrix.columns -ne (144*$CoefficientBasis*$CoefficientBasis+2)){throw 'Unexpected coefficient shape'}
}
