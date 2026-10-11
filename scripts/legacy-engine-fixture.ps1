# Shared fixture setup for the required legacy-only replay and its native regression.
# This copies the separately installed engine into disposable local storage; it is
# not part of the application/evidence distribution.
function New-LegacyEngineFixture {
    param([Parameter(Mandatory)][string]$Source, [Parameter(Mandatory)][string]$Destination)
    $sourcePath = [IO.Path]::GetFullPath($Source)
    $destinationPath = [IO.Path]::GetFullPath($Destination)
    # Unicode working/project directories are supported with relative input paths,
    # but a Unicode absolute RAYPATH reproducibly fails at rayinit.cal startup in
    # the pinned Windows package. Do not infer engine-path support from cwd tests.
    if ($destinationPath -match '[^\x00-\x7F]') {
        throw 'The pinned Windows Radiance CAL-library path must contain only ASCII characters. Supply -StaticEngineDirectory at a new ASCII-only local path; keep the project/evidence directory unchanged.'
    }
    if (Test-Path -LiteralPath $destinationPath) { throw 'The static engine fixture destination must be new; existing files are preserved.' }
    $manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../dependencies/radiance.json') -Raw | ConvertFrom-Json
    $relativePaths = [Collections.Generic.List[string]]::new()
    foreach ($name in @('gensky', 'oconv', 'rtrace')) {
        $relative = "bin/$name.exe"
        if ((Get-FileHash -LiteralPath (Join-Path $sourcePath $relative)).Hash.ToLowerInvariant() -ne $manifest.executables.$name.sha256) {
            throw "The source $name executable differs from the pinned Radiance engine."
        }
        $relativePaths.Add($relative)
    }
    $relativePaths.Add('bin/vcruntime140.dll')
    $libraryRoot = Join-Path $sourcePath 'lib'
    if (!(Test-Path -LiteralPath $libraryRoot -PathType Container)) { throw 'Source Radiance library directory is missing.' }
    if ((Get-Item -LiteralPath $libraryRoot).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Engine fixture must not follow library links.' }
    foreach ($entry in Get-ChildItem -LiteralPath $libraryRoot -Recurse -Force) {
        if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Engine fixture must not follow library links.' }
        if (!$entry.PSIsContainer) { $relativePaths.Add([IO.Path]::GetRelativePath($sourcePath, $entry.FullName).Replace('\','/')) }
    }
    $sourceHashes = [ordered]@{}
    foreach ($relative in ($relativePaths | Sort-Object)) {
        $sourceHashes[$relative] = (Get-FileHash -LiteralPath (Join-Path $sourcePath $relative)).Hash.ToLowerInvariant()
    }
    foreach ($name in @('rayinit.cal', 'skybright.cal')) {
        if ($sourceHashes["lib/$name"] -ne $manifest.critical_libraries.$name.sha256) { throw "Source library $name differs from the pinned Radiance engine." }
    }
    New-Item -ItemType Directory -Path $destinationPath | Out-Null
    foreach ($relative in $sourceHashes.Keys) {
        $target = Join-Path $destinationPath $relative
        New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $sourcePath $relative) -Destination $target
        if ((Get-FileHash -LiteralPath $target).Hash.ToLowerInvariant() -ne $sourceHashes[$relative]) { throw "Engine fixture copy differs: $relative" }
    }
    $executables = @(Get-ChildItem -LiteralPath (Join-Path $destinationPath 'bin') -Filter '*.exe' -File | Sort-Object Name | ForEach-Object Name)
    if (($executables -join ',') -ne 'gensky.exe,oconv.exe,rtrace.exe') { throw 'Legacy fixture must contain exactly the three static executables.' }
    $hashBytes = [Text.Encoding]::UTF8.GetBytes(($sourceHashes | ConvertTo-Json -Compress))
    return [pscustomobject]@{
        schema_version = 'greenlens.legacy-engine-fixture.v1'
        engine_release = $manifest.release
        file_count = $sourceHashes.Count
        copied_files_sha256 = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($hashBytes)).ToLowerInvariant()
        source_and_copy_identical = $true
        library_path_ascii = $true
        engine_executables = $executables
        annual_executables_present = $false
    }
}
