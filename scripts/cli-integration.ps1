[CmdletBinding()]
param([string]$Cli, [string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if (-not $Cli) { $Cli = Join-Path $env:CARGO_TARGET_DIR 'debug\greenlens.exe' }
if (-not (Test-Path -LiteralPath $Cli)) { throw 'Build greenlens-cli first or supply -Cli.' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $env:LOCALAPPDATA ("GreenLens\verification\CLI 空間 " + [guid]::NewGuid().ToString()) }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Use a fresh output directory.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$projectPath = Join-Path $OutputDirectory 'Original project'
function Invoke-GreenLens {
    param([string[]]$Arguments, [string]$OutputFile)
    $text = & $Cli @Arguments
    if ($LASTEXITCODE -ne 0) { throw "CLI failed: $($Arguments[0])" }
    if ($OutputFile) { [IO.File]::WriteAllText($OutputFile, ($text -join "`n"), [Text.UTF8Encoding]::new($false)) }
    return ($text -join "`n")
}
Invoke-GreenLens -Arguments @('doctor') -OutputFile (Join-Path $OutputDirectory 'doctor.json') | Out-Null
Invoke-GreenLens -Arguments @('validate', (Join-Path $sourceRoot 'examples/synthetic-a.json')) | Out-Null
foreach ($alternative in @('A','B')) {
    Invoke-GreenLens -Arguments @('run',(Join-Path $sourceRoot "examples/synthetic-$($alternative.ToLowerInvariant()).json"),'--project',$projectPath,'--alternative',$alternative) -OutputFile (Join-Path $OutputDirectory "run-$alternative.json") | Out-Null
}
Invoke-GreenLens -Arguments @('compare',$projectPath) -OutputFile (Join-Path $OutputDirectory 'comparison.json') | Out-Null
$copiedProject = Join-Path $OutputDirectory 'Relocated project'
Copy-Item -LiteralPath $projectPath -Destination $copiedProject -Recurse
$original = Get-Content -LiteralPath (Join-Path $projectPath 'project.json') -Raw | ConvertFrom-Json
foreach ($run in $original.runs) {
    foreach ($name in @('results.json','provenance.json','raw.rgb','checksums.json')) {
        $first = (Get-FileHash -LiteralPath (Join-Path $projectPath "$($run.path)/$name") -Algorithm SHA256).Hash
        $second = (Get-FileHash -LiteralPath (Join-Path $copiedProject "$($run.path)/$name") -Algorithm SHA256).Hash
        if ($first -ne $second) { throw 'Relocated evidence differs.' }
    }
}
# Make only these owned run scratch directories unavailable. Verify targets
# before moving; leave unrelated active jobs and cache data untouched.
$hidden = @()
try {
    foreach ($run in $original.runs) {
        $id = [guid]::Parse($run.id).ToString()
        $jobsRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens/jobs'))
        $scratch = [IO.Path]::GetFullPath((Join-Path $jobsRoot $id))
        if (-not $scratch.StartsWith($jobsRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Scratch containment failure' }
        if (Test-Path -LiteralPath $scratch) {
            $away = [IO.Path]::GetFullPath((Join-Path $OutputDirectory "unavailable-scratch-$id"))
            if (-not $away.StartsWith([IO.Path]::GetFullPath($OutputDirectory) + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Destination containment failure' }
            Move-Item -LiteralPath $scratch -Destination $away
            $hidden += @{ Source = $scratch; Away = $away }
        }
    }
    Invoke-GreenLens -Arguments @('compare',$copiedProject) -OutputFile (Join-Path $OutputDirectory 'relocated-comparison.json') | Out-Null
    $zip = Join-Path $OutputDirectory 'evidence.zip'
    Invoke-GreenLens -Arguments @('export',$copiedProject,$zip) | Out-Null
    Invoke-GreenLens -Arguments @('replay',$zip) -OutputFile (Join-Path $OutputDirectory 'replay.json') | Out-Null
} finally {
    foreach ($entry in $hidden) {
        if (-not (Test-Path -LiteralPath $entry.Source)) { Move-Item -LiteralPath $entry.Away -Destination $entry.Source }
    }
}
@{schema_version='0.1.0'; platform='native Windows'; verified_utc=[DateTime]::UtcNow.ToString('o'); checks=@('doctor','validate','real A/B runs','compatible comparison','copy exact evidence','reopen/export without scratch','ZIP replay tolerance'); passed=$true} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8
Write-Output "CLI integration passed. Evidence: $OutputDirectory"
