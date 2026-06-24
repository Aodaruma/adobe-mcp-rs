param(
    [string]$BridgeScriptPath,
    [string]$AeMcpPath,
    [string]$PrMcpPath,
    [string]$PsMcpPath,
    [string]$AiMcpPath,
    [switch]$SkipHostBridgeInstall,
    [switch]$SkipUserInstall
)

$ErrorActionPreference = "Stop"

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

function Resolve-IllustratorCepSource {
    $candidate = Join-Path $PSScriptRoot "illustrator-cep\mcp-bridge-illustrator"
    if (Test-Path -LiteralPath $candidate) {
        return (Resolve-Path -LiteralPath $candidate).Path
    }
    return $null
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

    $programFiles = @($env:ProgramFiles, ${env:ProgramFiles(x86)}) | Where-Object { $_ }
    foreach ($root in $programFiles) {
        $found = Get-ChildItem -LiteralPath $root -Recurse -Filter "UnifiedPluginInstallerAgent.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($found) {
            return $found.FullName
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
    $premiereTargets = Get-PremiereInstallPaths
    $uxpTargets = Get-UxpCapablePremierePaths -PremierePaths $premiereTargets
    if ($uxpTargets.Count -eq 0) {
        Write-Host "No UXP-capable Premiere Pro installation was detected. Skipped Premiere UXP deployment."
        return
    }

    $source = Resolve-PremiereUxpSource
    if (-not $source) {
        Write-Host "Premiere UXP bridge source not found. Skipped UXP deployment."
        return
    }

    $upia = Find-UpiaCommand
    if (-not $upia) {
        Write-Warning "UnifiedPluginInstallerAgent.exe was not found. Skipped Premiere UXP deployment."
        Write-Host "Premiere UXP bridge bundled. Load with Adobe UXP Developer Tool: $(Join-Path $source "manifest.json")"
        return
    }

    $ccx = New-CcxPackage -SourceDir $source
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

function Install-IllustratorCepBridge {
    $illustratorTargets = Get-IllustratorInstallPaths
    $source = Resolve-IllustratorCepSource
    if (-not $source) {
        Write-Host "Illustrator CEP extension not found. Skipped Illustrator CEP deployment."
        return
    }
    if ($illustratorTargets.Count -eq 0) {
        Write-Host "No Adobe Illustrator installation was detected. Skipped Illustrator CEP deployment."
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
    } catch {
        Write-Warning "Failed to install Illustrator CEP bridge: $($_.Exception.Message)"
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
    $paths = @()

    if ($env:CODEX_HOME) {
        $paths += (Join-Path $env:CODEX_HOME "config.toml")
    }
    if ($env:USERPROFILE) {
        $paths += (Join-Path $env:USERPROFILE ".codex\config.toml")
    }

    $usersRoot = "C:\Users"
    if (Test-Path -LiteralPath $usersRoot) {
        $paths += @(Get-ChildItem -LiteralPath $usersRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -notin @("All Users", "Default", "Default User", "Public") } |
            ForEach-Object { Join-Path $_.FullName ".codex\config.toml" })
    }

    return @($paths | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Sort-Object -Unique)
}

function Format-TomlLiteral {
    param([string]$Value)
    return "'" + $Value.Replace("'", "''") + "'"
}

function Remove-TrailingEmptyLines {
    param([string[]]$Lines)

    $result = @($Lines)
    while ($result.Count -gt 0 -and $result[-1] -eq "") {
        if ($result.Count -eq 1) {
            return @()
        }
        $result = @($result[0..($result.Count - 2)])
    }
    return $result
}

function Set-TomlScalar {
    param(
        [string[]]$Lines,
        [string]$Header,
        [string]$Key,
        [string]$ValueLine
    )

    $Lines = @($Lines)
    $headerLine = "[$Header]"
    $start = [Array]::IndexOf($Lines, $headerLine)
    if ($start -lt 0) {
        if ($Lines.Count -eq 0) {
            return @($headerLine, $ValueLine)
        }
        return @($Lines + @("", $headerLine, $ValueLine))
    }

    $end = $Lines.Count
    for ($i = $start + 1; $i -lt $Lines.Count; $i++) {
        if ($Lines[$i] -match '^\s*\[') {
            $end = $i
            break
        }
    }

    $keyPattern = '^\s*' + [regex]::Escape($Key) + '\s*='
    for ($i = $start + 1; $i -lt $end; $i++) {
        if ($Lines[$i] -match $keyPattern) {
            $Lines[$i] = $ValueLine
            return $Lines
        }
    }

    if ($end -eq $Lines.Count) {
        return @($Lines + $ValueLine)
    }
    return @($Lines[0..($end - 1)] + $ValueLine + $Lines[$end..($Lines.Count - 1)])
}

function Update-CodexMcpConfig {
    $aePath = Resolve-McpBinaryPath -ProvidedPath $AeMcpPath -FileName "ae-mcp.exe"
    $prPath = Resolve-McpBinaryPath -ProvidedPath $PrMcpPath -FileName "pr-mcp.exe"
    $psPath = Resolve-McpBinaryPath -ProvidedPath $PsMcpPath -FileName "ps-mcp.exe"
    $aiPath = Resolve-McpBinaryPath -ProvidedPath $AiMcpPath -FileName "ai-mcp.exe"
    if (-not $aePath -and -not $prPath -and -not $psPath -and -not $aiPath) {
        Write-Warning "MCP binaries were not found. Skipped Codex config update."
        return
    }

    $configs = Get-CodexConfigPaths
    if ($configs.Count -eq 0) {
        Write-Host "Codex config.toml was not found. Skipped Codex MCP server config update."
        return
    }

    foreach ($config in $configs) {
        try {
            $raw = Get-Content -Raw -LiteralPath $config
            $lines = Remove-TrailingEmptyLines -Lines @($raw -split "`r?`n", -1)

            if ($aePath) {
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.aftereffects" -Key "command" -ValueLine ("command = " + (Format-TomlLiteral $aePath))
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.aftereffects" -Key "args" -ValueLine 'args = ["serve-stdio"]'
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.aftereffects" -Key "startup_timeout_sec" -ValueLine "startup_timeout_sec = 180"
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.aftereffects" -Key "tool_timeout_sec" -ValueLine "tool_timeout_sec = 180"
            }

            if ($prPath) {
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.premiere" -Key "command" -ValueLine ("command = " + (Format-TomlLiteral $prPath))
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.premiere" -Key "args" -ValueLine 'args = ["serve-stdio"]'
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.premiere" -Key "startup_timeout_sec" -ValueLine "startup_timeout_sec = 180"
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.premiere" -Key "tool_timeout_sec" -ValueLine "tool_timeout_sec = 180"
            }

            if ($psPath) {
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.photoshop" -Key "command" -ValueLine ("command = " + (Format-TomlLiteral $psPath))
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.photoshop" -Key "args" -ValueLine 'args = ["serve-stdio"]'
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.photoshop" -Key "startup_timeout_sec" -ValueLine "startup_timeout_sec = 180"
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.photoshop" -Key "tool_timeout_sec" -ValueLine "tool_timeout_sec = 180"
            }

            if ($aiPath) {
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.illustrator" -Key "command" -ValueLine ("command = " + (Format-TomlLiteral $aiPath))
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.illustrator" -Key "args" -ValueLine 'args = ["serve-stdio"]'
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.illustrator" -Key "startup_timeout_sec" -ValueLine "startup_timeout_sec = 180"
                $lines = Set-TomlScalar -Lines $lines -Header "mcp_servers.illustrator" -Key "tool_timeout_sec" -ValueLine "tool_timeout_sec = 180"
            }

            Set-Content -LiteralPath $config -Value ($lines -join "`r`n") -Encoding UTF8
            Write-Host "Codex MCP server config updated: $config"
        } catch {
            Write-Warning "Failed to update Codex config '$config': $($_.Exception.Message)"
        }
    }
}

if (-not $SkipHostBridgeInstall) {
    $source = Resolve-BridgeScriptPath -InputPath $BridgeScriptPath
    $targets = Get-AeInstallPaths

    if ($targets.Count -eq 0) {
        Write-Host "No After Effects installation was detected under C:\Program Files\Adobe. Skipped AE bridge deployment."
    } else {
        $installed = 0
        foreach ($aePath in $targets) {
            $destDir = Join-Path $aePath "Support Files\Scripts\ScriptUI Panels"
            $destFile = Join-Path $destDir "mcp-bridge-auto.jsx"

            try {
                if (-not (Test-Path -LiteralPath $destDir)) {
                    New-Item -ItemType Directory -Path $destDir -Force | Out-Null
                }
                Copy-Item -LiteralPath $source -Destination $destFile -Force
                Write-Host "Installed: $destFile"
                $installed++
            } catch {
                Write-Warning "Failed to install bridge panel to '$destFile': $($_.Exception.Message)"
            }
        }

        Write-Host "Bridge deployment completed. Installed to $installed location(s)."
    }

    $premiereTargets = Get-PremiereInstallPaths
    $uxpPremiereTargets = Get-UxpCapablePremierePaths -PremierePaths $premiereTargets
    $premiereSource = Resolve-PremiereExtensionSource
    if (-not $premiereSource) {
        Write-Host "Premiere CEP extension not found. Skipped Premiere CEP deployment."
    } elseif ($premiereTargets.Count -eq 0) {
        Write-Host "No Adobe Premiere Pro installation was detected. Skipped Premiere CEP deployment."
    } elseif ($uxpPremiereTargets.Count -gt 0) {
        Write-Host "UXP-capable Premiere Pro installation detected. Skipped CEP fallback deployment."
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
        } catch {
            Write-Warning "Failed to install Premiere CEP bridge: $($_.Exception.Message)"
        }
    }

    Install-IllustratorCepBridge
}

if (-not $SkipUserInstall) {
    Install-PremiereUxpBridge
    Update-CodexMcpConfig
}
