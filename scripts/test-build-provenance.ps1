[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $env:LOCALAPPDATA ('GreenLens/provenance-tests/' + [guid]::NewGuid().ToString()) }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Provenance test destination must be new.' }
$repository = Join-Path $OutputDirectory 'source identity 測試'
New-Item -ItemType Directory -Path (Join-Path $repository 'src') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'build-provenance.rs') -Destination (Join-Path $repository 'build-provenance.rs')
@'
[package]
name = "greenlens-provenance-probe"
version = "0.0.0"
edition = "2021"
[workspace]
'@ | Set-Content -LiteralPath (Join-Path $repository 'Cargo.toml') -Encoding utf8
@'
mod build_provenance;
fn main() { build_provenance::emit(); }
'@ | Set-Content -LiteralPath (Join-Path $repository 'build.rs') -Encoding utf8
Move-Item -LiteralPath (Join-Path $repository 'build-provenance.rs') -Destination (Join-Path $repository 'build_provenance.rs')
@'
fn main() {
    println!("{}|{}", option_env!("GREENLENS_BUILD_SOURCE_COMMIT").unwrap_or("unknown"), option_env!("GREENLENS_BUILD_SOURCE_DIRTY").unwrap_or("unknown"));
}
'@ | Set-Content -LiteralPath (Join-Path $repository 'src/main.rs') -Encoding utf8
'initial input' | Set-Content -LiteralPath (Join-Path $repository 'input.txt') -Encoding utf8
'target/' | Set-Content -LiteralPath (Join-Path $repository '.gitignore') -Encoding utf8
$savedEnvironment = @{}
$gitOverrides = @('GIT_DIR','GIT_WORK_TREE','GIT_INDEX_FILE','GIT_COMMON_DIR')
$names = @('CARGO_TARGET_DIR','GREENLENS_SOURCE_COMMIT','GREENLENS_SOURCE_DIRTY','GREENLENS_EXPECT_SOURCE_COMMIT','GREENLENS_EXPECT_SOURCE_DIRTY','GREENLENS_BUILD_SOURCE_COMMIT','GREENLENS_BUILD_SOURCE_DIRTY') + $gitOverrides
foreach ($name in $names) { $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$checks = [Collections.Generic.List[object]]::new()
function Invoke-ProbeGit([string[]]$Arguments) {
    $output = & git -C $repository @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "Temporary repository command failed: $output" }
    return ($output -join "`n").Trim()
}
function Assert-Probe([string]$Name, [string]$Expected, [string]$FailureMessage = '') {
    $output = & cargo run --quiet --offline --manifest-path (Join-Path $repository 'Cargo.toml') 2>&1
    $exitCode = $LASTEXITCODE
    $text = ($output -join "`n").Trim()
    if ($FailureMessage) {
        if ($exitCode -eq 0 -or -not $text.Contains($FailureMessage)) { throw "$Name did not reject the invalid assertion: $text" }
    } elseif ($exitCode -ne 0 -or $text -ne $Expected) { throw "$Name expected '$Expected', got exit $exitCode and '$text'" }
    $checks.Add(@{name=$Name; passed=$true; expected_identity=$Expected; expected_rejection=$FailureMessage})
}
try {
    $env:CARGO_TARGET_DIR = Join-Path $OutputDirectory 'target'
    Remove-Item Env:GREENLENS_EXPECT_SOURCE_COMMIT, Env:GREENLENS_EXPECT_SOURCE_DIRTY -ErrorAction SilentlyContinue
    Remove-Item Env:GREENLENS_BUILD_SOURCE_COMMIT, Env:GREENLENS_BUILD_SOURCE_DIRTY -ErrorAction SilentlyContinue
    foreach ($name in $gitOverrides) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
    Invoke-ProbeGit @('init','--quiet') | Out-Null
    # Generate Cargo.lock before the clean baseline commit; no dependency fetch.
    & cargo generate-lockfile --offline --manifest-path (Join-Path $repository 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Probe lockfile generation failed' }
    Invoke-ProbeGit @('add','.') | Out-Null
    Invoke-ProbeGit @('-c','user.name=GreenLens validation','-c','user.email=validation@example.invalid','commit','--quiet','-m','Synthetic baseline') | Out-Null
    $firstCommit = Invoke-ProbeGit @('rev-parse','HEAD')
    # Deliberately stale legacy values remain in the same shell throughout.
    $env:GREENLENS_SOURCE_COMMIT = '0000000000000000000000000000000000000000'
    $env:GREENLENS_SOURCE_DIRTY = 'false'
    Assert-Probe 'clean despite stale legacy shell variables' "$firstCommit|false"
    $env:GREENLENS_BUILD_SOURCE_COMMIT = '1111111111111111111111111111111111111111'
    $env:GREENLENS_BUILD_SOURCE_DIRTY = 'true'
    Assert-Probe 'verified Git identity replaces inherited internal identity' "$firstCommit|false"
    Remove-Item Env:GREENLENS_BUILD_SOURCE_COMMIT, Env:GREENLENS_BUILD_SOURCE_DIRTY
    'modified input' | Set-Content -LiteralPath (Join-Path $repository 'input.txt') -Encoding utf8
    Assert-Probe 'tracked modification without re-sourcing shell' "$firstCommit|true"
    Invoke-ProbeGit @('restore','--','input.txt') | Out-Null
    Assert-Probe 'restored clean tree with reused Cargo cache' "$firstCommit|false"
    'untracked input' | Set-Content -LiteralPath (Join-Path $repository 'new.txt') -Encoding utf8
    Assert-Probe 'new untracked file with reused Cargo cache' "$firstCommit|true"
    Remove-Item -LiteralPath (Join-Path $repository 'new.txt')
    'next committed input' | Set-Content -LiteralPath (Join-Path $repository 'input.txt') -Encoding utf8
    Invoke-ProbeGit @('add','input.txt') | Out-Null
    Invoke-ProbeGit @('-c','user.name=GreenLens validation','-c','user.email=validation@example.invalid','commit','--quiet','-m','Synthetic next revision') | Out-Null
    $secondCommit = Invoke-ProbeGit @('rev-parse','HEAD')
    if ($firstCommit -eq $secondCommit) { throw 'Synthetic revision did not change' }
    Assert-Probe 'changed commit in reused shell and Cargo cache' "$secondCommit|false"
    $primaryRepository = $repository
    $linkedRepository = Join-Path $OutputDirectory 'linked worktree identity 空間'
    Invoke-ProbeGit @('worktree','add','--quiet','--detach',$linkedRepository,$firstCommit) | Out-Null
    $repository = $linkedRepository
    Assert-Probe 'linked worktree uses its own detached HEAD' "$firstCommit|false"
    # These valid overrides point at the other temporary checkout. None may
    # redirect the build helper away from the crate's actual worktree.
    $env:GIT_DIR = Join-Path $primaryRepository '.git'
    $env:GIT_WORK_TREE = $primaryRepository
    $env:GIT_INDEX_FILE = Join-Path $primaryRepository '.git/index'
    $env:GIT_COMMON_DIR = Join-Path $primaryRepository '.git'
    Assert-Probe 'linked worktree rejects inherited Git directory and index authority' "$firstCommit|false"
    'linked worktree modification' | Set-Content -LiteralPath (Join-Path $linkedRepository 'input.txt') -Encoding utf8
    Assert-Probe 'linked worktree dirty state survives poisoned Git overrides' "$firstCommit|true"
    foreach ($name in $gitOverrides) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
    $repository = $primaryRepository
    Assert-Probe 'another worktree modification does not dirty this checkout' "$secondCommit|false"
    $env:GREENLENS_EXPECT_SOURCE_COMMIT = $firstCommit
    $env:GREENLENS_EXPECT_SOURCE_DIRTY = 'false'
    Assert-Probe 'stale CI commit rejected' '' 'explicit CI source commit differs'
    $env:GREENLENS_EXPECT_SOURCE_COMMIT = $secondCommit
    Assert-Probe 'matching explicit CI assertions' "$secondCommit|false"
    'dirty again' | Set-Content -LiteralPath (Join-Path $repository 'input.txt') -Encoding utf8
    Assert-Probe 'incorrect clean CI assertion rejected' '' 'explicit CI dirty state differs'
    $env:GREENLENS_EXPECT_SOURCE_DIRTY = 'true'
    Assert-Probe 'matching explicit dirty CI assertion' "$secondCommit|true"
    Remove-Item Env:GREENLENS_EXPECT_SOURCE_DIRTY
    Assert-Probe 'partial CI assertion rejected' '' 'source identity assertions require both'
    Remove-Item Env:GREENLENS_EXPECT_SOURCE_COMMIT
    $archiveDirectory = Join-Path $OutputDirectory 'source archive without Git'
    New-Item -ItemType Directory -Path (Join-Path $archiveDirectory 'src') -Force | Out-Null
    foreach ($file in @('Cargo.toml','Cargo.lock','build.rs','build_provenance.rs','src/main.rs')) {
        Copy-Item -LiteralPath (Join-Path $repository $file) -Destination (Join-Path $archiveDirectory $file)
    }
    $repository = $archiveDirectory
    Assert-Probe 'source archive does not claim a clean known commit' 'unknown|unknown'
    $env:GREENLENS_BUILD_SOURCE_COMMIT = '1111111111111111111111111111111111111111'
    $env:GREENLENS_BUILD_SOURCE_DIRTY = 'false'
    Assert-Probe 'archive rejects inherited internal identity pair' '' 'unverifiable inherited build source identity'
    Remove-Item Env:GREENLENS_BUILD_SOURCE_DIRTY
    Assert-Probe 'archive rejects inherited internal commit alone' '' 'unverifiable inherited build source identity'
    Remove-Item Env:GREENLENS_BUILD_SOURCE_COMMIT
    $env:GREENLENS_BUILD_SOURCE_DIRTY = 'false'
    Assert-Probe 'archive rejects inherited internal clean flag alone' '' 'unverifiable inherited build source identity'
    Remove-Item Env:GREENLENS_BUILD_SOURCE_DIRTY
    Assert-Probe 'archive returns to unknown after clearing internal identity' 'unknown|unknown'
    @{schema_version='greenlens.build-provenance-tests.v0.2'; verified_utc=[DateTime]::UtcNow.ToString('o'); passed=$true; checks=@($checks)} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'summary.json') -Encoding utf8
    Write-Output "Build provenance checks passed ($($checks.Count)): $OutputDirectory"
} finally {
    foreach ($name in $names) {
        if ($null -eq $savedEnvironment[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
    }
}
