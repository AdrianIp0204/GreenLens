#Requires -Version 7.2
param([string]$OutputDirectory='')
$ErrorActionPreference='Stop'
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/Hammersley stream regression '+[Guid]::NewGuid().ToString('N')+' '+[char]0x7a7a+[char]0x9593)}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Choose a new retained directory'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
Add-Type -LiteralPath "$PSScriptRoot/annual-paired-primary-stream.cs"
$checks=[Collections.Generic.List[object]]::new()
$sensorText="1 2 0.8 0 0 1`n3 4 0.9 0 0 1`n"
$producer=[GreenLensPairedStream.HammersleyProducer]::new($sensorText,16384)
$path=Join-Path $OutputDirectory 'primary-rays.pts'
$stream=[IO.File]::Create($path)
try{$receipt=$producer.GenerateAsync($stream,[Threading.CancellationToken]::None).GetAwaiter().GetResult()}finally{$stream.Dispose()}
if($receipt.Rays -ne 32768 -or $receipt.Bytes -ne (Get-Item -LiteralPath $path).Length -or $receipt.Sha256 -ne (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()){throw 'Generated receipt differs from actual complete bytes'}
$checks.Add(@{name='complete two-sensor stream hash and count';passed=$true;receipt=$receipt})
$expectedInverse=@(0,.5,.25,.75,.125,.625,.375,.875)
for($i=0;$i -lt 8;$i++){if([GreenLensPairedStream.HammersleyProducer]::RadicalInverse($i) -ne $expectedInverse[$i]){throw 'Known base2 radical inverse differs'}}
$checks.Add(@{name='known independent base2 sequence';passed=$true})
$culture=[Globalization.CultureInfo]::InvariantCulture
$index=0;$maxNormError=0.;$directions=@();$lastSensor0=$null;$firstSensor1=$null
foreach($line in [IO.File]::ReadLines($path)){
 $tokens=$line.Split(' ');if($tokens.Count -ne 6){throw 'Native ASCII row shape differs'}
 $v=@($tokens|ForEach-Object{[double]::Parse($_,$culture)})
 if(@($v|Where-Object{![double]::IsFinite($_)}).Count){throw 'Nonfinite generated value'}
 $sensor=[int][Math]::Floor($index/16384);$sample=$index%16384
 if($v[0] -ne 1+2*$sensor -or $v[1] -ne 2+2*$sensor -or [Math]::Abs($v[2]-(.8001+.1*$sensor)) -gt 1e-15){throw 'Sensor-major origin/order differs'}
 $norm=$v[3]*$v[3]+$v[4]*$v[4]+$v[5]*$v[5]
 $maxNormError=[Math]::Max($maxNormError,[Math]::Abs($norm-1))
 if($v[5] -le 0 -or [Math]::Abs(($v[3]*$v[3]+$v[4]*$v[4])-($sample+.5)/16384) -gt 1e-14){throw 'Cosine radial distribution differs'}
 if($index -lt 8){$directions+=,@($v[3..5])}
 if($index -eq 16383){$lastSensor0=$line}
 if($index -eq 16384){$firstSensor1=$line}
 $index++
}
if($index -ne 32768 -or $maxNormError -gt 1e-14){throw 'Direction norm/count differs'}
for($i=0;$i -lt 8;$i++){
 $r=[Math]::Sqrt(($i+.5)/16384);$phi=2*[Math]::PI*$expectedInverse[$i]
 if([Math]::Abs($directions[$i][0]-$r*[Math]::Cos($phi)) -gt 1e-15 -or [Math]::Abs($directions[$i][1]-$r*[Math]::Sin($phi)) -gt 1e-15){throw 'Known Hammersley angular order differs'}
}
$checks.Add(@{name='all finite ASCII directions, cosine radial mapping and sensor-major order';passed=$true;maximum_norm_error=$maxNormError;last_first_sensor=$lastSensor0;first_second_sensor=$firstSensor1})
$bytes=[IO.File]::ReadAllBytes($path)
if($bytes[-1] -ne 10 -or ([Text.Encoding]::Latin1.GetString($bytes) -match '[\r\x80-\xff]')){throw 'Expected BOM-free ASCII/LF bytes'}
$checks.Add(@{name='BOM-free ASCII and LF terminators';passed=$true})
$cancel=[Threading.CancellationTokenSource]::new();$cancel.Cancel();$cancelStream=[IO.MemoryStream]::new();$rejected=$false
try{$null=$producer.GenerateAsync($cancelStream,$cancel.Token).GetAwaiter().GetResult()}catch{$rejected=$true}
if(!$rejected -or $cancelStream.Length){throw 'Precancelled producer wrote or advertised success'}
$cancelStream.Dispose();$cancel.Dispose();$checks.Add(@{name='pre-cancelled generation writes no partial success';passed=$true})
foreach($case in @(@{name='unsupported count';text=$sensorText;count=32768},@{name='missing newline';text=$sensorText.Trim();count=16384},@{name='nonfinite';text="NaN 0 0 0 0 1`n";count=16384},@{name='wrong normal';text="0 0 0 0 1 0`n";count=16384})){
 $rejected=$false;try{$null=[GreenLensPairedStream.HammersleyProducer]::new($case.text,$case.count)}catch{$rejected=$true}
 if(!$rejected){throw "Expected rejection: $($case.name)"};$checks.Add(@{name=$case.name;passed=$true})
}
@{schema_version='greenlens.hammersley-stream-regression.v1';passed=$true;completed_utc=[DateTime]::UtcNow.ToString('o');checks=$checks.ToArray();producer_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-paired-primary-stream.cs").Hash.ToLowerInvariant();test_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();scope='Generation/serialization tests only; no Radiance or annual accuracy acceptance'}|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8NoBOM
Write-Host "Nine Hammersley stream checks passed; retained $OutputDirectory"
