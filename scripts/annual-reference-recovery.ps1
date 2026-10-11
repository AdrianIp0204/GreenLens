# Explicit recovery of the first six successful engine processes after the
# original reporting-only PowerShell rung-expression failure. No source evidence
# is modified; commands always come from the trusted current harness.
. "$PSScriptRoot/annual-reference-identity.ps1"
function Read-ReferenceRecoveryJson([string]$Path){
 $options=@{InputObject=[IO.File]::ReadAllText($Path);Depth=100}
 if((Get-Command ConvertFrom-Json).Parameters.ContainsKey('DateKind')){$options.DateKind='String'}
 ConvertFrom-Json @options
}
function Test-ReferenceRecoveryNumber($Value,[double]$Minimum,[double]$Maximum){
 if($Value -isnot [byte] -and $Value -isnot [int16] -and $Value -isnot [int32] -and $Value -isnot [int64] -and $Value -isnot [single] -and $Value -isnot [double] -and $Value -isnot [decimal]){return $false}
 return [double]::IsFinite([double]$Value) -and [double]$Value -ge $Minimum -and [double]$Value -le $Maximum
}
function Initialize-ReferenceRecovery([string]$SourceDirectory,$Declaration){
 $source=[IO.Path]::GetFullPath($SourceDirectory)
 if($source.Equals([IO.Path]::GetFullPath($OutputDirectory),[StringComparison]::OrdinalIgnoreCase)){throw 'Recovery must use a new isolated directory'}
 foreach($name in @('declaration.json','harness-source.ps1','processes.json')){
  $item=Get-Item -LiteralPath (Join-Path $source $name)
  if($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Recovery source must contain regular files'}
 }
 $originalDeclaration=Read-ReferenceRecoveryJson (Join-Path $source 'declaration.json')
 $originalHash=(Get-FileHash -LiteralPath (Join-Path $source 'declaration.json')).Hash.ToLowerInvariant()
 $harnessHash=(Get-FileHash -LiteralPath (Join-Path $source 'harness-source.ps1')).Hash.ToLowerInvariant()
 $processHash=(Get-FileHash -LiteralPath (Join-Path $source 'processes.json')).Hash.ToLowerInvariant()
 if($harnessHash -cne '18646902250606280f96eff3325ff47f669772bb1f9f1f7154401f9429ec017f' -or $originalDeclaration.harness_sha256 -cne $harnessHash){throw 'Only the explicitly identified reporting-bug harness may be recovered'}
 if(Test-Path -LiteralPath (Join-Path $source 'controlled-reference.json')){throw 'This bounded recovery only handles the failure before its first case report'}
 $originalProcesses=@(Read-ReferenceRecoveryJson (Join-Path $source 'processes.json'))
 if($originalProcesses.Count -ne 6){throw 'Recovery requires exactly six completed original engine processes'}
 # Scientific declarations and every input must be identical. Only date and
 # reporting implementation identity change in the newly sealed declaration.
 foreach($property in $originalDeclaration.PSObject.Properties){
  if($property.Name -in @('declared_utc','harness_sha256')){continue}
  $current=$Declaration[$property.Name]
  if((ConvertTo-ReferenceCanonicalJson $property.Value) -cne (ConvertTo-ReferenceCanonicalJson $current)){throw "Recovery changed declared field $($property.Name)"}
 }
 if(@($originalDeclaration.PSObject.Properties).Count -ne $Declaration.Count){throw 'Recovery declaration field set differs'}
 foreach($property in $originalDeclaration.input_sha256.PSObject.Properties){
  $name=$property.Name;if($name -notmatch '^[A-Za-z0-9_.-]+$'){throw 'Unsafe recovery input name'}
  if((Get-FileHash -LiteralPath (Join-Path $source $name)).Hash.ToLowerInvariant() -cne $property.Value -or (Get-FileHash -LiteralPath (Join-Path $OutputDirectory $name)).Hash.ToLowerInvariant() -cne $property.Value){throw "Recovery input bytes changed: $name"}
 }
 $index=[int]$Declaration.executed_interval_indices[0];$hour="hour-$index"
 $outputs=@('transmission.oct',"$hour.oct","$hour-sun.oct","$hour-direct.rgb","$hour-finite.rgb","$hour-rounds256.rgb")
 $members=[ordered]@{}
 foreach($name in @($outputs)+@("$hour-rounds256.pts")){
  $item=Get-Item -LiteralPath (Join-Path $source $name)
  if($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -gt 536870912){throw 'Recovery member is not a bounded regular file'}
  $hash=(Get-FileHash -LiteralPath $item.FullName).Hash.ToLowerInvariant()
  Copy-Item -LiteralPath $item.FullName -Destination (Join-Path $OutputDirectory $name)
  if((Get-FileHash -LiteralPath (Join-Path $OutputDirectory $name)).Hash.ToLowerInvariant() -cne $hash -or (Get-FileHash -LiteralPath $item.FullName).Hash.ToLowerInvariant() -cne $hash){throw 'Recovery copy or source changed'}
  $members[$name]=@{sha256=$hash;bytes=$item.Length}
 }
 foreach($pair in @(@('declaration.json','recovery-original-declaration.json'),@('harness-source.ps1','recovery-original-harness.ps1'),@('processes.json','recovery-original-processes.json'))){Copy-Item -LiteralPath (Join-Path $source $pair[0]) -Destination (Join-Path $OutputDirectory $pair[1])}
 Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-recovery.ps1" -Destination (Join-Path $OutputDirectory 'recovery-helper.ps1')
 Copy-Item -LiteralPath "$PSScriptRoot/annual-reference-identity.ps1" -Destination (Join-Path $OutputDirectory 'recovery-identity-helper.ps1')
 if((Get-FileHash -LiteralPath (Join-Path $source 'processes.json')).Hash.ToLowerInvariant() -cne $processHash){throw 'Original process record changed during recovery'}
 $manifest=[ordered]@{schema_version='greenlens.controlled-reference-recovery.v1';status='prepared';reason='Original engine processes succeeded; PowerShell unparenthesized repeat-rung expression failed before reporting';original_declaration_sha256=$originalHash;original_harness_sha256=$harnessHash;original_processes_sha256=$processHash;corrected_declaration_sha256=(Get-FileHash -LiteralPath (Join-Path $OutputDirectory 'declaration.json')).Hash.ToLowerInvariant();helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-reference-recovery.ps1").Hash.ToLowerInvariant();identity_helper_sha256=(Get-FileHash -LiteralPath "$PSScriptRoot/annual-reference-identity.ps1").Hash.ToLowerInvariant();copied_process_count=6;reused_process_count=0;copied_members=$members;created_utc=[DateTime]::UtcNow.ToString('o');release_acceptance_complete=$false}
 $script:referenceRecovery=@{manifest=$manifest;original_processes=$originalProcesses;outputs=$outputs;cursor=0;original_sensors=[IO.File]::ReadAllText((Join-Path $source 'sensors.pts'));original_primary=[IO.File]::ReadAllText((Join-Path $source "$hour-rounds256.pts"))}
 $manifest|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'recovery.json') -Encoding utf8NoBOM
}
function Invoke-RecoverableReferenceEngine([string]$Name,[string[]]$Arguments,[string]$Output,[string]$InputText=''){
 $state=$script:referenceRecovery
 if($null -eq $state -or $state.cursor -ge 6){Invoke-ReferenceProcess -Executable (Join-Path $EngineRoot "bin/$Name.exe") -Name $Name -Arguments $Arguments -Output $Output -InputText $InputText;return}
 $cursor=$state.cursor;$record=$state.original_processes[$cursor]
 if($Output -cne $state.outputs[$cursor] -or $record.executable -cne "$Name.exe" -or (ConvertTo-Json -InputObject @($record.arguments) -Compress) -cne (ConvertTo-Json -InputObject @($Arguments) -Compress)){throw 'Recovered process order/arguments differ from the current trusted command'}
 if($record.completed -isnot [bool] -or !$record.completed -or $record.exit_code -ne 0 -or $null -ne $record.terminated_tree_reason -or !(Test-ReferenceRecoveryNumber $record.wall_seconds 0 7200) -or !(Test-ReferenceRecoveryNumber $record.sampled_runner_plus_engine_peak_bytes 1 2147483648) -or !(Test-ReferenceRecoveryNumber $record.sampled_scratch_peak_bytes 0 2147483648)){throw 'Only successful bounded original processes may be reused'}
 if($null -ne $record.sampled_peak_working_set_bytes -and !(Test-ReferenceRecoveryNumber $record.sampled_peak_working_set_bytes 0 2147483648)){throw 'Invalid original child memory measurement'}
 $started=[DateTimeOffset]::Parse($record.started_utc);$finished=[DateTimeOffset]::Parse($record.finished_utc)
 if($started.Offset.Ticks -ne 0 -or $finished.Offset.Ticks -ne 0 -or $finished -lt $started){throw 'Original process timestamps are invalid'}
 if($record.limits.timeout_seconds -ne 7200 -or $record.limits.memory_bytes -ne 2147483648 -or $record.limits.scratch_bytes -ne 2147483648 -or $record.limits.file_bytes -ne 536870912){throw 'Original process limits differ'}
 if($record.output_bytes -ne $state.manifest.copied_members[$Output].bytes -or (Get-FileHash -LiteralPath (Join-Path $OutputDirectory $Output)).Hash.ToLowerInvariant() -cne $state.manifest.copied_members[$Output].sha256){throw 'Recovered output bytes differ'}
 if(($cursor -eq 3 -and $InputText -cne $state.original_sensors) -or ($cursor -eq 5 -and $InputText -cne $state.original_primary)){throw 'Recovered explicit primary input differs'}
 if($cursor -eq 5){$null=[GreenLensReference.Aggregate]::Read((Join-Path $OutputDirectory $Output),(24*256*6))}
 $records.Add($record);$records|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $OutputDirectory 'processes.json') -Encoding utf8
 $state.cursor++;$state.manifest.reused_process_count=$state.cursor
 if($state.cursor -eq 6){$state.manifest.status='reused_successful_initial_processes'}
 $state.manifest|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $OutputDirectory 'recovery.json') -Encoding utf8NoBOM
 Write-Host "Reused original successful $Name output $Output; engine was not rerun"
}
