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

function Get-CepExtensionsRoot {
    if (Test-IsAdministrator) {
        return "C:\Program Files (x86)\Common Files\Adobe\CEP\extensions"
    }
    $appData = [Environment]::GetFolderPath("ApplicationData")
    return (Join-Path $appData "Adobe\CEP\extensions")
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

if (!(Test-Path $sourceScript)) {
    throw "Bridge script not found: $sourceScript"
}

$resolvedPreferredPath = Resolve-PreferredPathInput -ProvidedPath $AfterEffectsPath -RemainingArgs $args
$installTargets = Resolve-InstallTargets -PreferredPath $resolvedPreferredPath

Write-Host "Source      : $sourceScript"
Write-Host "Destinations:"
foreach ($aePath in $installTargets) {
    $destinationScript = Join-Path (Join-Path $aePath "Support Files\Scripts\ScriptUI Panels") "mcp-bridge-auto.jsx"
    Write-Host "  - $destinationScript"
}

if ($DryRun) {
    Write-Host "DryRun mode: no file copy was executed."
    exit 0
}

$installedDestinations = @()
foreach ($aePath in $installTargets) {
    $destinationFolder = Join-Path $aePath "Support Files\Scripts\ScriptUI Panels"
    $destinationScript = Join-Path $destinationFolder "mcp-bridge-auto.jsx"

    try {
        if (!(Test-Path $destinationFolder)) {
            New-Item -ItemType Directory -Path $destinationFolder -Force | Out-Null
        }
        Copy-Item -Path $sourceScript -Destination $destinationScript -Force
        $installedDestinations += $destinationScript
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
Write-Host ("Bridge script installed to {0} location(s)." -f $installedDestinations.Count)
foreach ($destination in $installedDestinations) {
    Write-Host "  - $destination"
}
Write-Host "Next steps:"
Write-Host "1. Open After Effects"
Write-Host "2. Edit > Preferences > Scripting & Expressions"
Write-Host "3. Enable Allow Scripts to Write Files and Access Network"
Write-Host "4. Restart After Effects"
Write-Host "5. Open Window > mcp-bridge-auto.jsx"

$premiereExtensionSource = Join-Path $repoRoot "src\premiere\cep\mcp-bridge-premiere"
$premiereUxpManifest = Join-Path $repoRoot "src\premiere\uxp\mcp-bridge-premiere\manifest.json"
if (Test-Path -LiteralPath $premiereExtensionSource) {
    $premiereTargets = Get-DetectedPremierePaths
    if ($premiereTargets.Count -eq 0) {
        Write-Host ""
        Write-Host "No Adobe Premiere Pro installation detected. Skipped Premiere bridge deployment."
    } else {
        $cepRoot = Get-CepExtensionsRoot
        $premiereDest = Join-Path $cepRoot "mcp-bridge-premiere"
        Write-Host ""
        Write-Host "Premiere CEP destination: $premiereDest"
        if ($DryRun) {
            Write-Host "DryRun mode: Premiere bridge not installed."
            exit 0
        }

        try {
            if (!(Test-Path -LiteralPath $cepRoot)) {
                New-Item -ItemType Directory -Path $cepRoot -Force | Out-Null
            }
            if (Test-Path -LiteralPath $premiereDest) {
                Remove-Item -LiteralPath $premiereDest -Recurse -Force
            }
            Copy-Item -Path $premiereExtensionSource -Destination $premiereDest -Recurse -Force
            Write-Host "Premiere bridge installed."
            Write-Host "Next steps (Premiere Pro):"
            Write-Host "1. Open Adobe Premiere Pro"
            if (Test-Path -LiteralPath $premiereUxpManifest) {
                Write-Host "2. Load the UXP plugin with Adobe UXP Developer Tool:"
                Write-Host "   $premiereUxpManifest"
                Write-Host "3. Window > UXP Plugins > Premiere MCP Bridge"
                Write-Host "4. Enable Auto-run commands"
                Write-Host "Legacy CEP fallback is also installed:"
                Write-Host "   Window > Extensions > Premiere MCP Bridge"
            } else {
                Write-Host "2. Window > Extensions > Premiere MCP Bridge"
                Write-Host "3. Enable Auto-run commands"
            }
        } catch {
            Write-Warning "Failed to install Premiere bridge: $($_.Exception.Message)"
        }
    }
}
