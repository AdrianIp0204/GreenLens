[CmdletBinding()]
param(
    [string]$OpenSkpCheckout,
    [string]$MinizCheckout,
    [string]$TinyGltfCheckout,
    [string]$OutputDirectory,
    [switch]$FunctionsOnly
)
$ErrorActionPreference='Stop'
function Resolve-AllocationTestPath([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) { throw 'A filesystem path is required.' }
    # PowerShell's location can differ from the process working directory after
    # Set-Location/Push-Location. Resolve caller paths through its provider first.
    $provider=$null
    $drive=$null
    $resolved=$ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path,[ref]$provider,[ref]$drive)
    if ($provider.Name -cne 'FileSystem') { throw 'Only filesystem paths are supported.' }
    return [IO.Path]::GetFullPath($resolved)
}
function Assert-AllocationTestDestination([string]$Path) {
    if (!$env:LOCALAPPDATA -or [string]::IsNullOrWhiteSpace($Path)) { throw 'An external destination is required.' }
    $candidate=Resolve-AllocationTestPath $Path
    $external=(Resolve-AllocationTestPath (Join-Path $env:LOCALAPPDATA 'GreenLens'))+[IO.Path]::DirectorySeparatorChar
    if(-not $candidate.StartsWith($external,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $candidate)) {
        throw 'Expected a new external GreenLens test directory; preserve previous evidence.'
    }
    for($ancestor=[IO.Path]::GetDirectoryName($candidate); $ancestor; $ancestor=[IO.Path]::GetDirectoryName($ancestor)) {
        if((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Reparse destination ancestry unsupported; no test files were written.'
        }
    }
    return $candidate
}
function Get-AllocationTestRepository { [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')) }
if($FunctionsOnly) { return }
foreach($inputPath in @($OpenSkpCheckout,$MinizCheckout,$TinyGltfCheckout)) {
    if([string]::IsNullOrWhiteSpace($inputPath)) { throw 'All three pinned dependency checkouts are required.' }
}
# Resolve caller-provided paths before entering this script's repository.
$OpenSkpCheckout=Resolve-AllocationTestPath $OpenSkpCheckout
$MinizCheckout=Resolve-AllocationTestPath $MinizCheckout
$TinyGltfCheckout=Resolve-AllocationTestPath $TinyGltfCheckout
$root=Assert-AllocationTestDestination $OutputDirectory
Push-Location (Get-AllocationTestRepository)
try {
New-Item -ItemType Directory -Path $root | Out-Null
. (Join-Path $PSScriptRoot 'dev-env.ps1')
$unitSource=Join-Path $root 'unit-source'
$unitBuild=Join-Path $root 'unit-build'
node tools/openskp-worker/prepare-preallocation-tests.mjs $MinizCheckout $unitSource $TinyGltfCheckout *> (Join-Path $root 'unit-preparation.log')
if($LASTEXITCODE -ne 0){throw 'Pinned isolated test source preparation failed'}
cmake -S tools/openskp-worker/tests -B $unitBuild -G Ninja "-DGREENLENS_PREALLOCATION_SOURCE=$unitSource" -DCMAKE_BUILD_TYPE=Release *> (Join-Path $root 'unit-configure.log')
if($LASTEXITCODE -ne 0){throw 'Isolated test configuration failed'}
cmake --build $unitBuild --parallel 2 *> (Join-Path $root 'unit-build.log')
if($LASTEXITCODE -ne 0){throw 'Isolated test compilation failed'}
foreach($name in @('preallocation','parser-allocation')) {
    & (Join-Path $unitBuild "greenlens-$name-tests.exe") *> (Join-Path $root "$name-tests.json")
    if($LASTEXITCODE -ne 0){throw "$name helper tests failed"}
}
& (Join-Path $unitBuild 'greenlens-serialization-tests.exe') $root *> (Join-Path $root 'serialization-tests.json')
if($LASTEXITCODE -ne 0){throw 'Serialization/hash unit tests failed'}
$prepared=Join-Path $root 'compile-source'
$build=Join-Path $root 'compile-only'
node tools/openskp-worker/prepare-source.mjs $OpenSkpCheckout $MinizCheckout $TinyGltfCheckout $prepared *> (Join-Path $root 'source-preparation.log')
if($LASTEXITCODE -ne 0){throw 'Pinned compile-only source preparation failed'}
cmake -S tools/openskp-worker -B $build -G Ninja "-DGREENLENS_PREPARED_SOURCE=$prepared" -DGREENLENS_COMPILE_ONLY=ON -DCMAKE_BUILD_TYPE=Release *> (Join-Path $root 'source-configure.log')
if($LASTEXITCODE -ne 0){throw 'Compile-only source configuration failed'}
cmake --build $build --parallel 2 *> (Join-Path $root 'source-build.log')
if($LASTEXITCODE -ne 0){throw 'Compile-only source check failed'}
if(Test-Path -LiteralPath (Join-Path $build 'greenlens-openskp-development-worker.exe')) {
    throw 'Compile-only target must not produce a worker executable'
}
& (Join-Path $build 'greenlens-instanced-export-tests.exe') *> (Join-Path $root 'instanced-export-tests.json')
if($LASTEXITCODE -ne 0){throw 'Original in-memory exporter checks failed'}
[ordered]@{schema_version='greenlens.openskp-allocation-source-check.v1';source_commit=(git rev-parse HEAD).Trim();source_dirty=[bool](git status --porcelain);skp_parses=0;worker_executions=0;ct1_cases=0;containment_qualified=$false;scope='Isolated helpers and synthetic exporter; source closure compiled to objects only';unit_manifest_sha256=(Get-FileHash -LiteralPath (Join-Path $unitSource 'unit-source-manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant();source_manifest_sha256=(Get-FileHash -LiteralPath (Join-Path $prepared 'source-manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant()} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'summary.json') -Encoding utf8
Get-Content -LiteralPath (Join-Path $root 'summary.json')
} finally { Pop-Location }
