[CmdletBinding()]
param([Parameter(Mandatory)][string]$VsDevCmdPath)
$ErrorActionPreference = 'Stop'
$greenLensDevCmd = [IO.Path]::GetFullPath($VsDevCmdPath)
if (-not (Test-Path -LiteralPath $greenLensDevCmd -PathType Leaf)) { throw 'The Visual Studio native environment initializer is missing.' }
# Launch-VsDevShell round-trips the whole environment through an ANSI `set`
# stream on some installations, replacing Unicode paths with question marks.
# Use the same official initializer with cmd's Unicode built-in output. The
# fixed command contains no caller text; its quoted path is a child-only env
# variable. cmd has its own quoting rules, so Arguments is intentional here.
$greenLensStart = [Diagnostics.ProcessStartInfo]::new()
$greenLensStart.FileName = Join-Path $env:SystemRoot 'System32\cmd.exe'
$greenLensStart.UseShellExecute = $false
$greenLensStart.CreateNoWindow = $true
$greenLensStart.RedirectStandardOutput = $true
$greenLensStart.RedirectStandardError = $true
$greenLensStart.StandardOutputEncoding = [Text.Encoding]::Unicode
$greenLensStart.StandardErrorEncoding = [Text.Encoding]::Unicode
$greenLensStart.Environment['GREENLENS_VSDEV_CMD_INTERNAL'] = $greenLensDevCmd
$greenLensStart.Arguments = '/d /u /v:off /s /c ""%GREENLENS_VSDEV_CMD_INTERNAL%" -no_logo -arch=amd64 -host_arch=amd64 >nul && set"'
$greenLensProcess = [Diagnostics.Process]::Start($greenLensStart)
try {
    $greenLensOutputTask = $greenLensProcess.StandardOutput.ReadToEndAsync()
    $greenLensErrorTask = $greenLensProcess.StandardError.ReadToEndAsync()
    if (-not $greenLensProcess.WaitForExit(120000)) {
        $greenLensProcess.Kill($true)
        throw 'Visual Studio environment initialization exceeded 120 seconds.'
    }
    $greenLensEnvironmentText = $greenLensOutputTask.GetAwaiter().GetResult()
    $null = $greenLensErrorTask.GetAwaiter().GetResult()
    if ($greenLensProcess.ExitCode -ne 0) { throw "Visual Studio environment initialization failed (exit $($greenLensProcess.ExitCode))." }
    $greenLensEnvironment = @{}
    foreach ($greenLensLine in ($greenLensEnvironmentText -split "`r?`n")) {
        if (-not $greenLensLine -or $greenLensLine.StartsWith('=')) { continue }
        $greenLensSeparator = $greenLensLine.IndexOf('=')
        if ($greenLensSeparator -lt 1) { throw 'Visual Studio returned a malformed environment entry.' }
        $greenLensName = $greenLensLine.Substring(0, $greenLensSeparator)
        if ($greenLensName -eq 'GREENLENS_VSDEV_CMD_INTERNAL') { continue }
        $greenLensEnvironment[$greenLensName] = $greenLensLine.Substring($greenLensSeparator + 1)
    }
    if (-not $greenLensEnvironment['VSCMD_VER'] -or $greenLensEnvironment['VSCMD_ARG_TGT_ARCH'] -ne 'x64') { throw 'Visual Studio did not initialize the required native x64 environment.' }
    foreach ($greenLensEntry in $greenLensEnvironment.GetEnumerator()) {
        [Environment]::SetEnvironmentVariable($greenLensEntry.Key, $greenLensEntry.Value, 'Process')
    }
} finally {
    $greenLensProcess.Dispose()
    # Environment contents remain process-local and must never enter logs.
    Remove-Variable greenLensEnvironmentText, greenLensEnvironment, greenLensStart -ErrorAction SilentlyContinue
}
