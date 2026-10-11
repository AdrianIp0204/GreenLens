#Requires -Version 7.5
# Original synthetic files only: no engine, application or physics execution.
[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference='Stop'
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/script-tests/job diagnostics 空間 '+[guid]::NewGuid().ToString('N'))}
if(Test-Path -LiteralPath $OutputDirectory){throw 'Test destination must be new.'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$source=Join-Path $OutputDirectory 'jobs'
$jobId='10000000-0000-4000-8000-000000000001'
$job=Join-Path $source $jobId
New-Item -ItemType Directory -Path $job -Force|Out-Null
$start=[datetimeoffset]::UtcNow.AddMinutes(-1)
$stamp=[datetimeoffset]::UtcNow.ToString('o')
$process=@{executable='rtrace.exe';arguments=@('-h','annual-solar.oct');exit_code=1;started_utc=$stamp;finished_utc=$stamp;elapsed_ms=2;stderr="Failed reading $job/owned-file"}
$resource=@{executable='rtrace.exe';started_utc=$stamp;finished_utc=$stamp;elapsed_ms=2;peak_child_working_set_bytes=100;peak_observed_runner_plus_child_working_set_bytes=200;peak_scratch_bytes=536870913;output_bytes=0;samples=1}
$annual=@{schema_version='greenlens.annual-resources.v1';measurement_policy='Synthetic resource failure';started_utc=$stamp;finished_utc=$stamp;total_wall_ms=2;peak_engine_working_set_bytes=100;peak_observed_runner_plus_engine_working_set_bytes=200;peak_scratch_bytes=536870913;max_scratch_bytes=2GB;max_matrix_bytes=512MB;max_observed_working_set_bytes=2GB;processes=@($resource)}
function Write-Fixture([string]$Name,$Value){ConvertTo-Json -InputObject $Value -Depth 20|Set-Content -LiteralPath (Join-Path $job $Name) -Encoding utf8NoBOM}
Write-Fixture 'rtrace.process.json' $process
Write-Fixture 'rtrace.resources.json' $resource
Write-Fixture 'rtrace.started.json' @{executable='rtrace.exe';started_utc=$stamp;pid=123}
Write-Fixture 'annual-logs.json' @($process)
Write-Fixture 'annual-resources.json' $annual
"Required failure at $job/owned-file"|Set-Content -LiteralPath (Join-Path $job 'rtrace.stderr')
foreach($name in @('environment.json','results.json','annual-sky.mtx','scene.rad','engine.exe')){'excluded synthetic data'|Set-Content -LiteralPath (Join-Path $job $name)}
New-Item -ItemType Directory -Path (Join-Path $job 'nested')|Out-Null
'must not be traversed'|Set-Content -LiteralPath (Join-Path $job 'nested/rtrace.stderr')
$before=(Get-FileHash -LiteralPath (Join-Path $job 'rtrace.process.json')).Hash
$checks=[Collections.Generic.List[string]]::new()
$serial=0
function Capture([hashtable]$Overrides=@{}){
    $script:serial++
    $parameters=@{JobRoot=$source;Destination=(Join-Path $OutputDirectory "capture-$script:serial");EarliestUtc=$start;WorkspaceRoot=(Join-Path $OutputDirectory 'source checkout')}
    foreach($key in $Overrides.Keys){$parameters[$key]=$Overrides[$key]}
    & "$PSScriptRoot/collect-job-diagnostics.ps1" @parameters
}
function Require([string]$Name,[bool]$Condition){if(!$Condition){throw "Failed: $Name"};$checks.Add($Name)}
function Reject([string]$Name,[scriptblock]$Action){$failed=$false;try{& $Action|Out-Null}catch{$failed=$true};Require $Name $failed}
$captured=Capture
Require 'Six allowed top-level process/resource/stderr records retained' ($captured.copied_files -eq 6 -and $captured.omitted.Count -eq 0)
$retained=Get-Content (Join-Path $OutputDirectory "capture-1/$jobId/rtrace.process.json") -Raw|ConvertFrom-Json -DateKind String
Require 'Known scratch path redacted without losing native error or exit' ($retained.stderr -ceq 'Failed reading <scratch>/owned-file' -and $retained.exit_code -eq 1)
Require 'Original failed diagnostic bytes unchanged' ((Get-FileHash (Join-Path $job 'rtrace.process.json')).Hash -ceq $before)
$files=@(Get-ChildItem (Join-Path $OutputDirectory 'capture-1') -Recurse -File)
Require 'Unknown metadata, nested files, matrices and binaries excluded' ($files.Count -eq 6 -and !@($files|Where-Object{$_.Name -in @('environment.json','results.json','annual-sky.mtx','scene.rad','engine.exe')}).Count)
Require 'Every retained byte count and hash matches capture inventory' (@($captured.records|Where-Object{(Get-FileHash (Join-Path $OutputDirectory ('capture-1/'+$_.file))).Hash.ToLowerInvariant() -cne $_.retained_sha256}).Count -eq 0)
$limited=Capture @{MaxFiles=1}
Require 'File count limit retains one and records all other omissions' ($limited.copied_files -eq 1 -and $limited.omitted.Count -eq 5)
$limited=Capture @{MaxTotalBytes=0}
Require 'Zero remaining total-byte allowance publishes nothing' ($limited.copied_files -eq 0 -and $limited.copied_bytes -eq 0 -and $limited.omitted.Count -eq 6)
$limited=Capture @{MaxFileBytes=1}
Require 'Per-file size cap rejects before unbounded content reads' ($limited.copied_files -eq 0 -and $limited.omitted.Count -eq 6)
$missing=Capture @{JobRoot=(Join-Path $OutputDirectory 'missing')}
Require 'No jobs after early setup failure is an explicit empty capture' (!$missing.root_available -and $missing.copied_files -eq 0)
$future=Capture @{EarliestUtc=[datetimeoffset]::UtcNow.AddDays(1)}
Require 'Earlier job directories are excluded' ($future.copied_files -eq 0)
$process.environment=@{SECRET='synthetic forbidden value'}
Write-Fixture 'rtrace.process.json' $process
$bad=Capture
Require 'Injected environment metadata rejected' ($bad.copied_files -eq 5 -and $bad.omitted.Count -eq 1)
$process.Remove('environment');Write-Fixture 'rtrace.process.json' $process
'Unknown path Z:\private-owner\model.rad'|Set-Content (Join-Path $job 'rtrace.stderr')
$bad=Capture
Require 'Unknown absolute path omitted instead of disclosed' ($bad.copied_files -eq 5 -and $bad.omitted.Count -eq 1)
'API_KEY=synthetic-not-a-real-secret'|Set-Content (Join-Path $job 'rtrace.stderr')
$bad=Capture
Require 'Credential-shaped and environment stderr omitted' ($bad.copied_files -eq 5 -and $bad.omitted.Count -eq 1)
'Expected native error'|Set-Content (Join-Path $job 'rtrace.stderr')
$process.started_utc=$start.AddDays(-1).ToString('o');Write-Fixture 'rtrace.process.json' $process
$bad=Capture
Require 'Earlier process timestamps rejected within a fresh directory' ($bad.copied_files -eq 5 -and $bad.omitted.Count -eq 1)
$process.started_utc=$stamp;Write-Fixture 'rtrace.process.json' $process
New-Item -ItemType Directory -Path (Join-Path $source 'not-a-uuid')|Out-Null
'ignored'|Set-Content (Join-Path $source 'not-a-uuid/rtrace.stderr')
$invalid=Capture
Require 'Noncanonical job names excluded' ($invalid.copied_files -eq 6)
$outside=Join-Path $OutputDirectory 'junction-target'
New-Item -ItemType Directory -Path $outside|Out-Null
'Do not follow'|Set-Content (Join-Path $outside 'rtrace.stderr')
$link=Join-Path $source '10000000-0000-4000-8000-000000000002'
New-Item -ItemType Junction -Path $link -Target $outside|Out-Null
$linked=Capture
Require 'UUID-named directory junction is never followed' ($linked.copied_files -eq 6 -and @($linked.omitted|Where-Object reason -CEQ 'reparse job directory').Count -eq 1)
Reject 'Existing capture cannot be overwritten' {Capture @{Destination=(Join-Path $OutputDirectory 'capture-1')}}
Reject 'Destination inside a worker job tree rejected' {Capture @{Destination=(Join-Path $source 'capture')}}
# Execute the actual workflow collectors against isolated original fixtures. A
# refused optional stderr must not make complete compulsory evidence fail, while
# a refused project/report file must still fail its existing success guard.
$workflow=Get-Content (Join-Path $PSScriptRoot '../.github/workflows/windows.yml') -Raw
$collectorCases=@(
    @{name='selected-hour';step='Collect bounded selected-hour evidence even after failure';root='ci-selected-hour 空間';upload='CI_CASE_UPLOAD';extra='Portable copy/forbidden.exe'},
    @{name='annual';step='Collect bounded annual project and diagnostics even after failure';root='ci-annual 空間';upload='CI_CASE_UPLOAD';extra='project/forbidden.exe'},
    @{name='replay';step='Collect bounded replay diagnostics even after failure';root='ci-annual-replay 空間';upload='CI_REPLAY_UPLOAD';extra='oversize.json'}
)
$envNames=@('LOCALAPPDATA','RUNNER_TEMP','GITHUB_ENV','GITHUB_SHA','OBSERVED_JOB_STATUS','CI_JOB_STARTED_UTC','CI_CASE_UPLOAD','CI_REPLAY_UPLOAD','ANNUAL_ALTERNATIVE','ANNUAL_PROFILE','REPLAY_ALTERNATIVE')
$savedEnvironment=@{}
foreach($name in $envNames){$savedEnvironment[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
Push-Location (Join-Path $PSScriptRoot '..')
try{
    foreach($case in $collectorCases){
        $pattern='(?ms)^      - name: '+[regex]::Escape($case.step)+'\r?\n.*?^        run: \|\r?\n(?<body>(?:^          [^\r\n]*\r?\n|^\r?\n)+)'
        $match=[regex]::Match($workflow,$pattern)
        if(!$match.Success){throw 'Required workflow collector could not be located.'}
        $body=[regex]::Replace($match.Groups['body'].Value,'(?m)^          ','')
        $block=[scriptblock]::Create($body)
        $caseRoot=Join-Path $OutputDirectory ('workflow-'+$case.name)
        $env:LOCALAPPDATA=Join-Path $caseRoot 'local'
        $env:GITHUB_ENV=Join-Path $caseRoot 'github-env.txt'
        $env:GITHUB_SHA='0000000000000000000000000000000000000000'
        $env:OBSERVED_JOB_STATUS='success'
        $env:CI_JOB_STARTED_UTC=$start.ToString('o')
        $env:ANNUAL_ALTERNATIVE='A';$env:ANNUAL_PROFILE='shipping';$env:REPLAY_ALTERNATIVE='A'
        $requiredRoot=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/'+$case.root)
        $optionalRoot=Join-Path $env:LOCALAPPDATA ('GreenLens/jobs/'+$jobId)
        New-Item -ItemType Directory -Path $requiredRoot,$optionalRoot -Force|Out-Null
        '{}'|Set-Content (Join-Path $requiredRoot 'summary.json')
        '{}'|Set-Content (Join-Path $requiredRoot 'replay.json')
        'Unknown path Z:\private-owner\model.rad'|Set-Content (Join-Path $optionalRoot 'rtrace.stderr')
        $env:RUNNER_TEMP=Join-Path $caseRoot 'optional-omitted'
        New-Item -ItemType Directory -Path $env:RUNNER_TEMP|Out-Null
        & $block|Out-Null
        $upload=Join-Path $env:RUNNER_TEMP ('greenlens-'+$case.name+'-evidence')
        $report=Get-Content (Join-Path $upload 'capture.json') -Raw|ConvertFrom-Json -DateKind String
        Require ($case.name+': optional diagnostic omission preserves complete evidence success') (@($report.omitted).Count -eq 0 -and @($report.worker_diagnostics.omitted).Count -eq 1)
        $extra=Join-Path $requiredRoot $case.extra
        New-Item -ItemType Directory -Path (Split-Path -Parent $extra) -Force|Out-Null
        if($case.name -eq 'replay'){
            $stream=[IO.File]::Create($extra)
            try{$stream.SetLength(8MB+1)}finally{$stream.Dispose()}
        }else{'excluded original fixture'|Set-Content $extra}
        $env:RUNNER_TEMP=Join-Path $caseRoot 'required-omitted'
        New-Item -ItemType Directory -Path $env:RUNNER_TEMP|Out-Null
        Reject ($case.name+': required evidence omission still fails the collector') {& $block}
        $upload=Join-Path $env:RUNNER_TEMP ('greenlens-'+$case.name+'-evidence')
        $report=Get-Content (Join-Path $upload 'capture.json') -Raw|ConvertFrom-Json -DateKind String
        Require ($case.name+': required and optional omissions remain separate in the retained report') (@($report.omitted).Count -eq 1 -and @($report.worker_diagnostics.omitted).Count -eq 1)
    }
}finally{
    Pop-Location
    foreach($name in $envNames){[Environment]::SetEnvironmentVariable($name,$savedEnvironment[$name],'Process')}
}
$summary=@{schema_version='greenlens.job-diagnostic-capture-tests.v1';passed=$true;checks=$checks.ToArray();scope='Original synthetic filesystem/metadata/privacy fixtures only; no engine, real resource failure or release acceptance.'}
$summary|ConvertTo-Json -Depth 8|Set-Content (Join-Path $OutputDirectory 'summary.json') -Encoding utf8NoBOM
$summary|ConvertTo-Json -Depth 8
