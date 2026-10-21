#Requires -Version 7.2
# Original synthetic path/CWD checks; no network, build, parser or process probe.
[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$expectedRepository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$requestedGuardOutput=$OutputDirectory
. (Join-Path $PSScriptRoot 'test-openskp-allocation.ps1') -FunctionsOnly
$OutputDirectory=$requestedGuardOutput
$checks=[Collections.Generic.List[string]]::new()
function Check-AllocationGuard([bool]$Value,[string]$Name) {
    if(-not $Value) { throw "Allocation path regression: $Name" }
    $checks.Add($Name)
}
function Reject-AllocationGuard([scriptblock]$Action,[string]$Name) {
    $rejected=$false
    try { $null=& $Action } catch { $rejected=$true }
    Check-AllocationGuard $rejected $Name
}
if(-not $env:LOCALAPPDATA) { throw 'Native Windows LOCALAPPDATA is required.' }
if([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory=Join-Path $env:LOCALAPPDATA ("GreenLens/verification/allocation-guards-"+[Guid]::NewGuid().ToString('N'))
}
$testRoot=Assert-AllocationTestDestination $OutputDirectory
New-Item -ItemType Directory -Path $testRoot | Out-Null
try {
    Reject-AllocationGuard { Assert-AllocationTestDestination '' } 'Missing destination rejected'
    Reject-AllocationGuard { Assert-AllocationTestDestination '   ' } 'Whitespace destination rejected'
    $localApplicationData=$env:LOCALAPPDATA
    try {
        Remove-Item Env:LOCALAPPDATA
        Reject-AllocationGuard { Assert-AllocationTestDestination $testRoot } 'Missing external root rejected'
    } finally { $env:LOCALAPPDATA=$localApplicationData }
    Reject-AllocationGuard { Assert-AllocationTestDestination $testRoot } 'Existing directory preserved'
    $sentinel=Join-Path $testRoot 'original.txt'
    [IO.File]::WriteAllText($sentinel,"Original synthetic allocation guard fixture.`n",[Text.UTF8Encoding]::new($false))
    $sentinelHash=(Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash
    Reject-AllocationGuard { Assert-AllocationTestDestination $sentinel } 'Existing file preserved'
    $outside=Join-Path $env:LOCALAPPDATA 'not-greenlens-allocation-guard/new'
    Reject-AllocationGuard { Assert-AllocationTestDestination $outside } 'Outside external root rejected'
    $sibling=Join-Path $env:LOCALAPPDATA 'GreenLens-sibling/new'
    Reject-AllocationGuard { Assert-AllocationTestDestination $sibling } 'Lexical sibling prefix rejected'
    $traversal=Join-Path $env:LOCALAPPDATA 'GreenLens/../not-greenlens-allocation-guard/new'
    Reject-AllocationGuard { Assert-AllocationTestDestination $traversal } 'Normalized parent escape rejected'
    $nested=Join-Path $testRoot 'absent-parent/absent-child/new'
    Check-AllocationGuard ((Assert-AllocationTestDestination $nested) -ceq [IO.Path]::GetFullPath($nested)) 'Valid nested absent destination admitted'
    Check-AllocationGuard (-not (Test-Path -LiteralPath (Join-Path $testRoot 'absent-parent'))) 'Admission creates no parent or output'

    # Both locations are original external test directories. The junction is
    # retained as evidence; no recursive deletion or unrelated target is used.
    $target=Join-Path $testRoot 'junction-target'
    New-Item -ItemType Directory -Path $target | Out-Null
    $targetSentinel=Join-Path $target 'original.txt'
    [IO.File]::WriteAllText($targetSentinel,'Keep unchanged',[Text.UTF8Encoding]::new($false))
    $targetHash=(Get-FileHash -LiteralPath $targetSentinel -Algorithm SHA256).Hash
    $junction=Join-Path $testRoot 'junction'
    New-Item -ItemType Junction -Path $junction -Target $target | Out-Null
    Check-AllocationGuard ([bool]((Get-Item -LiteralPath $junction -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) 'Synthetic junction confirmed'
    Reject-AllocationGuard { Assert-AllocationTestDestination (Join-Path $junction 'new') } 'Immediate junction ancestor rejected'
    Reject-AllocationGuard { Assert-AllocationTestDestination (Join-Path $junction 'absent-parent/new') } 'Higher junction beyond absent parent rejected'
    Check-AllocationGuard (@(Get-ChildItem -LiteralPath $target -Force).Count -eq 1 -and
        (Get-FileHash -LiteralPath $targetSentinel -Algorithm SHA256).Hash -ceq $targetHash) 'Rejected junction paths wrote nothing and retained original bytes'
    Check-AllocationGuard ((Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash -ceq $sentinelHash) 'Existing fixture bytes unchanged'

    $previousLocation=(Get-Location).Path
    Push-Location -LiteralPath $testRoot
    try {
        Check-AllocationGuard ((Get-AllocationTestRepository) -ceq $expectedRepository) 'Repository identity independent of external caller CWD'
        Check-AllocationGuard ((Get-Location).Path -ceq $testRoot) 'Repository lookup does not change caller CWD'
    } finally { Pop-Location }
    Check-AllocationGuard ((Get-Location).Path -ceq $previousLocation) 'Caller CWD restored after test'
    $receipt=[ordered]@{
        schema_version='greenlens.openskp-allocation-guard-tests.v1';passed=$checks.Count;failed=0
        network_requests=0;builds=0;skp_parses=0;worker_executions=0;ct1_cases=0
        containment_qualified=$false;checks=$checks
    }
    $receipt | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $testRoot 'guard-tests.json') -Encoding utf8
    $receipt | ConvertTo-Json -Depth 4
} catch {
    [IO.File]::WriteAllText((Join-Path $testRoot 'GUARD_TEST_FAILED.txt'),[string]$_,[Text.UTF8Encoding]::new($false))
    throw
}
