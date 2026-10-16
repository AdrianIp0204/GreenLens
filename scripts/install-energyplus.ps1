param([string]$InstallRoot = (Join-Path $env:LOCALAPPDATA 'GreenLens/engines/energyplus-26.2.0'), [switch]$VerifyOnly)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$manifestPath = Join-Path $PSScriptRoot '../dependencies/energyplus.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -AsHashtable
if ($manifest.schema -ne 'greenlens.energyplus-dependency.v1' -or $manifest.platform -ne 'windows-x86_64') { throw 'Unsupported EnergyPlus dependency manifest' }
$InstallRoot = [IO.Path]::GetFullPath($InstallRoot)
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar
if (($InstallRoot + [IO.Path]::DirectorySeparatorChar).StartsWith($sourceRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Install EnergyPlus outside the source checkout' }
function Assert-NoReparse([string]$Path) {
    for ($p = [IO.Path]::GetFullPath($Path); $p; $p = [IO.Path]::GetDirectoryName($p)) {
        if ((Test-Path -LiteralPath $p) -and ((Get-Item -LiteralPath $p -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Reparse path is unsupported: $p" }
    }
}
function Assert-File([string]$Path, [long]$Bytes, [string]$Hash) {
    Assert-NoReparse $Path
    if (!(Test-Path -LiteralPath $Path -PathType Leaf) -or (Get-Item -LiteralPath $Path).Length -ne $Bytes -or (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Hash) { throw "Pinned file mismatch: $Path" }
}
function Get-EnergyPlusRelease([string]$ReleaseApi) {
    # Keep optional CI credentials on this exact metadata endpoint only. Asset
    # downloads remain unauthenticated; never print or persist request headers.
    if ($ReleaseApi -cne 'https://api.github.com/repos/NatLabRockies/EnergyPlus/releases/tags/v26.2.0') { throw 'Unexpected EnergyPlus release API endpoint' }
    $releaseHeaders = @{ Accept = 'application/vnd.github+json' }
    if (![string]::IsNullOrWhiteSpace($env:GITHUB_TOKEN)) { $releaseHeaders.Authorization = "Bearer $env:GITHUB_TOKEN" }
    return Invoke-RestMethod -Uri $ReleaseApi -Headers $releaseHeaders -MaximumRedirection 0
}
Assert-NoReparse $InstallRoot
$archive = Join-Path $InstallRoot $manifest.asset
$packageParent = Join-Path $InstallRoot 'package'
$engineRoot = Join-Path $packageParent $manifest.package_directory
if (!$VerifyOnly) {
    New-Item -ItemType Directory -Path $InstallRoot -Force | Out-Null
    if (!(Test-Path -LiteralPath $archive)) {
        $release = Get-EnergyPlusRelease $manifest.release_api
        $asset = @($release.assets | Where-Object { $_.id -eq $manifest.asset_id })
        if ($release.tag_name -cne $manifest.tag -or $asset.Count -ne 1 -or $asset[0].name -cne $manifest.asset -or $asset[0].size -ne $manifest.bytes -or $asset[0].digest -cne ('sha256:' + $manifest.sha256) -or $asset[0].browser_download_url -cne $manifest.download_url) { throw 'Official release metadata differs from the pinned dependency' }
        $partial = Join-Path $InstallRoot ('download-' + [guid]::NewGuid().ToString('N') + '.partial')
        Invoke-WebRequest -Uri $manifest.download_url -OutFile $partial
        Assert-File $partial $manifest.bytes $manifest.sha256
        Move-Item -LiteralPath $partial -Destination $archive
        $release | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $InstallRoot 'official-release.json') -Encoding utf8
    }
}
Assert-File $archive $manifest.bytes $manifest.sha256
if (!(Test-Path -LiteralPath $engineRoot)) {
    if ($VerifyOnly) { throw 'Pinned EnergyPlus installation is absent' }
    if (Test-Path -LiteralPath $packageParent) { throw 'Incomplete package directory exists; preserve it and choose a fresh installation root' }
    $stage = Join-Path $InstallRoot ('extract-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $stage | Out-Null
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        if ($zip.Entries.Count -ne $manifest.zip_entries -or $zip.Entries.Count -gt $manifest.extraction_limits.entries) { throw 'ZIP entry count differs' }
        $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $total = 0L
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\','/')
            if (!$name.StartsWith($manifest.package_directory + '/', [StringComparison]::Ordinal) -or $name.Contains(':') -or $name.StartsWith('/') -or $name -match '(^|/)\.\.?(/|$)' -or !$names.Add($name.TrimEnd('/'))) { throw "Unsafe or duplicate ZIP member: $name" }
            if ((($entry.ExternalAttributes -shr 16) -band 0xF000) -eq 0xA000) { throw 'ZIP symbolic links are unsupported' }
            if ($entry.Length -gt $manifest.extraction_limits.entry_bytes) { throw 'ZIP member exceeds declared limit' }
            $total += $entry.Length
            if ($total -gt $manifest.extraction_limits.uncompressed_bytes) { throw 'ZIP exceeds declared total limit' }
        }
        if ($total -ne $manifest.zip_uncompressed_bytes) { throw 'ZIP expanded size differs' }
        $actualTotal = 0L
        $buffer = [byte[]]::new(65536)
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\','/')
            $target = [IO.Path]::GetFullPath((Join-Path $stage $name))
            if (!$target.StartsWith($stage + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'ZIP target escapes staging' }
            if ($name.EndsWith('/')) { New-Item -ItemType Directory -Path $target -Force | Out-Null; continue }
            New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($target)) -Force | Out-Null
            $inputStream = $entry.Open(); $outputStream = [IO.File]::Open($target, [IO.FileMode]::CreateNew)
            try {
                $actual = 0L
                while (($count = $inputStream.Read($buffer,0,$buffer.Length)) -gt 0) {
                    $actual += $count; $actualTotal += $count
                    if ($actual -gt $entry.Length -or $actual -gt $manifest.extraction_limits.entry_bytes -or $actualTotal -gt $manifest.extraction_limits.uncompressed_bytes) { throw 'Actual decompression exceeds declared bounds' }
                    $outputStream.Write($buffer,0,$count)
                }
                if ($actual -ne $entry.Length) { throw 'Actual ZIP member length differs' }
            } finally { $outputStream.Dispose(); $inputStream.Dispose() }
        }
        if ($actualTotal -ne $manifest.zip_uncompressed_bytes) { throw 'Actual expanded package size differs' }
        foreach ($name in $manifest.critical_files.Keys) { $pin=$manifest.critical_files[$name]; Assert-File (Join-Path (Join-Path $stage $manifest.package_directory) $name) $pin.bytes $pin.sha256 }
    } finally { $zip.Dispose() }
    # Only an entirely verified fresh staging directory is promoted. Failed stages remain inspectable.
    Move-Item -LiteralPath $stage -Destination $packageParent
}
foreach ($name in $manifest.critical_files.Keys) { $pin=$manifest.critical_files[$name]; Assert-File (Join-Path $engineRoot $name) $pin.bytes $pin.sha256 }
[ordered]@{schema='greenlens.energyplus-installation.v1';version=$manifest.version;engine_root=$engineRoot;archive_sha256=$manifest.sha256;verified_files=$manifest.critical_files.Count;native_execution_verified=$false} | ConvertTo-Json
