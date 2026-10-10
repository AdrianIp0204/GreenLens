#Requires -Version 7.2
param([string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/engines/radiance-6.0.2'))
$ErrorActionPreference = 'Stop'
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../dependencies/radiance.json') -Raw | ConvertFrom-Json
$releaseHeaders = @{ Accept = 'application/vnd.github+json' }
if ($env:GITHUB_TOKEN) {
    # The CI token is sent only to this fixed official API endpoint, never to
    # release-asset redirects. Do not print or persist authorization headers.
    if ($manifest.release_api -ne 'https://api.github.com/repos/LBNL-ETA/Radiance/releases/tags/rad6R0P2') { throw 'Unexpected release API endpoint for authenticated verification.' }
    $releaseHeaders.Authorization = "Bearer $env:GITHUB_TOKEN"
}
$release = Invoke-RestMethod -Uri $manifest.release_api -Headers $releaseHeaders -MaximumRedirection 0
$asset = @($release.assets | Where-Object name -eq $manifest.asset)
if ($asset.Count -ne 1 -or $asset[0].digest -ne "sha256:$($manifest.sha256)" -or $asset[0].size -ne $manifest.bytes -or $asset[0].browser_download_url -ne $manifest.download_url) {
    throw 'Official release asset no longer matches the pinned manifest; review upstream before changing the pin.'
}
New-Item -ItemType Directory -Path $InstallDirectory -Force | Out-Null
$archivePath = Join-Path $InstallDirectory $manifest.asset
if (!(Test-Path -LiteralPath $archivePath)) { Invoke-WebRequest -Uri $manifest.download_url -OutFile $archivePath }
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant() -ne $manifest.sha256) { throw 'Radiance archive SHA-256 mismatch; archive was not extracted.' }
$packagePath = Join-Path $InstallDirectory 'package'
if (!(Test-Path -LiteralPath $packagePath)) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
    try {
        foreach ($entry in $zip.Entries) {
            $relative = $entry.FullName.Replace('\','/')
            if ($relative.StartsWith('/') -or $relative -match '(^|/)\.\.(/|$)|:') { throw "Unsafe ZIP entry: $relative" }
        }
    } finally { $zip.Dispose() }
    Expand-Archive -LiteralPath $archivePath -DestinationPath $packagePath
}
foreach ($name in @('gensky','oconv','rtrace')) {
    $path = Join-Path $packagePath "bin/$name.exe"
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant() -ne $manifest.executables.$name.sha256) { throw "Installed $name differs from pinned binary" }
}
foreach ($name in @('rayinit.cal','skybright.cal')) {
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $packagePath "lib/$name")).Hash.ToLowerInvariant() -ne $manifest.critical_libraries.$name.sha256) { throw "Installed library $name differs from pinned package" }
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../third-party/Radiance-LICENSE.txt') -Destination (Join-Path $InstallDirectory 'Radiance-LICENSE.txt') -Force
Write-Output "Verified separate upstream Radiance installation: $packagePath"
Write-Output 'No PATH or global environment configuration changed. GreenLens discovers the default per-user location.'
