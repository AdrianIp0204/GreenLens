#Requires -Version 7.2
# No network, installation, engine or renderer invocation. Exercise the actual
# installer metadata helpers with a request recorder and a synthetic token.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$results = [Collections.Generic.List[string]]::new()
function Assert-InstallerAuth([bool]$Condition,[string]$Name) {
    if (!$Condition) { throw "Installer authentication regression: $Name" }
    $results.Add($Name)
}
$previousToken = $env:GITHUB_TOKEN
try {
    foreach ($case in @(
        @{script='install-energyplus.ps1';helper='Get-EnergyPlusRelease';uri='https://api.github.com/repos/NatLabRockies/EnergyPlus/releases/tags/v26.2.0'},
        @{script='install-typst.ps1';helper='Get-TypstRelease';uri='https://api.github.com/repos/typst/typst/releases/tags/v0.15.1'}
    )) {
        $tokens=$null;$errors=$null
        $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $case.script),[ref]$tokens,[ref]$errors)
        Assert-InstallerAuth (@($errors).Count -eq 0) ($case.script+': valid syntax')
        $definitions=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$true)|Where-Object Name -CEQ $case.helper)
        Assert-InstallerAuth ($definitions.Count -eq 1) ($case.script+': one metadata helper')
        $module=New-Module -ArgumentList $definitions[0].Extent.Text,$case.helper -ScriptBlock {
            param($definition,$helper)
            $script:requests=[Collections.Generic.List[object]]::new()
            function Invoke-RestMethod {
                param([string]$Uri,[hashtable]$Headers,[int]$MaximumRedirection)
                $script:requests.Add(@{uri=$Uri;headers=$Headers;maximum_redirection=$MaximumRedirection})
                return @{fixture='original synthetic release metadata'}
            }
            . ([scriptblock]::Create($definition))
            function Read-Requests { return ,$script:requests }
            function Clear-Requests { $script:requests.Clear() }
            Export-ModuleMember -Function $helper,Read-Requests,Clear-Requests
        }
        $invoke=$module.ExportedFunctions[$case.helper]
        $read=$module.ExportedFunctions['Read-Requests']
        $clear=$module.ExportedFunctions['Clear-Requests']
        foreach ($tokenCase in @(@{value=$null;name='absent'},@{value='  ';name='blank'},@{value='synthetic-test-token-not-a-credential';name='present'})) {
            $env:GITHUB_TOKEN=$tokenCase.value
            & $clear
            $response=& $invoke $case.uri
            $requests=& $read
            $request=$requests[0]
            Assert-InstallerAuth ($requests.Count -eq 1 -and $request.uri -ceq $case.uri -and $request.maximum_redirection -eq 0 -and $request.headers.Accept -ceq 'application/vnd.github+json') ($case.script+': '+$tokenCase.name+' token uses only fixed nonredirecting API request')
            $expectedAuth=$tokenCase.name -ceq 'present'
            Assert-InstallerAuth ($request.headers.ContainsKey('Authorization') -eq $expectedAuth -and (!$expectedAuth -or $request.headers.Authorization -ceq ('Bearer '+$tokenCase.value))) ($case.script+': '+$tokenCase.name+' optional authentication')
            Assert-InstallerAuth ($response.fixture -ceq 'original synthetic release metadata' -and ($response|ConvertTo-Json -Compress) -notmatch 'synthetic-test-token') ($case.script+': '+$tokenCase.name+' response excludes credentials')
        }
        foreach ($badUri in @('https://example.invalid/release',($case.uri+'?redirect=1'),($case.uri -replace '^https:','http:'))) {
            & $clear
            $rejected=$false
            try { $null=& $invoke $badUri } catch { $rejected=$_.Exception.Message -match '^Unexpected .* release API endpoint$' }
            Assert-InstallerAuth ($rejected -and (& $read).Count -eq 0) ($case.script+': rejects unexpected endpoint before any request')
        }
        $downloads=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -ceq 'Invoke-WebRequest'},$true))
        Assert-InstallerAuth ($downloads.Count -ge 1 -and @($downloads|Where-Object { @($_.CommandElements|Where-Object { $_ -is [Management.Automation.Language.CommandParameterAst] -and $_.ParameterName -in @('Headers','Authentication','Token','Credential','PreserveAuthorizationOnRedirect') }).Count }).Count -eq 0) ($case.script+': archive/font requests receive no credential arguments')
    }
} finally {
    $env:GITHUB_TOKEN=$previousToken
}
[ordered]@{schema='greenlens.installer-auth-tests.v1';passed=$results.Count;failed=0;network_requests=0;engine_invocations=0;renderer_invocations=0;checks=$results.ToArray()}|ConvertTo-Json -Depth 5
