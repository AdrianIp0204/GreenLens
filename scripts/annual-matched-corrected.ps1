#Requires -Version 7.2
# Match actual v2/v3 adapter output to independently traced identical patch skies
# and continuous solar sources. Input is trusted local adapter scratch, never
# an imported command/scene. Failed hours and all sensor values are retained.
param(
    [Parameter(Mandatory)][string]$CandidateDirectory,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [int]$ReferenceSamples=262144,
    [int]$ReferenceBounces=9,
    [string]$ReferenceDirectory='',
    [int[]]$IntervalIndices=@()
)
$ErrorActionPreference='Stop'
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Use a new matched investigation output directory to preserve earlier evidence.' }
. "$PSScriptRoot/annual-matched-evidence.ps1"
$harnessSource=[IO.File]::ReadAllText($PSCommandPath)
$kernelHash=Get-MatchedKernelHash $harnessSource
. "$PSScriptRoot/annual-feasibility.ps1" -OutputDirectory $OutputDirectory -FunctionsOnly -ProcessTimeoutSeconds 7200
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $OutputDirectory 'harness-source.ps1')
@{harness_sha256=(Get-MatchedHash (Join-Path $OutputDirectory 'harness-source.ps1'));numerical_kernel_sha256=$kernelHash;retained_before_execution=$true}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $OutputDirectory 'harness-source.json') -Encoding utf8
$recipe=Get-Content -LiteralPath (Join-Path $CandidateDirectory 'annual-recipe.json') -Raw|ConvertFrom-Json
$methodTuple=Get-MatchedMethodTuple $recipe
if($recipe.interval_indices.Count -ne 8760 -or ($recipe.interval_indices -join ',') -ne ((0..8759) -join ',')){throw 'This harness requires complete annual output with all 8760 ordered intervals'}
$sensorCount=[int]$recipe.sensor_count
if($sensorCount -lt 1 -or $sensorCount -gt 256){throw 'Invalid sensor count'}
$mf=[int]$recipe.profile.sky_basis_mf
$inputSensors=[IO.File]::ReadAllText((Join-Path $CandidateDirectory 'annual-sensors.pts'))
$scene=[IO.File]::ReadAllText((Join-Path $CandidateDirectory 'annual-scene.rad'))
if($scene -match '(?m)^\s*!'){throw 'Shell scene directives are forbidden'}
Write-Input 'scene.rad' $scene
Write-Input 'sensors.pts' $inputSensors
Copy-Item -LiteralPath (Join-Path $CandidateDirectory 'annual-weather.epw') -Destination (Join-Path $OutputDirectory 'weather.epw')
Copy-Item -LiteralPath (Join-Path $CandidateDirectory 'annual-sky.mtx') -Destination (Join-Path $OutputDirectory 'identical-sky.mtx')
Copy-Item -LiteralPath (Join-Path $CandidateDirectory 'annual-suns.rad') -Destination (Join-Path $OutputDirectory 'suns.rad')
$candidateFile=Join-Path $CandidateDirectory 'annual-combined.f64'
if((Get-Item -LiteralPath $candidateFile).Length -ne 8760*$sensorCount*3*8){throw 'Truncated or excess candidate RGB bytes'}
$inventory=Get-Content -LiteralPath "$PSScriptRoot/../dependencies/radiance.json" -Raw|ConvertFrom-Json
foreach($name in @('rtrace','oconv')){if((Get-FileHash (Join-Path $EngineRoot "bin/$name.exe")).Hash.ToLowerInvariant() -ne $inventory.executables.$name.sha256){throw "Pinned executable mismatch: $name"}}
foreach($name in @('rayinit.cal','skybright.cal')){if((Get-FileHash (Join-Path $EngineRoot "lib/$name")).Hash.ToLowerInvariant() -ne $inventory.critical_libraries.$name.sha256){throw "Pinned library mismatch: $name"}}
if((Get-FileHash (Join-Path $EngineRoot 'lib/reinhart.cal')).Hash.ToLowerInvariant() -ne $inventory.annual_workflow_review.additional_critical_libraries.'reinhart.cal'.sha256){throw 'Pinned basis mismatch'}
$sky=Read-Matrix 'identical-sky.mtx'
if($sky.columns -ne 8760 -or $sky.rows -ne (144*$mf*$mf+2)){throw 'Candidate sky dimensions differ from recipe'}
$plan=Get-Content -LiteralPath "$PSScriptRoot/annual-validation-plan.json" -Raw|ConvertFrom-Json
$weatherRows=@([IO.File]::ReadAllLines((Join-Path $OutputDirectory 'weather.epw'))|Select-Object -Skip 8)
function Condition([int]$Index){
    $fields=$weatherRows[$Index].Split(',');$dni=[double]::Parse($fields[14],$culture);$dhi=[double]::Parse($fields[15],$culture)
    if($dhi -lt 50){return 'low_diffuse'};if($dni/$dhi -ge 3){return 'clear'};if($dni/$dhi -ge .5){return 'mixed'};return 'overcast'
}
$indices=[Collections.Generic.List[int]]::new()
foreach($interval in $plan.fixed_intervals){$date=[DateTime]::new(2001,$interval.month,$interval.day);$indices.Add(($date.DayOfYear-1)*24+$interval.hour_ending-1)}
$classes=@($indices|ForEach-Object{Condition $_})
foreach($class in @('clear','mixed','overcast')){if($classes -notcontains $class){for($index=0;$index -lt 8760;$index++){if((Condition $index) -eq $class -and $index%24 -ge 8 -and $index%24 -le 16){$indices.Add($index);break}}}}
$declaredIndices=@($indices)
$indices=Select-MatchedIntervals $declaredIndices $IntervalIndices
$sunByIndex=@{}
foreach($match in [regex]::Matches([IO.File]::ReadAllText((Join-Path $OutputDirectory 'suns.rad')),'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')){$sunByIndex[[int](([int]$match.Groups[1].Value-30)/60)]=$match.Value}
function Hash([string]$Path){return (Get-FileHash -LiteralPath $Path).Hash.ToLowerInvariant()}
$referenceIdentity=[ordered]@{scene_sha256=(Hash (Join-Path $OutputDirectory 'scene.rad'));sensor_sha256=(Hash (Join-Path $OutputDirectory 'sensors.pts'));sky_sha256=(Hash (Join-Path $OutputDirectory 'identical-sky.mtx'));sun_sha256=(Hash (Join-Path $OutputDirectory 'suns.rad'));engine=$recipe.engine;ab=$ReferenceBounces;ad=$ReferenceSamples;lw=1e-9;lr=16;finite_samples=4096;mf=$mf;interval_indices=$declaredIndices}
if($ReferenceDirectory){
    $cached=Get-Content -LiteralPath (Join-Path $ReferenceDirectory 'declaration.json') -Raw|ConvertFrom-Json
    Assert-MatchedReference $cached
    Assert-MatchedPhysicalReference $referenceIdentity $cached.reference
    Assert-MatchedSource $ReferenceDirectory $kernelHash
    foreach($pair in @(@('scene.rad','scene_sha256'),@('sensors.pts','sensor_sha256'),@('identical-sky.mtx','sky_sha256'),@('suns.rad','sun_sha256'))){if((Hash (Join-Path $ReferenceDirectory $pair[0])) -cne $cached.reference.($pair[1])){throw 'Cached physical input bytes changed'}}
    # Preserve the old reference's actual identity; bind the new candidate below.
    $referenceIdentity=$cached.reference
}
$identityText=$referenceIdentity|ConvertTo-Json -Depth 8 -Compress
$identityHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($utf8.GetBytes($identityText))).ToLowerInvariant()
$declaration=[ordered]@{schema_version='greenlens.corrected-matched-investigation.v1';declared_utc=[DateTime]::UtcNow.ToString('o');candidate_recipe_sha256=(Hash (Join-Path $CandidateDirectory 'annual-recipe.json'));candidate_rgb_sha256=(Hash $candidateFile);candidate_profile=$recipe.profile;reference=$referenceIdentity;reference_identity_sha256=$identityHash;reference_policy='Identical generated diffuse/ground patch sky plus actual continuous solar source. High-quality native full trace minus native centroid direct trace plus independent all-ray 4096-direction solid-angle quadrature. Reflected solar in this reference remains continuous centroid-sampled; direct finite shadows are separately checked analytically.';tolerance='max(5.0 lx, 5% of reference) at every sensor';shadow_fixture='Separate elevated-radiance eleven-point finite-disc fixture and native all-ray/culling equality regression; not replaced by these weather hours';results='not executed at declaration'}
$declaration.numerical_kernel_sha256=$kernelHash
$declaration.candidate_method_tuple=$methodTuple
$declaration.reference_reuse_policy='Exact physical input bytes, executable/library identities, ordered interval plan and numerical kernel/settings must match. A retained reference keeps its original method provenance; candidate recipe and final RGB identities are separate.'
$declaration.executed_interval_indices=@($indices)
$declaration.shard=($indices.Count -ne $declaredIndices.Count)
$declaration|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'declaration.json') -Encoding utf8
if($ReferenceDirectory){
    $cached=Get-Content -LiteralPath (Join-Path $ReferenceDirectory 'declaration.json') -Raw|ConvertFrom-Json
    Assert-MatchedReference $cached
    if($cached.reference_identity_sha256 -ne $identityHash){throw 'Reference cache identity mismatch'}
    Assert-MatchedSource $ReferenceDirectory $kernelHash
    $cachedReport=Get-Content -LiteralPath (Join-Path $ReferenceDirectory 'matched-corrected.json') -Raw|ConvertFrom-Json
    Assert-MatchedReference $cachedReport.declaration
    if($cachedReport.declaration.reference_identity_sha256 -ne $identityHash){throw 'Cached report identity mismatch'}
}
Write-Input 'matched-basis.cal' ("MF:$mf;`n"+[IO.File]::ReadAllText((Join-Path $EngineRoot 'lib/reinhart.cal')))
Write-Input 'receiver.rad' "void glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"
Invoke-Engine 'oconv' @('-f','scene.rad','receiver.rad') 'transmission.oct'
if(-not ('GreenLensMatched.Direct' -as [type])){
Add-Type -TypeDefinition @'
using System; using System.IO; using System.Text; using System.Globalization; using System.Collections.Generic;
namespace GreenLensMatched {
public static class Direct {
static readonly CultureInfo C=CultureInfo.InvariantCulture;
static double[] Values(string s) { var t=s.Split((char[])null,StringSplitOptions.RemoveEmptyEntries);var v=new double[t.Length];for(int i=0;i<t.Length;i++){v[i]=double.Parse(t[i],C);if(!double.IsFinite(v[i]))throw new InvalidDataException("Nonfinite ray value");}return v; }
static double[][] Sensors(string text){var l=text.Trim().Split('\n');var s=new double[l.Length][];for(int i=0;i<l.Length;i++){s[i]=Values(l[i]);if(s[i].Length!=6)throw new InvalidDataException("Invalid sensor");}return s;}
static double[][] Directions(string text,out double[] radiance,out double omega){var t=text.Split((char[])null,StringSplitOptions.RemoveEmptyEntries);if(t.Length!=19)throw new InvalidDataException("Expected one generated solar source");radiance=new[]{double.Parse(t[6],C),double.Parse(t[7],C),double.Parse(t[8],C)};double[] w={double.Parse(t[15],C),double.Parse(t[16],C),double.Parse(t[17],C)};double n=Math.Sqrt(w[0]*w[0]+w[1]*w[1]+w[2]*w[2]);for(int i=0;i<3;i++)w[i]/=n;double[] u={w[1],-w[0],0};n=Math.Sqrt(u[0]*u[0]+u[1]*u[1]);if(n<1e-9)u=new[]{0d,1d,0d};else for(int i=0;i<3;i++)u[i]/=n;double[] v={w[1]*u[2]-w[2]*u[1],w[2]*u[0]-w[0]*u[2],w[0]*u[1]-w[1]*u[0]};double radius=double.Parse(t[18],C)*Math.PI/360;omega=2*Math.PI*(1-Math.Cos(radius));var rays=new double[4096][];for(int ring=0;ring<64;ring++){double mu=1-(1-Math.Cos(radius))*(ring+.5)/64,r=Math.Sqrt(1-mu*mu);for(int az=0;az<64;az++){double phi=2*Math.PI*(az+.5)/64;var d=new double[3];for(int k=0;k<3;k++)d[k]=mu*w[k]+r*(Math.Cos(phi)*u[k]+Math.Sin(phi)*v[k]);rays[ring*64+az]=d;}}return rays;}
public static string Rays(string sensors,string sun){var s=Sensors(sensors);var d=Directions(sun,out _,out _);var b=new StringBuilder();foreach(var p in s)foreach(var ray in d){for(int k=0;k<3;k++)b.Append((p[k]+.0001*p[k+3]).ToString("R",C)).Append(' ');for(int k=0;k<3;k++)b.Append(ray[k].ToString("R",C)).Append(k==2?'\n':' ');}return b.ToString();}
public static double[][] Finite(string sensors,string sun,string output){var s=Sensors(sensors);var rays=Directions(sun,out var source,out double omega);var values=Values(File.ReadAllText(output));if(values.Length!=s.Length*4096*3)throw new InvalidDataException("Finite output count");var result=new double[s.Length][];for(int i=0;i<s.Length;i++){result[i]=new double[3];for(int j=0;j<4096;j++){var d=rays[j];double cosine=Math.Max(0,s[i][3]*d[0]+s[i][4]*d[1]+s[i][5]*d[2]);if(d[2]<=0)cosine=0;for(int k=0;k<3;k++){double t=values[(i*4096+j)*3+k];if(t<0||t>1.000001)throw new InvalidDataException("Invalid directional transmission");result[i][k]+=t*source[k]*cosine*omega/4096;}}}return result;}
public static double[][] Rgb(string file,int sensors){var v=Values(File.ReadAllText(file));if(v.Length!=sensors*3)throw new InvalidDataException("RGB count");var r=new double[sensors][];for(int i=0;i<sensors;i++){r[i]=new[]{v[i*3],v[i*3+1],v[i*3+2]};foreach(double x in r[i])if(x<0)throw new InvalidDataException("Negative RGB");}return r;}
public static double[][] Candidate(string file,int sensors,int hour){using var b=new BinaryReader(File.OpenRead(file));b.BaseStream.Seek((long)hour*sensors*24,SeekOrigin.Begin);var r=new double[sensors][];for(int i=0;i<sensors;i++){r[i]=new[]{b.ReadDouble(),b.ReadDouble(),b.ReadDouble()};foreach(double x in r[i])if(!double.IsFinite(x)||x<0)throw new InvalidDataException("Invalid candidate RGB");}return r;}
public static double Lux(double[] rgb){foreach(double x in rgb)if(!double.IsFinite(x)||x<0)throw new InvalidDataException("Invalid corrected reference RGB");return 179*(.265074126*rgb[0]+.670114631*rgb[1]+.064811243*rgb[2]);}
}}
'@
}
$checks=[Collections.Generic.List[object]]::new()
$rayOptions=@('-h','-faa','-ov','-n','1','-ab','0','-ad','1','-as','0','-aa','0','-av','0','0','0','-lw','1e-9','-lr','16','-dt','0','-dc','1','-dj','0','-ds','0.05','-st','0','-ss','0','-u-')
foreach($index in $indices){
    if(!$sunByIndex.ContainsKey($index)){throw "Predeclared interval $index has no emitted source; investigate without dropping it"}
    $name="hour-$index";$sun=$sunByIndex[$index];Write-Input "$name-sun.rad" $sun
    $data=[Text.StringBuilder]::new("1`n0 $($sky.rows-1) $($sky.rows)`n")
    for($patch=0;$patch -lt $sky.rows;$patch++){$offset=($patch*8760+$index)*3;$rgb=$sky.values[$offset..($offset+2)];if($rgb[0] -ne $rgb[1] -or $rgb[0] -ne $rgb[2]){throw 'Expected declared neutral sky; do not discard channel differences'};$null=$data.Append($rgb[0].ToString('R',$culture)).Append("`n")}
    Write-Input "$name.dat" $data.ToString()
    Write-Input "$name.rad" ("void brightdata distribution`n4 noop $name.dat matched-basis.cal rbin`n0`n0`ndistribution glow sky_glow`n0`n0`n4 1 1 1 0`nsky_glow source dome`n0`n0`n4 0 0 1 360`n"+$sun)
    if($ReferenceDirectory){
        if((Hash (Join-Path $ReferenceDirectory "$name.rad")) -ne (Hash (Join-Path $OutputDirectory "$name.rad"))){throw 'Cached sky differs'}
        $cachedChecks=@($cachedReport.checks|Where-Object{$_.interval_index -eq $index})
        if($cachedChecks.Count -ne 1){throw 'Cached interval is missing or duplicated'}
        Assert-MatchedRaw $ReferenceDirectory $cachedChecks[0]
        foreach($suffix in @('reference.rgb','direct.rgb','finite.rgb')){Copy-Item -LiteralPath (Join-Path $ReferenceDirectory "$name-$suffix") -Destination (Join-Path $OutputDirectory "$name-$suffix")}
    }else{
        Invoke-Engine 'oconv' @('-f','scene.rad',"$name.rad") "$name.oct"
        $full=@($rayOptions);$full[[Array]::IndexOf($full,'-ab')+1]="$ReferenceBounces";$full[[Array]::IndexOf($full,'-ad')+1]="$ReferenceSamples"
        Invoke-Engine 'rtrace' (@('-I+')+$full+@("$name.oct")) "$name-reference.rgb" $inputSensors
        Invoke-Engine 'oconv' @('-f','scene.rad',"$name-sun.rad") "$name-sun.oct"
        Invoke-Engine 'rtrace' (@('-I+')+$rayOptions+@("$name-sun.oct")) "$name-direct.rgb" $inputSensors
        Invoke-Engine 'rtrace' ($rayOptions+@('transmission.oct')) "$name-finite.rgb" ([GreenLensMatched.Direct]::Rays($inputSensors,$sun))
    }
    $fullRgb=[GreenLensMatched.Direct]::Rgb((Join-Path $OutputDirectory "$name-reference.rgb"),$sensorCount)
    $directRgb=[GreenLensMatched.Direct]::Rgb((Join-Path $OutputDirectory "$name-direct.rgb"),$sensorCount)
    $finiteRgb=[GreenLensMatched.Direct]::Finite($inputSensors,$sun,(Join-Path $OutputDirectory "$name-finite.rgb"))
    $candidateRgb=[GreenLensMatched.Direct]::Candidate($candidateFile,$sensorCount,$index)
    $values=[Collections.Generic.List[object]]::new();$failures=0;$maxRatio=0.0
    for($sensor=0;$sensor -lt $sensorCount;$sensor++){
        $referenceRgb=[double[]]::new(3);for($c=0;$c -lt 3;$c++){$referenceRgb[$c]=$fullRgb[$sensor][$c]-$directRgb[$sensor][$c]+$finiteRgb[$sensor][$c]}
        $reference=[GreenLensMatched.Direct]::Lux($referenceRgb);$candidate=[GreenLensMatched.Direct]::Lux($candidateRgb[$sensor]);$error=[Math]::Abs($candidate-$reference);$allowed=[Math]::Max(5.0,.05*$reference);$pass=$error -le $allowed
        if(!$pass){$failures++};$maxRatio=[Math]::Max($maxRatio,$error/$allowed)
        $values.Add(@{sensor_index=$sensor;candidate_lux=$candidate;reference_lux=$reference;absolute_error_lux=$error;allowed_error_lux=$allowed;passed=$pass;reference_centroid_direct_lux=[GreenLensMatched.Direct]::Lux($directRgb[$sensor]);reference_finite_direct_lux=[GreenLensMatched.Direct]::Lux($finiteRgb[$sensor])})
    }
    $checks.Add(@{interval_index=$index;condition=(Condition $index);pass=($failures -eq 0);failure_count=$failures;maximum_error_over_tolerance=$maxRatio;sensors=$values;reference_sha256=(Hash (Join-Path $OutputDirectory "$name-reference.rgb"));direct_sha256=(Hash (Join-Path $OutputDirectory "$name-direct.rgb"));finite_sha256=(Hash (Join-Path $OutputDirectory "$name-finite.rgb"));reference_reused=[bool]$ReferenceDirectory})
    @{declaration=$declaration;checks=$checks;completed=($checks.Count -eq $indices.Count);pass=($checks.Count -eq $indices.Count -and @($checks|Where-Object{!$_.pass}).Count -eq 0)}|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'matched-corrected.json') -Encoding utf8
    Write-Host "CORRECTED MATCHED interval ${index}: $failures failures; maximum error/tolerance $maxRatio"
}
if(@($checks|Where-Object{!$_.pass}).Count){throw 'Required matched-sky tolerance failed; every sensor and failure is retained in matched-corrected.json.'}
