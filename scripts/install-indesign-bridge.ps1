param(
    [string]$Source,
    [string[]]$Destination,
    [Alias("Uninstall")]
    [switch]$Remove,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

if (-not $Remove) {
    if ([string]::IsNullOrWhiteSpace($Source)) {
        $bundledSource = Join-Path $PSScriptRoot "mcp-bridge-indesign.idjs"
        if (Test-Path -LiteralPath $bundledSource -PathType Leaf) {
            $Source = $bundledSource
        } else {
            $Source = Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..")) "src\indesign\uxp\mcp-bridge-indesign.idjs"
        }
    }
    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "InDesign bridge source not found: $Source"
    }
    $Source = (Resolve-Path -LiteralPath $Source).Path
}

$targets = @()
foreach ($item in @($Destination)) {
    if (-not [string]::IsNullOrWhiteSpace($item)) {
        $targets += $item
    }
}

if ($targets.Count -eq 0) {
    $preferenceRoot = Join-Path $env:APPDATA "Adobe\InDesign"
    if (Test-Path -LiteralPath $preferenceRoot) {
        $versionFolders = @(Get-ChildItem -LiteralPath $preferenceRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^Version\s+' })
        foreach ($versionFolder in $versionFolders) {
            $localeFolders = @(Get-ChildItem -LiteralPath $versionFolder.FullName -Directory -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -match '^[a-z]{2}_[A-Z]{2}$' })
            foreach ($localeFolder in $localeFolders) {
                $targets += Join-Path $localeFolder.FullName "Scripts\Startup Scripts"
            }
        }
    }
}

$targets = @($targets | Sort-Object -Unique)
if ($targets.Count -eq 0) {
    throw "No InDesign preference profile found. Pass -Destination with the Startup Scripts folder for the installed version and locale."
}

foreach ($target in $targets) {
    $target = [IO.Path]::GetFullPath($target).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    if ((Split-Path -Leaf $target) -ne "Startup Scripts" -or (Split-Path -Leaf (Split-Path -Parent $target)) -ne "Scripts") {
        throw "Destination must be an explicit InDesign Scripts\Startup Scripts directory: $target"
    }
    $destinationFile = Join-Path $target "mcp-bridge-indesign.idjs"
    if ($Remove) {
        if ($DryRun) {
            Write-Host "Would remove fixed bridge file: $destinationFile"
        } elseif (Test-Path -LiteralPath $destinationFile -PathType Leaf) {
            Remove-Item -LiteralPath $destinationFile -Force
            Write-Host "Removed: $destinationFile"
        } else {
            Write-Host "Not installed: $destinationFile"
        }
        continue
    }
    if ($DryRun) {
        Write-Host "Would install: $destinationFile"
        continue
    }
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    Copy-Item -LiteralPath $Source -Destination $destinationFile -Force
    Write-Host "Installed: $destinationFile"
}

if ($Remove) {
    Write-Host "Restart InDesign to unload the removed Startup Script."
} else {
    Write-Host "Restart InDesign, then verify list-indesign-instances and run-bridge-test."
}
