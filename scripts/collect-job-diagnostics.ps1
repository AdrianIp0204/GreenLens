#Requires -Version 7.5
# Read-only bounded capture of known worker diagnostics, never run evidence.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Destination,
    [Parameter(Mandatory)][datetimeoffset]$EarliestUtc,
    [string]$JobRoot = (Join-Path $env:LOCALAPPDATA 'GreenLens/jobs'),
    [string]$WorkspaceRoot = ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))),
    [ValidateRange(0,8388608)][long]$MaxFileBytes = 8MB,
    [ValidateRange(0,268435456)][long]$MaxTotalBytes = 64MB,
    [ValidateRange(0,5000)][int]$MaxFiles = 100
)
$ErrorActionPreference='Stop'
$utf8=[Text.UTF8Encoding]::new($false,$true)
$report=[ordered]@{schema_version='greenlens.job-diagnostic-capture.v1';scope='Sanitized diagnostic copies only, not immutable evidence or completed results.';earliest_utc=$EarliestUtc.ToUniversalTime().ToString('o');copied_files=0;copied_bytes=0;records=@();omitted=@();root_available=$false;scanned_files=0;scan_truncated=$false}
$records=[Collections.Generic.List[object]]::new()
$omitted=[Collections.Generic.List[object]]::new()
$processNames=@('oconv','gensky','rtrace','gendaymtx','rcontrib','dctimestep')
$processKeys=@('executable','arguments','exit_code','started_utc','finished_utc','elapsed_ms','stderr','resource_failure','process_tree_terminated','cancelled')
$resourceKeys=@('executable','started_utc','finished_utc','elapsed_ms','peak_child_working_set_bytes','peak_observed_runner_plus_child_working_set_bytes','peak_scratch_bytes','output_bytes','samples')
$annualKeys=@('schema_version','measurement_policy','started_utc','finished_utc','total_wall_ms','peak_engine_working_set_bytes','peak_observed_runner_plus_engine_working_set_bytes','peak_scratch_bytes','max_scratch_bytes','max_matrix_bytes','max_observed_working_set_bytes','processes')
function Assert-NoReparseAncestors([string]$Path) {
    $item=Get-Item -LiteralPath $Path -Force
    while($null -ne $item){
        if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Diagnostic path contains a reparse point.'}
        $item=if($item -is [IO.DirectoryInfo]){$item.Parent}else{$item.Directory}
    }
}
function Assert-DiagnosticObject($Value,[string[]]$Allowed,[string[]]$Required) {
    if($Value -isnot [Collections.IDictionary]){throw 'Unexpected diagnostic JSON object.'}
    foreach($key in $Value.Keys){if($key -cnotin $Allowed){throw 'Unexpected diagnostic metadata field.'}}
    foreach($key in $Required){if(!$Value.Contains($key)){throw 'Required diagnostic metadata field is absent.'}}
}
function Assert-ProcessRecord($Value,[string]$Kind) {
    switch($Kind){
        'process' { Assert-DiagnosticObject $Value $processKeys @('executable','arguments','exit_code','started_utc','finished_utc'); if($Value.arguments -isnot [array] -or @($Value.arguments|Where-Object{$_ -isnot [string]}).Count){throw 'Unexpected process arguments.'} }
        'resources' { Assert-DiagnosticObject $Value $resourceKeys $resourceKeys }
        'started' { Assert-DiagnosticObject $Value @('executable','pid','started_utc') @('executable','pid','started_utc') }
    }
    if($Value.executable -cnotin @($processNames|ForEach-Object{"$_.exe"})){throw 'Unexpected diagnostic executable.'}
    $stamp=[datetimeoffset]::Parse($Value.started_utc,[Globalization.CultureInfo]::InvariantCulture)
    if($stamp.Offset -ne [timespan]::Zero -or $stamp -lt $EarliestUtc){throw 'Diagnostic belongs to an earlier job.'}
}
function Assert-DiagnosticJson($Value,[string]$Name) {
    if($Name -ceq 'annual-logs.json'){
        if($Value -isnot [array]){throw 'Expected process record array.'}
        foreach($record in $Value){Assert-ProcessRecord $record 'process'}
    }elseif($Name -ceq 'annual-resources.json'){
        Assert-DiagnosticObject $Value $annualKeys $annualKeys
        if($Value.schema_version -cne 'greenlens.annual-resources.v1' -or $Value.processes -isnot [array]){throw 'Unexpected annual resources schema.'}
        foreach($record in $Value.processes){Assert-ProcessRecord $record 'resources'}
    }elseif($Name -cmatch '^(oconv|gensky|rtrace|gendaymtx|rcontrib|dctimestep)\.(process|resources|started)\.json$'){
        Assert-ProcessRecord $Value $Matches[2]
    }else{throw 'Unexpected diagnostic JSON filename.'}
}
function Protect-DiagnosticText([string]$Text,[string]$Scratch) {
    # Replace only known task/machine roots. Any other absolute path or secret-
    # shaped text is omitted, not copied into the public artifact.
    $roots=@(@{path=$Scratch;label='<scratch>'},@{path=$WorkspaceRoot;label='<source>'},@{path=$env:RUNNER_TEMP;label='<runner-temp>'},@{path=$env:LOCALAPPDATA;label='<local-app-data>'},@{path=$env:USERPROFILE;label='<user-profile>'})
    foreach($entry in @($roots|Where-Object{$_.path}|Sort-Object {$_.path.Length} -Descending)){
        foreach($variant in @($entry.path,$entry.path.Replace('\','/'))){
            $Text=[regex]::Replace($Text,[regex]::Escape($variant),$entry.label,[Text.RegularExpressions.RegexOptions]::IgnoreCase)
        }
    }
    if($Text -match '(?im)((?<![A-Za-z0-9])[a-z]:[\\/]|\\\\|(?<!:)//[^<\s/]|gh[pousr]_|github_pat_|-----BEGIN[^\r\n]*PRIVATE KEY|(?:password|passwd|api[_-]?key|secret|access[_-]?token)\s*[:=]|^\s*[A-Z_][A-Z0-9_]{2,}=)'){throw 'Diagnostic text contains an unreviewed path, environment value or credential pattern.'}
    return $Text
}
function Protect-DiagnosticValue($Value,[string]$Scratch){
    if($Value -is [string]){return Protect-DiagnosticText $Value $Scratch}
    if($Value -is [Collections.IDictionary]){
        $result=[ordered]@{}
        foreach($key in $Value.Keys){$result[$key]=Protect-DiagnosticValue $Value[$key] $Scratch}
        return $result
    }
    if($Value -is [array]){return ,@($Value|ForEach-Object{Protect-DiagnosticValue $_ $Scratch})}
    return $Value
}
$JobRoot=[IO.Path]::GetFullPath($JobRoot)
$Destination=[IO.Path]::GetFullPath($Destination)
if(Test-Path -LiteralPath $Destination){throw 'Diagnostic destination must be new.'}
$parent=Split-Path -Parent $Destination
if(!(Test-Path -LiteralPath $parent -PathType Container)){throw 'Diagnostic destination parent must exist.'}
Assert-NoReparseAncestors $parent
if($Destination.StartsWith($JobRoot.TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $Destination -ieq $JobRoot){throw 'Diagnostic destination must be outside the source job tree.'}
New-Item -ItemType Directory -Path $Destination|Out-Null
if(Test-Path -LiteralPath $JobRoot -PathType Container){
    Assert-NoReparseAncestors $JobRoot
    $report.root_available=$true
    $directories=@(Get-ChildItem -LiteralPath $JobRoot -Directory -Force|Sort-Object Name)
    if($directories.Count -gt 5000){throw 'Diagnostic job directory count exceeds bound.'}
    :jobs foreach($directory in $directories){
        $id=[guid]::Empty
        if(![guid]::TryParseExact($directory.Name,'D',[ref]$id) -or $id.ToString() -cne $directory.Name -or $directory.CreationTimeUtc -lt $EarliestUtc.UtcDateTime){continue}
        if($directory.Attributes -band [IO.FileAttributes]::ReparsePoint){$omitted.Add(@{file=$directory.Name;reason='reparse job directory'});continue}
        $files=@(Get-ChildItem -LiteralPath $directory.FullName -File -Force|Where-Object{$_.Name -cmatch '^(annual-(logs|resources)\.json|(oconv|gensky|rtrace|gendaymtx|rcontrib|dctimestep)\.(process\.json|resources\.json|started\.json|stderr))$'}|Sort-Object Name)
        foreach($file in $files){
            if($report.scanned_files -ge 5000){$report.scan_truncated=$true;$omitted.Add(@{file='remaining-job-diagnostics';reason='5000-file inspection bound reached'});break jobs}
            $report.scanned_files++
            $relative=$directory.Name+'/'+$file.Name
            try{
                if($file.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'reparse diagnostic file'}
                if($file.Length -gt $MaxFileBytes -or $report.copied_files -ge $MaxFiles){throw 'bounded diagnostic upload'}
                $stream=[IO.File]::OpenRead($file.FullName)
                try{
                    $buffer=[byte[]]::new([int]$MaxFileBytes+1)
                    $size=$stream.ReadAtLeast($buffer,$buffer.Length,$false)
                    if($size -gt $MaxFileBytes){throw 'diagnostic grew beyond file cap'}
                    $bytes=[byte[]]::new($size)
                    [Array]::Copy($buffer,$bytes,$size)
                }finally{$stream.Dispose()}
                $text=$utf8.GetString($bytes)
                if($file.Extension -ceq '.json'){
                    $value=ConvertFrom-Json -InputObject $text -Depth 40 -AsHashtable -DateKind String -NoEnumerate
                    Assert-DiagnosticJson $value $file.Name
                    $text=(ConvertTo-Json -InputObject (Protect-DiagnosticValue $value $directory.FullName) -Depth 40).Replace("`r`n","`n")+"`n"
                }else{$text=(Protect-DiagnosticText $text $directory.FullName).Replace("`r`n","`n")}
                $retained=$utf8.GetBytes($text)
                if($retained.Length -gt $MaxFileBytes -or $report.copied_bytes+$retained.Length -gt $MaxTotalBytes){throw 'bounded diagnostic upload'}
                $target=Join-Path $Destination $relative
                New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force|Out-Null
                [IO.File]::WriteAllBytes($target,$retained)
                $report.copied_files++;$report.copied_bytes+=$retained.Length
                $records.Add(@{file=$relative;source_bytes=$bytes.Length;source_sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant();retained_bytes=$retained.Length;retained_sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($retained)).ToLowerInvariant();sanitized=$true})
            }catch{
                # Exception text may itself contain a private source path.
                $omitted.Add(@{file=$relative;reason='Rejected by bounded metadata/path/privacy validation; source untouched.'})
            }
        }
    }
}
$report.records=$records.ToArray();$report.omitted=$omitted.ToArray()
return $report
