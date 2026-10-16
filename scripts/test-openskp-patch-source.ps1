#Requires -Version 7.2
# No network, Git process, converter, model, engine, or renderer execution.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'prepare-openskp-patch-source.ps1')
$checks = [Collections.Generic.List[string]]::new()
function Check([bool]$Value,[string]$Name) { if (!$Value) { throw "Patch source regression: $Name" }; $checks.Add($Name) }
function Reject([scriptblock]$Action,[string]$Name) {
    $rejected=$false
    try { & $Action } catch { $rejected=$true }
    Check $rejected $Name
}
$spec=Get-OpenSkpPatchSpec
$notice=[IO.File]::ReadAllBytes((Join-Path $PSScriptRoot '../tools/openskp-worker/licenses/LICENSE.openskp'))
$noticeLf=[Text.Encoding]::UTF8.GetBytes([Text.Encoding]::UTF8.GetString($notice).Replace("`r`n","`n"))
Check ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($noticeLf)).ToLowerInvariant() -ceq $spec.license_sha256) 'Pinned LF source license differs from retained notice only in declared CRLF spelling'
$noticeCrlf=[Text.Encoding]::UTF8.GetBytes([Text.Encoding]::UTF8.GetString($noticeLf).Replace("`n","`r`n"))
Check ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($noticeCrlf)).ToLowerInvariant() -ceq $spec.notice_sha256) 'Reviewed original CRLF notice digest remains unchanged despite checkout EOL policy'
Check ((Select-OpenSkpGitExecutable @([pscustomobject]@{Source='first-git.exe'},[pscustomobject]@{Source='second-git.exe'})) -ceq 'first-git.exe') 'Multiple discovered Git paths select exactly the first application'
Reject { Select-OpenSkpGitExecutable @() } 'No Git application fails explicitly'
Assert-OpenSkpPatchRoot '' '.git' 'false'
Check $true 'Own non-bare repository root accepts logical or physical directory spelling'
Reject { Assert-OpenSkpPatchRoot 'nested/' '../.git' 'false' } 'Parent repository lookup rejected'
Reject { Assert-OpenSkpPatchRoot '' 'another/.git' 'false' } 'External Git directory rejected'
Reject { Assert-OpenSkpPatchRoot '' '.git' 'true' } 'Bare cache rejected'
Assert-OpenSkpPatchIdentity $spec.origin $spec.commit '' $spec.license_sha256
Check $true 'Exact origin, commit, clean status and notice admitted'
Reject { Assert-OpenSkpPatchIdentity 'https://example.invalid/openskp.git' $spec.commit '' $spec.license_sha256 } 'Wrong origin rejected'
Reject { Assert-OpenSkpPatchIdentity ($spec.origin+'/') $spec.commit '' $spec.license_sha256 } 'Origin alias not silently accepted'
Reject { Assert-OpenSkpPatchIdentity $spec.origin ('0'*40) '' $spec.license_sha256 } 'Wrong commit rejected'
Reject { Assert-OpenSkpPatchIdentity $spec.origin $spec.commit ' M packages/cpp/src/core.cpp' $spec.license_sha256 } 'Modified tracked source rejected'
Reject { Assert-OpenSkpPatchIdentity $spec.origin $spec.commit '?? extra.txt' $spec.license_sha256 } 'Untracked source rejected'
Reject { Assert-OpenSkpPatchIdentity $spec.origin $spec.commit '' ('0'*64) } 'Changed retained notice rejected'
$bytes=[Text.Encoding]::UTF8.GetBytes("Original synthetic text.`n")
Assert-OpenSkpPatchBytes $bytes $bytes 'synthetic.cpp'
Check $true 'Exact source bytes admitted'
$changed=$bytes.Clone();$changed[0]=$changed[0] -bxor 1
Reject { Assert-OpenSkpPatchBytes $changed $bytes 'synthetic.cpp' } 'Changed byte rejected despite equal length'
Reject { Assert-OpenSkpPatchBytes ([byte[]]::new(4MB+1)) ([byte[]]::new(0)) 'synthetic.cpp' } 'Oversized working source rejected'
Reject { Assert-OpenSkpPatchBytes ([byte[]]::new(0)) ([byte[]]::new(4MB+1)) 'synthetic.cpp' } 'Oversized pinned blob rejected'
Check ($spec.files.Count -eq 13 -and @($spec.files|Select-Object -Unique).Count -eq 13) 'Exact 12 patch sources plus notice'
Check (@($spec.files|Where-Object{$_ -match '\.(skp|glb|exe|dll)$'}).Count -eq 0) 'No model or executable in sparse source list'
$workflow=Get-Content -LiteralPath (Join-Path $PSScriptRoot '../.github/workflows/energy-windows.yml') -Raw
Check ($workflow.Contains('GREENLENS_OPEN_SKP_AUDIT_SOURCE=') -and $workflow.IndexOf('prepare-openskp-patch-source.ps1') -lt $workflow.IndexOf('node --test tools/openskp-worker/patches.test.mjs')) 'CI provisions explicit source before required patch tests'
$tokens=$null;$errors=$null
$null=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'prepare-openskp-patch-source.ps1'),[ref]$tokens,[ref]$errors)
Check (@($errors).Count -eq 0) 'Source helper parses without running acquisition'
[ordered]@{schema_version='greenlens.openskp-patch-source-tests.v1';passed=$checks.Count;failed=0;network_requests=0;git_processes=0;converter_executions=0;model_reads=0;checks=$checks}|ConvertTo-Json -Depth 4
