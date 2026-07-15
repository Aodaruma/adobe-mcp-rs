param(
    [string]$AfterEffectsPath,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Normalize-PathText {
    param([string]$PathText)

    if ([string]::IsNullOrWhiteSpace($PathText)) {
        return $null
    }

    return $PathText.Trim().Trim('"').Trim("'")
}

function Resolve-PreferredPathInput {
    param(
        [string]$ProvidedPath,
        [object[]]$RemainingArgs
    )

    $candidate = Normalize-PathText -PathText $ProvidedPath
    if (-not $candidate) {
        return $null
    }

    if (Test-Path -LiteralPath $candidate) {
        return $candidate
    }

    if ($RemainingArgs -and $RemainingArgs.Count -gt 0) {
        $parts = @($candidate)

        foreach ($token in $RemainingArgs) {
            $segment = [string]$token
            if ($segment.StartsWith("-")) {
                break
            }
            $parts += $segment
        }

        if ($parts.Count -gt 1) {
            $joined = Normalize-PathText -PathText ($parts -join " ")
            if ($joined) {
                if (Test-Path -LiteralPath $joined) {
                    return $joined
                }

                # gsudo/cmd argument parsing can leave a trailing "\" artifact.
                if ($joined.Length -gt 3 -and $joined.EndsWith("\")) {
                    $trimmed = $joined.Substring(0, $joined.Length - 1)
                    if (Test-Path -LiteralPath $trimmed) {
                        return $trimmed
                    }
                }

                return $joined
            }
        }
    }

    return $candidate
}

function Get-DetectedAfterEffectsPaths {
    $detected = @()
    $possiblePaths = @(
        "C:\Program Files\Adobe\Adobe After Effects 2030",
        "C:\Program Files\Adobe\Adobe After Effects 2029",
        "C:\Program Files\Adobe\Adobe After Effects 2028",
        "C:\Program Files\Adobe\Adobe After Effects 2027",
        "C:\Program Files\Adobe\Adobe After Effects 2026",
        "C:\Program Files\Adobe\Adobe After Effects 2025",
        "C:\Program Files\Adobe\Adobe After Effects 2024",
        "C:\Program Files\Adobe\Adobe After Effects 2023",
        "C:\Program Files\Adobe\Adobe After Effects 2022",
        "C:\Program Files\Adobe\Adobe After Effects 2021"
    )

    foreach ($path in $possiblePaths) {
        if (Test-Path -LiteralPath $path) {
            $detected += $path
        }
    }

    $adobeRoot = "C:\Program Files\Adobe"
    if (Test-Path -LiteralPath $adobeRoot) {
        $dynamicPaths = Get-ChildItem -LiteralPath $adobeRoot -Directory |
            Where-Object { $_.Name -match '^Adobe After Effects (\d{4})$' } |
            Sort-Object { [int]($_.Name -replace '^Adobe After Effects ', '') } -Descending |
            ForEach-Object { $_.FullName }

        foreach ($path in $dynamicPaths) {
            if ($detected -notcontains $path) {
                $detected += $path
            }
        }
    }

    return $detected
}

function Get-DetectedPremierePaths {
    $detected = @()
    $possiblePaths = @(
        "C:\Program Files\Adobe\Adobe Premiere Pro 2030",
        "C:\Program Files\Adobe\Adobe Premiere Pro 2029",
        "C:\Program Files\Adobe\Adobe Premiere Pro 2028",
        "C:\Program Files\Adobe\Adobe Premiere Pro 2027",
        "C:\Program Files\Adobe\Adobe Premiere Pro 2026",
        "C:\Program Files\Adobe\Adobe Premiere Pro 2025",
        "C:\Program Files\Adobe\Adobe Premiere Pro 2024"
    )

    foreach ($path in $possiblePaths) {
        if (Test-Path -LiteralPath $path) {
            $detected += $path
        }
    }

    $adobeRoot = "C:\Program Files\Adobe"
    if (Test-Path -LiteralPath $adobeRoot) {
        $dynamicPaths = Get-ChildItem -LiteralPath $adobeRoot -Directory |
            Where-Object { $_.Name -match '^Adobe Premiere Pro (\d{4})$' } |
            Sort-Object { [int]($_.Name -replace '^Adobe Premiere Pro ', '') } -Descending |
            ForEach-Object { $_.FullName }

        foreach ($path in $dynamicPaths) {
            if ($detected -notcontains $path) {
                $detected += $path
            }
        }
    }

    return $detected
}

function Get-PremiereProductVersion {
    param([string]$PremierePath)

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

    $version = Get-PremiereProductVersion -PremierePath $PremierePath
    return ($version -ne $null -and $version -ge [version]"25.6.0")
}

function Get-UxpCapablePremierePaths {
    param([string[]]$PremierePaths)

    return @($PremierePaths | Where-Object { Test-PremiereSupportsUxp -PremierePath $_ })
}

function Get-CepExtensionsRoot {
    if (Test-IsAdministrator) {
        return "C:\Program Files (x86)\Common Files\Adobe\CEP\extensions"
    }
    $appData = [Environment]::GetFolderPath("ApplicationData")
    return (Join-Path $appData "Adobe\CEP\extensions")
}

function Find-UpiaCommand {
    $candidates = @(
        "C:\Program Files\Common Files\Adobe\Adobe Desktop Common\RemoteComponents\UPI\UnifiedPluginInstallerAgent\UnifiedPluginInstallerAgent.exe",
        "C:\Program Files (x86)\Common Files\Adobe\Adobe Desktop Common\RemoteComponents\UPI\UnifiedPluginInstallerAgent\UnifiedPluginInstallerAgent.exe"
    )

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    return $null
}

function New-CcxPackage {
    param([string]$SourceDir)

    $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("premiere-mcp-uxp-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
    $zipPath = Join-Path $tempRoot "Premiere-MCP-Bridge.zip"
    $ccxPath = Join-Path $tempRoot "Premiere-MCP-Bridge.ccx"
    Compress-Archive -Path (Join-Path $SourceDir "*") -DestinationPath $zipPath -Force
    Move-Item -LiteralPath $zipPath -Destination $ccxPath -Force
    return $ccxPath
}

function Install-PremiereUxpBridge {
    param(
        [string]$SourceDir,
        [string[]]$PremierePaths,
        [switch]$DryRun
    )

    $uxpTargets = Get-UxpCapablePremierePaths -PremierePaths $PremierePaths
    if ($uxpTargets.Count -eq 0) {
        Write-Host "No UXP-capable Premiere Pro installation was detected. Skipped Premiere UXP deployment."
        return
    }

    if (-not (Test-Path -LiteralPath (Join-Path $SourceDir "manifest.json"))) {
        Write-Host "Premiere UXP bridge source not found. Skipped UXP deployment."
        return
    }

    $upia = Find-UpiaCommand
    if (-not $upia) {
        Write-Warning "UnifiedPluginInstallerAgent.exe was not found. Skipped Premiere UXP deployment."
        Write-Host "Premiere UXP bridge bundled. Load with Adobe UXP Developer Tool: $(Join-Path $SourceDir "manifest.json")"
        return
    }

    if ($DryRun) {
        Write-Host "DryRun mode: Premiere UXP bridge would be installed with UPIA: $upia"
        return
    }

    $ccx = New-CcxPackage -SourceDir $SourceDir
    try {
        $output = & $upia /install $ccx 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "Premiere UXP install failed: $output"
            return
        }
        Write-Host "Premiere UXP bridge installed via Unified Plugin Installer Agent."
        Write-Host ("UXP-capable Premiere Pro target(s): {0}" -f ($uxpTargets -join ", "))
    } finally {
        $tempRoot = Split-Path -Parent $ccx
        if (Test-Path -LiteralPath $tempRoot) {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Resolve-McpBinaryPath {
    param(
        [string]$RepoRoot,
        [string]$WindowsFileName,
        [string]$UnixFileName
    )

    $repoBinary = Join-Path $RepoRoot (Join-Path "target\release" $WindowsFileName)
    if (Test-Path -LiteralPath $repoBinary) {
        return (Resolve-Path -LiteralPath $repoBinary).Path
    }

    $installed = Join-Path "C:\Program Files\AfterEffectsMcp" $WindowsFileName
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
    param(
        [string]$RepoRoot,
        [switch]$DryRun
    )

    $aePath = Resolve-McpBinaryPath -RepoRoot $RepoRoot -WindowsFileName "ae-mcp.exe" -UnixFileName "ae-mcp"
    $prPath = Resolve-McpBinaryPath -RepoRoot $RepoRoot -WindowsFileName "pr-mcp.exe" -UnixFileName "pr-mcp"
    $idPath = Resolve-McpBinaryPath -RepoRoot $RepoRoot -WindowsFileName "id-mcp.exe" -UnixFileName "id-mcp"
    if (-not $aePath -and -not $prPath -and -not $idPath) {
        Write-Warning "MCP binaries were not found. Skipped Codex config update."
        return
    }

    $configs = Get-CodexConfigPaths
    if ($configs.Count -eq 0) {
        Write-Warning "Current-user Codex config path could not be resolved. Skipped Codex MCP server config update."
        return
    }

    $servers = @()
    if ($aePath) {
        $servers += [pscustomobject]@{ Name = "aftereffects"; Header = "mcp_servers.aftereffects"; Command = $aePath }
    }
    if ($prPath) {
        $servers += [pscustomobject]@{ Name = "premiere"; Header = "mcp_servers.premiere"; Command = $prPath }
    }
    if ($idPath) {
        $servers += [pscustomobject]@{ Name = "indesign"; Header = "mcp_servers.indesign"; Command = $idPath }
    }

    foreach ($config in $configs) {
        try {
            $result = Add-MissingCodexMcpServers -ConfigPath $config -Servers $servers -DryRun:$DryRun
            if ($result.Skipped.Count -gt 0) {
                Write-Host ("Existing Codex MCP table(s) left unchanged: {0}" -f ($result.Skipped -join ", "))
            }
            if (-not $result.Changed) {
                Write-Host "Codex MCP server config already contains every available server table: $config"
            } elseif ($DryRun) {
                Write-Host ("DryRun mode: would add Codex MCP table(s) {0}: {1}" -f ($result.Added -join ", "), $config)
            } else {
                Write-Host ("Codex MCP server config added missing table(s) {0}: {1}" -f ($result.Added -join ", "), $config)
            }
        } catch {
            Write-Warning "Failed to update Codex config '$config': $($_.Exception.Message)"
        }
    }
}

function Resolve-InstallTargets {
    param([string]$PreferredPath)

    if ($PreferredPath) {
        if (Test-Path -LiteralPath $PreferredPath) {
            return @((Resolve-Path -LiteralPath $PreferredPath).Path)
        }
        throw "Specified After Effects path not found: $PreferredPath"
    }

    $detected = Get-DetectedAfterEffectsPaths
    if (-not $detected -or $detected.Count -eq 0) {
        throw "After Effects install path was not detected. Pass -AfterEffectsPath explicitly."
    }

    return $detected
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$sourceScript = Join-Path $repoRoot "src\scripts\mcp-bridge-auto.jsx"
$sourceStartupScript = Join-Path $repoRoot "src\scripts\mcp-bridge-startup.jsx"
$sourceShutdownScript = Join-Path $repoRoot "src\scripts\mcp-bridge-shutdown.jsx"

if (!(Test-Path $sourceScript)) {
    throw "Bridge script not found: $sourceScript"
}
if (!(Test-Path $sourceStartupScript)) {
    throw "Bridge startup script not found: $sourceStartupScript"
}
if (!(Test-Path $sourceShutdownScript)) {
    throw "Bridge shutdown script not found: $sourceShutdownScript"
}

$resolvedPreferredPath = Resolve-PreferredPathInput -ProvidedPath $AfterEffectsPath -RemainingArgs $args
$installTargets = Resolve-InstallTargets -PreferredPath $resolvedPreferredPath

Write-Host "Source      : $sourceScript"
Write-Host "Destinations:"
foreach ($aePath in $installTargets) {
    $destinationScript = Join-Path (Join-Path $aePath "Support Files\Scripts\ScriptUI Panels") "mcp-bridge-auto.jsx"
    $startupScript = Join-Path (Join-Path $aePath "Support Files\Scripts\Startup") "mcp-bridge-startup.jsx"
    $shutdownScript = Join-Path (Join-Path $aePath "Support Files\Scripts\Shutdown") "mcp-bridge-shutdown.jsx"
    Write-Host "  - $destinationScript"
    Write-Host "  - $startupScript"
    Write-Host "  - $shutdownScript"
}

if ($DryRun) {
    Write-Host "DryRun mode: no file copy was executed."
} else {
    $installedDestinations = @()
    foreach ($aePath in $installTargets) {
        $destinationFolder = Join-Path $aePath "Support Files\Scripts\ScriptUI Panels"
        $destinationScript = Join-Path $destinationFolder "mcp-bridge-auto.jsx"
        $startupFolder = Join-Path $aePath "Support Files\Scripts\Startup"
        $startupScript = Join-Path $startupFolder "mcp-bridge-startup.jsx"
        $shutdownFolder = Join-Path $aePath "Support Files\Scripts\Shutdown"
        $shutdownScript = Join-Path $shutdownFolder "mcp-bridge-shutdown.jsx"

        try {
            if (!(Test-Path $destinationFolder)) {
                New-Item -ItemType Directory -Path $destinationFolder -Force | Out-Null
            }
            if (!(Test-Path $startupFolder)) {
                New-Item -ItemType Directory -Path $startupFolder -Force | Out-Null
            }
            if (!(Test-Path $shutdownFolder)) {
                New-Item -ItemType Directory -Path $shutdownFolder -Force | Out-Null
            }
            Copy-Item -Path $sourceScript -Destination $destinationScript -Force
            Copy-Item -Path $sourceStartupScript -Destination $startupScript -Force
            Copy-Item -Path $sourceShutdownScript -Destination $shutdownScript -Force
            $installedDestinations += $destinationScript
            $installedDestinations += $startupScript
            $installedDestinations += $shutdownScript
        } catch {
            if (-not (Test-IsAdministrator)) {
                Write-Error @"
Copy failed. Administrator privileges are required.
Re-run in elevated PowerShell:
  powershell -ExecutionPolicy Bypass -File .\scripts\install-bridge.ps1
Target: $destinationScript
Original error: $($_.Exception.Message)
"@
                exit 1
            }
            throw
        }
    }

    Write-Host ""
    Write-Host ("Bridge files installed to {0} path(s)." -f $installedDestinations.Count)
    foreach ($destination in $installedDestinations) {
        Write-Host "  - $destination"
    }
}
Write-Host "Next steps:"
Write-Host "1. Open After Effects"
Write-Host "2. Edit > Preferences > Scripting & Expressions"
Write-Host "3. Enable Allow Scripts to Write Files and Access Network"
Write-Host "4. Restart After Effects"
Write-Host "5. The MCP bridge starts headlessly; no panel or Auto-run checkbox is required"

$premiereUxpSource = Join-Path $repoRoot "src\premiere\uxp\mcp-bridge-premiere"
$premiereExtensionSource = Join-Path $repoRoot "src\premiere\cep\mcp-bridge-premiere"
$premiereTargets = Get-DetectedPremierePaths
$uxpPremiereTargets = Get-UxpCapablePremierePaths -PremierePaths $premiereTargets
if ($premiereTargets.Count -eq 0) {
    Write-Host ""
    Write-Host "No Adobe Premiere Pro installation detected. Skipped Premiere bridge deployment."
} else {
    Write-Host ""
    if ($uxpPremiereTargets.Count -gt 0) {
        Write-Host "UXP-capable Premiere Pro detected. Installing Premiere UXP bridge and skipping CEP fallback."
        Install-PremiereUxpBridge -SourceDir $premiereUxpSource -PremierePaths $premiereTargets -DryRun:$DryRun
        Write-Host "Next steps (Premiere Pro):"
        Write-Host "1. Open Adobe Premiere Pro"
        Write-Host "2. Window > UXP Plugins > Premiere MCP Bridge"
        Write-Host "3. Enable Auto-run commands"
    } elseif (-not (Test-Path -LiteralPath $premiereExtensionSource)) {
        Write-Host "Premiere CEP extension source not found. Skipped Premiere CEP fallback deployment."
    } else {
        $cepRoot = Get-CepExtensionsRoot
        $premiereDest = Join-Path $cepRoot "mcp-bridge-premiere"
        Write-Host "No UXP-capable Premiere Pro detected. Installing CEP fallback."
        Write-Host "Premiere CEP destination: $premiereDest"
        if ($DryRun) {
            Write-Host "DryRun mode: Premiere CEP bridge would be installed."
        } else {
            try {
                if (!(Test-Path -LiteralPath $cepRoot)) {
                    New-Item -ItemType Directory -Path $cepRoot -Force | Out-Null
                }
                if (Test-Path -LiteralPath $premiereDest) {
                    Remove-Item -LiteralPath $premiereDest -Recurse -Force
                }
                Copy-Item -Path $premiereExtensionSource -Destination $premiereDest -Recurse -Force
                Write-Host "Premiere CEP bridge installed."
            } catch {
                Write-Warning "Failed to install Premiere CEP bridge: $($_.Exception.Message)"
            }

            Write-Host "Next steps (Premiere Pro):"
            Write-Host "1. Open Adobe Premiere Pro"
            Write-Host "2. Window > Extensions > Premiere MCP Bridge"
            Write-Host "3. Enable Auto-run commands"
        }
    }
}

$indesignSource = Join-Path $repoRoot "src\indesign\uxp\mcp-bridge-indesign.idjs"
$indesignPreferenceRoot = Join-Path $env:APPDATA "Adobe\InDesign"
$indesignStartupTargets = @()
if (Test-Path -LiteralPath $indesignPreferenceRoot) {
    $versionFolders = @(Get-ChildItem -LiteralPath $indesignPreferenceRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^Version\s+' })
    foreach ($versionFolder in $versionFolders) {
        $localeFolders = @(Get-ChildItem -LiteralPath $versionFolder.FullName -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^[a-z]{2}_[A-Z]{2}$' })
        foreach ($localeFolder in $localeFolders) {
            $indesignStartupTargets += Join-Path $localeFolder.FullName "Scripts\Startup Scripts"
        }
    }
}

Write-Host ""
if (-not (Test-Path -LiteralPath $indesignSource)) {
    Write-Host "InDesign UXP startup bridge source not found. Skipped InDesign deployment."
} elseif ($indesignStartupTargets.Count -eq 0) {
    Write-Host "No existing InDesign preference profile was detected. Skipped InDesign deployment."
    Write-Host "See docs/setup-codex-mcp.md for the manual Startup Scripts path."
} else {
    foreach ($target in $indesignStartupTargets) {
        $destination = Join-Path $target "mcp-bridge-indesign.idjs"
        if ($DryRun) {
            Write-Host "DryRun mode: InDesign bridge would be installed to $destination"
        } else {
            New-Item -ItemType Directory -Path $target -Force | Out-Null
            Copy-Item -LiteralPath $indesignSource -Destination $destination -Force
            Write-Host "InDesign startup bridge installed: $destination"
        }
    }
    Write-Host "Restart InDesign; no panel or Auto-run toggle is required."
}

Update-CodexMcpConfig -RepoRoot $repoRoot -DryRun:$DryRun
