#Requires -Version 7.2
# Independent projected half-hemisphere and repeated-primary ordering fixture.
param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 600
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
$options=@('-I+','-h','-faa','-n','1','-ab','1','-ad','8192','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-','-V+','-m','unit')
$repeat=64;$inputText=("0 0 0 0 0 1`n"*$repeat)+("0 0 0 0 1 0`n"*$repeat)
Write-Input 'rays.pts' $inputText
Write-Input 'scene.rad' "void glow unit`n0`n0`n4 100 50 25 0`nunit source east_half`n0`n0`n4 1 0 0 180`n"
[ordered]@{scope='Two ordered sensors, each with 64 repeated identical primary records, under a non-gray east hemisphere';declared_utc=[DateTime]::UtcNow.ToString('o');arguments=$options;repeat=$repeat;sensor_count=2;expected_rgb=@((100*[Math]::PI/2),(50*[Math]::PI/2),(25*[Math]::PI/2));analytical_relative_tolerance=.01;average_relative_tolerance=1e-6;harness_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();release_acceptance_complete=$false}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Invoke-Engine 'oconv' @('-f','scene.rad') 'scene.oct'
Invoke-Engine 'rcontrib' ($options+@('-y','2','-c','64','scene.oct')) 'averaged.rgb' $inputText
Invoke-Engine 'rcontrib' ($options+@('-y','128','-c','1','scene.oct')) 'individual.rgb' $inputText
function Values([string]$Name,[int]$Count){$v=@([IO.File]::ReadAllText((Join-Path $OutputDirectory $Name)).Trim() -split '\s+'|ForEach-Object{[double]::Parse($_,$culture)});if($v.Count -ne $Count -or @($v|Where-Object{![double]::IsFinite($_) -or $_ -lt 0}).Count){throw 'Invalid native output.'};return ,$v}
$averaged=Values 'averaged.rgb' 6;$individual=Values 'individual.rgb' 384;$rows=@()
foreach($sensor in 0,1){[double[]]$mean=@(0,0,0);$unique=[Collections.Generic.HashSet[string]]::new();foreach($record in 0..63){$offset=($sensor*64+$record)*3;for($channel=0;$channel -lt 3;$channel++){$mean[$channel]+=$individual[$offset+$channel]/64};$null=$unique.Add(($individual[$offset..($offset+2)] -join ','))};$maxAnalytical=0.;$maxAverage=0.;foreach($channel in 0..2){$expected=@(100,50,25)[$channel]*[Math]::PI/2;$maxAnalytical=[Math]::Max($maxAnalytical,[Math]::Abs($averaged[$sensor*3+$channel]-$expected)/$expected);$maxAverage=[Math]::Max($maxAverage,[Math]::Abs($averaged[$sensor*3+$channel]-$mean[$channel])/$mean[$channel])};$rows+=@{sensor_index=$sensor;native_average=@($averaged[($sensor*3)..($sensor*3+2)]);f64_average=$mean;unique_primary_results=$unique.Count;maximum_analytical_relative_error=$maxAnalytical;maximum_average_relative_error=$maxAverage;pass=($maxAnalytical -le .01 -and $maxAverage -le 1e-6 -and $unique.Count -gt 1)}}
[ordered]@{sensors=$rows;passed=(@($rows|Where-Object{!$_.pass}).Count -eq 0);raw_sha256=@{averaged=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'averaged.rgb')).Hash.ToLowerInvariant();individual=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'individual.rgb')).Hash.ToLowerInvariant()}}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
$rows|ConvertTo-Json -Depth 5
if(@($rows|Where-Object{!$_.pass}).Count){throw 'Analytical, ordering or nonidentical-primary check failed; evidence retained.'}
