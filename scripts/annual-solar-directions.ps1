#Requires -Version 7.2
# Original synthetic engine research. These short EPWs are not application imports
# or annual weather datasets. No Radiance formula is used by the NOAA comparator.
param(
    [string]$SolarEngineRoot = (Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2/package'),
    [string]$SolarOutputDirectory = (Join-Path $env:LOCALAPPDATA ("GreenLens/verification/v0.3 solar directions 空間/" + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')))
)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $SolarOutputDirectory) {
    if (@(Get-ChildItem -LiteralPath $SolarOutputDirectory -Force).Count -ne 0) {
        throw 'Use a new empty evidence directory; previous attempts must be retained.'
    }
}
. "$PSScriptRoot/annual-feasibility.ps1" -EngineRoot $SolarEngineRoot -OutputDirectory $SolarOutputDirectory -FunctionsOnly
$solarScriptHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
$solarDependency = Get-Content -LiteralPath "$PSScriptRoot/../dependencies/radiance.json" -Raw | ConvertFrom-Json
$solarExecutableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $EngineRoot 'bin/gendaymtx.exe')).Hash.ToLowerInvariant()
if ($solarExecutableHash -ne $solarDependency.annual_workflow_review.executables.gendaymtx.sha256) {
    throw 'gendaymtx does not match the reviewed pinned engine.'
}
$versionExecutableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $EngineRoot 'bin/rcontrib.exe')).Hash.ToLowerInvariant()
if ($versionExecutableHash -ne $solarDependency.annual_workflow_review.executables.rcontrib.sha256) {
    throw 'Version-reporting executable does not match the pinned engine.'
}

function Solar-Reference([double]$Latitude, [double]$Longitude, [double]$UtcOffset, [int]$Month, [int]$Day, [int]$HourEnding) {
    # NOAA's independent low-order fractional-year approximation, east-positive
    # longitude and UTC offset. ENU components avoid azimuth quadrant ambiguity.
    $ordinal = ([DateTime]::new(2001,$Month,$Day)).DayOfYear
    $hourMidpoint = $HourEnding - 0.5
    $gamma = 2 * [Math]::PI / 365 * ($ordinal - 1 + ($hourMidpoint - 12) / 24)
    $equationMinutes = 229.18 * (0.000075 + 0.001868*[Math]::Cos($gamma) - 0.032077*[Math]::Sin($gamma) - 0.014615*[Math]::Cos(2*$gamma) - 0.040849*[Math]::Sin(2*$gamma))
    $declination = 0.006918 - 0.399912*[Math]::Cos($gamma) + 0.070257*[Math]::Sin($gamma) - 0.006758*[Math]::Cos(2*$gamma) + 0.000907*[Math]::Sin(2*$gamma) - 0.002697*[Math]::Cos(3*$gamma) + 0.00148*[Math]::Sin(3*$gamma)
    $hourAngle = (($hourMidpoint*60 + $equationMinutes + 4*$Longitude - 60*$UtcOffset) / 4 - 180) * [Math]::PI / 180
    $latitudeRadians = $Latitude * [Math]::PI / 180
    $enu = @(
        (-[Math]::Cos($declination)*[Math]::Sin($hourAngle)),
        ([Math]::Cos($latitudeRadians)*[Math]::Sin($declination) - [Math]::Sin($latitudeRadians)*[Math]::Cos($declination)*[Math]::Cos($hourAngle)),
        ([Math]::Sin($latitudeRadians)*[Math]::Sin($declination) + [Math]::Cos($latitudeRadians)*[Math]::Cos($declination)*[Math]::Cos($hourAngle))
    )
    return [ordered]@{direction_enu=$enu; altitude_degrees=[Math]::Asin($enu[2])*180/[Math]::PI; equation_of_time_minutes=$equationMinutes; declination_degrees=$declination*180/[Math]::PI; representative_local_hour=$hourMidpoint; expected_solar_minute=($ordinal-1)*1440+[int]($hourMidpoint*60)}
}
function Solar-Angle([double[]]$First, [double[]]$Second) {
    $cross = @(($First[1]*$Second[2]-$First[2]*$Second[1]), ($First[2]*$Second[0]-$First[0]*$Second[2]), ($First[0]*$Second[1]-$First[1]*$Second[0]))
    $crossLength = [Math]::Sqrt($cross[0]*$cross[0]+$cross[1]*$cross[1]+$cross[2]*$cross[2])
    $dot = $First[0]*$Second[0]+$First[1]*$Second[1]+$First[2]*$Second[2]
    return [Math]::Atan2($crossLength,$dot)*180/[Math]::PI
}
function Solar-Fixture([string]$Id, [double]$Latitude, [double]$Longitude, [double]$UtcOffset, [int]$Month, [int]$Day, [int[]]$Hours, [int[]]$NextDayHours=@()) {
    $testCases = [Collections.Generic.List[object]]::new()
    $firstDate = [DateTime]::new(2001,$Month,$Day)
    foreach ($hourEnding in $Hours) { $testCases.Add(@{month=$Month;day=$Day;hour_ending=$hourEnding}) }
    foreach ($hourEnding in $NextDayHours) { $next=$firstDate.AddDays(1); $testCases.Add(@{month=$next.Month;day=$next.Day;hour_ending=$hourEnding}) }
    return [ordered]@{id=$Id;latitude_deg_north=$Latitude;longitude_deg_east=$Longitude;utc_offset_hours=$UtcOffset;first_month=$Month;first_day=$Day;days=$(if($NextDayHours.Count){2}else{1});cases=$testCases.ToArray()}
}
$solarFixtures = @(
    (Solar-Fixture 'equator-greenwich' 0 0 0 3 21 @(1,6,7,8,9,10,12,13,14,16,18,19,24)),
    (Solar-Fixture 'longitude-east15' 0 15 0 3 21 @(9,13)),
    (Solar-Fixture 'longitude-west15' 0 -15 0 3 21 @(9,13)),
    (Solar-Fixture 'matched-east-zone' 0 30 2 3 21 @(9,13)),
    (Solar-Fixture 'matched-west-zone' 0 -75 -5 3 21 @(9,13)),
    (Solar-Fixture 'clock-plus-one-zone' 0 0 1 3 21 @(10,14)),
    (Solar-Fixture 'half-hour-zone' 0 82.5 5.5 3 21 @(9,13)),
    (Solar-Fixture 'artificial-hour24-east180' 0 180 0 3 21 @(23,24) @(1,2)),
    (Solar-Fixture 'artificial-hour24-west180' 0 -180 0 3 21 @(23,24) @(1,2)),
    (Solar-Fixture 'north-summer' 35 0 0 6 21 @(9,13,17)),
    (Solar-Fixture 'south-summer' -35 0 0 12 21 @(9,13,17))
)
$solarPairs = @(
    @('longitude-east15-0321-h09','equator-greenwich-0321-h10'),
    @('longitude-east15-0321-h13','equator-greenwich-0321-h14'),
    @('longitude-west15-0321-h09','equator-greenwich-0321-h08'),
    @('longitude-west15-0321-h13','equator-greenwich-0321-h12'),
    @('matched-east-zone-0321-h09','equator-greenwich-0321-h09'),
    @('matched-east-zone-0321-h13','equator-greenwich-0321-h13'),
    @('matched-west-zone-0321-h09','equator-greenwich-0321-h09'),
    @('matched-west-zone-0321-h13','equator-greenwich-0321-h13'),
    @('clock-plus-one-zone-0321-h10','equator-greenwich-0321-h09'),
    @('clock-plus-one-zone-0321-h14','equator-greenwich-0321-h13'),
    @('half-hour-zone-0321-h09','equator-greenwich-0321-h09'),
    @('half-hour-zone-0321-h13','equator-greenwich-0321-h13'),
    @('artificial-hour24-east180-0321-h24','artificial-hour24-west180-0321-h24'),
    @('artificial-hour24-east180-0322-h01','artificial-hour24-west180-0322-h01')
)
$solarDeclaredCases = [Collections.Generic.List[object]]::new()
foreach ($fixture in $solarFixtures) {
    foreach ($case in $fixture.cases) {
        $reference = Solar-Reference $fixture.latitude_deg_north $fixture.longitude_deg_east $fixture.utc_offset_hours $case.month $case.day $case.hour_ending
        if ([Math]::Abs($reference.altitude_degrees) -lt 3) { throw 'Declared case is too close to the horizon for the independent approximate comparison.' }
        $solarDeclaredCases.Add([ordered]@{id=('{0}-{1:00}{2:00}-h{3:00}' -f $fixture.id,$case.month,$case.day,$case.hour_ending);fixture=$fixture.id;month=$case.month;day=$case.day;hour_ending=$case.hour_ending;reference=$reference;expected_source_present=($reference.altitude_degrees -gt 0)})
    }
}
$solarDeclaration = [ordered]@{
    schema_version='greenlens.solar-direction-verification.v1';declared_utc=[DateTime]::UtcNow.ToString('o');status='Declared before any engine execution';
    purpose='Independent EPW-to-solar-direction/time/sign fixture; no illuminance or full-year accuracy claim';
    engine_tag=$solarDependency.tag;engine_commit=$solarDependency.commit;gendaymtx_sha256=$solarExecutableHash;script_sha256=$solarScriptHash;
    reference_source='https://gml.noaa.gov/grad/solcalc/solareqns.PDF';
    engine_sources=@('https://raw.githubusercontent.com/LBNL-ETA/Radiance/rad6R0P2/src/gen/gendaymtx.c','https://raw.githubusercontent.com/LBNL-ETA/Radiance/rad6R0P2/src/gen/sun.c');
    independent_vector_angle_limit_degrees=1.0;equivalent_offset_angle_limit_degrees=0.0002;printed_vector_norm_limit=0.000002;solar_id_tolerance_minutes=0;
    tolerance_reason='Independent low-order NOAA and pinned Radiance ephemerides differ; one degree detects hour/midpoint/sign mistakes without claiming exact ephemeris agreement. Exact paired relationships separately constrain sign/offset handling at printed-vector precision.';
    conventions='ENU; longitude east-positive degrees; fixed UTC offset in hours (east-positive); EPW hour ending 1..24 minute60 represented at hour-0.5; non-leap ordinal, no DST or -i horizon time adjustment; geometric altitude, no refraction';
    horizon_scope='Presence/absence only for declared centers more than three degrees from horizon; partial-disc horizon accuracy is outside this fixture';
    weather_scope='Original non-climatic short EPWs, one or two complete local days; deliberately bypass shared full-year application importer and are not user-importable annual data. Longitude +/-180 with UTC0 is an artificial parser fixture.';
    fixtures=$solarFixtures;cases=$solarDeclaredCases.ToArray();equivalent_pairs=$solarPairs;
    exact_hour24_check=@('artificial-hour24-east180-0321-h24','artificial-hour24-east180-0322-h01');
    cardinal_checks=@{morning_positive_east='equator-greenwich-0321-h09';afternoon_negative_east='equator-greenwich-0321-h16';near_noon_up_gt_0_95='equator-greenwich-0321-h13'}
}
Write-Input 'declaration.json' ($solarDeclaration | ConvertTo-Json -Depth 12)
# No child process has run before this immutable declaration is written.
Invoke-Engine 'rcontrib' @('-version') 'engine-version.txt'
if (([IO.File]::ReadAllText((Join-Path $OutputDirectory 'engine-version.txt'))).Trim() -ne $solarDependency.version_output) { throw 'Unexpected engine version output.' }
$solarResults = [Collections.Generic.List[object]]::new()
$solarSourceMap = @{}
foreach ($fixture in $solarFixtures) {
    $weatherText = [Text.StringBuilder]::new()
    $header = 'LOCATION,Synthetic solar direction,Test,ZZZ,GreenLens original,000000,{0},{1},{2},0' -f $fixture.latitude_deg_north.ToString('R',$culture),$fixture.longitude_deg_east.ToString('R',$culture),$fixture.utc_offset_hours.ToString('R',$culture)
    $null = $weatherText.Append($header).Append("`nDESIGN CONDITIONS,0`nTYPICAL/EXTREME PERIODS,0`nGROUND TEMPERATURES,0`nHOLIDAYS/DAYLIGHT SAVINGS,No,0,0,0`nCOMMENTS 1,Original non-climatic solar direction fixture`nCOMMENTS 2,Short native-engine fixture not a valid annual application import`n")
    $firstDate = [DateTime]::new(2001,$fixture.first_month,$fixture.first_day)
    $lastDate = $firstDate.AddDays($fixture.days-1)
    $null = $weatherText.Append(('DATA PERIODS,1,1,Synthetic,Monday,{0}/{1},{2}/{3}' -f $firstDate.Month,$firstDate.Day,$lastDate.Month,$lastDate.Day)).Append("`n")
    for ($recordIndex=0;$recordIndex -lt $fixture.days*24;$recordIndex++) {
        $stamp = $firstDate.AddHours($recordIndex)
        $match = @($solarDeclaredCases | Where-Object { $_.fixture -eq $fixture.id -and $_.month -eq $stamp.Month -and $_.day -eq $stamp.Day -and $_.hour_ending -eq $stamp.Hour+1 })
        $dni = if ($match.Count -and $match[0].expected_source_present) {600} else {0}
        $dhi = if ($dni -gt 0) {100} else {0}
        $fields = @(2001,$stamp.Month,$stamp.Day,($stamp.Hour+1),60,'?9?9?9?9',25,15,60,101325,0,0,300,($dni+$dhi),$dni,$dhi,0,0,0,0,180,1,5,5,20,77777,9,999999999,20,0.1,0,88,0.2,0,0)
        $null = $weatherText.Append(($fields -join ',')).Append("`n")
    }
    $weatherName = "$($fixture.id).epw"; $sunName = "$($fixture.id).sun.rad"; $modifierName = "$($fixture.id).modifiers.txt"
    Write-Input $weatherName $weatherText.ToString()
    Invoke-Engine 'gendaymtx' @('-O0','-n','-D',$sunName,'-M',$modifierName,$weatherName) "$($fixture.id).stdout.txt"
    $sourceText = [IO.File]::ReadAllText((Join-Path $OutputDirectory $sunName))
    $sources = @{}
    foreach ($match in [regex]::Matches($sourceText,'(?s)void light solar(\d+)\s+0\s+0\s+3\s+.*?(?=\s*void light|\z)')) {
        $tokens = $match.Value.Split([char[]]" `t`r`n",[StringSplitOptions]::RemoveEmptyEntries)
        $solarMinute = [int]$match.Groups[1].Value
        if ($tokens.Count -ne 19 -or $tokens[9] -ne "solar$solarMinute" -or $tokens[11] -ne "sun$solarMinute" -or $sources.ContainsKey($solarMinute)) { throw 'Unexpected or duplicate solar source structure.' }
        $vector = @($tokens[15..17] | ForEach-Object { [double]::Parse($_,$culture) })
        $length = [Math]::Sqrt($vector[0]*$vector[0]+$vector[1]*$vector[1]+$vector[2]*$vector[2])
        if (![double]::IsFinite($length) -or $length -le 0) { throw 'Invalid source direction.' }
        $sources[$solarMinute] = @{direction_enu=@($vector|ForEach-Object {$_/$length});printed_direction_enu=$vector;printed_norm_error=[Math]::Abs($length-1);rgb=@($tokens[6..8]|ForEach-Object {[double]::Parse($_,$culture)});diameter_degrees=[double]::Parse($tokens[18],$culture)}
    }
    foreach ($case in $solarDeclaredCases | Where-Object { $_.fixture -eq $fixture.id }) {
        $minute = [int]$case.reference.expected_solar_minute
        $present = $sources.ContainsKey($minute)
        $actual = if ($present) {$sources[$minute]} else {$null}
        $angle = if ($present) {Solar-Angle $case.reference.direction_enu $actual.direction_enu} else {$null}
        $passed = $present -eq $case.expected_source_present
        if ($present) {
            $passed = $passed -and $angle -le $solarDeclaration.independent_vector_angle_limit_degrees -and $actual.printed_norm_error -le $solarDeclaration.printed_vector_norm_limit -and $actual.diameter_degrees -eq 0.533 -and $actual.rgb[0] -gt 0
            $solarSourceMap[$case.id] = @{minute=$minute;direction=$actual.direction_enu}
        }
        $solarResults.Add([ordered]@{id=$case.id;expected_solar_minute=$minute;source_present=$present;expected_source_present=$case.expected_source_present;noaa_reference=$case.reference;actual=$actual;angular_error_degrees=$angle;passed=$passed})
    }
}
$solarPairResults = foreach ($pair in $solarPairs) {
    $present = $solarSourceMap.ContainsKey($pair[0]) -and $solarSourceMap.ContainsKey($pair[1])
    $angle = if ($present) {Solar-Angle $solarSourceMap[$pair[0]].direction $solarSourceMap[$pair[1]].direction} else {$null}
    [ordered]@{first=$pair[0];second=$pair[1];angle_degrees=$angle;passed=($present -and $angle -le $solarDeclaration.equivalent_offset_angle_limit_degrees)}
}
$hour24Case = $solarSourceMap[$solarDeclaration.exact_hour24_check[0]]
$nextDayCase = $solarSourceMap[$solarDeclaration.exact_hour24_check[1]]
$hour24Result = [ordered]@{hour24_solar_minute=$hour24Case.minute;next_day_hour1_solar_minute=$nextDayCase.minute;difference_minutes=$nextDayCase.minute-$hour24Case.minute;passed=($null -ne $hour24Case -and $null -ne $nextDayCase -and $nextDayCase.minute-$hour24Case.minute -eq 60)}
$cardinal = [ordered]@{
    morning_positive_east=($solarSourceMap[$solarDeclaration.cardinal_checks.morning_positive_east].direction[0] -gt 0);
    afternoon_negative_east=($solarSourceMap[$solarDeclaration.cardinal_checks.afternoon_negative_east].direction[0] -lt 0);
    near_noon_up_gt_0_95=($solarSourceMap[$solarDeclaration.cardinal_checks.near_noon_up_gt_0_95].direction[2] -gt 0.95)
}
$maximumAngle = ($solarResults | Where-Object {$_.source_present} | ForEach-Object {$_.angular_error_degrees} | Measure-Object -Maximum).Maximum
$solarEvidence = [ordered]@{
    schema_version='greenlens.solar-direction-verification.v1';completed_utc=[DateTime]::UtcNow.ToString('o');
    declaration_sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $OutputDirectory 'declaration.json')).Hash.ToLowerInvariant();
    script_sha256=$solarScriptHash;engine_version=$solarDependency.version_output;gendaymtx_sha256=$solarExecutableHash;
    path_has_spaces=$OutputDirectory.Contains(' ');path_has_non_ascii=[bool]($OutputDirectory -match '[^\x00-\x7F]');
    fixture_count=$solarFixtures.Count;case_count=$solarResults.Count;direction_count=@($solarResults|Where-Object source_present).Count;absent_source_case_count=@($solarResults|Where-Object {!$_.expected_source_present}).Count;
    independent_vector_angle_limit_degrees=$solarDeclaration.independent_vector_angle_limit_degrees;maximum_independent_angle_degrees=$maximumAngle;
    equivalent_offset_angle_limit_degrees=$solarDeclaration.equivalent_offset_angle_limit_degrees;maximum_equivalent_pair_angle_degrees=($solarPairResults|ForEach-Object {$_.angle_degrees}|Measure-Object -Maximum).Maximum;
    cases=$solarResults.ToArray();equivalent_pairs=@($solarPairResults);hour24_rollover=$hour24Result;cardinal_checks=$cardinal;
    passed=(@($solarResults|Where-Object {!$_.passed}).Count -eq 0 -and @($solarPairResults|Where-Object {!$_.passed}).Count -eq 0 -and $hour24Result.passed -and @($cardinal.Values|Where-Object {!$_}).Count -eq 0)
}
Write-Input 'result.json' ($solarEvidence|ConvertTo-Json -Depth 12)
$hashes = [ordered]@{}
foreach ($file in Get-ChildItem -LiteralPath $OutputDirectory -File | Sort-Object Name) { $hashes[$file.Name]=(Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash.ToLowerInvariant() }
Write-Input 'checksums.json' ($hashes|ConvertTo-Json -Depth 4)
[pscustomobject]$solarEvidence | Select-Object passed,fixture_count,case_count,direction_count,absent_source_case_count,maximum_independent_angle_degrees,maximum_equivalent_pair_angle_degrees | ConvertTo-Json | Write-Host
Write-Host "Evidence directory: $OutputDirectory"
if (!$solarEvidence.passed) { throw 'Solar-direction verification failed; all outputs remain retained. Do not change tolerances to accept this run.' }
