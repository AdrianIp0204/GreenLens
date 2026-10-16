#Requires -Version 7.5
<#
Assemble nine distinct genuine retained runs for bounded history-cache checks.
The index contains nine { root, run_id } entries. This is test-fixture assembly,
not an application import API. Only the new manifest is composed; original
summaries, run bytes and weather assets are copied without modification.
No engine is invoked. The caller must run the required Rust admission/test.
#>
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Index,
      [Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
$historyOutput=$OutputDirectory; $historyIndex=$Index
. "$PSScriptRoot/prepare-energy-remount-fixture.ps1" -FunctionsOnly
$output=Resolve-RemountPlainPath $historyOutput
$allowed=Resolve-RemountPlainPath (Join-Path $env:LOCALAPPDATA 'GreenLens/verification')
if(!$output.StartsWith($allowed+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $output)){throw 'Require a fresh fixture beneath GreenLens verification storage'}
$entries=@(Get-Content -LiteralPath $historyIndex -Raw|ConvertFrom-Json -AsHashtable -DateKind String)
if($entries.Count -ne 9){throw 'Require exactly nine predeclared retained runs'}
$ids=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$selected=[Collections.Generic.List[object]]::new(); $weather=[ordered]@{}; $base=$null
foreach($entry in $entries){
 $root=Resolve-RemountPlainPath $entry.root
 if(!$root.StartsWith($allowed+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $output.StartsWith($root+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Source must be separate owned verification storage'}
 if(!$ids.Add($entry.run_id)){throw 'Each run must have a distinct original identity'}
 $document=Get-Content -LiteralPath (Join-Path $root 'project.json') -Raw|ConvertFrom-Json -AsHashtable -DateKind String -Depth 100
 $runs=@($document.energy.runs|Where-Object id -CEQ $entry.run_id)
 if($runs.Count -ne 1 -or $runs[0].path -cne "runs/$($entry.run_id)"){throw 'Declared run is not present under its original safe path'}
 $summary=$runs[0]; $before=Get-RemountInventory (Join-Path $root $summary.path)
 $selected.Add(@{root=$root;summary=$summary;before=$before})
 if($null -eq $base){$base=$document}
 foreach($asset in $document.energy.weather){
  if($asset.path -cne "weather/$($asset.id)" -or $asset.id -cnotmatch '^[0-9a-f-]{36}$'){throw 'Unsafe weather asset identity'}
  $inventory=Get-RemountInventory (Join-Path $root $asset.path)
  if($weather.Contains($asset.id)){
   if(($weather[$asset.id].asset|ConvertTo-Json -Depth 100 -Compress) -cne ($asset|ConvertTo-Json -Depth 100 -Compress)){throw 'Conflicting weather summary identity'}
   Assert-RemountSameInventory $weather[$asset.id].before $inventory
  }else{$weather[$asset.id]=@{root=$root;asset=$asset;before=$inventory}}
 }
}
# A minimal source manifest avoids unrelated architectural/workbench/session
# dependencies. Do not strip dependencies from an arbitrary rich project.
if($base.Contains('architecture') -or $base.Contains('energy_workbench') -or @($base.runs).Count -ne 0 -or ($base.energy.Contains('sensitivity_sessions') -and @($base.energy.sensitivity_sessions).Count -ne 0)){throw 'The first indexed source must be a minimal energy-only project without dependent history'}
$base.energy.runs=@($selected|ForEach-Object {$_.summary})
$base.energy.weather=@($weather.Values|ForEach-Object {$_.asset})
New-Item -ItemType Directory -Path $output|Out-Null
New-Item -ItemType Directory -Path (Join-Path $output 'runs'),(Join-Path $output 'weather')|Out-Null
foreach($item in $selected){Copy-Item -LiteralPath (Join-Path $item.root $item.summary.path) -Destination (Join-Path $output $item.summary.path) -Recurse}
foreach($item in $weather.Values){Copy-Item -LiteralPath (Join-Path $item.root $item.asset.path) -Destination (Join-Path $output $item.asset.path) -Recurse}
$base|ConvertTo-Json -Depth 100|Set-Content -LiteralPath (Join-Path $output 'project.json') -Encoding utf8NoBOM
$all=Get-RemountInventory $output
foreach($item in $selected){
 Assert-RemountSameInventory $item.before (Get-RemountInventory (Join-Path $item.root $item.summary.path))
 Assert-RemountSameInventory $item.before (Get-RemountInventory (Join-Path $output $item.summary.path))
}
foreach($item in $weather.Values){Assert-RemountSameInventory $item.before (Get-RemountInventory (Join-Path $output $item.asset.path))}
[ordered]@{schema='greenlens.energy-history-fixture.v1';selected_run_ids=@($selected.summary.id);files=$all.Count;bytes=($all.Values.bytes|Measure-Object -Sum).Sum;native_simulations=0;run_bytes_unchanged=$true;rust_admission='required, not yet executed'}|ConvertTo-Json -Depth 10
