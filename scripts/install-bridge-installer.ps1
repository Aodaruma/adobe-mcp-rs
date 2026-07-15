param(
    [string]$BridgeScriptPath,
    [string]$BridgeStartupScriptPath,
    [string]$BridgeShutdownScriptPath,
    [string]$InDesignBridgeScriptPath,
    [string]$AeMcpPath,
    [string]$PrMcpPath,
    [string]$PsMcpPath,
    [string]$AiMcpPath,
    [string]$IdMcpPath,
    [switch]$PlanInstall,
    [switch]$FinalizeInstall,
    [switch]$InteractiveInstall,
    [switch]$LaunchInteractiveInstall,
    [string]$CleanupLaunchTaskName,
    [switch]$RemoveAutostart,
    [switch]$NonInteractive,
    [switch]$SkipHostBridgeInstall,
    [switch]$SkipUserInstall
)

$ErrorActionPreference = "Stop"
$PackageVersion = "0.5.0"
$InstallerStateRoot = Join-Path $env:ProgramData "AfterEffectsMcp"
$InstallSelectionPath = Join-Path $InstallerStateRoot "install-selection.json"
$InstallReportPath = Join-Path $InstallerStateRoot "install-report.json"
$InstallLogPath = Join-Path $InstallerStateRoot "install-bridge-installer.log"
$script:DefaultInstallSelectionWritten = $false

function Ensure-InstallerStateRoot {
    if (-not (Test-Path -LiteralPath $InstallerStateRoot)) {
        New-Item -ItemType Directory -Path $InstallerStateRoot -Force | Out-Null
    }

    try {
        $acl = Get-Acl -LiteralPath $InstallerStateRoot
        $usersSid = New-Object System.Security.Principal.SecurityIdentifier("S-1-5-32-545")
        $rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
            $usersSid,
            [System.Security.AccessControl.FileSystemRights]::Modify,
            [System.Security.AccessControl.InheritanceFlags]"ContainerInherit,ObjectInherit",
            [System.Security.AccessControl.PropagationFlags]::None,
            [System.Security.AccessControl.AccessControlType]::Allow
        )
        $acl.SetAccessRule($rule)
        Set-Acl -LiteralPath $InstallerStateRoot -AclObject $acl
    } catch {}
}

function Read-JsonFile {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    $raw = Get-Content -Raw -LiteralPath $Path
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $null
    }
    return $raw | ConvertFrom-Json
}

function Write-JsonFile {
    param(
        [string]$Path,
        [object]$Value
    )

    Ensure-InstallerStateRoot
    $Value | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Path -Encoding UTF8
}

function Write-InstallerLog {
    param([string]$Message)

    try {
        Ensure-InstallerStateRoot
        $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
        Add-Content -LiteralPath $InstallLogPath -Value "[$timestamp] $Message" -Encoding UTF8
    } catch {}
}

function ConvertTo-QuotedProcessArgument {
    param([string]$Value)

    if ($null -eq $Value) {
        return '""'
    }

    return '"' + ($Value -replace '"', '\"') + '"'
}

function Add-PathArgument {
    param(
        [System.Collections.Generic.List[string]]$Arguments,
        [string]$Name,
        [string]$Value
    )

    if (-not [string]::IsNullOrWhiteSpace($Value)) {
        $Arguments.Add($Name) | Out-Null
        $Arguments.Add((ConvertTo-QuotedProcessArgument -Value $Value)) | Out-Null
    }
}

function Get-InstallerScriptPath {
    $scriptPath = $PSCommandPath
    if ([string]::IsNullOrWhiteSpace($scriptPath)) {
        $scriptPath = $MyInvocation.MyCommand.Path
    }
    if ([string]::IsNullOrWhiteSpace($scriptPath)) {
        throw "Could not resolve installer script path."
    }
    return $scriptPath
}

function Get-InteractiveUserAccount {
    try {
        $explorers = @(Get-CimInstance Win32_Process -Filter "name = 'explorer.exe'" -ErrorAction Stop |
            Sort-Object CreationDate -Descending)
        foreach ($explorer in $explorers) {
            $owner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwner -ErrorAction SilentlyContinue
            if ($owner -and $owner.User) {
                if ($owner.Domain) {
                    return ("{0}\{1}" -f $owner.Domain, $owner.User)
                }
                return $owner.User
            }
        }
    } catch {
        Write-InstallerLog -Message ("Failed to detect explorer.exe owner: {0}" -f $_.Exception.Message)
    }

    try {
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
        if ($identity -and -not [string]::IsNullOrWhiteSpace($identity.Name)) {
            return $identity.Name
        }
    } catch {}

    return $null
}

function New-InteractiveInstallerArguments {
    param([string]$ScriptPath)

    $argumentList = [System.Collections.Generic.List[string]]::new()
    $argumentList.Add("-NoProfile") | Out-Null
    $argumentList.Add("-Sta") | Out-Null
    $argumentList.Add("-WindowStyle") | Out-Null
    $argumentList.Add("Normal") | Out-Null
    $argumentList.Add("-ExecutionPolicy") | Out-Null
    $argumentList.Add("Bypass") | Out-Null
    $argumentList.Add("-File") | Out-Null
    $argumentList.Add((ConvertTo-QuotedProcessArgument -Value $scriptPath)) | Out-Null
    Add-PathArgument -Arguments $argumentList -Name "-BridgeScriptPath" -Value $BridgeScriptPath
    Add-PathArgument -Arguments $argumentList -Name "-BridgeStartupScriptPath" -Value $BridgeStartupScriptPath
    Add-PathArgument -Arguments $argumentList -Name "-BridgeShutdownScriptPath" -Value $BridgeShutdownScriptPath
    Add-PathArgument -Arguments $argumentList -Name "-InDesignBridgeScriptPath" -Value $InDesignBridgeScriptPath
    Add-PathArgument -Arguments $argumentList -Name "-AeMcpPath" -Value $AeMcpPath
    Add-PathArgument -Arguments $argumentList -Name "-PrMcpPath" -Value $PrMcpPath
    Add-PathArgument -Arguments $argumentList -Name "-PsMcpPath" -Value $PsMcpPath
    Add-PathArgument -Arguments $argumentList -Name "-AiMcpPath" -Value $AiMcpPath
    Add-PathArgument -Arguments $argumentList -Name "-IdMcpPath" -Value $IdMcpPath
    $argumentList.Add("-InteractiveInstall") | Out-Null
    return @($argumentList.ToArray())
}

function New-InstallWorkerArguments {
    param([string]$ScriptPath)

    $argumentList = [System.Collections.Generic.List[string]]::new()
    $argumentList.Add("-NoProfile") | Out-Null
    $argumentList.Add("-ExecutionPolicy") | Out-Null
    $argumentList.Add("Bypass") | Out-Null
    $argumentList.Add("-File") | Out-Null
    $argumentList.Add((ConvertTo-QuotedProcessArgument -Value $scriptPath)) | Out-Null
    Add-PathArgument -Arguments $argumentList -Name "-BridgeScriptPath" -Value $BridgeScriptPath
    Add-PathArgument -Arguments $argumentList -Name "-BridgeStartupScriptPath" -Value $BridgeStartupScriptPath
    Add-PathArgument -Arguments $argumentList -Name "-BridgeShutdownScriptPath" -Value $BridgeShutdownScriptPath
    Add-PathArgument -Arguments $argumentList -Name "-InDesignBridgeScriptPath" -Value $InDesignBridgeScriptPath
    Add-PathArgument -Arguments $argumentList -Name "-AeMcpPath" -Value $AeMcpPath
    Add-PathArgument -Arguments $argumentList -Name "-PrMcpPath" -Value $PrMcpPath
    Add-PathArgument -Arguments $argumentList -Name "-PsMcpPath" -Value $PsMcpPath
    Add-PathArgument -Arguments $argumentList -Name "-AiMcpPath" -Value $AiMcpPath
    Add-PathArgument -Arguments $argumentList -Name "-IdMcpPath" -Value $IdMcpPath
    return @($argumentList.ToArray())
}

function Test-IsAdministrator {
    try {
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
        $principal = New-Object Security.Principal.WindowsPrincipal($identity)
        return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } catch {
        return $false
    }
}

function Start-ElevatedInstallWorkerAndWait {
    $scriptPath = Get-InstallerScriptPath
    $workerArgs = @(New-InstallWorkerArguments -ScriptPath $scriptPath)

    Write-InstallerLog -Message ("Launching elevated install worker: powershell.exe {0}" -f ($workerArgs -join " "))
    try {
        $process = Start-Process -FilePath "powershell.exe" -ArgumentList $workerArgs -Verb RunAs -WindowStyle Hidden -Wait -PassThru
        if ($process) {
            Write-InstallerLog -Message ("Elevated install worker exited with code {0}." -f $process.ExitCode)
        } else {
            Write-InstallerLog -Message "Elevated install worker completed without process details."
        }
    } catch {
        Write-InstallerLog -Message ("Elevated install worker failed: {0}" -f $_.Exception.Message)
        Add-InstallReport -Key "host-integration" -Status "failed" -Message ("Elevation failed: {0}" -f $_.Exception.Message)
    }
}

function Start-InteractiveInstallerScheduledTask {
    param(
        [string]$ScriptPath,
        [string[]]$Arguments
    )

    $taskName = "AdobeMcpHostIntegration-{0}" -f ([guid]::NewGuid().ToString("N"))
    $account = Get-InteractiveUserAccount
    if ([string]::IsNullOrWhiteSpace($account)) {
        throw "Could not detect an interactive Windows user."
    }

    $taskArgs = [System.Collections.Generic.List[string]]::new()
    foreach ($arg in $Arguments) {
        $taskArgs.Add($arg) | Out-Null
    }
    $taskArgs.Add("-CleanupLaunchTaskName") | Out-Null
    $taskArgs.Add((ConvertTo-QuotedProcessArgument -Value $taskName)) | Out-Null
    $argumentLine = $taskArgs.ToArray() -join " "

    Write-InstallerLog -Message ("Registering interactive scheduled task '$taskName' for '$account': powershell.exe {0}" -f $argumentLine)
    $action = New-ScheduledTaskAction -Execute "powershell.exe" -Argument $argumentLine
    $trigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(5)
    $principal = New-ScheduledTaskPrincipal -UserId $account -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
    Start-ScheduledTask -TaskName $taskName
    Write-InstallerLog -Message ("Interactive scheduled task started: {0}" -f $taskName)
}

function Remove-LaunchScheduledTask {
    param([string]$TaskName)

    if ([string]::IsNullOrWhiteSpace($TaskName)) {
        return
    }

    try {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction Stop
        Write-InstallerLog -Message ("Removed launch scheduled task: {0}" -f $TaskName)
    } catch {
        Write-InstallerLog -Message ("Failed to remove launch scheduled task '{0}': {1}" -f $TaskName, $_.Exception.Message)
    }
}

function Start-InteractiveInstallerProcess {
    $scriptPath = Get-InstallerScriptPath
    $installerArgs = @(New-InteractiveInstallerArguments -ScriptPath $scriptPath)

    try {
        Start-InteractiveInstallerScheduledTask -ScriptPath $scriptPath -Arguments $installerArgs
        return
    } catch {
        Write-InstallerLog -Message ("Interactive scheduled task launch failed: {0}" -f $_.Exception.Message)
    }

    Write-InstallerLog -Message ("Falling back to ShellExecute runas: powershell.exe {0}" -f ($installerArgs -join " "))
    Start-Process -FilePath "powershell.exe" -ArgumentList $installerArgs -Verb RunAs -WindowStyle Normal | Out-Null
    Write-InstallerLog -Message "Interactive installer process launched via ShellExecute runas."
}

function Get-DisplayVersion {
    param([object]$Value)

    if ($null -eq $Value) {
        return "not installed"
    }
    $text = [string]$Value
    if ([string]::IsNullOrWhiteSpace($text)) {
        return "unknown"
    }
    return $text
}

function Join-OptionalPath {
    param(
        [string]$Path,
        [string]$ChildPath
    )

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }
    return (Join-Path $Path $ChildPath)
}

function Get-JsonManifestVersion {
    param([string]$ManifestPath)

    if ([string]::IsNullOrWhiteSpace($ManifestPath)) {
        return $null
    }

    try {
        $manifest = Read-JsonFile -Path $ManifestPath
        if ($manifest -and $manifest.version) {
            return [string]$manifest.version
        }
    } catch {}
    return $null
}

function Get-CepManifestVersion {
    param([string]$ManifestPath)

    if ([string]::IsNullOrWhiteSpace($ManifestPath)) {
        return $null
    }

    try {
        if (-not (Test-Path -LiteralPath $ManifestPath)) {
            return $null
        }
        [xml]$manifest = Get-Content -Raw -LiteralPath $ManifestPath
        if ($manifest.ExtensionManifest.ExtensionBundleVersion) {
            return [string]$manifest.ExtensionManifest.ExtensionBundleVersion
        }
    } catch {}
    return $null
}

function Get-PanelScriptVersion {
    param([string]$ScriptPath)

    try {
        if (-not (Test-Path -LiteralPath $ScriptPath)) {
            return $null
        }
        $match = Select-String -LiteralPath $ScriptPath -Pattern 'AE_MCP_BRIDGE_VERSION\s*=\s*"([^"]+)"' -List
        if ($match -and $match.Matches.Count -gt 0) {
            return $match.Matches[0].Groups[1].Value
        }
    } catch {}
    return $null
}

function Get-UxpInstalledVersion {
    param(
        [string]$PluginId,
        [string]$InfoFileName
    )

    try {
        $infoPath = Join-Path $env:APPDATA "Adobe\UXP\PluginsInfo\v1\$InfoFileName"
        $info = Read-JsonFile -Path $infoPath
        if ($info -and $info.plugins) {
            foreach ($plugin in @($info.plugins)) {
                if ($plugin.pluginId -eq $PluginId) {
                    return [string]$plugin.versionString
                }
            }
        }
    } catch {}

    try {
        $externalRoot = Join-Path $env:APPDATA "Adobe\UXP\Plugins\External"
        if (Test-Path -LiteralPath $externalRoot) {
            $candidate = Get-ChildItem -LiteralPath $externalRoot -Directory -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -like "$PluginId*" } |
                Sort-Object LastWriteTime -Descending |
                Select-Object -First 1
            if ($candidate) {
                return Get-JsonManifestVersion -ManifestPath (Join-Path $candidate.FullName "manifest.json")
            }
        }
    } catch {}

    return $null
}

function Get-VersionValues {
    param([string]$Text)

    $versions = @()
    if ([string]::IsNullOrWhiteSpace($Text)) {
        return $versions
    }

    foreach ($match in [regex]::Matches($Text, '\d+(?:\.\d+){1,3}')) {
        try {
            $versions += [version]$match.Value
        } catch {}
    }
    return $versions
}

function Test-VersionDowngrade {
    param(
        [string]$OldVersion,
        [string]$NewVersion
    )

    $newVersions = @(Get-VersionValues -Text $NewVersion)
    if ($newVersions.Count -eq 0) {
        return $false
    }
    $newestNew = @($newVersions | Sort-Object -Descending | Select-Object -First 1)[0]

    foreach ($old in @(Get-VersionValues -Text $OldVersion)) {
        if ($old -gt $newestNew) {
            return $true
        }
    }
    return $false
}

function New-InstallPlanItem {
    param(
        [string]$Key,
        [string]$Label,
        [bool]$Available,
        [bool]$Selected,
        [string]$OldVersion,
        [string]$NewVersion,
        [string]$Note
    )

    $oldDisplay = Get-DisplayVersion $OldVersion
    $newDisplay = Get-DisplayVersion $NewVersion
    $isDowngrade = Test-VersionDowngrade -OldVersion $oldDisplay -NewVersion $newDisplay
    $displayNote = $Note
    if ($isDowngrade) {
        $displayNote = "Warning: installed version is newer; this will downgrade it."
        if (-not [string]::IsNullOrWhiteSpace($Note)) {
            $displayNote = "$displayNote $Note"
        }
    }

    [pscustomobject]@{
        key = $Key
        label = $Label
        available = $Available
        selected = ($Available -and $Selected)
        oldVersion = $oldDisplay
        newVersion = $newDisplay
        note = $displayNote
        downgrade = $isDowngrade
    }
}

function Resolve-BridgeScriptPath {
    param([string]$InputPath)

    if (-not [string]::IsNullOrWhiteSpace($InputPath) -and (Test-Path -LiteralPath $InputPath)) {
        return (Resolve-Path -LiteralPath $InputPath).Path
    }

    $fallback = Join-Path $PSScriptRoot "mcp-bridge-auto.jsx"
    if (Test-Path -LiteralPath $fallback) {
        return (Resolve-Path -LiteralPath $fallback).Path
    }

    throw "Bridge script not found. Provide -BridgeScriptPath or place mcp-bridge-auto.jsx beside this script."
}

function Resolve-BridgeStartupScriptPath {
    param([string]$InputPath)

    if (-not [string]::IsNullOrWhiteSpace($InputPath) -and (Test-Path -LiteralPath $InputPath)) {
        return (Resolve-Path -LiteralPath $InputPath).Path
    }

    $fallback = Join-Path $PSScriptRoot "mcp-bridge-startup.jsx"
    if (Test-Path -LiteralPath $fallback) {
        return (Resolve-Path -LiteralPath $fallback).Path
    }

    throw "Bridge startup script not found. Provide -BridgeStartupScriptPath or place mcp-bridge-startup.jsx beside this script."
}

function Resolve-BridgeShutdownScriptPath {
    param([string]$InputPath)

    if (-not [string]::IsNullOrWhiteSpace($InputPath) -and (Test-Path -LiteralPath $InputPath)) {
        return (Resolve-Path -LiteralPath $InputPath).Path
    }

    $fallback = Join-Path $PSScriptRoot "mcp-bridge-shutdown.jsx"
    if (Test-Path -LiteralPath $fallback) {
        return (Resolve-Path -LiteralPath $fallback).Path
    }

    throw "Bridge shutdown script not found. Provide -BridgeShutdownScriptPath or place mcp-bridge-shutdown.jsx beside this script."
}

function Resolve-InDesignBridgeScriptPath {
    param([string]$InputPath)

    if (-not [string]::IsNullOrWhiteSpace($InputPath) -and (Test-Path -LiteralPath $InputPath -PathType Leaf)) {
        return (Resolve-Path -LiteralPath $InputPath).Path
    }

    $fallback = Join-Path $PSScriptRoot "mcp-bridge-indesign.idjs"
    if (Test-Path -LiteralPath $fallback -PathType Leaf) {
        return (Resolve-Path -LiteralPath $fallback).Path
    }

    return $null
}

function Get-InDesignStartupScriptPaths {
    $preferenceRoot = Join-Path $env:APPDATA "Adobe\InDesign"
    if (-not (Test-Path -LiteralPath $preferenceRoot -PathType Container)) {
        return @()
    }

    $targets = @()
    foreach ($versionFolder in @(Get-ChildItem -LiteralPath $preferenceRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^Version\s+' })) {
        foreach ($localeFolder in @(Get-ChildItem -LiteralPath $versionFolder.FullName -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^[a-z]{2}_[A-Z]{2}$' })) {
            $targets += Join-Path $localeFolder.FullName "Scripts\Startup Scripts"
        }
    }
    return @($targets | Sort-Object -Unique)
}

function Get-InDesignBridgeVersion {
    param([string]$ScriptPath)

    try {
        if (-not (Test-Path -LiteralPath $ScriptPath -PathType Leaf)) {
            return $null
        }
        $match = Select-String -LiteralPath $ScriptPath -Pattern 'BRIDGE_VERSION\s*=\s*"([^"]+)"' -List
        if ($match -and $match.Matches.Count -gt 0) {
            return $match.Matches[0].Groups[1].Value
        }
    } catch {}
    return $null
}

function Get-InstalledInDesignBridgeVersion {
    $versions = @()
    foreach ($startupPath in Get-InDesignStartupScriptPaths) {
        $scriptPath = Join-Path $startupPath "mcp-bridge-indesign.idjs"
        $version = Get-InDesignBridgeVersion -ScriptPath $scriptPath
        if ($version) {
            $versions += $version
        } elseif (Test-Path -LiteralPath $scriptPath -PathType Leaf) {
            $versions += "unknown"
        }
    }
    $versions = @($versions | Sort-Object -Unique)
    if ($versions.Count -eq 0) {
        return $null
    }
    return ($versions -join ", ")
}

function Install-InDesignStartupBridge {
    if (-not (Test-InstallComponentSelected -Key "indesign-startup")) {
        Add-InstallReport -Key "indesign-startup" -Status "skipped" -Message "Not selected or not available."
        return
    }

    $source = Resolve-InDesignBridgeScriptPath -InputPath $InDesignBridgeScriptPath
    if (-not $source) {
        Write-Warning "InDesign Startup Script source was not found."
        Add-InstallReport -Key "indesign-startup" -Status "skipped" -Message "InDesign Startup Script source not found."
        return
    }

    $targets = @(Get-InDesignStartupScriptPaths)
    if ($targets.Count -eq 0) {
        Write-Host "No current-user InDesign preference profile was detected. The bridge remains bundled beside the installer."
        Add-InstallReport -Key "indesign-startup" -Status "skipped" -Message "No current-user InDesign preference profile detected; use install-indesign-bridge.ps1 with -Destination."
        return
    }

    $installed = 0
    foreach ($target in $targets) {
        $destination = Join-Path $target "mcp-bridge-indesign.idjs"
        try {
            if (-not (Test-Path -LiteralPath $target -PathType Container)) {
                New-Item -ItemType Directory -Path $target -Force | Out-Null
            }
            Copy-Item -LiteralPath $source -Destination $destination -Force
            Write-Host "InDesign Startup Script installed: $destination"
            $installed++
        } catch {
            Write-Warning "Failed to install InDesign Startup Script to '$target': $($_.Exception.Message)"
        }
    }
    Add-InstallReport -Key "indesign-startup" -Status "installed" -Message "Installed to $installed detected current-user InDesign profile(s)."
}

function Get-AeInstallPaths {
    $adobeRoot = "C:\Program Files\Adobe"
    if (-not (Test-Path -LiteralPath $adobeRoot)) {
        return @()
    }

    return @(Get-ChildItem -LiteralPath $adobeRoot -Directory |
        Where-Object { $_.Name -match '^Adobe After Effects (\d{4})$' } |
        Sort-Object { [int]($_.Name -replace '^Adobe After Effects ', '') } -Descending |
        ForEach-Object { $_.FullName })
}

function Get-PremiereInstallPaths {
    $adobeRoot = "C:\Program Files\Adobe"
    if (-not (Test-Path -LiteralPath $adobeRoot)) {
        return @()
    }

    return @(Get-ChildItem -LiteralPath $adobeRoot -Directory |
        Where-Object { $_.Name -match '^Adobe Premiere Pro (\d{4})$' } |
        Sort-Object { [int]($_.Name -replace '^Adobe Premiere Pro ', '') } -Descending |
        ForEach-Object { $_.FullName })
}

function Get-IllustratorInstallPaths {
    $adobeRoot = "C:\Program Files\Adobe"
    if (-not (Test-Path -LiteralPath $adobeRoot)) {
        return @()
    }

    return @(Get-ChildItem -LiteralPath $adobeRoot -Directory |
        Where-Object { $_.Name -match '^Adobe Illustrator (\d{4})$' } |
        Sort-Object { [int]($_.Name -replace '^Adobe Illustrator ', '') } -Descending |
        ForEach-Object { $_.FullName })
}

function Get-PhotoshopInstallPaths {
    $adobeRoot = "C:\Program Files\Adobe"
    if (-not (Test-Path -LiteralPath $adobeRoot)) {
        return @()
    }

    return @(Get-ChildItem -LiteralPath $adobeRoot -Directory |
        Where-Object { $_.Name -match '^Adobe Photoshop (\d{4})$' } |
        Sort-Object { [int]($_.Name -replace '^Adobe Photoshop ', '') } -Descending |
        ForEach-Object { $_.FullName })
}

function Get-PremiereProductVersion {
    param([string]$PremierePath)

    if ([string]::IsNullOrWhiteSpace($PremierePath)) {
        return $null
    }

    $exePath = Join-Path $PremierePath "Adobe Premiere Pro.exe"
    if (-not (Test-Path -LiteralPath $exePath)) {
        return $null
    }

    $rawVersion = (Get-Item -LiteralPath $exePath).VersionInfo.ProductVersion
    if ([string]::IsNullOrWhiteSpace($rawVersion)) {
        $rawVersion = (Get-Item -LiteralPath $exePath).VersionInfo.FileVersion
    }
    if ([string]::IsNullOrWhiteSpace($rawVersion)) {
        return $null
    }

    $match = [regex]::Match($rawVersion, '(\d+)\.(\d+)(?:\.(\d+))?')
    if (-not $match.Success) {
        return $null
    }

    $patch = if ($match.Groups[3].Success) { $match.Groups[3].Value } else { "0" }
    return [version]::new([int]$match.Groups[1].Value, [int]$match.Groups[2].Value, [int]$patch)
}

function Test-PremiereSupportsUxp {
    param([string]$PremierePath)

    if ([string]::IsNullOrWhiteSpace($PremierePath)) {
        return $false
    }

    $version = Get-PremiereProductVersion -PremierePath $PremierePath
    return ($version -ne $null -and $version -ge [version]"25.6.0")
}

function Get-UxpCapablePremierePaths {
    param([string[]]$PremierePaths)

    return @($PremierePaths | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Where-Object { Test-PremiereSupportsUxp -PremierePath $_ })
}

function Resolve-PremiereExtensionSource {
    $candidate = Join-Path $PSScriptRoot "premiere-cep\mcp-bridge-premiere"
    if (Test-Path -LiteralPath $candidate) {
        return (Resolve-Path -LiteralPath $candidate).Path
    }
    return $null
}

function Resolve-PremiereUxpSource {
    $candidate = Join-Path $PSScriptRoot "premiere-uxp\mcp-bridge-premiere"
    if (Test-Path -LiteralPath (Join-Path $candidate "manifest.json")) {
        return (Resolve-Path -LiteralPath $candidate).Path
    }
    return $null
}

function Resolve-PhotoshopUxpSource {
    $candidate = Join-Path $PSScriptRoot "photoshop-uxp\mcp-bridge-photoshop"
    if (Test-Path -LiteralPath (Join-Path $candidate "manifest.json")) {
        return (Resolve-Path -LiteralPath $candidate).Path
    }
    return $null
}

function Resolve-IllustratorCepSource {
    $candidate = Join-Path $PSScriptRoot "illustrator-cep\mcp-bridge-illustrator"
    if (Test-Path -LiteralPath $candidate) {
        return (Resolve-Path -LiteralPath $candidate).Path
    }
    return $null
}

function Get-InstalledAePanelVersion {
    $versions = @()
    foreach ($aePath in Get-AeInstallPaths) {
        $panelPath = Join-Path $aePath "Support Files\Scripts\ScriptUI Panels\mcp-bridge-auto.jsx"
        $version = Get-PanelScriptVersion -ScriptPath $panelPath
        if ($version) {
            $versions += $version
        } elseif (Test-Path -LiteralPath $panelPath) {
            $versions += "unknown"
        }
    }
    $versions = @($versions | Sort-Object -Unique)
    if ($versions.Count -eq 0) {
        return $null
    }
    return ($versions -join ", ")
}

function Get-InstallPlan {
    Write-InstallerLog -Message ("Get-InstallPlan started. PSScriptRoot='{0}', UserInteractive={1}, Identity='{2}'" -f $PSScriptRoot, [Environment]::UserInteractive, [Security.Principal.WindowsIdentity]::GetCurrent().Name)

    $bridgeScript = $null
    $bridgeStartupScript = $null
    $bridgeShutdownScript = $null
    try {
        $bridgeScript = Resolve-BridgeScriptPath -InputPath $BridgeScriptPath
        $bridgeStartupScript = Resolve-BridgeStartupScriptPath -InputPath $BridgeStartupScriptPath
        $bridgeShutdownScript = Resolve-BridgeShutdownScriptPath -InputPath $BridgeShutdownScriptPath
    } catch {
        Write-InstallerLog -Message ("Get-InstallPlan: bridge script resolution failed: {0}" -f $_.Exception.Message)
    }
    Write-InstallerLog -Message ("Get-InstallPlan: bridgeScript='{0}'" -f (Get-DisplayVersion $bridgeScript))
    Write-InstallerLog -Message ("Get-InstallPlan: bridgeStartupScript='{0}'" -f (Get-DisplayVersion $bridgeStartupScript))
    Write-InstallerLog -Message ("Get-InstallPlan: bridgeShutdownScript='{0}'" -f (Get-DisplayVersion $bridgeShutdownScript))

    Write-InstallerLog -Message "Get-InstallPlan: detecting installed Adobe hosts."
    $aeTargets = Get-AeInstallPaths
    $premiereTargets = Get-PremiereInstallPaths
    $premiereUxpTargets = Get-UxpCapablePremierePaths -PremierePaths $premiereTargets
    $photoshopTargets = Get-PhotoshopInstallPaths
    $illustratorTargets = Get-IllustratorInstallPaths
    $indesignStartupTargets = @(Get-InDesignStartupScriptPaths)
    Write-InstallerLog -Message ("Get-InstallPlan: detected AE={0}, Premiere={1}, PremiereUXP={2}, Photoshop={3}, Illustrator={4}, InDesignProfiles={5}" -f $aeTargets.Count, $premiereTargets.Count, $premiereUxpTargets.Count, $photoshopTargets.Count, $illustratorTargets.Count, $indesignStartupTargets.Count)

    Write-InstallerLog -Message "Get-InstallPlan: resolving UnifiedPluginInstallerAgent."
    $upia = Find-UpiaCommand
    Write-InstallerLog -Message ("Get-InstallPlan: UPIA='{0}'" -f (Get-DisplayVersion $upia))

    Write-InstallerLog -Message "Get-InstallPlan: resolving bundled bridge sources."
    $premiereUxpSource = Resolve-PremiereUxpSource
    $photoshopUxpSource = Resolve-PhotoshopUxpSource
    $premiereCepSource = Resolve-PremiereExtensionSource
    $illustratorCepSource = Resolve-IllustratorCepSource
    $indesignSource = Resolve-InDesignBridgeScriptPath -InputPath $InDesignBridgeScriptPath
    Write-InstallerLog -Message ("Get-InstallPlan: sources PremiereUXP='{0}', PhotoshopUXP='{1}', PremiereCEP='{2}', IllustratorCEP='{3}', InDesign='{4}'" -f (Get-DisplayVersion $premiereUxpSource), (Get-DisplayVersion $photoshopUxpSource), (Get-DisplayVersion $premiereCepSource), (Get-DisplayVersion $illustratorCepSource), (Get-DisplayVersion $indesignSource))

    $items = @()
    Write-InstallerLog -Message "Get-InstallPlan: building After Effects item."
    $items += New-InstallPlanItem `
        -Key "aftereffects-panel" `
        -Label "After Effects headless Startup bridge" `
        -Available ([bool]$bridgeScript -and [bool]$bridgeStartupScript -and [bool]$bridgeShutdownScript -and $aeTargets.Count -gt 0) `
        -Selected $true `
        -OldVersion (Get-InstalledAePanelVersion) `
        -NewVersion (Get-PanelScriptVersion -ScriptPath $bridgeScript) `
        -Note "Installs the runtime and headless Startup bootstrap into detected After Effects versions."

    Write-InstallerLog -Message "Get-InstallPlan: building Premiere UXP item."
    $items += New-InstallPlanItem `
        -Key "premiere-uxp" `
        -Label "Premiere Pro UXP bridge" `
        -Available ([bool]$premiereUxpSource -and $premiereUxpTargets.Count -gt 0 -and [bool]$upia) `
        -Selected $true `
        -OldVersion (Get-UxpInstalledVersion -PluginId "io.github.aodaruma.premiere-mcp-bridge" -InfoFileName "premierepro.json") `
        -NewVersion (Get-JsonManifestVersion -ManifestPath (Join-OptionalPath -Path $premiereUxpSource -ChildPath "manifest.json")) `
        -Note "Preferred for Premiere Pro 25.6 or newer."

    Write-InstallerLog -Message "Get-InstallPlan: building Premiere CEP item."
    $items += New-InstallPlanItem `
        -Key "premiere-cep" `
        -Label "Premiere Pro CEP fallback" `
        -Available ([bool]$premiereCepSource -and $premiereTargets.Count -gt 0 -and $premiereUxpTargets.Count -eq 0) `
        -Selected $true `
        -OldVersion (Get-CepManifestVersion -ManifestPath "C:\Program Files (x86)\Common Files\Adobe\CEP\extensions\mcp-bridge-premiere\CSXS\manifest.xml") `
        -NewVersion (Get-CepManifestVersion -ManifestPath (Join-OptionalPath -Path $premiereCepSource -ChildPath "CSXS\manifest.xml")) `
        -Note "Only used when no UXP-capable Premiere Pro is detected."

    Write-InstallerLog -Message "Get-InstallPlan: building Photoshop UXP item."
    $items += New-InstallPlanItem `
        -Key "photoshop-uxp" `
        -Label "Photoshop UXP bridge" `
        -Available ([bool]$photoshopUxpSource -and $photoshopTargets.Count -gt 0 -and [bool]$upia) `
        -Selected $true `
        -OldVersion (Get-UxpInstalledVersion -PluginId "io.github.aodaruma.photoshop-mcp-bridge" -InfoFileName "PS.json") `
        -NewVersion (Get-JsonManifestVersion -ManifestPath (Join-OptionalPath -Path $photoshopUxpSource -ChildPath "manifest.json")) `
        -Note "Installs the plugin so it appears under Photoshop > Plugins."

    Write-InstallerLog -Message "Get-InstallPlan: building Illustrator CEP item."
    $items += New-InstallPlanItem `
        -Key "illustrator-cep" `
        -Label "Illustrator CEP bridge" `
        -Available ([bool]$illustratorCepSource -and $illustratorTargets.Count -gt 0) `
        -Selected $true `
        -OldVersion (Get-CepManifestVersion -ManifestPath "C:\Program Files (x86)\Common Files\Adobe\CEP\extensions\mcp-bridge-illustrator\CSXS\manifest.xml") `
        -NewVersion (Get-CepManifestVersion -ManifestPath (Join-OptionalPath -Path $illustratorCepSource -ChildPath "CSXS\manifest.xml")) `
        -Note "Installs the CEP panel under Window > Extensions."

    Write-InstallerLog -Message "Get-InstallPlan: building InDesign Startup Script item."
    $items += New-InstallPlanItem `
        -Key "indesign-startup" `
        -Label "InDesign UXP Startup Script" `
        -Available ([bool]$indesignSource) `
        -Selected $true `
        -OldVersion (Get-InstalledInDesignBridgeVersion) `
        -NewVersion (Get-InDesignBridgeVersion -ScriptPath $indesignSource) `
        -Note ("Installs only into detected current-user preference profiles ({0} detected); otherwise use the bundled dedicated installer with an explicit destination." -f $indesignStartupTargets.Count)

    Write-InstallerLog -Message "Get-InstallPlan: resolving Codex config paths."
    $codexConfigPaths = @(Get-CodexConfigPaths)
    Write-InstallerLog -Message ("Get-InstallPlan: Codex config count={0}" -f $codexConfigPaths.Count)

    Write-InstallerLog -Message "Get-InstallPlan: building Codex config item."
    $items += New-InstallPlanItem `
        -Key "codex-config" `
        -Label "Codex MCP config" `
        -Available ($codexConfigPaths.Count -gt 0) `
        -Selected $true `
        -OldVersion "configured / not configured" `
        -NewVersion $PackageVersion `
        -Note "Adds missing current-user Codex config.toml tables; existing MCP tables are left unchanged."

    Write-InstallerLog -Message ("Get-InstallPlan completed. Item count={0}" -f $items.Count)
    return $items
}

function Write-DefaultInstallSelection {
    $items = @(Get-InstallPlan)
    Write-JsonFile -Path $InstallSelectionPath -Value ([pscustomobject]@{
        packageVersion = $PackageVersion
        cancelled = $false
        createdAt = (Get-Date).ToString("o")
        items = $items
    })
    Write-JsonFile -Path $InstallReportPath -Value @()
    return $items
}

function Show-InstallPlanDialog {
    Write-InstallerLog -Message "Show-InstallPlanDialog entered."
    $items = @(Get-InstallPlan)
    Write-InstallerLog -Message ("Show-InstallPlanDialog: plan item count={0}" -f $items.Count)
    if ($NonInteractive -or -not [Environment]::UserInteractive) {
        Write-InstallerLog -Message ("Install plan dialog skipped. NonInteractive={0}, UserInteractive={1}" -f $NonInteractive, [Environment]::UserInteractive)
        return Write-DefaultInstallSelection
    }

    try {
        Write-InstallerLog -Message "Show-InstallPlanDialog: loading Windows Forms assemblies."
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -AssemblyName System.Drawing
        Write-InstallerLog -Message "Show-InstallPlanDialog: Windows Forms assemblies loaded."
    } catch {
        Write-InstallerLog -Message ("Windows Forms unavailable for install plan dialog: {0}" -f $_.Exception.Message)
        Write-Warning "Windows Forms is unavailable. Proceeding with default install selection."
        return Write-DefaultInstallSelection
    }

    Write-InstallerLog -Message "Show-InstallPlanDialog: creating form."
    $form = New-Object System.Windows.Forms.Form
    $form.Text = "Adobe MCP $PackageVersion - Install Options"
    $form.StartPosition = "CenterScreen"
    $form.Size = New-Object System.Drawing.Size(820, 460)
    $form.MinimumSize = New-Object System.Drawing.Size(760, 420)
    $form.ShowInTaskbar = $true
    $form.TopMost = $true
    $form.Add_Shown({ $this.Activate(); $this.BringToFront() })

    $title = New-Object System.Windows.Forms.Label
    $title.Text = "Select host integrations to install"
    $title.AutoSize = $true
    $title.Font = New-Object System.Drawing.Font($title.Font, [System.Drawing.FontStyle]::Bold)
    $title.Location = New-Object System.Drawing.Point(12, 12)
    $form.Controls.Add($title)

    $downgradeItems = @($items | Where-Object { [bool]$_.downgrade })
    $gridTop = 42
    $gridHeight = 315
    if ($downgradeItems.Count -gt 0) {
        $warning = New-Object System.Windows.Forms.Label
        $warning.Text = "Warning: one or more components will install an older version than currently installed."
        $warning.AutoSize = $true
        $warning.ForeColor = [System.Drawing.Color]::DarkOrange
        $warning.Location = New-Object System.Drawing.Point(12, 34)
        $form.Controls.Add($warning)
        $gridTop = 62
        $gridHeight = 295
    }

    $grid = New-Object System.Windows.Forms.DataGridView
    $grid.Location = New-Object System.Drawing.Point(12, $gridTop)
    $grid.Size = New-Object System.Drawing.Size(780, $gridHeight)
    $grid.Anchor = [System.Windows.Forms.AnchorStyles]"Top,Bottom,Left,Right"
    $grid.AllowUserToAddRows = $false
    $grid.AllowUserToDeleteRows = $false
    $grid.RowHeadersVisible = $false
    $grid.SelectionMode = "FullRowSelect"
    $grid.AutoSizeColumnsMode = "Fill"

    $installColumn = New-Object System.Windows.Forms.DataGridViewCheckBoxColumn
    $installColumn.HeaderText = "Install"
    $installColumn.FillWeight = 12
    $grid.Columns.Add($installColumn) | Out-Null
    $grid.Columns.Add("component", "Component") | Out-Null
    $grid.Columns["component"].FillWeight = 30
    $grid.Columns.Add("oldVersion", "Current") | Out-Null
    $grid.Columns["oldVersion"].FillWeight = 18
    $grid.Columns.Add("newVersion", "New") | Out-Null
    $grid.Columns["newVersion"].FillWeight = 14
    $grid.Columns.Add("note", "Notes") | Out-Null
    $grid.Columns["note"].FillWeight = 44

    Write-InstallerLog -Message "Show-InstallPlanDialog: populating grid rows."
    foreach ($item in $items) {
        $rowIndex = $grid.Rows.Add($item.selected, $item.label, $item.oldVersion, $item.newVersion, $item.note)
        $row = $grid.Rows[$rowIndex]
        $row.Tag = $item.key
        if (-not $item.available) {
            $row.Cells[0].Value = $false
            $row.Cells[0].ReadOnly = $true
            $row.DefaultCellStyle.ForeColor = [System.Drawing.Color]::Gray
        } elseif ([bool]$item.downgrade) {
            $row.DefaultCellStyle.ForeColor = [System.Drawing.Color]::DarkOrange
        }
    }
    $form.Controls.Add($grid)
    Write-InstallerLog -Message "Show-InstallPlanDialog: grid rows populated."

    $installButton = New-Object System.Windows.Forms.Button
    $installButton.Text = "Install Selected"
    $installButton.Anchor = [System.Windows.Forms.AnchorStyles]"Bottom,Right"
    $installButton.Location = New-Object System.Drawing.Point(565, 372)
    $installButton.Size = New-Object System.Drawing.Size(110, 32)
    $installButton.DialogResult = [System.Windows.Forms.DialogResult]::OK
    $form.Controls.Add($installButton)
    $form.AcceptButton = $installButton

    $cancelButton = New-Object System.Windows.Forms.Button
    $cancelButton.Text = "Skip Host Setup"
    $cancelButton.Anchor = [System.Windows.Forms.AnchorStyles]"Bottom,Right"
    $cancelButton.Location = New-Object System.Drawing.Point(682, 372)
    $cancelButton.Size = New-Object System.Drawing.Size(110, 32)
    $cancelButton.DialogResult = [System.Windows.Forms.DialogResult]::Cancel
    $form.Controls.Add($cancelButton)
    $form.CancelButton = $cancelButton

    Write-InstallerLog -Message "Show-InstallPlanDialog: form constructed."
    Write-InstallerLog -Message "Displaying install plan dialog."
    $dialogResult = $form.ShowDialog()
    Write-InstallerLog -Message ("Install plan dialog result: {0}" -f $dialogResult)
    if ($dialogResult -eq [System.Windows.Forms.DialogResult]::Cancel) {
        foreach ($item in $items) {
            $item.selected = $false
        }
        Write-JsonFile -Path $InstallSelectionPath -Value ([pscustomobject]@{
            packageVersion = $PackageVersion
            cancelled = $true
            createdAt = (Get-Date).ToString("o")
            items = $items
        })
        Write-JsonFile -Path $InstallReportPath -Value @()
        return $items
    }

    for ($i = 0; $i -lt $grid.Rows.Count; $i++) {
        $key = [string]$grid.Rows[$i].Tag
        $item = $items | Where-Object { $_.key -eq $key } | Select-Object -First 1
        if ($null -ne $item) {
            $item.selected = [bool]$grid.Rows[$i].Cells[0].Value -and [bool]$item.available
        }
    }

    Write-JsonFile -Path $InstallSelectionPath -Value ([pscustomobject]@{
        packageVersion = $PackageVersion
        cancelled = $false
        createdAt = (Get-Date).ToString("o")
        items = $items
    })
    Write-JsonFile -Path $InstallReportPath -Value @()
    return $items
}

function Get-InstallSelection {
    $selection = Read-JsonFile -Path $InstallSelectionPath
    $shouldWriteDefault = (([bool]$NonInteractive -and -not $script:DefaultInstallSelectionWritten) -or -not $selection -or [string]$selection.packageVersion -ne $PackageVersion)
    if ($shouldWriteDefault) {
        Write-InstallerLog -Message ("Writing default install selection. NonInteractive={0}, ExistingVersion='{1}', PackageVersion='{2}'" -f $NonInteractive, $(if ($selection) { [string]$selection.packageVersion } else { "none" }), $PackageVersion)
        Write-DefaultInstallSelection | Out-Null
        $script:DefaultInstallSelectionWritten = $true
        $selection = Read-JsonFile -Path $InstallSelectionPath
    }
    return $selection
}

function Get-InstallSelectionItem {
    param([string]$Key)

    $selection = Get-InstallSelection
    if ($selection.cancelled) {
        return $null
    }
    foreach ($item in @($selection.items)) {
        if ($item.key -eq $Key) {
            return $item
        }
    }
    return $null
}

function Test-InstallComponentSelected {
    param([string]$Key)

    $item = Get-InstallSelectionItem -Key $Key
    return ($item -and [bool]$item.available -and [bool]$item.selected)
}

function Add-InstallReport {
    param(
        [string]$Key,
        [string]$Status,
        [string]$Message
    )

    $item = Get-InstallSelectionItem -Key $Key
    $label = if ($item) { [string]$item.label } else { $Key }
    $oldVersion = if ($item) { [string]$item.oldVersion } else { "unknown" }
    $newVersion = if ($item) { [string]$item.newVersion } else { $PackageVersion }
    $reports = @()
    $existing = Read-JsonFile -Path $InstallReportPath
    if ($existing) {
        $reports = @($existing)
    }
    $reports += [pscustomobject]@{
        key = $Key
        label = $label
        oldVersion = $oldVersion
        newVersion = $newVersion
        status = $Status
        message = $Message
        updatedAt = (Get-Date).ToString("o")
    }
    Write-JsonFile -Path $InstallReportPath -Value $reports
}

function Get-InstallSummaryRows {
    $selection = Get-InstallSelection
    $reports = @()
    $existing = Read-JsonFile -Path $InstallReportPath
    if ($existing) {
        $reports = @($existing)
    }

    $rows = @()
    foreach ($item in @($selection.items)) {
        $report = $reports | Where-Object { $_.key -eq $item.key } | Select-Object -Last 1
        $status = "skipped"
        $message = "Not selected."
        if ([bool]$item.selected -and [bool]$item.available) {
            $status = "pending"
            $message = "No installer action reported a result."
        } elseif (-not [bool]$item.available) {
            $message = "Not available on this machine."
        }
        if ($report) {
            $status = [string]$report.status
            $message = [string]$report.message
        }
        $rows += [pscustomobject]@{
            Component = [string]$item.label
            Selected = if ([bool]$item.selected) { "yes" } else { "no" }
            Current = [string]$item.oldVersion
            New = [string]$item.newVersion
            Status = $status
            Message = $message
        }
    }
    return $rows
}

function Show-InstallSummaryDialog {
    $rows = @(Get-InstallSummaryRows)
    if ($NonInteractive -or -not [Environment]::UserInteractive) {
        Write-InstallerLog -Message ("Install summary dialog skipped. NonInteractive={0}, UserInteractive={1}" -f $NonInteractive, [Environment]::UserInteractive)
        Write-Host "Adobe MCP $PackageVersion install summary:"
        foreach ($row in $rows) {
            Write-Host ("- {0}: {1} ({2} -> {3}) {4}" -f $row.Component, $row.Status, $row.Current, $row.New, $row.Message)
        }
        return
    }

    try {
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -AssemblyName System.Drawing
    } catch {
        Write-InstallerLog -Message ("Windows Forms unavailable for install summary dialog: {0}" -f $_.Exception.Message)
        foreach ($row in $rows) {
            Write-Host ("- {0}: {1} ({2} -> {3}) {4}" -f $row.Component, $row.Status, $row.Current, $row.New, $row.Message)
        }
        return
    }

    $form = New-Object System.Windows.Forms.Form
    $form.Text = "Adobe MCP $PackageVersion - Installation Complete"
    $form.StartPosition = "CenterScreen"
    $form.Size = New-Object System.Drawing.Size(900, 460)
    $form.MinimumSize = New-Object System.Drawing.Size(820, 420)
    $form.ShowInTaskbar = $true
    $form.TopMost = $true
    $form.Add_Shown({ $this.Activate(); $this.BringToFront() })

    $title = New-Object System.Windows.Forms.Label
    $title.Text = "Installation complete"
    $title.AutoSize = $true
    $title.Font = New-Object System.Drawing.Font($title.Font, [System.Drawing.FontStyle]::Bold)
    $title.Location = New-Object System.Drawing.Point(12, 12)
    $form.Controls.Add($title)

    $hint = New-Object System.Windows.Forms.Label
    $hint.Text = "Restart Adobe host apps if a newly installed panel is not visible."
    $hint.AutoSize = $true
    $hint.Location = New-Object System.Drawing.Point(12, 34)
    $form.Controls.Add($hint)

    $grid = New-Object System.Windows.Forms.DataGridView
    $grid.Location = New-Object System.Drawing.Point(12, 62)
    $grid.Size = New-Object System.Drawing.Size(860, 300)
    $grid.Anchor = [System.Windows.Forms.AnchorStyles]"Top,Bottom,Left,Right"
    $grid.AllowUserToAddRows = $false
    $grid.AllowUserToDeleteRows = $false
    $grid.ReadOnly = $true
    $grid.RowHeadersVisible = $false
    $grid.SelectionMode = "FullRowSelect"
    $grid.AutoSizeColumnsMode = "Fill"
    $grid.Columns.Add("component", "Component") | Out-Null
    $grid.Columns["component"].FillWeight = 24
    $grid.Columns.Add("selected", "Selected") | Out-Null
    $grid.Columns["selected"].FillWeight = 10
    $grid.Columns.Add("current", "Current") | Out-Null
    $grid.Columns["current"].FillWeight = 12
    $grid.Columns.Add("new", "New") | Out-Null
    $grid.Columns["new"].FillWeight = 10
    $grid.Columns.Add("status", "Status") | Out-Null
    $grid.Columns["status"].FillWeight = 12
    $grid.Columns.Add("message", "Message") | Out-Null
    $grid.Columns["message"].FillWeight = 42

    foreach ($row in $rows) {
        $rowIndex = $grid.Rows.Add($row.Component, $row.Selected, $row.Current, $row.New, $row.Status, $row.Message)
        if ($row.Status -eq "failed") {
            $grid.Rows[$rowIndex].DefaultCellStyle.ForeColor = [System.Drawing.Color]::DarkRed
        } elseif ($row.Status -eq "skipped") {
            $grid.Rows[$rowIndex].DefaultCellStyle.ForeColor = [System.Drawing.Color]::Gray
        }
    }
    $form.Controls.Add($grid)

    $okButton = New-Object System.Windows.Forms.Button
    $okButton.Text = "OK"
    $okButton.Anchor = [System.Windows.Forms.AnchorStyles]"Bottom,Right"
    $okButton.Location = New-Object System.Drawing.Point(792, 374)
    $okButton.Size = New-Object System.Drawing.Size(80, 32)
    $okButton.DialogResult = [System.Windows.Forms.DialogResult]::OK
    $form.Controls.Add($okButton)
    $form.AcceptButton = $okButton

    Write-InstallerLog -Message "Displaying install summary dialog."
    $form.ShowDialog() | Out-Null
    Write-InstallerLog -Message "Install summary dialog closed."
}

function Find-UpiaCommand {
    Write-InstallerLog -Message "Find-UpiaCommand started."
    $candidates = @(
        "C:\Program Files\Common Files\Adobe\Adobe Desktop Common\RemoteComponents\UPI\UnifiedPluginInstallerAgent\UnifiedPluginInstallerAgent.exe",
        "C:\Program Files (x86)\Common Files\Adobe\Adobe Desktop Common\RemoteComponents\UPI\UnifiedPluginInstallerAgent\UnifiedPluginInstallerAgent.exe"
    )

    foreach ($candidate in $candidates) {
        Write-InstallerLog -Message ("Find-UpiaCommand: checking '{0}'" -f $candidate)
        if (Test-Path -LiteralPath $candidate) {
            $resolved = (Resolve-Path -LiteralPath $candidate).Path
            Write-InstallerLog -Message ("Find-UpiaCommand: found known path '{0}'" -f $resolved)
            return $resolved
        }
    }

    $searchRoots = @(
        "C:\Program Files\Common Files\Adobe\Adobe Desktop Common\RemoteComponents",
        "C:\Program Files (x86)\Common Files\Adobe\Adobe Desktop Common\RemoteComponents"
    )
    foreach ($root in $searchRoots) {
        if (-not (Test-Path -LiteralPath $root)) {
            Write-InstallerLog -Message ("Find-UpiaCommand: skipped missing root '{0}'" -f $root)
            continue
        }

        Write-InstallerLog -Message ("Find-UpiaCommand: searching Adobe root '{0}'" -f $root)
        $found = Get-ChildItem -LiteralPath $root -Directory -ErrorAction SilentlyContinue |
            ForEach-Object {
                Get-ChildItem -LiteralPath $_.FullName -Recurse -Filter "UnifiedPluginInstallerAgent.exe" -ErrorAction SilentlyContinue
            } |
            Select-Object -First 1
        if ($found) {
            Write-InstallerLog -Message ("Find-UpiaCommand: found searched path '{0}'" -f $found.FullName)
            return $found.FullName
        }
    }

    Write-InstallerLog -Message "Find-UpiaCommand completed without a result."
    return $null
}

function New-CcxPackage {
    param(
        [string]$SourceDir,
        [string]$PackageName
    )

    $safeName = ($PackageName -replace '[^A-Za-z0-9_.-]', '-')
    $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ($safeName + "-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
    $zipPath = Join-Path $tempRoot ($safeName + ".zip")
    $ccxPath = Join-Path $tempRoot ($safeName + ".ccx")
    Compress-Archive -Path (Join-Path $SourceDir "*") -DestinationPath $zipPath -Force
    Move-Item -LiteralPath $zipPath -Destination $ccxPath -Force
    return $ccxPath
}

function Install-PremiereUxpBridge {
    if (-not (Test-InstallComponentSelected -Key "premiere-uxp")) {
        Add-InstallReport -Key "premiere-uxp" -Status "skipped" -Message "Not selected or not available."
        return
    }

    $premiereTargets = Get-PremiereInstallPaths
    $uxpTargets = Get-UxpCapablePremierePaths -PremierePaths $premiereTargets
    if ($uxpTargets.Count -eq 0) {
        Write-Host "No UXP-capable Premiere Pro installation was detected. Skipped Premiere UXP deployment."
        Add-InstallReport -Key "premiere-uxp" -Status "skipped" -Message "No UXP-capable Premiere Pro installation was detected."
        return
    }

    $source = Resolve-PremiereUxpSource
    if (-not $source) {
        Write-Host "Premiere UXP bridge source not found. Skipped UXP deployment."
        Add-InstallReport -Key "premiere-uxp" -Status "skipped" -Message "Premiere UXP bridge source not found."
        return
    }

    $upia = Find-UpiaCommand
    if (-not $upia) {
        Write-Warning "UnifiedPluginInstallerAgent.exe was not found. Skipped Premiere UXP deployment."
        Write-Host "Premiere UXP bridge bundled. Load with Adobe UXP Developer Tool: $(Join-Path $source "manifest.json")"
        Add-InstallReport -Key "premiere-uxp" -Status "skipped" -Message "UnifiedPluginInstallerAgent.exe was not found."
        return
    }

    $ccx = New-CcxPackage -SourceDir $source -PackageName "Premiere-MCP-Bridge"
    try {
        $output = & $upia /install $ccx 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "Premiere UXP install failed: $output"
            Add-InstallReport -Key "premiere-uxp" -Status "failed" -Message "UPIA failed: $output"
            return
        }
        Write-Host "Premiere UXP bridge installed via Unified Plugin Installer Agent."
        Write-Host ("UXP-capable Premiere Pro target(s): {0}" -f ($uxpTargets -join ", "))
        Add-InstallReport -Key "premiere-uxp" -Status "installed" -Message ("Installed via UPIA for: {0}" -f ($uxpTargets -join ", "))
    } finally {
        $tempRoot = Split-Path -Parent $ccx
        if (Test-Path -LiteralPath $tempRoot) {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Install-PhotoshopUxpBridge {
    if (-not (Test-InstallComponentSelected -Key "photoshop-uxp")) {
        Add-InstallReport -Key "photoshop-uxp" -Status "skipped" -Message "Not selected or not available."
        return
    }

    $photoshopTargets = Get-PhotoshopInstallPaths
    if ($photoshopTargets.Count -eq 0) {
        Write-Host "No Adobe Photoshop installation was detected. Skipped Photoshop UXP deployment."
        Add-InstallReport -Key "photoshop-uxp" -Status "skipped" -Message "No Adobe Photoshop installation was detected."
        return
    }

    $source = Resolve-PhotoshopUxpSource
    if (-not $source) {
        Write-Host "Photoshop UXP bridge source not found. Skipped UXP deployment."
        Add-InstallReport -Key "photoshop-uxp" -Status "skipped" -Message "Photoshop UXP bridge source not found."
        return
    }

    $upia = Find-UpiaCommand
    if (-not $upia) {
        Write-Warning "UnifiedPluginInstallerAgent.exe was not found. Skipped Photoshop UXP deployment."
        Write-Host "Photoshop UXP bridge bundled. Load with Adobe UXP Developer Tool: $(Join-Path $source "manifest.json")"
        Add-InstallReport -Key "photoshop-uxp" -Status "skipped" -Message "UnifiedPluginInstallerAgent.exe was not found."
        return
    }

    $ccx = New-CcxPackage -SourceDir $source -PackageName "Photoshop-MCP-Bridge"
    try {
        $output = & $upia /install $ccx 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "Photoshop UXP install failed: $output"
            Add-InstallReport -Key "photoshop-uxp" -Status "failed" -Message "UPIA failed: $output"
            return
        }
        Write-Host "Photoshop UXP bridge installed via Unified Plugin Installer Agent."
        Add-InstallReport -Key "photoshop-uxp" -Status "installed" -Message ("Installed via UPIA. Photoshop target(s): {0}" -f ($photoshopTargets -join ", "))
    } finally {
        $tempRoot = Split-Path -Parent $ccx
        if (Test-Path -LiteralPath $tempRoot) {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Install-IllustratorCepBridge {
    if (-not (Test-InstallComponentSelected -Key "illustrator-cep")) {
        Add-InstallReport -Key "illustrator-cep" -Status "skipped" -Message "Not selected or not available."
        return
    }

    $illustratorTargets = Get-IllustratorInstallPaths
    $source = Resolve-IllustratorCepSource
    if (-not $source) {
        Write-Host "Illustrator CEP extension not found. Skipped Illustrator CEP deployment."
        Add-InstallReport -Key "illustrator-cep" -Status "skipped" -Message "Illustrator CEP extension source not found."
        return
    }
    if ($illustratorTargets.Count -eq 0) {
        Write-Host "No Adobe Illustrator installation was detected. Skipped Illustrator CEP deployment."
        Add-InstallReport -Key "illustrator-cep" -Status "skipped" -Message "No Adobe Illustrator installation was detected."
        return
    }

    $cepRoot = "C:\Program Files (x86)\Common Files\Adobe\CEP\extensions"
    $illustratorDest = Join-Path $cepRoot "mcp-bridge-illustrator"

    try {
        if (-not (Test-Path -LiteralPath $cepRoot)) {
            New-Item -ItemType Directory -Path $cepRoot -Force | Out-Null
        }
        if (Test-Path -LiteralPath $illustratorDest) {
            Remove-Item -LiteralPath $illustratorDest -Recurse -Force
        }
        Copy-Item -LiteralPath $source -Destination $illustratorDest -Recurse -Force
        Write-Host "Illustrator CEP bridge installed: $illustratorDest"
        Add-InstallReport -Key "illustrator-cep" -Status "installed" -Message "Installed to $illustratorDest"
    } catch {
        Write-Warning "Failed to install Illustrator CEP bridge: $($_.Exception.Message)"
        Add-InstallReport -Key "illustrator-cep" -Status "failed" -Message $_.Exception.Message
    }
}

function Enable-CepDebugMode {
    $versions = @("10", "11", "12", "13")
    foreach ($version in $versions) {
        $key = "HKCU:\Software\Adobe\CSXS.$version"
        try {
            if (-not (Test-Path -LiteralPath $key)) {
                New-Item -Path $key -Force | Out-Null
            }
            New-ItemProperty -Path $key -Name "PlayerDebugMode" -Value "1" -PropertyType String -Force | Out-Null
        } catch {
            Write-Warning "Failed to set CEP debug mode for CSXS.${version}: $($_.Exception.Message)"
        }
    }
}

function Resolve-McpBinaryPath {
    param(
        [string]$ProvidedPath,
        [string]$FileName
    )

    if (-not [string]::IsNullOrWhiteSpace($ProvidedPath) -and (Test-Path -LiteralPath $ProvidedPath)) {
        return (Resolve-Path -LiteralPath $ProvidedPath).Path
    }

    $besideScript = Join-Path $PSScriptRoot $FileName
    if (Test-Path -LiteralPath $besideScript) {
        return (Resolve-Path -LiteralPath $besideScript).Path
    }

    $installed = Join-Path "C:\Program Files\AfterEffectsMcp" $FileName
    if (Test-Path -LiteralPath $installed) {
        return (Resolve-Path -LiteralPath $installed).Path
    }

    return $null
}

function Get-CodexConfigPaths {
    $codexHome = $null
    if (-not [string]::IsNullOrWhiteSpace($env:CODEX_HOME)) {
        $codexHome = $env:CODEX_HOME
    } else {
        $userProfile = $env:USERPROFILE
        if ([string]::IsNullOrWhiteSpace($userProfile)) {
            $userProfile = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
        }
        if (-not [string]::IsNullOrWhiteSpace($userProfile)) {
            $codexHome = Join-Path $userProfile ".codex"
        }
    }

    if ([string]::IsNullOrWhiteSpace($codexHome)) {
        return @()
    }
    return @((Join-Path $codexHome "config.toml"))
}

function Format-TomlLiteral {
    param([string]$Value)
    $escaped = $Value.Replace('\', '\\').Replace('"', '\"')
    return '"' + $escaped + '"'
}

function Test-TomlTableExists {
    param(
        [string[]]$Lines,
        [string]$Header
    )

    $segments = @($Header -split '\.')
    $headerPattern = (($segments | ForEach-Object { [regex]::Escape($_) }) -join '\s*\.\s*')
    $pattern = '^\s*\[\s*' + $headerPattern + '\s*\]\s*(?:#.*)?$'
    foreach ($line in @($Lines)) {
        if ($line -match $pattern) {
            return $true
        }
    }
    return $false
}

function New-CodexMcpServerSection {
    param(
        [string]$Header,
        [string]$Command
    )

    return @(
        "[$Header]",
        ("command = " + (Format-TomlLiteral $Command)),
        'args = ["serve-stdio"]',
        "startup_timeout_sec = 180",
        "tool_timeout_sec = 180"
    )
}

function Add-MissingCodexMcpServers {
    param(
        [string]$ConfigPath,
        [object[]]$Servers,
        [switch]$DryRun
    )

    $configExists = Test-Path -LiteralPath $ConfigPath -PathType Leaf
    $raw = if ($configExists) { [System.IO.File]::ReadAllText($ConfigPath) } else { "" }
    $lines = @($raw -split "`r?`n")
    $sectionTexts = New-Object System.Collections.Generic.List[string]
    $addedNames = New-Object System.Collections.Generic.List[string]
    $skippedNames = New-Object System.Collections.Generic.List[string]

    foreach ($server in @($Servers)) {
        if (Test-TomlTableExists -Lines $lines -Header $server.Header) {
            $skippedNames.Add([string]$server.Name) | Out-Null
            continue
        }
        $section = @(New-CodexMcpServerSection -Header $server.Header -Command $server.Command)
        $sectionTexts.Add(($section -join "`r`n")) | Out-Null
        $addedNames.Add([string]$server.Name) | Out-Null
    }

    if ($sectionTexts.Count -gt 0 -and -not $DryRun) {
        $parent = Split-Path -Parent $ConfigPath
        if (-not [string]::IsNullOrWhiteSpace($parent) -and -not (Test-Path -LiteralPath $parent)) {
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
        }

        $body = ($sectionTexts.ToArray() -join "`r`n`r`n") + "`r`n"
        $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
        if (-not $configExists -or $raw.Length -eq 0) {
            [System.IO.File]::WriteAllText($ConfigPath, $body, $utf8NoBom)
        } else {
            if ($raw -match '(?:\r\n|\n|\r){2}$') {
                $prefix = ""
            } elseif ($raw.EndsWith("`n") -or $raw.EndsWith("`r")) {
                $prefix = "`r`n"
            } else {
                $prefix = "`r`n`r`n"
            }
            [System.IO.File]::AppendAllText($ConfigPath, $prefix + $body, $utf8NoBom)
        }
    }

    return [pscustomobject]@{
        Added = @($addedNames.ToArray())
        Skipped = @($skippedNames.ToArray())
        Changed = ($sectionTexts.Count -gt 0)
    }
}

function Update-CodexMcpConfig {
    if (-not (Test-InstallComponentSelected -Key "codex-config")) {
        Add-InstallReport -Key "codex-config" -Status "skipped" -Message "Not selected or not available."
        return
    }

    $aePath = Resolve-McpBinaryPath -ProvidedPath $AeMcpPath -FileName "ae-mcp.exe"
    $prPath = Resolve-McpBinaryPath -ProvidedPath $PrMcpPath -FileName "pr-mcp.exe"
    $psPath = Resolve-McpBinaryPath -ProvidedPath $PsMcpPath -FileName "ps-mcp.exe"
    $aiPath = Resolve-McpBinaryPath -ProvidedPath $AiMcpPath -FileName "ai-mcp.exe"
    $idPath = Resolve-McpBinaryPath -ProvidedPath $IdMcpPath -FileName "id-mcp.exe"
    if (-not $aePath -and -not $prPath -and -not $psPath -and -not $aiPath -and -not $idPath) {
        Write-Warning "MCP binaries were not found. Skipped Codex config update."
        Add-InstallReport -Key "codex-config" -Status "skipped" -Message "MCP binaries were not found."
        return
    }

    $configs = @(Get-CodexConfigPaths)
    if ($configs.Count -eq 0) {
        Write-Warning "Current-user Codex config path could not be resolved. Skipped Codex MCP server config update."
        Add-InstallReport -Key "codex-config" -Status "skipped" -Message "Current-user Codex config path could not be resolved."
        return
    }

    $servers = @()
    if ($aePath) {
        $servers += [pscustomobject]@{ Name = "aftereffects"; Header = "mcp_servers.aftereffects"; Command = $aePath }
    }
    if ($prPath) {
        $servers += [pscustomobject]@{ Name = "premiere"; Header = "mcp_servers.premiere"; Command = $prPath }
    }
    if ($psPath) {
        $servers += [pscustomobject]@{ Name = "photoshop"; Header = "mcp_servers.photoshop"; Command = $psPath }
    }
    if ($aiPath) {
        $servers += [pscustomobject]@{ Name = "illustrator"; Header = "mcp_servers.illustrator"; Command = $aiPath }
    }
    if ($idPath) {
        $servers += [pscustomobject]@{ Name = "indesign"; Header = "mcp_servers.indesign"; Command = $idPath }
    }

    $added = 0
    $skipped = 0
    $failed = 0
    foreach ($config in $configs) {
        try {
            $result = Add-MissingCodexMcpServers -ConfigPath $config -Servers $servers
            $added += $result.Added.Count
            $skipped += $result.Skipped.Count
            if ($result.Skipped.Count -gt 0) {
                Write-Host ("Existing Codex MCP table(s) left unchanged: {0}" -f ($result.Skipped -join ", "))
            }
            if ($result.Changed) {
                Write-Host ("Codex MCP server config added missing table(s) {0}: {1}" -f ($result.Added -join ", "), $config)
            } else {
                Write-Host "Codex MCP server config already contains every available server table: $config"
            }
        } catch {
            Write-Warning "Failed to update Codex config '$config': $($_.Exception.Message)"
            $failed++
        }
    }
    if ($failed -gt 0 -and $added -eq 0) {
        Add-InstallReport -Key "codex-config" -Status "failed" -Message "Failed to update $failed current-user Codex config file(s)."
    } elseif ($added -gt 0) {
        Add-InstallReport -Key "codex-config" -Status "installed" -Message "Added $added missing MCP table(s); left $skipped existing table(s) unchanged."
    } else {
        Add-InstallReport -Key "codex-config" -Status "skipped" -Message "All $skipped available MCP table(s) already existed and were left unchanged."
    }
}

function Get-McpAutostartEntries {
    return @(
        [pscustomobject]@{ Key = "aftereffects"; EntryName = "AfterEffectsMcp"; BinaryPath = (Resolve-McpBinaryPath -ProvidedPath $AeMcpPath -FileName "ae-mcp.exe") },
        [pscustomobject]@{ Key = "premiere"; EntryName = "PremiereMcp"; BinaryPath = (Resolve-McpBinaryPath -ProvidedPath $PrMcpPath -FileName "pr-mcp.exe") },
        [pscustomobject]@{ Key = "photoshop"; EntryName = "PhotoshopMcp"; BinaryPath = (Resolve-McpBinaryPath -ProvidedPath $PsMcpPath -FileName "ps-mcp.exe") },
        [pscustomobject]@{ Key = "illustrator"; EntryName = "IllustratorMcp"; BinaryPath = (Resolve-McpBinaryPath -ProvidedPath $AiMcpPath -FileName "ai-mcp.exe") },
        [pscustomobject]@{ Key = "indesign"; EntryName = "InDesignMcp"; BinaryPath = (Resolve-McpBinaryPath -ProvidedPath $IdMcpPath -FileName "id-mcp.exe") }
    )
}

function Get-AutostartRegistryValue {
    param([string]$EntryName)

    $runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
    try {
        return Get-ItemPropertyValue -LiteralPath $runKey -Name $EntryName -ErrorAction Stop
    } catch {
        return $null
    }
}

function Repair-McpAutostartRegistrations {
    foreach ($entry in Get-McpAutostartEntries) {
        $registered = Get-AutostartRegistryValue -EntryName $entry.EntryName
        if ($null -eq $registered) {
            Add-InstallReport -Key ("autostart-{0}" -f $entry.Key) -Status "skipped" -Message "Current user has not opted in to autostart."
            continue
        }
        if ([string]::IsNullOrWhiteSpace($entry.BinaryPath) -or -not (Test-Path -LiteralPath $entry.BinaryPath)) {
            Add-InstallReport -Key ("autostart-{0}" -f $entry.Key) -Status "failed" -Message "Registered autostart could not be repaired because the MCP binary was not found."
            continue
        }

        try {
            $output = & $entry.BinaryPath autostart --entry-name $entry.EntryName install 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "autostart install exited with code $LASTEXITCODE`: $output"
            }
            Write-InstallerLog -Message ("Repaired current-user autostart '{0}': {1}" -f $entry.EntryName, $output)
            Add-InstallReport -Key ("autostart-{0}" -f $entry.Key) -Status "installed" -Message "Existing current-user registration updated for the installed executable."
        } catch {
            Write-InstallerLog -Message ("Failed to repair autostart '{0}': {1}" -f $entry.EntryName, $_.Exception.Message)
            Add-InstallReport -Key ("autostart-{0}" -f $entry.Key) -Status "failed" -Message $_.Exception.Message
        }
    }
}

function Remove-McpAutostartRegistrations {
    $runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
    foreach ($entry in Get-McpAutostartEntries) {
        if ($null -eq (Get-AutostartRegistryValue -EntryName $entry.EntryName)) {
            continue
        }

        if (-not [string]::IsNullOrWhiteSpace($entry.BinaryPath) -and (Test-Path -LiteralPath $entry.BinaryPath)) {
            try {
                $stopOutput = & $entry.BinaryPath autostart --entry-name $entry.EntryName stop 2>&1
                Write-InstallerLog -Message ("Stopped daemon for '{0}' during uninstall: {1}" -f $entry.EntryName, $stopOutput)
            } catch {
                Write-InstallerLog -Message ("Failed to stop daemon for '{0}' during uninstall: {1}" -f $entry.EntryName, $_.Exception.Message)
            }

            try {
                $removeOutput = & $entry.BinaryPath autostart --entry-name $entry.EntryName uninstall 2>&1
                if ($LASTEXITCODE -eq 0) {
                    Write-InstallerLog -Message ("Removed current-user autostart '{0}': {1}" -f $entry.EntryName, $removeOutput)
                    continue
                }
                Write-InstallerLog -Message ("autostart uninstall for '{0}' exited with code {1}: {2}" -f $entry.EntryName, $LASTEXITCODE, $removeOutput)
            } catch {
                Write-InstallerLog -Message ("Failed to invoke autostart uninstall for '{0}': {1}" -f $entry.EntryName, $_.Exception.Message)
            }
        }

        try {
            Remove-ItemProperty -LiteralPath $runKey -Name $entry.EntryName -ErrorAction Stop
            Write-InstallerLog -Message ("Removed current-user autostart '{0}' through registry fallback." -f $entry.EntryName)
        } catch {
            Write-InstallerLog -Message ("Failed to remove current-user autostart '{0}': {1}" -f $entry.EntryName, $_.Exception.Message)
        }
    }
}

if ($RemoveAutostart) {
    Remove-McpAutostartRegistrations
    exit 0
}

if ($LaunchInteractiveInstall) {
    try {
        Start-InteractiveInstallerProcess
    } catch {
        Write-InstallerLog -Message ("Failed to launch interactive installer: {0}" -f $_.Exception.Message)
        throw
    }
    exit 0
}

if ($InteractiveInstall) {
    Write-InstallerLog -Message "Interactive installer started."
    Remove-LaunchScheduledTask -TaskName $CleanupLaunchTaskName
}

if ($PlanInstall -or $InteractiveInstall) {
    Write-InstallerLog -Message "Showing install plan dialog."
    try {
        Show-InstallPlanDialog | Out-Null
    } catch {
        Write-InstallerLog -Message ("Show-InstallPlanDialog failed: {0}" -f $_.Exception.Message)
        Write-InstallerLog -Message ("Show-InstallPlanDialog stack: {0}" -f $_.ScriptStackTrace)
        throw
    }
    Write-InstallerLog -Message "Install plan dialog completed."
    if ($PlanInstall -and -not $InteractiveInstall) {
        exit 0
    }
}

if ($InteractiveInstall -and -not (Test-IsAdministrator)) {
    Write-InstallerLog -Message "Interactive installer is running without administrator rights; delegating install work to elevated worker."
    Start-ElevatedInstallWorkerAndWait
    Write-InstallerLog -Message "Showing install summary dialog after elevated worker."
    Show-InstallSummaryDialog
    Write-InstallerLog -Message "Install summary dialog completed after elevated worker."
    exit 0
}

if (-not $SkipHostBridgeInstall) {
    $targets = Get-AeInstallPaths

    if (-not (Test-InstallComponentSelected -Key "aftereffects-panel")) {
        Write-Host "After Effects headless bridge deployment skipped by install selection."
        Add-InstallReport -Key "aftereffects-panel" -Status "skipped" -Message "Not selected or not available."
    } elseif ($targets.Count -eq 0) {
        Write-Host "No After Effects installation was detected under C:\Program Files\Adobe. Skipped AE bridge deployment."
        Add-InstallReport -Key "aftereffects-panel" -Status "skipped" -Message "No After Effects installation was detected."
    } else {
        $source = $null
        $startupSource = $null
        $shutdownSource = $null
        try {
            $source = Resolve-BridgeScriptPath -InputPath $BridgeScriptPath
            $startupSource = Resolve-BridgeStartupScriptPath -InputPath $BridgeStartupScriptPath
            $shutdownSource = Resolve-BridgeShutdownScriptPath -InputPath $BridgeShutdownScriptPath
        } catch {
            Write-Warning "After Effects bridge source was not found. Skipped AE bridge deployment."
            Add-InstallReport -Key "aftereffects-panel" -Status "skipped" -Message "After Effects runtime or startup source was not found."
        }

        $installed = 0
        if ($source -and $startupSource -and $shutdownSource) {
            foreach ($aePath in $targets) {
                $destDir = Join-Path $aePath "Support Files\Scripts\ScriptUI Panels"
                $destFile = Join-Path $destDir "mcp-bridge-auto.jsx"
                $startupDir = Join-Path $aePath "Support Files\Scripts\Startup"
                $startupFile = Join-Path $startupDir "mcp-bridge-startup.jsx"
                $shutdownDir = Join-Path $aePath "Support Files\Scripts\Shutdown"
                $shutdownFile = Join-Path $shutdownDir "mcp-bridge-shutdown.jsx"

                try {
                    if (-not (Test-Path -LiteralPath $destDir)) {
                        New-Item -ItemType Directory -Path $destDir -Force | Out-Null
                    }
                    if (-not (Test-Path -LiteralPath $startupDir)) {
                        New-Item -ItemType Directory -Path $startupDir -Force | Out-Null
                    }
                    if (-not (Test-Path -LiteralPath $shutdownDir)) {
                        New-Item -ItemType Directory -Path $shutdownDir -Force | Out-Null
                    }
                    Copy-Item -LiteralPath $source -Destination $destFile -Force
                    Copy-Item -LiteralPath $startupSource -Destination $startupFile -Force
                    Copy-Item -LiteralPath $shutdownSource -Destination $shutdownFile -Force
                    Write-Host "Installed: $destFile"
                    Write-Host "Installed headless startup bootstrap: $startupFile"
                    Write-Host "Installed shutdown cleanup: $shutdownFile"
                    $installed++
                } catch {
                    Write-Warning "Failed to install bridge runtime/startup bootstrap to '$aePath': $($_.Exception.Message)"
                }
            }

            Write-Host "Bridge deployment completed. Installed runtime and headless startup bootstrap to $installed location(s)."
            Add-InstallReport -Key "aftereffects-panel" -Status "installed" -Message "Installed runtime and headless startup bootstrap to $installed After Effects location(s)."
        }
    }

    $premiereTargets = Get-PremiereInstallPaths
    $uxpPremiereTargets = Get-UxpCapablePremierePaths -PremierePaths $premiereTargets
    $premiereSource = Resolve-PremiereExtensionSource
    if (-not (Test-InstallComponentSelected -Key "premiere-cep")) {
        Write-Host "Premiere CEP bridge deployment skipped by install selection."
        Add-InstallReport -Key "premiere-cep" -Status "skipped" -Message "Not selected or not available."
    } elseif (-not $premiereSource) {
        Write-Host "Premiere CEP extension not found. Skipped Premiere CEP deployment."
        Add-InstallReport -Key "premiere-cep" -Status "skipped" -Message "Premiere CEP extension source not found."
    } elseif ($premiereTargets.Count -eq 0) {
        Write-Host "No Adobe Premiere Pro installation was detected. Skipped Premiere CEP deployment."
        Add-InstallReport -Key "premiere-cep" -Status "skipped" -Message "No Adobe Premiere Pro installation was detected."
    } elseif ($uxpPremiereTargets.Count -gt 0) {
        Write-Host "UXP-capable Premiere Pro installation detected. Skipped CEP fallback deployment."
        Add-InstallReport -Key "premiere-cep" -Status "skipped" -Message "UXP-capable Premiere Pro installation detected."
    } else {
        $cepRoot = "C:\Program Files (x86)\Common Files\Adobe\CEP\extensions"
        $premiereDest = Join-Path $cepRoot "mcp-bridge-premiere"

        try {
            if (-not (Test-Path -LiteralPath $cepRoot)) {
                New-Item -ItemType Directory -Path $cepRoot -Force | Out-Null
            }
            if (Test-Path -LiteralPath $premiereDest) {
                Remove-Item -LiteralPath $premiereDest -Recurse -Force
            }
            Copy-Item -LiteralPath $premiereSource -Destination $premiereDest -Recurse -Force
            Write-Host "Premiere CEP bridge installed: $premiereDest"
            Add-InstallReport -Key "premiere-cep" -Status "installed" -Message "Installed to $premiereDest"
        } catch {
            Write-Warning "Failed to install Premiere CEP bridge: $($_.Exception.Message)"
            Add-InstallReport -Key "premiere-cep" -Status "failed" -Message $_.Exception.Message
        }
    }

    Install-IllustratorCepBridge
}

if (-not $SkipUserInstall) {
    if ((Test-InstallComponentSelected -Key "premiere-cep") -or (Test-InstallComponentSelected -Key "illustrator-cep")) {
        Enable-CepDebugMode
    }
    Install-PremiereUxpBridge
    Install-PhotoshopUxpBridge
    Install-InDesignStartupBridge
    Update-CodexMcpConfig
    Repair-McpAutostartRegistrations
}

if ($FinalizeInstall -or $InteractiveInstall) {
    Write-InstallerLog -Message "Showing install summary dialog."
    Show-InstallSummaryDialog
    Write-InstallerLog -Message "Install summary dialog completed."
}
