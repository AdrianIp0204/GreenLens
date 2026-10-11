#Requires -Version 7.2
# Reproduce the retained high-ad failures. Exit zero means the investigation ran,
# not that its scientific checks passed: inspect each explicit pass field.
param([Parameter(Mandatory)][string]$OutputDirectory,[ValidateSet('rtrace','rcontrib')][string]$Program='rtrace')
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a new retained evidence directory.'}
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 600
$counts=@(262144,1048576,4194304,8388608)
Add-Type -TypeDefinition @'
public static class GreenLensFloatAccumulation {
    public static double[] Reproduce(int ad) {
        int n = (int)(System.Math.Sqrt(ad) + 0.5);
        int count = n * n;
        float term = (float)((float)System.Math.PI / (double)count);
        float sum = 0;
        for (int i = 0; i < count; i++) sum += term;
        return new double[] { count, term, sum };
    }
}
'@
[ordered]@{scope='Independent uniform hemisphere diagnostic of high-ad arithmetic and contribution cutoff; not release acceptance';declared_utc=[DateTime]::UtcNow.ToString('o');program=$Program;sample_counts=$counts;expected_rgb_irradiance=@([Math]::PI,[Math]::PI,[Math]::PI);relative_tolerance=.01;script_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
Write-Input 'unit.rad' "void glow unit`n0`n0`n4 1 1 1 0`nunit source sky`n0`n0`n4 0 0 1 180`n"
Invoke-Engine 'oconv' @('-f','unit.rad') 'unit.oct'
$results=[Collections.Generic.List[object]]::new()
foreach($division in $counts){
 $file="unit-$division.rgb";$options=@('-I+','-h','-faa','-n','1','-ab','1','-ad',"$division",'-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
 if($Program -eq 'rtrace'){$options+=@('-ov')}else{$options+=@('-y','1','-V+','-m','unit')}
 Invoke-Engine $Program ($options+@('unit.oct')) $file "0 0 0 0 0 1`n"
 $rgb=@([IO.File]::ReadAllText((Join-Path $OutputDirectory $file)).Trim() -split '\s+'|ForEach-Object{[double]::Parse($_,$culture)})
 if($rgb.Count -ne 3 -or @($rgb|Where-Object{![double]::IsFinite($_) -or $_ -lt 0}).Count){throw 'Malformed native output.'}
 $error=[Math]::Abs($rgb[0]-[Math]::PI)/[Math]::PI
 $results.Add([ordered]@{samples=$division;rgb=$rgb;relative_error=$error;pass=($error -le .01);independent_float_accumulation=([GreenLensFloatAccumulation]::Reproduce($division));raw_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory $file)).Hash.ToLowerInvariant()})
 $results|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8
 Write-Host "$Program uniform ad$division relative error $error"
}
