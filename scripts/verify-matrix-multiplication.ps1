# Independent component-wise RGB arithmetic, separate from Radiance transport.
# The fixed tolerance covers ASCII output rounding; it is not a daylight or
# physical-validation tolerance and does not change any release requirement.
function Assert-IndependentRgbProduct {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Coefficients,
        [Parameter(Mandatory)]$Sky,
        [Parameter(Mandatory)]$Product,
        [ValidateRange(0,8760)][int]$MaximumColumns = 0,
        [string]$EvidencePath
    )
    $absoluteTolerance = 1e-7
    $relativeTolerance = 1e-5
    foreach ($matrix in @($Coefficients,$Sky,$Product)) {
        if ($matrix.rows -lt 1 -or $matrix.columns -lt 1 -or $matrix.components -ne 3 -or $matrix.values.Length -ne [long]$matrix.rows*$matrix.columns*3) { throw 'Independent RGB multiplication requires exact positive matrix shapes and three components.' }
    }
    if ($Coefficients.columns -ne $Sky.rows -or $Product.rows -ne $Coefficients.rows -or $Product.columns -ne $Sky.columns) { throw 'Independent RGB multiplication dimensions are incompatible.' }
    $columns = if ($MaximumColumns) { [Math]::Min($MaximumColumns,$Sky.columns) } else { $Sky.columns }
    $checks = 0L
    $maximumError = 0.0
    $maximumRatio = 0.0
    $failures = [Collections.Generic.List[object]]::new()
    for ($sensor=0; $sensor -lt $Coefficients.rows; $sensor++) {
        for ($time=0; $time -lt $columns; $time++) {
            for ($component=0; $component -lt 3; $component++) {
                $expected = 0.0
                for ($patch=0; $patch -lt $Sky.rows; $patch++) {
                    $a = [double]$Coefficients.values[($sensor*$Coefficients.columns+$patch)*3+$component]
                    $b = [double]$Sky.values[($patch*$Sky.columns+$time)*3+$component]
                    if (![double]::IsFinite($a) -or ![double]::IsFinite($b) -or $a -lt 0 -or $b -lt 0) { throw 'Independent RGB multiplication encountered invalid input.' }
                    $expected += $a*$b
                }
                $actual = [double]$Product.values[($sensor*$Product.columns+$time)*3+$component]
                if (![double]::IsFinite($expected) -or ![double]::IsFinite($actual) -or $actual -lt 0) { throw 'Independent RGB multiplication encountered invalid output.' }
                $error = [Math]::Abs($actual-$expected)
                $allowed = [Math]::Max($absoluteTolerance,$relativeTolerance*[Math]::Abs($expected))
                $maximumError = [Math]::Max($maximumError,$error)
                $maximumRatio = [Math]::Max($maximumRatio,$error/$allowed)
                $checks++
                if ($error -gt $allowed) { $failures.Add(@{sensor=$sensor;column=$time;component=$component;expected=$expected;actual=$actual;absolute_error=$error;allowed_error=$allowed}) }
            }
        }
    }
    $result = [ordered]@{schema_version='greenlens.independent-rgb-product.v1';passed=($failures.Count -eq 0);rows=$Coefficients.rows;inner_dimension=$Sky.rows;checked_columns=$columns;total_columns=$Sky.columns;component_checks=$checks;absolute_tolerance=$absoluteTolerance;relative_tolerance=$relativeTolerance;maximum_absolute_rgb_error=$maximumError;maximum_error_over_tolerance=$maximumRatio;failures=$failures.ToArray();scope='Independent component-wise arithmetic and ASCII rounding only; not physical validation.'}
    if ($EvidencePath) { $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $EvidencePath -Encoding utf8NoBOM }
    if (!$result.passed) { throw "Independent RGB multiplication failed $($failures.Count) of $checks components; maximum error/tolerance $maximumRatio. Retain evidence and investigate." }
    return $result
}
