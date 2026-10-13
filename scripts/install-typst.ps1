#Requires -Version 7.2
param(
 [string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA 'GreenLens/renderers/typst-0.15.1'),
 [string]$ArchivePath = '',
 [string]$FontCacheDirectory = ''
)
$ErrorActionPreference = 'Stop'
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../dependencies/typst.json') -Raw | ConvertFrom-Json
$root = [IO.Path]::GetFullPath($InstallDirectory)
$allowed = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens')).TrimEnd('\') + '\'
if (!$root.StartsWith($allowed,[StringComparison]::OrdinalIgnoreCase)) { throw 'Renderer installation must be below LOCALAPPDATA/GreenLens, outside synced source.' }
function Assert-Regular([string]$Path) {
 $item = Get-Item -LiteralPath $Path -Force
 if ($item.PSIsContainer -or ($item.Attributes -band ([IO.FileAttributes]::ReparsePoint -bor [IO.FileAttributes]::Offline))) { throw 'Expected an available regular file.' }
}
function Assert-Seal([string]$Path,$Seal) {
 Assert-Regular $Path
 if ((Get-Item -LiteralPath $Path).Length -ne $Seal.bytes -or (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Seal.sha256) { throw "Pinned renderer resource differs: $([IO.Path]::GetFileName($Path))" }
}
$ancestor=$root
while($ancestor) {
 if((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Installation may not traverse a reparse point.'}
 $ancestor=[IO.Path]::GetDirectoryName($ancestor)
}
New-Item -ItemType Directory -Path $root -Force | Out-Null
if (!$ArchivePath) {
 # Network is installation-only. Report generation never invokes this script.
 $release = Invoke-RestMethod -Uri $manifest.release_api
 $asset = @($release.assets | Where-Object name -eq $manifest.asset)
 if ($asset.Count -ne 1 -or $asset[0].digest -ne "sha256:$($manifest.sha256)" -or $asset[0].size -ne $manifest.bytes -or $asset[0].browser_download_url -ne $manifest.download_url) { throw 'Official release asset differs from reviewed pin.' }
 $ArchivePath = Join-Path $root $manifest.asset
 if (!(Test-Path -LiteralPath $ArchivePath)) { Invoke-WebRequest -Uri $manifest.download_url -OutFile $ArchivePath }
}
Assert-Seal $ArchivePath $manifest
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($ArchivePath)
try {
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
 $total=0L
 foreach($entry in $zip.Entries) {
  if($entry.FullName -eq "$($manifest.archive_root)/" -and $entry.Length -eq 0){continue}
  $name=$entry.FullName
  if(!$name.StartsWith("$($manifest.archive_root)/",[StringComparison]::Ordinal) -or $name.Contains('\')){throw 'Unexpected archive path.'}
  $leaf=$name.Substring($manifest.archive_root.Length+1)
  $seal=$manifest.members.PSObject.Properties[$leaf]
  if(!$seal -or !$seen.Add($leaf) -or $leaf -match '[/\\:]' -or $entry.Length -ne $seal.Value.bytes){throw 'Unexpected or duplicate archive member.'}
  $total += $entry.Length
  if($entry.Length -gt 64MB -or $total -gt 80MB){throw 'Renderer archive exceeds fixed bounds.'}
  $input=$entry.Open();$buffer=[IO.MemoryStream]::new()
  try {
   $chunk=[byte[]]::new(65536);$count=0L
   while(($n=$input.Read($chunk,0,[int][Math]::Min(65536,$seal.Value.bytes-$count+1))) -gt 0){$count+=$n;if($count -gt $seal.Value.bytes){throw 'Expanded member exceeds declared bytes.'};$buffer.Write($chunk,0,$n)}
   if($count -ne $seal.Value.bytes){throw 'Truncated renderer member.'}
   $bytes=$buffer.ToArray();$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
   if($hash -ne $seal.Value.sha256){throw 'Expanded renderer member hash mismatch.'}
   $destination=Join-Path $root $leaf
   if(Test-Path -LiteralPath $destination){Assert-Seal $destination $seal.Value}else{[IO.File]::WriteAllBytes($destination,$bytes)}
  } finally {$input.Dispose();$buffer.Dispose()}
 }
 if($seen.Count -ne @($manifest.members.PSObject.Properties).Count){throw 'Renderer archive member closure failed.'}
} finally {$zip.Dispose()}
$fonts=Join-Path $root 'fonts';New-Item -ItemType Directory -Path $fonts -Force | Out-Null
if((Get-Item -LiteralPath $fonts -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Font directory may not be a reparse point.'}
foreach($font in @($manifest.fonts.files)+@($manifest.fonts.license_file)) {
 $path=Join-Path $fonts $font.name
 if(!(Test-Path -LiteralPath $path)) {
  if($FontCacheDirectory){$source=Join-Path $FontCacheDirectory $font.name;Assert-Seal $source $font;Copy-Item -LiteralPath $source -Destination $path}
  else{Invoke-WebRequest -Uri $font.url -OutFile $path}
 }
 Assert-Seal $path $font
}
$version=& (Join-Path $root 'typst.exe') --version
if($LASTEXITCODE -ne 0 -or $version -ne 'typst 0.15.1 (9dfd3a08)'){throw 'Pinned executable version check failed.'}
[ordered]@{schema_version='greenlens.typst-install.v1';version=$version;binary_sha256=$manifest.members.'typst.exe'.sha256;fonts_verified=$manifest.fonts.files.Count;redistribution_approved=$false;path=$root}|ConvertTo-Json
$global:LASTEXITCODE=0
