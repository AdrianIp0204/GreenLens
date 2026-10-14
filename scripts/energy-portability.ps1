# Full compact-evidence copy/report/export with original weather/run scratch hidden.
# Own verification inputs only; all renames are restored in finally.
[CmdletBinding(DefaultParameterSetName='Run')]
param(
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Cli,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$Project,
 [Parameter(Mandatory,ParameterSetName='Run')][string]$OutputDirectory,
 [Parameter(ParameterSetName='Run')][switch]$Replay,
 [Parameter(ParameterSetName='Run')][string]$BaselineRun,
 [Parameter(ParameterSetName='Run')][string]$ProposedRun,
 [Parameter(Mandatory,ParameterSetName='Functions')][switch]$FunctionsOnly
)
function Restore-EnergyVerificationPaths($Moves){
 $errors=[Collections.Generic.List[string]]::new()
 for($index=$Moves.Count-1;$index -ge 0;$index--){
  $move=$Moves[$index]
  try {
   if(Test-Path -LiteralPath $move.original){throw 'Original path was recreated externally; hidden evidence preserved'}
   Move-Item -LiteralPath $move.hidden -Destination $move.original -ErrorAction Stop
  }catch{$errors.Add("$($move.original): $($_.Exception.Message)")}
 }
 if($errors.Count){throw ('Cold verification restore conflicts: '+($errors -join '; '))}
}
if($FunctionsOnly){return}
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
$base=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens'))
$Project=[IO.Path]::GetFullPath($Project); $OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
if(!$Project.StartsWith((Join-Path $base 'verification')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Cold verification source must be an owned GreenLens verification project'}
if(!$OutputDirectory.StartsWith((Join-Path $base 'verification')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Cold verification outputs belong under GreenLens verification'}
if($OutputDirectory.StartsWith($Project+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $Project.StartsWith($OutputDirectory+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Cold source and output must be independent directories'}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Evidence output must be new'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$frozenCli=Join-Path $OutputDirectory 'greenlens.exe'; Copy-Item -LiteralPath $Cli -Destination $frozenCli
$cliHash=(Get-FileHash -LiteralPath $frozenCli).Hash.ToLowerInvariant()
$copy=Join-Path $OutputDirectory 'cold copy 空間'
$commands=[Collections.Generic.List[object]]::new()
function Invoke-Cli([string]$Name,[string[]]$Arguments){
 if((Get-FileHash -LiteralPath $frozenCli).Hash.ToLowerInvariant() -cne $cliHash){throw 'Frozen CLI changed'}
 $watch=[Diagnostics.Stopwatch]::StartNew()
 & $frozenCli @Arguments 1> (Join-Path $OutputDirectory "$Name.json") 2> (Join-Path $OutputDirectory "$Name.stderr.txt")
 $code=$LASTEXITCODE; $watch.Stop()
 $commands.Add([ordered]@{name=$Name;exit_code=$code;wall_seconds=$watch.Elapsed.TotalSeconds})
 ConvertTo-Json -InputObject @($commands.ToArray()) -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'commands.json') -Encoding utf8NoBOM
 if($code -ne 0){throw "Cold verification $Name failed; retained diagnostics are authoritative"}
}
Invoke-Cli 'copy' @('copy',$Project,$copy)
$doc=Get-Content -LiteralPath (Join-Path $copy 'project.json') -Raw|ConvertFrom-Json -AsHashtable
$runs=@($doc.energy.runs)
if([string]::IsNullOrEmpty($BaselineRun) -ne [string]::IsNullOrEmpty($ProposedRun)){throw 'Supply both explicit comparison run IDs or neither'}
if($BaselineRun){
 if($runs.Count -gt 4 -or @($runs|Where-Object { $_.id -ceq $BaselineRun -and $_.alternative_id -ceq 'A' }).Count -ne 1 -or @($runs|Where-Object { $_.id -ceq $ProposedRun -and $_.alternative_id -ceq 'B' }).Count -ne 1){throw 'Select one retained A/B pair within the four-run workbench verification bound'}
 $a=$BaselineRun; $b=$ProposedRun
}else{
 if($runs.Count -ne 2 -or @($runs|Where-Object alternative_id -eq 'A').Count -ne 1 -or @($runs|Where-Object alternative_id -eq 'B').Count -ne 1){throw 'Multiple retained runs require explicit BaselineRun and ProposedRun; no favorable selection is inferred'}
 $a=($runs|Where-Object alternative_id -eq A).id; $b=($runs|Where-Object alternative_id -eq B).id
}
$snapshot=[ordered]@{}
foreach($directory in @('runs','weather','energy-assessments')){
 $directoryPath=Join-Path $copy $directory
 if(Test-Path -LiteralPath $directoryPath){foreach($file in Get-ChildItem -LiteralPath $directoryPath -Recurse -File){$snapshot[[IO.Path]::GetRelativePath($copy,$file.FullName).Replace('\','/')]=(Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant()}}
}
$moves=[Collections.Generic.List[object]]::new();$oldEngine=$env:GREENLENS_ENERGYPLUS_DIR
try {
 # Only known UUID-named run scratch owned by this project's retained runs.
 foreach($target in @($Project)+@($runs|ForEach-Object { $id=[guid]::Parse($_.id).ToString(); Join-Path $base "jobs/$id" })){
  $resolved=[IO.Path]::GetFullPath($target)
  if(!$resolved.StartsWith($base+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Unsafe cold verification path'}
  if(Test-Path -LiteralPath $resolved){
   $hidden=$resolved+'.unavailable-'+[guid]::NewGuid().ToString('N')
   if((Split-Path -Parent $resolved) -cne (Split-Path -Parent $hidden) -or (Test-Path -LiteralPath $hidden)){throw 'Unsafe hidden destination'}
   Move-Item -LiteralPath $resolved -Destination $hidden
   $moves.Add(@{original=$resolved;hidden=$hidden})
  }
 }
 $env:GREENLENS_ENERGYPLUS_DIR=Join-Path $OutputDirectory 'intentionally-unavailable-engine'
 & $frozenCli energy doctor 1> (Join-Path $OutputDirectory 'missing-engine.json') 2> (Join-Path $OutputDirectory 'missing-engine.stderr.txt')
 if($LASTEXITCODE -eq 0){throw 'Engine was not made unavailable to this verification process'}
 Invoke-Cli 'inspect-A' @('energy','inspect',$copy,$a)
 Invoke-Cli 'inspect-B' @('energy','inspect',$copy,$b)
 Invoke-Cli 'report' @('energy','report',$copy,$a,$b,(Join-Path $OutputDirectory 'reports'))
 $zip=Join-Path $OutputDirectory 'energy-evidence.zip'
 Invoke-Cli 'export' @('energy','export',$copy,$a,$b,$zip)
 Invoke-Cli 'package-inspect' @('energy','package-inspect',$zip)
 foreach($entry in $snapshot.GetEnumerator()) {if((Get-FileHash -LiteralPath (Join-Path $copy $entry.Key)).Hash.ToLowerInvariant() -cne $entry.Value){throw "Immutable copy changed: $($entry.Key)"}}
 $cold=[ordered]@{schema='greenlens.energy-cold-portability.v1';cli_sha256=$cliHash;original_project_unavailable=$true;original_job_scratch_unavailable=$true;engine_discovery_failed=$true;retained_files=$snapshot;report_and_export_completed=$true;package_bytes=(Get-Item -LiteralPath $zip).Length;package_sha256=(Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant();scientific_qualification_asserted=$false}
 $cold|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'cold-evidence.json') -Encoding utf8NoBOM
} finally {
 $env:GREENLENS_ENERGYPLUS_DIR=$oldEngine
 Restore-EnergyVerificationPaths $moves
}
if($Replay){Invoke-Cli 'replay' @('energy','replay',(Join-Path $OutputDirectory 'energy-evidence.zip'))}
Write-Output $OutputDirectory
