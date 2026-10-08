param(
    [string]$InstallDir = $PSScriptRoot,
    [switch]$Remove,
    [switch]$ShowResult,
    [switch]$NoLaunch,
    [switch]$FunctionsOnly
)
$ErrorActionPreference = 'Stop'

function Get-DesktopRunValue {
    param([string]$Path, [string]$Name)
    $key = Get-ItemProperty -LiteralPath $Path -ErrorAction SilentlyContinue
    if ($key -and $key.PSObject.Properties[$Name]) { return $key.PSObject.Properties[$Name].Value }
    return $null
}

function Update-DesktopCodexText {
    param([string]$Text, [string]$Directory, [string[]]$OldDirectories)
    $names = [ordered]@{ aftereffects='ae-mcp.exe'; premiere='pr-mcp.exe'; photoshop='ps-mcp.exe'; illustrator='ai-mcp.exe'; indesign='id-mcp.exe' }
    $seen = @{}
    $section = ''
    $lines = [Collections.Generic.List[string]]::new()
    foreach ($line in ($Text -split '\r?\n')) {
        if ($line -match '^\s*\[') {
            $section = ''
            # Recognize bare and quoted keys, including child tables, without rewriting permissions.
            if ($line -match '^\s*\[\s*(?:mcp_servers|"mcp_servers"|''mcp_servers'')\s*\.\s*["'']?(aftereffects|premiere|photoshop|illustrator|indesign)["'']?\s*(\.|\])') {
                $seen[$Matches[1]] = $true
                if ($Matches[2] -eq ']') { $section = $Matches[1] }
            }
        }
        $replacement = $line
        if ($section -and $line -match '^\s*command\s*=\s*("(?:[^"\\]|\\.)*"|''[^'']*'')\s*(?:#.*)?$') {
            $literal = $Matches[1]
            $old = if ($literal.StartsWith('"')) { $literal | ConvertFrom-Json } else { $literal.Substring(1, $literal.Length - 2) }
            $known = @($OldDirectories) + @($Directory)
            foreach ($root in $known) {
                if ($old.Replace('/', '\') -ieq (Join-Path $root $names[$section])) {
                    $replacement = 'command = ' + (ConvertTo-Json -Compress (Join-Path $Directory $names[$section]))
                    break
                }
            }
        }
        $lines.Add($replacement)
    }
    foreach ($name in $names.Keys) {
        if (-not $seen.ContainsKey($name)) {
            $lines.Add('')
            $lines.Add("[mcp_servers.$name]")
            $lines.Add('command = ' + (ConvertTo-Json -Compress (Join-Path $Directory $names[$name])))
            $lines.Add('args = ["serve-stdio"]')
            $lines.Add('startup_timeout_sec = 180')
            $lines.Add('tool_timeout_sec = 180')
        }
    }
    return ($lines -join "`r`n")
}

function Backup-DesktopPath {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return }
    New-Item -ItemType Directory -Path $script:BackupDir -Force | Out-Null
    $target = Join-Path $script:BackupDir ([guid]::NewGuid().ToString('N') + '-' + (Split-Path -Leaf $Path))
    Copy-Item -LiteralPath $Path -Destination $target -Recurse -Force
    @{ original=$Path; backup=$target } | ConvertTo-Json -Compress |
        Add-Content -LiteralPath (Join-Path $script:BackupDir 'restore-map.jsonl') -Encoding UTF8
}

function Disable-PremiereLegacyPanel {
    param([string]$Source, [string]$Destination)
    Backup-DesktopPath $Destination
    New-Item -ItemType Directory -Path (Join-Path $Destination 'CSXS') -Force | Out-Null
    [xml]$manifest = Get-Content -Raw -LiteralPath (Join-Path $Source 'CSXS/manifest.xml')
    $manifest.ExtensionManifest.ExtensionBundleName = 'Premiere MCP Bridge (Legacy CEP)'
    $dispatch = $manifest.ExtensionManifest.DispatchInfoList.Extension.DispatchInfo
    $dispatch.Resources.MainPath = './legacy-notice.html'
    foreach ($node in @($manifest.SelectNodes('//ScriptPath | //CEFCommandLine | //StartOn'))) {
        [void]$node.ParentNode.RemoveChild($node)
    }
    $dispatch.Lifecycle.AutoVisible = 'false'
    $dispatch.UI.Menu = 'Premiere MCP Bridge (Legacy CEP)'
    $manifest.Save((Join-Path $Destination 'CSXS/manifest.xml'))
    '<!doctype html><meta charset="utf-8"><title>Legacy CEP bridge</title><p>This is the old CEP panel. It is disabled. Use Plugins &gt; Premiere MCP Bridge (UXP). The bridge connects automatically.</p>' |
        Set-Content -LiteralPath (Join-Path $Destination 'legacy-notice.html') -Encoding UTF8
}

if ($FunctionsOnly) { return }

$InstallDir = (Resolve-Path -LiteralPath $InstallDir).Path
$app = Join-Path $InstallDir 'adobe-mcp-app.exe'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$shortcutRoot = Join-Path ([Environment]::GetFolderPath('Programs')) 'Adobe MCP'
$profileDir = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'adobe-mcp'
$script:BackupDir = Join-Path $profileDir ('upgrade-backup-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
$reportPath = Join-Path $profileDir 'desktop-install-result.txt'
New-Item -ItemType Directory -Path $profileDir -Force | Out-Null

try {
    if ($Remove) {
        $registered = Get-DesktopRunValue $runKey AdobeMcpDesktop
        if ($registered -and $registered.Contains($app)) { Remove-ItemProperty -LiteralPath $runKey -Name AdobeMcpDesktop }
        $shell = New-Object -ComObject WScript.Shell
        foreach ($link in @(Get-ChildItem -LiteralPath $shortcutRoot -Filter '*.lnk' -ErrorAction SilentlyContinue)) {
            $target = $shell.CreateShortcut($link.FullName)
            if ($target.TargetPath.StartsWith($InstallDir + '\', [StringComparison]::OrdinalIgnoreCase) -or $target.Arguments.Contains($InstallDir)) {
                Remove-Item -LiteralPath $link.FullName -Force
            }
        }
        return
    }
    if (-not (Test-Path -LiteralPath $app)) { throw "Desktop app missing: $app" }
    if ([Security.Principal.WindowsIdentity]::GetCurrent().IsSystem) { throw 'Run user setup as the desktop user, not SYSTEM.' }

    # Create a retry entry before any optional Adobe integration can fail.
    New-Item -ItemType Directory -Path $shortcutRoot -Force | Out-Null
    $shell = New-Object -ComObject WScript.Shell
    foreach ($entry in @(
        @{ Name='Adobe MCP'; Target=$app; Args='' },
        @{ Name='Adobe MCP - Beta guide'; Target=(Join-Path $InstallDir 'README-beta.txt'); Args='' },
        @{ Name='Adobe MCP - Repair setup'; Target="$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"; Args="-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$InstallDir\install-desktop-integration.ps1`" -ShowResult" }
    )) {
        $link = $shell.CreateShortcut((Join-Path $shortcutRoot ($entry.Name + '.lnk')))
        $link.TargetPath = $entry.Target
        $link.Arguments = $entry.Args
        $link.WorkingDirectory = $InstallDir
        $link.IconLocation = "$app,0"
        $link.Save()
    }

    $oldDirs = @((Join-Path $env:LOCALAPPDATA 'Programs\AfterEffectsMcp'), (Join-Path $env:LOCALAPPDATA 'Programs\AdobeMcp'), (Join-Path $env:ProgramFiles 'AfterEffectsMcp'))
    $live = @(Get-CimInstance Win32_Process | Where-Object {
        $_.ExecutablePath -and (Split-Path -Parent $_.ExecutablePath) -in ($oldDirs + @($InstallDir)) -and
        ($_.Name -eq 'adobe-mcp-app.exe' -or ($_.Name -match '^(ae|pr|ps|ai|id)-mcp.exe$' -and $_.CommandLine -match 'serve-daemon'))
    })
    if ($live.Count) { throw 'Close Adobe MCP / legacy MCP daemons, then run Adobe MCP - Repair setup from Start. Pending Adobe work is never force-stopped.' }

    $legacyRunNames = @('AfterEffectsMcp','PremiereMcp','PhotoshopMcp','IllustratorMcp','InDesignMcp')
    $runBackup = @{}
    foreach ($name in ($legacyRunNames + @('AdobeMcpDesktop'))) {
        $value = Get-DesktopRunValue $runKey $name
        if ($null -ne $value) { $runBackup[$name] = $value }
    }
    New-Item -ItemType Directory -Path $script:BackupDir -Force | Out-Null
    $runBackup | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $script:BackupDir 'autostart.json') -Encoding UTF8

    $configRoot = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $env:USERPROFILE '.codex' }
    $configPath = Join-Path $configRoot 'config.toml'
    $before = if (Test-Path -LiteralPath $configPath) { [IO.File]::ReadAllText($configPath) } else { '' }
    $after = Update-DesktopCodexText -Text $before -Directory $InstallDir -OldDirectories $oldDirs
    if ($after -ne $before) {
        Backup-DesktopPath $configPath
        New-Item -ItemType Directory -Path $configRoot -Force | Out-Null
        [IO.File]::WriteAllText($configPath, $after, [Text.UTF8Encoding]::new($false))
    }
    foreach ($name in $legacyRunNames) {
        if ($runBackup.ContainsKey($name)) { Remove-ItemProperty -LiteralPath $runKey -Name $name }
    }
    # Preserve the existing opt-in; a fresh install can enable login launch in the tray menu.
    if ($runBackup.Count) {
        New-Item -Path $runKey -Force | Out-Null
        $desktopArgs = ''
        if ($runBackup.ContainsKey('AdobeMcpDesktop') -and $runBackup.AdobeMcpDesktop -match '^"[^"]+"(.*)$') { $desktopArgs = $Matches[1] }
        Set-ItemProperty -LiteralPath $runKey -Name AdobeMcpDesktop -Value ('"' + $app + '"' + $desktopArgs)
    }

    # Keep a path map and retire only recognized executables, never unrelated files.
    foreach ($root in $oldDirs) {
        if ($root -ieq $InstallDir -or -not $root.StartsWith($env:LOCALAPPDATA + '\', [StringComparison]::OrdinalIgnoreCase)) { continue }
        if ((Test-Path -LiteralPath $root) -and ((Get-Item -LiteralPath $root).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Legacy directory is a link; review before migrating: $root" }
        foreach ($binary in @('adobe-mcp-app.exe','ae-mcp.exe','pr-mcp.exe','ps-mcp.exe','ai-mcp.exe','id-mcp.exe')) {
            $path = Join-Path $root $binary
            if (Test-Path -LiteralPath $path) {
                $resolved = (Resolve-Path -LiteralPath $path).Path
                if (-not $resolved.StartsWith([IO.Path]::GetFullPath($root) + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Unexpected legacy path: $resolved" }
                $retired = Join-Path $script:BackupDir ('retired-' + [guid]::NewGuid().ToString('N') + '-' + $binary)
                Move-Item -LiteralPath $path -Destination $retired
                @{ original=$path; backup=$retired } | ConvertTo-Json -Compress |
                    Add-Content -LiteralPath (Join-Path $script:BackupDir 'restore-map.jsonl') -Encoding UTF8
            }
        }
        if (Test-Path -LiteralPath $root) {
            "Replaced by Adobe MCP Beta at $InstallDir. Backup: $script:BackupDir" |
                Set-Content -LiteralPath (Join-Path $root 'REPLACED-BY-ADOBE-MCP.txt') -Encoding UTF8
        }
    }

    . (Join-Path $InstallDir 'install-bridge-installer.ps1') -FunctionsOnly
    $InstallerStateRoot = Join-Path $profileDir 'installer'
    $InstallSelectionPath = Join-Path $InstallerStateRoot 'install-selection.json'
    $InstallReportPath = Join-Path $InstallerStateRoot 'install-report.json'
    $InstallLogPath = Join-Path $InstallerStateRoot 'install.log'
    $NonInteractive = $true
    New-Item -ItemType Directory -Path $InstallerStateRoot -Force | Out-Null
    '[]' | Set-Content -LiteralPath $InstallReportPath -Encoding UTF8
    foreach ($hostName in @('photoshop','premiere')) {
        $external = Join-Path $env:APPDATA 'Adobe\UXP\Plugins\External'
        foreach ($plugin in @(Get-ChildItem -LiteralPath $external -Directory -Filter "io.github.aodaruma.$hostName-mcp-bridge*" -ErrorAction SilentlyContinue)) { Backup-DesktopPath $plugin.FullName }
    }
    foreach ($directory in @(Get-InDesignStartupScriptPaths)) { Backup-DesktopPath (Join-Path $directory 'mcp-bridge-indesign.idjs') }
    Install-PremiereUxpBridge
    Install-PhotoshopUxpBridge
    Install-InDesignStartupBridge
    if (Test-InstallComponentSelected -Key 'premiere-cep') { Enable-CepDebugMode }
    $reports = @(Read-JsonFile $InstallReportPath)
    if (@($reports | Where-Object { $_.key -eq 'premiere-uxp' -and $_.status -eq 'installed' }).Count) {
        Disable-PremiereLegacyPanel -Source (Join-Path $InstallDir 'premiere-cep\mcp-bridge-premiere') -Destination (Join-Path $env:APPDATA 'Adobe\CEP\extensions\mcp-bridge-premiere')
    }
    # User CEP overrides the old machine copy. Back it up before replacing it.
    if (@(Get-IllustratorInstallPaths).Count) {
        $destination = Join-Path $env:APPDATA 'Adobe\CEP\extensions\mcp-bridge-illustrator'
        Backup-DesktopPath $destination
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        Copy-Item -Path (Join-Path $InstallDir 'illustrator-cep\mcp-bridge-illustrator\*') -Destination $destination -Recurse -Force
        Enable-CepDebugMode
    }
    $failed = @($reports | Where-Object status -eq 'failed')
    $message = "Adobe MCP Beta setup completed.`r`nBackup: $script:BackupDir`r`nAdobe integration: $InstallReportPath`r`nMachine integration: $env:ProgramData\AfterEffectsMcp\install-report.json`r`nRestart Adobe apps and Codex to use the new bridge/commands."
    if ($failed.Count) { $message += "`r`nATTENTION: $($failed.Count) Adobe integration(s) failed. See the report and retry Repair setup." }
    $message | Set-Content -LiteralPath $reportPath -Encoding UTF8
    if (-not $NoLaunch) { Start-Process -FilePath $app -WindowStyle Hidden }
    if ($ShowResult) { Start-Process notepad.exe -ArgumentList ('"' + $reportPath + '"') }
} catch {
    "Setup needs attention: $($_.Exception.Message)`r`nBackup: $script:BackupDir`r`nRetry: Start > Adobe MCP - Repair setup" |
        Set-Content -LiteralPath $reportPath -Encoding UTF8
    if ($ShowResult) { Start-Process notepad.exe -ArgumentList ('"' + $reportPath + '"') }
    throw
}
