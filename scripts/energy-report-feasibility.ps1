#Requires -Version 7.5
param(
 [string]$RendererDirectory=(Join-Path $env:LOCALAPPDATA 'GreenLens/renderers/typst-0.15.1'),
 [string]$OutputDirectory=(Join-Path $env:LOCALAPPDATA ('GreenLens/verification/energy report Gate0 '+[guid]::NewGuid().ToString('N')+' 空間'))
)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest=Get-Content -LiteralPath (Join-Path $repo 'dependencies/typst.json') -Raw|ConvertFrom-Json
$exe=Join-Path $RendererDirectory 'typst.exe';$fontRoot=Join-Path $RendererDirectory 'fonts'
function Seal([string]$Path){[ordered]@{bytes=(Get-Item -LiteralPath $Path).Length;sha256=(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}}
function AssertSeal([string]$Path,$Expected){$f=Get-Item -LiteralPath $Path -Force;if($f.PSIsContainer -or ($f.Attributes -band ([IO.FileAttributes]::ReparsePoint -bor [IO.FileAttributes]::Offline))){throw 'Renderer inputs must be available regular files.'};$s=Seal $Path;if($s.bytes -ne $Expected.bytes -or $s.sha256 -ne $Expected.sha256){throw 'Pinned renderer/font differs.'}}
AssertSeal $exe $manifest.members.'typst.exe'
foreach($font in $manifest.fonts.files){AssertSeal (Join-Path $fontRoot $font.name) $font}
AssertSeal (Join-Path $fontRoot $manifest.fonts.license_file.name) $manifest.fonts.license_file
if((Get-Item -LiteralPath $fontRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Font root may not be a reparse point.'}
if(@(Get-ChildItem -LiteralPath $fontRoot -Force).Count -ne 3 -or @(Get-ChildItem -LiteralPath $fontRoot -Force -Directory).Count){throw 'Font discovery root must contain exactly the two pinned fonts and their license.'}
if(Test-Path -LiteralPath $OutputDirectory){throw 'A fresh report output directory is required.'}
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
$allowed=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'GreenLens')).TrimEnd('\')+'\'
if(!$OutputDirectory.StartsWith($allowed,[StringComparison]::OrdinalIgnoreCase)){throw 'Use an external LOCALAPPDATA/GreenLens output directory.'}
$ancestor=$OutputDirectory
while($ancestor){if((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Report paths may not traverse reparse points.'};$ancestor=[IO.Path]::GetDirectoryName($ancestor)}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
$stage=Join-Path $OutputDirectory 'Report staging 空間';New-Item -ItemType Directory -Path $stage|Out-Null
foreach($name in @('feasibility.typ','model.svg','chart.svg')){Copy-Item -LiteralPath (Join-Path $repo "reports/energy/$name") -Destination (Join-Path $stage $name)}
$fixture=[ordered]@{
 schema_version='greenlens.energy-renderer-fixture.v1';title='Energy analysis / reporting proof';cjk='綠鏡能源分析 · 建築能源比較 · 室內舒適度'
 report_id='renderer-fixture-001';model_id='synthetic-four-zone-display';baseline_run='not-run-baseline';proposed_run='not-run-proposed';revision='Gate 0 / 2026-10-13'
 time_convention='Fixed local standard time; example only';scope='Original synthetic renderer fixture. No professional sign-off, certification, engine results or private reference values.'
 assessments=@(@{axis='Execution';status='Renderer completed; thermal simulation not run'},@{axis='Integrity';status='Renderer inputs and outputs hash-bound'},@{axis='Current relationship';status='Fixture data only; no project relationship'},@{axis='Scientific qualification';status='Unassessed energy method; no numerical acceptance'})
 performance=@(@{label='Cooling electricity';baseline='1,000.000';proposed='800.000';unit='kWh';note='Synthetic value'},@{label='Supply fans';baseline='120.000';proposed='135.000';unit='kWh';note='Negative saving: -12.5%'},@{label='Exterior lights';baseline='0.000';proposed='0.000';unit='kWh';note='True zero; savings N/A'},@{label='Heating';baseline='Not applicable';proposed='Not applicable';unit='kWh';note='Cooling-only scope'},@{label='Operational emissions';baseline='Missing';proposed='Missing';unit='kgCO2e';note='No factor supplied'},@{label='Lift electricity';baseline='Unsupported';proposed='Unsupported';unit='kWh';note='No complete-building claim'})
 inputs=@(1..96|ForEach-Object{[ordered]@{id=('ROW-{0:d3}'-f $_);parameter=('Original synthetic parameter {0}'-f $_);value=if($_%4 -eq 0){'Missing'}elseif($_%4 -eq 1){'0.000 W/m²'}elseif($_%4 -eq 2){'Not applicable'}else{'Unsupported'};source='Original fixture / 表格分頁測試; no inferred thermal properties'}})
 adversarial=@('#panic("EXECUTED")','#read("../outside-secret.txt")','#import "@preview/not-a-package:0.0.0": *','<script>alert("literal")</script>','[brackets] {braces} #hash $math$ @label \\path','"quoted" & <angle> / Unicode 安全資料')
}
$utf8=[Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText((Join-Path $stage 'report-data.json'),($fixture|ConvertTo-Json -Depth 12)+"`n",$utf8)
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'outside-secret.txt'),'OUTSIDE_SENTINEL_MUST_NOT_APPEAR',$utf8)
foreach($name in @('empty-packages','empty-package-cache')){New-Item -ItemType Directory -Path (Join-Path $stage $name)|Out-Null}
$originalOutput=$OutputDirectory;$OutputDirectory=$stage;$EngineRoot=$RendererDirectory
$records=[Collections.Generic.List[object]]::new()
. (Join-Path $PSScriptRoot 'annual-paired-process.ps1')
$envNames=@('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','TYPST_FONT_PATHS','TYPST_ROOT','TYPST_PACKAGE_PATH','TYPST_PACKAGE_CACHE_PATH','TYPST_FEATURES')
$saved=@{};foreach($name in $envNames){$saved[$name]=@{present=(Test-Path -LiteralPath "Env:$name");value=[Environment]::GetEnvironmentVariable($name,'Process')}}
$started=[DateTime]::UtcNow.ToString('o');$success=$false
try {
 foreach($name in @('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY')){[Environment]::SetEnvironmentVariable($name,'http://127.0.0.1:9','Process')}
 foreach($name in $envNames|Where-Object{$_ -notin @('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY')}){Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue}
 $common=@('--root','.','--font-path',$fontRoot,'--ignore-system-fonts','--ignore-embedded-fonts','--package-path','empty-packages','--package-cache-path','empty-package-cache','--creation-timestamp','0','--jobs','1','--diagnostic-format','short')
 foreach($item in @(@{name='pdf';output='report.pdf';extra=@('--deps','pdf-dependencies.json')},@{name='repeat';output='report-repeat.pdf';extra=@()},@{name='png';output='page-{0p}-of-{t}.png';extra=@('--ppi','96')})){
  Invoke-PairedProcess -Executable $exe -Name typst -Arguments (@('compile','feasibility.typ',$item.output)+$common+$item.extra) -Output ($item.name+'.stdout') -TimeoutSeconds 120 -MemoryLimitBytes 2147483648 -ScratchLimitBytes 268435456 -FileLimitBytes 67108864
 }
 if((Seal (Join-Path $stage 'report.pdf')).sha256 -ne (Seal (Join-Path $stage 'report-repeat.pdf')).sha256){throw 'Identical inputs did not produce byte-identical PDFs.'}
 $pages=@(Get-ChildItem -LiteralPath $stage -Filter 'page-*.png')
 if($pages.Count -lt 5 -or $pages.Count -gt 12){throw 'Unexpected multipage fixture count.'}
 if(@(Get-ChildItem -LiteralPath (Join-Path $stage 'empty-packages') -Recurse -File).Count -or @(Get-ChildItem -LiteralPath (Join-Path $stage 'empty-package-cache') -Recurse -File).Count){throw 'Package directories changed.'}
 # Deliberately trusted negative fixture, never loaded from a user's package.
 [IO.File]::WriteAllText((Join-Path $stage 'root-denial.typ'),'#read("../outside-secret.txt")',$utf8)
 $denied=$false
 try{Invoke-PairedProcess -Executable $exe -Name typst -Arguments (@('compile','root-denial.typ','must-not-exist.pdf')+$common) -Output 'root-denial.stdout' -TimeoutSeconds 15 -MemoryLimitBytes 2147483648 -ScratchLimitBytes 268435456 -FileLimitBytes 67108864}catch{if($records.Count -eq 4 -and $records[3].exit_code -ne 0 -and $records[3].stderr -match 'outside.*root|access denied'){ $denied=$true }else{throw}}
 if(!$denied -or (Test-Path -LiteralPath (Join-Path $stage 'must-not-exist.pdf'))){throw 'Staging root read-denial guard failed.'}
 AssertSeal $exe $manifest.members.'typst.exe';foreach($font in $manifest.fonts.files){AssertSeal (Join-Path $fontRoot $font.name) $font}
 $success=$true
} finally {
 foreach($name in $envNames){if($saved[$name].present){[Environment]::SetEnvironmentVariable($name,$saved[$name].value,'Process')}else{Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue}}
 $OutputDirectory=$originalOutput
}
$sanitized=($records|ConvertTo-Json -Depth 12).Replace($fontRoot.Replace('\','\\'),'<renderer-fonts>').Replace($stage.Replace('\','\\'),'<report-stage>')|ConvertFrom-Json -DateKind String
# Native diagnostics can resolve Windows application virtualization aliases. Keep
# raw stderr externally, but publish only the known relative diagnostic suffix.
foreach($record in $sanitized){if($record.stderr){$at=$record.stderr.IndexOf('root-denial.typ:');if($at -lt 0){throw 'Unexpected diagnostic requires manual sanitization.'};$record.stderr=$record.stderr.Substring($at)}}
$deps=Get-Content -LiteralPath (Join-Path $stage 'pdf-dependencies.json') -Raw|ConvertFrom-Json
$expectedNames=@('feasibility.typ','report-data.json','model.svg','chart.svg')
if(@($deps.inputs).Count -ne 4 -or @($deps.outputs).Count -ne 1 -or $deps.outputs[0] -ne 'report.pdf'){throw 'Unexpected compilation dependency closure.'}
$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach($dependency in $deps.inputs){$leaf=[IO.Path]::GetFileName($dependency);if($leaf -notin $expectedNames -or !$seen.Add($leaf) -or (Seal $dependency).sha256 -ne (Seal (Join-Path $stage $leaf)).sha256){throw 'Compiler read an unexpected or changed input.'}}
$inputs=[ordered]@{};foreach($name in @('feasibility.typ','model.svg','chart.svg','report-data.json')){$inputs[$name]=Seal (Join-Path $stage $name)}
$outputs=[ordered]@{};foreach($file in @(Get-Item -LiteralPath (Join-Path $stage 'report.pdf'))+@($pages)){$outputs[$file.Name]=Seal $file.FullName}
$summary=[ordered]@{schema_version='greenlens.energy-report-feasibility.v1';completed=$success;started_utc=$started;finished_utc=[DateTime]::UtcNow.ToString('o');scope='Synthetic renderer feasibility only; no energy execution or scientific qualification';renderer=$manifest.version;renderer_sha256=$manifest.members.'typst.exe'.sha256;font_family=$manifest.fonts.family;font_seals=$manifest.fonts.files|Select-Object name,bytes,sha256;input_seals=$inputs;output_seals=$outputs;png_pages=$pages.Count;pdf_repeat_byte_identical=$true;outside_root_read_rejected=$true;package_directories_unchanged=$true;network_policy='Owned import-free template, explicit empty package roots, process-local unreachable HTTP/HTTPS proxies; no OS network sandbox claimed';system_fonts_used=$false;embedded_fonts_used=$false;visual_review_complete=$false;redistribution_review_complete=$false;processes=@($sanitized);script_sha256=(Seal $PSCommandPath).sha256;supervisor_sha256=(Seal (Join-Path $PSScriptRoot 'annual-paired-process.ps1')).sha256;native_supervisor_sha256=(Seal (Join-Path $PSScriptRoot 'annual-paired-native-process.cs')).sha256}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'summary.json'),($summary|ConvertTo-Json -Depth 16)+"`n",$utf8)
Write-Output "Renderer evidence: $OutputDirectory"
$global:LASTEXITCODE=0
