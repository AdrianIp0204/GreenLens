#Requires -Version 7.2
# Pure declaration/coverage checks. No engine, filesystem or imported commands run.
# Unknown declaration fields participate in identity; only documented per-shard
# bookkeeping and per-alternative physical fields are separated below.

function Convert-ReferenceJsonElementCanonical {
 param([Parameter(Mandatory)][System.Text.Json.JsonElement]$Element)
 switch ($Element.ValueKind.ToString()) {
  'Object' {
   $names=[Collections.Generic.List[string]]::new()
   foreach($property in $Element.EnumerateObject()){$names.Add($property.Name)}
   $names.Sort([StringComparer]::Ordinal)
   $parts=[Collections.Generic.List[string]]::new()
   foreach($name in $names){
    $key=ConvertTo-Json -InputObject $name -Compress
    $parts.Add($key+':'+(Convert-ReferenceJsonElementCanonical $Element.GetProperty($name)))
   }
   return '{'+($parts -join ',')+'}'
  }
  'Array' {
   $parts=[Collections.Generic.List[string]]::new()
   foreach($item in $Element.EnumerateArray()){$parts.Add((Convert-ReferenceJsonElementCanonical $item))}
   return '['+($parts -join ',')+']'
  }
  'String' { return (ConvertTo-Json -InputObject $Element.GetString() -Compress) }
  'Number' { return $Element.GetRawText() }
  'True' { return 'true' }
  'False' { return 'false' }
  'Null' { return 'null' }
  default { throw 'Unsupported value in reference JSON identity.' }
 }
}

function ConvertTo-ReferenceCanonicalJson {
 [CmdletBinding()]
 param([Parameter(Mandatory)][AllowNull()][object]$Value)
 # ConvertTo-Json -InputObject preserves empty/singleton arrays. JSON parsing
 # rejects non-JSON values; numeric representation is preserved conservatively.
 $WarningPreference='Stop'
 $json=ConvertTo-Json -InputObject $Value -Depth 100 -Compress
 $document=[System.Text.Json.JsonDocument]::Parse($json)
 try { return (Convert-ReferenceJsonElementCanonical $document.RootElement) }
 finally { $document.Dispose() }
}

function Copy-ReferenceJsonObject {
 param([Parameter(Mandatory)][object]$Value)
 $canonical=ConvertTo-ReferenceCanonicalJson $Value
 if(!$canonical.StartsWith('{')){throw 'Reference declaration must be a JSON object.'}
 $options=@{InputObject=$canonical;AsHashtable=$true;Depth=100}
 if((Get-Command ConvertFrom-Json).Parameters.ContainsKey('DateKind')){$options.DateKind='String'}
 return (ConvertFrom-Json @options)
}

function Assert-ReferenceIntervalValue {
 param([Parameter(Mandatory)][object]$Value)
 if($Value -is [string] -or $Value -is [bool] -or $Value -isnot [ValueType]){throw 'Reference interval must be an integer JSON number.'}
 try {$number=[double]$Value} catch {throw 'Reference interval must be an integer JSON number.'}
 if(![double]::IsFinite($number) -or $number -ne [Math]::Floor($number) -or $number -lt 0 -or $number -gt 8759){throw 'Reference interval is outside the supported year.'}
 return [int]$number
}

function Get-ReferenceFixedIntervals {
 return @(1904,1908,1912,4112,4116,4120,6320,6324,6328,8504,8508,8512)
}

function Assert-ReferenceShardIdentity {
 [CmdletBinding()]
 param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Declarations)
 if(!$Declarations.Count){throw 'At least one baseline declaration is required.'}
 $fixed=Get-ReferenceFixedIntervals
 $fixedJson=ConvertTo-ReferenceCanonicalJson $fixed
 $commonJson=$null
 $alternatives=[ordered]@{}
 $retained=[Collections.Generic.List[object]]::new()
 foreach($declaration in $Declarations){
  $original=Copy-ReferenceJsonObject $declaration
  foreach($field in @('schema_version','alternative','declared_interval_indices','executed_interval_indices','normalized_model','input_sha256','declared_utc')){
   if(!$original.Contains($field)){throw "Reference declaration omits $field."}
  }
  if($original.schema_version -cne 'greenlens.controlled-reference.v1'){throw 'Unsupported reference declaration schema.'}
  if($original.alternative -isnot [string] -or $original.alternative -cnotin @('A','B')){throw 'Reference alternative must be exactly A or B.'}
  if((ConvertTo-ReferenceCanonicalJson $original.declared_interval_indices) -cne $fixedJson){throw 'The original declared interval list differs in values or order from the fixed twelve-case plan.'}
  if($original.executed_interval_indices -isnot [Collections.IList] -or !$original.executed_interval_indices.Count){throw 'Executed interval indices must be a nonempty array subset of the fixed plan.'}
  $seen=[Collections.Generic.HashSet[int]]::new()
  foreach($value in $original.executed_interval_indices){
   $index=Assert-ReferenceIntervalValue $value
   if($index -notin $fixed -or !$seen.Add($index)){throw 'Executed intervals must be a unique subset of the original twelve-case plan.'}
  }
  if($original.normalized_model -isnot [Collections.IDictionary] -or !$original.normalized_model.Count){throw 'Reference declaration requires the complete normalized model.'}
  if($original.input_sha256 -isnot [Collections.IDictionary]){throw 'Reference input hashes must be an object.'}
  foreach($name in @('scene.rad','sensors.pts','identical-sky.mtx','suns.rad','weather.epw','source-recipe.json','model.json','study.json','weather-asset.json')){
   if(!$original.input_sha256.Contains($name) -or $original.input_sha256[$name] -isnot [string] -or $original.input_sha256[$name] -cnotmatch '^[0-9a-f]{64}$'){throw "Reference input hash is missing or malformed: $name."}
  }
  $within=Copy-ReferenceJsonObject $original
  # These fields are reporting/shard bookkeeping, not scientific identity.
  $null=$within.Remove('declared_utc')
  $null=$within.Remove('executed_interval_indices')
  $withinJson=ConvertTo-ReferenceCanonicalJson $within
  $alternative=$original.alternative
  if($alternatives.Contains($alternative)){
   if($alternatives[$alternative] -cne $withinJson){throw "Reference physical/protocol identity differs within alternative $alternative."}
  }else{$alternatives[$alternative]=$withinJson}
  $common=Copy-ReferenceJsonObject $within
  $null=$common.Remove('alternative')
  $null=$common.Remove('normalized_model')
  # The source recipe itself binds the alternative's scene/model. Each of
  # these exceptions remains strict in the within-alternative identity above.
  foreach($name in @('scene.rad','model.json','source-recipe.json')){$null=$common.input_sha256.Remove($name)}
  $candidate=ConvertTo-ReferenceCanonicalJson $common
  if($null -eq $commonJson){$commonJson=$candidate}
  elseif($candidate -cne $commonJson){throw 'Reference numeric/protocol/helper/engine/weather identity differs across baseline declarations.'}
  $retained.Add($original)
 }
 return [pscustomobject][ordered]@{
  schema_version='greenlens.reference-shard-identity.v1'
  common_identity=$commonJson
  alternative_identities=$alternatives
  declarations=$retained.ToArray()
 }
}

function Test-ReferenceCaseCoverage {
 [CmdletBinding()]
 param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Cases,[switch]$RequireComplete)
 # Cases are already recomputed completed cases, with alternative/interval_index.
 # Numerical and sensitivity qualification belong to the trusted collector.
 $fixed=Get-ReferenceFixedIntervals
 $expected=@(foreach($alternative in @('A','B')){foreach($index in $fixed){"${alternative}:$index"}})
 $completed=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
 foreach($case in $Cases){
  $value=Copy-ReferenceJsonObject $case
  if(!$value.Contains('alternative') -or !$value.Contains('interval_index')){throw 'Completed reference case requires alternative and interval_index.'}
  if($value.alternative -isnot [string] -or $value.alternative -cnotin @('A','B')){throw 'Completed reference alternative must be exactly A or B.'}
  $index=Assert-ReferenceIntervalValue $value.interval_index
  $key="$($value.alternative):$index"
  if($key -cnotin $expected){throw "Completed reference case is outside the declared plan: $key."}
  if(!$completed.Add($key)){throw "Duplicate completed reference case: $key. Equal values do not authorize deduplication."}
 }
 $missing=@($expected|Where-Object{!$completed.Contains($_)})
 $complete=$missing.Count -eq 0
 if($RequireComplete -and !$complete){throw ('Reference coverage is incomplete; missing '+($missing -join ', ')+'.')}
 return [pscustomobject][ordered]@{
  expected_case_keys=$expected
  completed_case_keys=@($expected|Where-Object{$completed.Contains($_)})
  missing_case_keys=$missing
  completed_case_count=$completed.Count
  complete=$complete
 }
}
