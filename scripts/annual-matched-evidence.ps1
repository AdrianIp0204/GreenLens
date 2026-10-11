#Requires -Version 7.2
# Local research evidence helpers. These never execute scene/package commands.
function Get-MatchedHash([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}
function Get-MatchedTransportNames([string]$Kind) {
    if ($Kind -eq 'coefficient') { return @('scene.rad','sensors.pts','receiver.rad','declaration.json','coefficients.mtx','direct-coefficients.mtx','processes.json') }
    if ($Kind -eq 'solar') { return @('scene.rad','sensors.pts','selected-suns.rad','selected-modifiers.txt','selected-skies.mtx','declaration.json','solar-declaration.json','solar-rgb.mtx','solar-centroid-direct.mtx','processes.json') }
    throw 'Unsupported research transport kind.'
}
function Assert-MatchedTransport([string]$Directory, [string]$Kind) {
    $seal=Get-Content -LiteralPath (Join-Path $Directory 'transport-integrity.json') -Raw|ConvertFrom-Json
    if ($seal.schema_version -ne 'greenlens.research-transport-integrity.v1' -or $seal.kind -ne $Kind -or $seal.captured_after_execution -ne $true) { throw 'Unsupported research transport seal.' }
    $expected=@(Get-MatchedTransportNames $Kind|Sort-Object)
    $actual=@($seal.files.psobject.Properties.Name|Sort-Object)
    if (($expected -join ',') -cne ($actual -join ',')) { throw 'Research transport seal file set is incomplete or unexpected.' }
    foreach ($name in $expected) {
        if ($seal.files.$name -cnotmatch '^[0-9a-f]{64}$' -or (Get-MatchedHash (Join-Path $Directory $name)) -cne $seal.files.$name) { throw 'Retained research transport changed after sealing.' }
    }
}
function Get-MatchedReferenceHash($Reference) {
    $text=$Reference|ConvertTo-Json -Depth 8 -Compress
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($text))).ToLowerInvariant()
}
function Get-MatchedPhysicalReferenceHash($Reference) {
    # Method provenance is distinct from the unchanged reference calculation.
    # Clone before removing the three identity fields; never rewrite old evidence.
    $physical=$Reference|ConvertTo-Json -Depth 12|ConvertFrom-Json -AsHashtable
    $pair=$physical.engine.adapter_version+'|'+$physical.engine.method_version
    if ($pair -notin @('radiance-climate-direct-corrected/2|perez-epw-dewpoint3-reinhart-patch-indirect-finite-direct/2','radiance-climate-explicit-indirect-finite-direct/3|perez-epw-dewpoint3-reinhart-explicit-indirect-finite-direct/3')) { throw 'Unsupported reference method tuple.' }
    foreach ($name in @('adapter_version','method_version','fingerprint')) { $null=$physical.engine.Remove($name) }
    return Get-MatchedReferenceHash $physical
}
function Get-MatchedMethodTuple($Recipe) {
    $tuple=$Recipe.schema_version+'|'+$Recipe.method_version+'|'+$Recipe.engine.adapter_version+'|'+$Recipe.engine.method_version
    $supported=@('greenlens.annual-recipe.v2|perez-epw-dewpoint3-reinhart-patch-indirect-finite-direct/2|radiance-climate-direct-corrected/2|perez-epw-dewpoint3-reinhart-patch-indirect-finite-direct/2','greenlens.annual-recipe.v3|perez-epw-dewpoint3-reinhart-explicit-indirect-finite-direct/3|radiance-climate-explicit-indirect-finite-direct/3|perez-epw-dewpoint3-reinhart-explicit-indirect-finite-direct/3')
    if ($tuple -notin $supported) { throw 'Unsupported annual matched method tuple.' }
    return $tuple
}
function Assert-MatchedPhysicalReference($First, $Second) {
    if ((Get-MatchedPhysicalReferenceHash $First) -cne (Get-MatchedPhysicalReferenceHash $Second)) { throw 'Reference physical inputs, engine or numerical settings differ.' }
}
function Assert-MatchedReference($Declaration) {
    if ($Declaration.reference_identity_sha256 -cne (Get-MatchedReferenceHash $Declaration.reference)) { throw 'Matched reference identity checksum mismatch.' }
}
function Get-MatchedKernelHash([string]$Source) {
    # Bind the numerical C# helper and exact base rtrace options. Geometry/sky,
    # engine, reference ab/ad and full interval plan are separately identity-bound.
    $helper = [regex]::Match($Source, "(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@")
    $options = [regex]::Match($Source, '(?m)^\$rayOptions=@\([^\r\n]+\)')
    if (!$helper.Success -or !$options.Success) { throw 'Missing matched-reference numerical kernel.' }
    $text = $helper.Groups[1].Value.Replace("`r`n", "`n") + "`n" + $options.Value
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($text))).ToLowerInvariant()
}
function Assert-MatchedSource([string]$Directory, [string]$ExpectedKernel) {
    $path = Join-Path $Directory 'harness-source.ps1'
    $record = Get-Content -LiteralPath (Join-Path $Directory 'harness-source.json') -Raw | ConvertFrom-Json
    if ($record.harness_sha256 -cne (Get-MatchedHash $path)) { throw 'Preserved matched harness source checksum mismatch.' }
    if ((Get-MatchedKernelHash ([IO.File]::ReadAllText($path))) -cne $ExpectedKernel) { throw 'Matched reference numerical kernel mismatch.' }
}
function Assert-MatchedRaw([string]$Directory, $Check) {
    foreach ($kind in @('reference', 'direct', 'finite')) {
        $expected = $Check."${kind}_sha256"
        if ($expected -cnotmatch '^[0-9a-f]{64}$' -or (Get-MatchedHash (Join-Path $Directory "hour-$($Check.interval_index)-$kind.rgb")) -cne $expected) {
            throw "Matched raw $kind checksum mismatch for interval $($Check.interval_index)."
        }
    }
}
function Select-MatchedIntervals([int[]]$Declared, [int[]]$Requested) {
    if (@($Declared | Select-Object -Unique).Count -ne $Declared.Count) { throw 'Duplicate declared matched interval.' }
    if (!$Requested.Count) { return ,$Declared }
    if (@($Requested | Select-Object -Unique).Count -ne $Requested.Count) { throw 'Duplicate requested matched interval.' }
    foreach ($index in $Requested) { if ($Declared -notcontains $index) { throw 'Requested matched interval is not in the predeclared plan.' } }
    return ,@($Declared | Where-Object { $Requested -contains $_ })
}
function Join-MatchedEvidence([string[]]$Directories) {
    if (!$Directories.Count) { throw 'Matched aggregation requires at least one evidence directory.' }
    $baseline = $null
    $kernel = $null
    $sensorCount = 0
    $byIndex = @{}
    $sources = [Collections.Generic.List[object]]::new()
    foreach ($directory in $Directories) {
        $reportPath = Join-Path $directory 'matched-corrected.json'
        $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
        $declaration = $report.declaration
        Assert-MatchedReference $declaration
        if (!$baseline) {
            $baseline = $declaration
            $kernel = Get-MatchedKernelHash ([IO.File]::ReadAllText((Join-Path $directory 'harness-source.ps1')))
            $sensorCount = @([IO.File]::ReadAllLines((Join-Path $directory 'sensors.pts')) | Where-Object { $_.Trim() }).Count
            if ($sensorCount -lt 1 -or $sensorCount -gt 256) { throw 'Invalid matched sensor count.' }
        }
        Assert-MatchedSource $directory $kernel
        foreach ($field in @('reference_identity_sha256', 'candidate_recipe_sha256', 'candidate_rgb_sha256')) {
            if ($declaration.$field -cnotmatch '^[0-9a-f]{64}$' -or $declaration.$field -cne $baseline.$field) { throw "Matched aggregation identity mismatch: $field." }
        }
        if ((Get-MatchedHash (Join-Path $directory 'sensors.pts')) -cne $baseline.reference.sensor_sha256) { throw 'Matched sensor identity mismatch.' }
        foreach ($check in $report.checks) {
            $index = [int]$check.interval_index
            if ($baseline.reference.interval_indices -notcontains $index) { throw 'Unexpected interval in matched aggregate.' }
            if ($byIndex.ContainsKey($index)) { throw 'Duplicate matched interval across shards; no result may be silently replaced.' }
            if (@($check.sensors).Count -ne $sensorCount) { throw 'Incomplete matched sensor coverage.' }
            Assert-MatchedRaw $directory $check
            $failures = 0
            for ($sensor = 0; $sensor -lt $sensorCount; $sensor++) {
                $value = $check.sensors[$sensor]
                if ($value.sensor_index -ne $sensor) { throw 'Matched sensor order mismatch.' }
                foreach ($field in @('candidate_lux', 'reference_lux')) {
                    if ($null -eq $value.$field -or ![double]::IsFinite([double]$value.$field) -or $value.$field -lt 0) { throw 'Invalid matched illuminance.' }
                }
                $error = [Math]::Abs($value.candidate_lux - $value.reference_lux)
                $allowed = [Math]::Max(5.0, .05 * $value.reference_lux)
                $passed = $error -le $allowed
                if ($value.absolute_error_lux -ne $error -or $value.allowed_error_lux -ne $allowed -or $value.passed -isnot [bool] -or $value.passed -ne $passed) { throw 'Matched arithmetic or fixed tolerance mismatch.' }
                if (!$passed) { $failures++ }
            }
            if ($check.failure_count -ne $failures -or $check.pass -isnot [bool] -or $check.pass -ne ($failures -eq 0)) { throw 'Matched failure summary mismatch.' }
            $byIndex[$index] = $check
        }
        $sources.Add(@{ report_sha256 = (Get-MatchedHash $reportPath); harness_sha256 = (Get-MatchedHash (Join-Path $directory 'harness-source.ps1')); interval_indices = @($report.checks.interval_index) })
    }
    $expected = @($baseline.reference.interval_indices)
    if (@($expected | Select-Object -Unique).Count -ne $expected.Count -or $byIndex.Count -ne $expected.Count) { throw 'Incomplete matched interval coverage; all predeclared intervals are required exactly once.' }
    $checks = @($expected | ForEach-Object { if (!$byIndex.ContainsKey([int]$_)) { throw 'Missing predeclared matched interval.' }; $byIndex[[int]$_] })
    return [ordered]@{ schema_version='greenlens.corrected-matched-aggregate.v1'; completed=$true; pass=(@($checks | Where-Object { !$_.pass }).Count -eq 0); declaration=$baseline; numerical_kernel_sha256=$kernel; sensor_count=$sensorCount; sources=$sources; checks=$checks; scope='Matched-sky evidence only; shadow fixtures and annual convergence are separate required checks.' }
}
