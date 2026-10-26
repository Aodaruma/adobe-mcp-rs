$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $repo 'scripts/install-desktop-integration.ps1') -FunctionsOnly
function Assert([bool]$condition, [string]$message) { if (-not $condition) { throw $message } }
Assert ($null -eq (Get-DesktopRunValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' ('Missing-' + [guid]::NewGuid()))) 'Missing registry value must not abort a fresh install.'
$oldRoot = 'C:\Users\Example\AppData\Local\Programs\AfterEffectsMcp'
$newRoot = 'C:\Program Files\AfterEffectsMcp'
$inputText = @'
approval_policy = "on-request"
[mcp_servers."photoshop"]
command = 'C:\Users\Example\AppData\Local\Programs\AfterEffectsMcp\ps-mcp.exe'
args = ["serve-stdio", "--config", "custom.toml"]
[mcp_servers.photoshop.tools.run-script]
approval_mode = "prompt"
[mcp_servers.illustrator]
command = "C:\\custom\\ai-wrapper.exe"
args = ["custom"]
[mcp_servers.'premiere'.tools.list-instances]
approval_mode = "approve"
'@
$output = Update-DesktopCodexText $inputText $newRoot @($oldRoot)
Assert ($output.Contains('command = "C:\\Program Files\\AfterEffectsMcp\\ps-mcp.exe"')) 'Old command was not migrated.'
foreach ($preserved in @('approval_policy = "on-request"','approval_mode = "prompt"','args = ["serve-stdio", "--config", "custom.toml"]','command = "C:\\custom\\ai-wrapper.exe"')) {
    Assert ($output.Contains($preserved)) "Lost custom configuration: $preserved"
}
Assert (-not $output.Contains('[mcp_servers.premiere]')) 'Child-only TOML table would be redefined.'
Assert ($output.Contains('[mcp_servers.aftereffects]')) 'Missing host was not added.'
Assert ((Update-DesktopCodexText $output $newRoot @($oldRoot)) -ceq $output) 'Repeated setup must be idempotent.'

$testRoot = Join-Path $repo ('target/installer-test-' + [guid]::NewGuid().ToString('N'))
$script:BackupDir = Join-Path $testRoot 'backup'
$destination = Join-Path $testRoot 'legacy-premiere'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
Set-Content -LiteralPath (Join-Path $destination 'custom.txt') -Value 'preserve me'
Disable-PremiereLegacyPanel -Source (Join-Path $repo 'src/premiere/cep/mcp-bridge-premiere') -Destination $destination
[xml]$manifest = Get-Content -Raw -LiteralPath (Join-Path $destination 'CSXS/manifest.xml')
Assert ($manifest.SelectNodes('//ScriptPath | //CEFCommandLine | //StartOn').Count -eq 0) 'Legacy CEP can still execute code.'
Assert ($manifest.ExtensionManifest.DispatchInfoList.Extension.DispatchInfo.Resources.MainPath -eq './legacy-notice.html') 'Legacy notice not loaded.'
Assert ((Get-ChildItem -LiteralPath $script:BackupDir -Recurse -Filter custom.txt).Count -eq 1) 'Existing panel was not backed up.'

foreach ($scriptFile in @('scripts/package-windows.ps1','scripts/install-desktop-integration.ps1','scripts/install-bridge-installer.ps1')) {
    $tokens = $null; $parseErrors = $null
    [void][Management.Automation.Language.Parser]::ParseFile((Join-Path $repo $scriptFile), [ref]$tokens, [ref]$parseErrors)
    Assert ($parseErrors.Count -eq 0) "PowerShell syntax: $scriptFile $parseErrors"
}
Write-Host 'PASS: config migration, permission preservation, repeat setup, legacy CEP retirement/backup, script syntax.'
