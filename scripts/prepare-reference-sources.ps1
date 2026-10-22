# Only these trusted checked-out source files may receive newline preparation.
$script:PilotPreparationSource=$PSCommandPath
function Prepare-ReferenceSources([string]$SourceRoot,$Pins) {
 $allowed=@('scripts/annual-controlled-reference.ps1','scripts/annual-reference-process.ps1','scripts/annual-reference-aggregation.cs','scripts/annual-matched-corrected.ps1','scripts/annual-feasibility.ps1','scripts/annual-matched-evidence.ps1','scripts/verify-matrix-multiplication.ps1','docs/v0.3-reference-protocol.md','dependencies/radiance.json')
 $properties=@($Pins.PSObject.Properties)
 if(($properties.Name|Sort-Object) -join '|' -cne (($allowed|Sort-Object)-join '|')){throw 'Source pin paths differ from the fixed nine-file allowlist.'}
 $utf8=[Text.UTF8Encoding]::new($false,$true)
 $plans=@()
 foreach($name in $allowed){
  $expected=$Pins.$name
  if($expected -cnotmatch '^[0-9a-f]{64}$'){throw 'Invalid expected source digest.'}
  $path=Join-Path $SourceRoot $name;$item=Get-Item -LiteralPath $path -Force
  if($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -gt 2097152){throw 'Checked-out source is not a bounded regular file.'}
  $bytes=[IO.File]::ReadAllBytes($path)
  $before=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
  $chosen=$bytes;$convention='preserved';$changed=$false
  if($before -cne $expected){
   $text=$utf8.GetString($bytes)
   if($text -match '\r(?!\n)'){throw 'Lone-CR source content cannot be repaired as checkout EOL.'}
   $lf=$text.Replace("`r`n","`n");$chosen=$null
   foreach($candidate in @(@('lf',$lf),@('crlf',$lf.Replace("`n","`r`n")))){
    $candidateBytes=$utf8.GetBytes($candidate[1])
    $candidateHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($candidateBytes)).ToLowerInvariant()
    if($candidateHash -ceq $expected){$chosen=$candidateBytes;$convention=$candidate[0];break}
   }
   if($null -eq $chosen){throw "Non-newline source change rejected: $name"}
   $changed=$true
  }
  $plans+=@{path=$path;name=$name;bytes=$chosen;before_sha256=$before;after_sha256=$expected;changed=$changed;convention=$convention}
 }
 # Validate every candidate before changing any trusted source file.
 $records=@()
 foreach($plan in $plans){
  if($plan.changed){[IO.File]::WriteAllBytes($plan.path,$plan.bytes)}
  if((Get-FileHash -LiteralPath $plan.path).Hash.ToLowerInvariant() -cne $plan.after_sha256){throw 'Prepared source digest differs.'}
  $records+=[ordered]@{path=$plan.name;before_sha256=$plan.before_sha256;after_sha256=$plan.after_sha256;changed=$plan.changed;newline_convention=$plan.convention;bytes=$plan.bytes.Length}
 }
 return [ordered]@{schema_version='greenlens.reference-source-newlines.v1';recorded_utc=[DateTime]::UtcNow.ToString('o');policy='Only fixed trusted checkout source LF/CRLF conversion whose exact bytes match the sealed local digest; no data files or embedded archive code executed';preparation_source_sha256=(Get-FileHash -LiteralPath $script:PilotPreparationSource).Hash.ToLowerInvariant();files=$records}
}
