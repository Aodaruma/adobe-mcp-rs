param(
    [string]$BridgeScriptPath
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

function Resolve-PremiereExtensionSource {
    $candidate = Join-Path $PSScriptRoot "premiere-cep\mcp-bridge-premiere"
    if (Test-Path -LiteralPath $candidate) {
        return (Resolve-Path -LiteralPath $candidate).Path
    }
    return $null
}

function Resolve-PremiereUxpManifest {
    $candidate = Join-Path $PSScriptRoot "premiere-uxp\mcp-bridge-premiere\manifest.json"
    if (Test-Path -LiteralPath $candidate) {
        return (Resolve-Path -LiteralPath $candidate).Path
    }
    return $null
}

$source = Resolve-BridgeScriptPath -InputPath $BridgeScriptPath
$targets = Get-AeInstallPaths

if ($targets.Count -eq 0) {
    Write-Host "No After Effects installation was detected under C:\Program Files\Adobe. Skipped bridge deployment."
    exit 0
}

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

$premiereTargets = Get-PremiereInstallPaths
$premiereSource = Resolve-PremiereExtensionSource
if (-not $premiereSource) {
    Write-Host "Premiere CEP extension not found. Skipped Premiere bridge deployment."
    exit 0
}

if ($premiereTargets.Count -eq 0) {
    Write-Host "No Adobe Premiere Pro installation was detected. Skipped Premiere bridge deployment."
    exit 0
}

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
    Write-Host "Premiere bridge installed: $premiereDest"
} catch {
    Write-Warning "Failed to install Premiere bridge: $($_.Exception.Message)"
}

$premiereUxpManifest = Resolve-PremiereUxpManifest
if ($premiereUxpManifest) {
    Write-Host "Premiere UXP bridge bundled. Load with Adobe UXP Developer Tool: $premiereUxpManifest"
}
