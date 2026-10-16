#Requires -Version 7.2
# Source-only CI prerequisite. No converter build, model read, or runtime adoption.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-OpenSkpPatchSpec {
    [pscustomobject]@{
        origin = 'https://github.com/iamahsanmehmood/openskp.git'
        commit = 'dceb9fff0efc9668961530b535d5375c001343dc'
        # Git's exact LF blob and the separately retained reviewed CRLF notice.
        license_sha256 = '7c70ac0b7916390744a5f6361c59008cada08149a54348530b767953e050dbc6'
        notice_sha256 = '837fabd97124295917c7317d51290182760c4b36f008a0f2323cd3c4fc8dbbea'
        files = @('LICENSE', 'packages/cpp/src/internal.hpp', 'packages/cpp/src/core.cpp',
            'packages/cpp/src/tlv.cpp', 'packages/cpp/src/geometry.cpp', 'packages/cpp/src/legacy.cpp',
            'packages/cpp/src/face_groups.cpp', 'packages/cpp/src/triangulator.cpp',
            'packages/cpp/src/earcut.cpp', 'packages/cpp/include/openskp/instanced_scene.hpp',
            'packages/cpp/src/instanced_scene.cpp', 'packages/cpp/src/instanced_glb.cpp', 'packages/cpp/src/vff.cpp')
    }
}

function Assert-OpenSkpPatchIdentity([string]$Origin, [string]$Commit, [string]$Status, [string]$LicenseSha256) {
    $spec = Get-OpenSkpPatchSpec
    if ($Origin -cne $spec.origin) { throw 'Pinned patch-test source has an unexpected origin; preserve it and inspect the conflict.' }
    if ($Commit -cne $spec.commit) { throw 'Pinned patch-test source has an unexpected commit; no checkout is overwritten.' }
    if ($Status.Length -ne 0) { throw 'Pinned patch-test source is modified or contains untracked files; preserve it.' }
    if ($LicenseSha256 -cne $spec.license_sha256) { throw 'Pinned patch-test source license digest differs.' }
}

function Assert-OpenSkpPatchBytes([byte[]]$Actual, [byte[]]$Pinned, [string]$Relative) {
    if ($Actual.Length -gt 4MB -or $Pinned.Length -gt 4MB) { throw "Patch-test source exceeds the 4 MiB file bound: $Relative" }
    $actualHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Actual))
    $pinnedHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Pinned))
    if ($actualHash -cne $pinnedHash) { throw "Patch-test working bytes differ from the pinned Git blob: $Relative" }
}

function Select-OpenSkpGitExecutable([object[]]$Commands) {
    if ($Commands.Count -eq 0 -or [string]::IsNullOrWhiteSpace([string]$Commands[0].Source)) { throw 'Git executable unavailable.' }
    return [string]$Commands[0].Source
}

function Assert-OpenSkpPatchRoot([string]$Prefix, [string]$GitDirectory, [string]$Bare) {
    # Git can return a package-virtualized physical path for a valid logical
    # LOCALAPPDATA alias. Establish repository-root context without guessing it.
    if ($Prefix.Length -ne 0 -or $GitDirectory -cne '.git' -or $Bare -cne 'false') {
        throw 'Patch-test cache is not its own non-bare repository root; preserve it.'
    }
}

function Invoke-OpenSkpSourceGit([string[]]$Arguments) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Select-OpenSkpGitExecutable @(Get-Command git -CommandType Application -ErrorAction Stop)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [Text.UTF8Encoding]::new($false, $true)
    $start.StandardErrorEncoding = [Text.UTF8Encoding]::new($false, $true)
    $start.Environment['GIT_TERMINAL_PROMPT'] = '0'
    $start.Environment['GCM_INTERACTIVE'] = 'Never'
    foreach ($variable in @('GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE')) { $null = $start.Environment.Remove($variable) }
    foreach ($argument in @('-c', 'core.longpaths=true', '-c', 'core.autocrlf=false', '-c', 'protocol.file.allow=never') + $Arguments) {
        $start.ArgumentList.Add($argument)
    }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (!$process.Start()) { throw 'Could not start source-only Git operation.' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (!$process.WaitForExit(180000)) {
            $process.Kill($true)
            $process.WaitForExit()
            throw 'Source-only Git operation exceeded 180 seconds; partial source is retained.'
        }
        $output = $stdout.GetAwaiter().GetResult()
        $errors = $stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "Source-only Git operation failed (exit $($process.ExitCode)); no existing source is reset." }
        if ($output.Length -gt 4MB -or $errors.Length -gt 4MB) { throw 'Source-only Git output exceeded its 4 MiB bound.' }
        return $output
    } finally { $process.Dispose() }
}

function Initialize-OpenSkpPatchSource {
    if (!$env:LOCALAPPDATA) { throw 'LOCALAPPDATA is required for external source-only acquisition.' }
    $spec = Get-OpenSkpPatchSpec
    $repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
    $manifest = Get-Content -LiteralPath (Join-Path $repository 'tools/openskp-worker/dependencies.json') -Raw | ConvertFrom-Json
    $component = $manifest.components[0]
    $patches = Get-Content -LiteralPath (Join-Path $repository 'tools/openskp-worker/patches.mjs') -Raw
    if ($component.commit -cne $spec.commit -or ($component.source + '.git') -cne $spec.origin -or
        $component.notice_sha256 -cne $spec.notice_sha256 -or !$patches.Contains("export const pin = '$($spec.commit)';")) {
        throw 'Source-only acquisition pin disagrees with the reviewed dependency/patch contract.'
    }
    $destination = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA "GreenLens/dependencies/openskp-patch-tests/$($spec.commit)"))
    for ($ancestor = $destination; $ancestor; $ancestor = [IO.Path]::GetDirectoryName($ancestor)) {
        if ((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Source-only destination ancestry contains a reparse point.'
        }
    }
    if (!(Test-Path -LiteralPath $destination)) {
        New-Item -ItemType Directory -Path $destination | Out-Null
        $null = Invoke-OpenSkpSourceGit @('init', $destination)
        $null = Invoke-OpenSkpSourceGit @('-C', $destination, 'remote', 'add', 'origin', $spec.origin)
        $null = Invoke-OpenSkpSourceGit @('-C', $destination, 'config', 'core.sparseCheckout', 'true')
        # Only reviewed text sources and their notice enter the working checkout.
        [IO.File]::WriteAllLines((Join-Path $destination '.git/info/sparse-checkout'),
            @($spec.files | ForEach-Object { '/' + $_ }), [Text.UTF8Encoding]::new($false))
        $null = Invoke-OpenSkpSourceGit @('-C', $destination, 'fetch', '--depth=1', '--filter=blob:none', '--no-tags', 'origin', $spec.commit)
        $null = Invoke-OpenSkpSourceGit @('-C', $destination, 'checkout', '--detach', $spec.commit)
    }
    # Never repair/reset a pre-existing directory. Every reuse verifies actual source bytes.
    $gitDirectory = Get-Item -LiteralPath (Join-Path $destination '.git') -Force
    if (!$gitDirectory.PSIsContainer -or ($gitDirectory.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Patch-test cache must retain its own ordinary Git directory.' }
    $prefix = (Invoke-OpenSkpSourceGit @('-C', $destination, 'rev-parse', '--show-prefix')).Trim()
    $gitPath = (Invoke-OpenSkpSourceGit @('-C', $destination, 'rev-parse', '--git-dir')).Trim()
    $bare = (Invoke-OpenSkpSourceGit @('-C', $destination, 'rev-parse', '--is-bare-repository')).Trim()
    Assert-OpenSkpPatchRoot $prefix $gitPath $bare
    $origin = (Invoke-OpenSkpSourceGit @('-C', $destination, 'remote', 'get-url', 'origin')).Trim()
    $commit = (Invoke-OpenSkpSourceGit @('-C', $destination, 'rev-parse', 'HEAD')).Trim()
    $status = (Invoke-OpenSkpSourceGit @('-C', $destination, 'status', '--porcelain', '--untracked-files=all')).Trim()
    $notice = (Get-FileHash -LiteralPath (Join-Path $destination 'LICENSE') -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert-OpenSkpPatchIdentity $origin $commit $status $notice
    $files = @()
    foreach ($relative in $spec.files) {
        $file = Join-Path $destination $relative
        $item = Get-Item -LiteralPath $file -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Unexpected source member: $relative" }
        if ($item.Length -gt 4MB) { throw "Patch-test source exceeds the 4 MiB file bound: $relative" }
        for ($ancestor = $item.DirectoryName; $ancestor -and $ancestor.Length -ge $destination.Length; $ancestor = [IO.Path]::GetDirectoryName($ancestor)) {
            if ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse source ancestry: $relative" }
        }
        $actual = [IO.File]::ReadAllBytes($file)
        $blob = [Text.Encoding]::UTF8.GetBytes((Invoke-OpenSkpSourceGit @('-C', $destination, 'cat-file', 'blob', "$($spec.commit):$relative")))
        Assert-OpenSkpPatchBytes $actual $blob $relative
        $files += [pscustomobject]@{path=$relative;bytes=$actual.Length;sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($actual)).ToLowerInvariant()}
    }
    [pscustomobject]@{schema_version='greenlens.openskp-patch-source.v1';origin=$origin;commit=$commit;source_path=(Join-Path $destination 'packages/cpp');files=$files;converter_executions=0;model_reads=0}
}

if ($MyInvocation.InvocationName -ne '.') { Initialize-OpenSkpPatchSource | ConvertTo-Json -Depth 5 }
