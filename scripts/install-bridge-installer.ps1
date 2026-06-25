param(
    [string]$BridgeScriptPath,
    [string]$AeMcpPath,
    [string]$PrMcpPath,
    [string]$PsMcpPath,
    [string]$AiMcpPath,
    [switch]$PlanInstall,
    [switch]$FinalizeInstall,
    [switch]$InteractiveInstall,
    [switch]$NonInteractive,
    [switch]$SkipHostBridgeInstall,
    [switch]$SkipUserInstall
)

$ErrorActionPreference = "Stop"
$PackageVersion = "0.4.2"
$InstallerStateRoot = Join-Path $env:ProgramData "AfterEffectsMcp"
$InstallSelectionPath = Join-Path $InstallerStateRoot "install-selection.json"
$InstallReportPath = Join-Path $InstallerStateRoot "install-report.json"

function Ensure-InstallerStateRoot {
    if (-not (Test-Path -LiteralPath $InstallerStateRoot)) {
        New-Item -ItemType Directory -Path $InstallerStateRoot -Force | Out-Null
    }
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

function Get-JsonManifestVersion {
    param([string]$ManifestPath)

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

    [pscustomobject]@{
        key = $Key
        label = $Label
        available = $Available
        selected = ($Available -and $Selected)
        oldVersion = (Get-DisplayVersion $OldVersion)
        newVersion = (Get-DisplayVersion $NewVersion)
        note = $Note
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
    $bridgeScript = $null
    try {
        $bridgeScript = Resolve-BridgeScriptPath -InputPath $BridgeScriptPath
    } catch {}

    $premiereTargets = Get-PremiereInstallPaths
    $premiereUxpTargets = Get-UxpCapablePremierePaths -PremierePaths $premiereTargets
    $photoshopTargets = Get-PhotoshopInstallPaths
    $illustratorTargets = Get-IllustratorInstallPaths
    $upia = Find-UpiaCommand

    $premiereUxpSource = Resolve-PremiereUxpSource
    $photoshopUxpSource = Resolve-PhotoshopUxpSource
    $premiereCepSource = Resolve-PremiereExtensionSource
    $illustratorCepSource = Resolve-IllustratorCepSource

    $items = @()
    $items += New-InstallPlanItem `
        -Key "aftereffects-panel" `
        -Label "After Effects ScriptUI panel" `
        -Available ([bool]$bridgeScript -and (Get-AeInstallPaths).Count -gt 0) `
        -Selected $true `
        -OldVersion (Get-InstalledAePanelVersion) `
        -NewVersion (Get-PanelScriptVersion -ScriptPath $bridgeScript) `
        -Note "Installs mcp-bridge-auto.jsx into detected After Effects versions."

    $items += New-InstallPlanItem `
        -Key "premiere-uxp" `
        -Label "Premiere Pro UXP bridge" `
        -Available ([bool]$premiereUxpSource -and $premiereUxpTargets.Count -gt 0 -and [bool]$upia) `
        -Selected $true `
        -OldVersion (Get-UxpInstalledVersion -PluginId "io.github.aodaruma.premiere-mcp-bridge" -InfoFileName "premierepro.json") `
        -NewVersion (Get-JsonManifestVersion -ManifestPath (Join-Path $premiereUxpSource "manifest.json")) `
        -Note "Preferred for Premiere Pro 25.6 or newer."

    $items += New-InstallPlanItem `
        -Key "premiere-cep" `
        -Label "Premiere Pro CEP fallback" `
        -Available ([bool]$premiereCepSource -and $premiereTargets.Count -gt 0 -and $premiereUxpTargets.Count -eq 0) `
        -Selected $true `
        -OldVersion (Get-CepManifestVersion -ManifestPath "C:\Program Files (x86)\Common Files\Adobe\CEP\extensions\mcp-bridge-premiere\CSXS\manifest.xml") `
        -NewVersion (Get-CepManifestVersion -ManifestPath (Join-Path $premiereCepSource "CSXS\manifest.xml")) `
        -Note "Only used when no UXP-capable Premiere Pro is detected."

    $items += New-InstallPlanItem `
        -Key "photoshop-uxp" `
        -Label "Photoshop UXP bridge" `
        -Available ([bool]$photoshopUxpSource -and $photoshopTargets.Count -gt 0 -and [bool]$upia) `
        -Selected $true `
        -OldVersion (Get-UxpInstalledVersion -PluginId "io.github.aodaruma.photoshop-mcp-bridge" -InfoFileName "PS.json") `
        -NewVersion (Get-JsonManifestVersion -ManifestPath (Join-Path $photoshopUxpSource "manifest.json")) `
        -Note "Installs the plugin so it appears under Photoshop > Plugins."

    $items += New-InstallPlanItem `
        -Key "illustrator-cep" `
        -Label "Illustrator CEP bridge" `
        -Available ([bool]$illustratorCepSource -and $illustratorTargets.Count -gt 0) `
        -Selected $true `
        -OldVersion (Get-CepManifestVersion -ManifestPath "C:\Program Files (x86)\Common Files\Adobe\CEP\extensions\mcp-bridge-illustrator\CSXS\manifest.xml") `
        -NewVersion (Get-CepManifestVersion -ManifestPath (Join-Path $illustratorCepSource "CSXS\manifest.xml")) `
        -Note "Installs the CEP panel under Window > Extensions."

    $items += New-InstallPlanItem `
        -Key "codex-config" `
        -Label "Codex MCP config" `
        -Available ((Get-CodexConfigPaths).Count -gt 0) `
        -Selected $true `
        -OldVersion "configured / not configured" `
        -NewVersion $PackageVersion `
        -Note "Updates Codex config.toml entries for installed MCP binaries."

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
    $items = @(Get-InstallPlan)
    if ($NonInteractive -or -not [Environment]::UserInteractive) {
        return Write-DefaultInstallSelection
    }

    try {
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -AssemblyName System.Drawing
    } catch {
        Write-Warning "Windows Forms is unavailable. Proceeding with default install selection."
        return Write-DefaultInstallSelection
    }

    $form = New-Object System.Windows.Forms.Form
    $form.Text = "Adobe MCP $PackageVersion - Install Options"
    $form.StartPosition = "CenterScreen"
    $form.Size = New-Object System.Drawing.Size(820, 460)
    $form.MinimumSize = New-Object System.Drawing.Size(760, 420)
    $form.TopMost = $true

    $title = New-Object System.Windows.Forms.Label
    $title.Text = "Select host integrations to install"
    $title.AutoSize = $true
    $title.Font = New-Object System.Drawing.Font($title.Font, [System.Drawing.FontStyle]::Bold)
    $title.Location = New-Object System.Drawing.Point(12, 12)
    $form.Controls.Add($title)

    $grid = New-Object System.Windows.Forms.DataGridView
    $grid.Location = New-Object System.Drawing.Point(12, 42)
    $grid.Size = New-Object System.Drawing.Size(780, 315)
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

    foreach ($item in $items) {
        $rowIndex = $grid.Rows.Add($item.selected, $item.label, $item.oldVersion, $item.newVersion, $item.note)
        $row = $grid.Rows[$rowIndex]
        $row.Tag = $item.key
        if (-not $item.available) {
            $row.Cells[0].Value = $false
            $row.Cells[0].ReadOnly = $true
            $row.DefaultCellStyle.ForeColor = [System.Drawing.Color]::Gray
        }
    }
    $form.Controls.Add($grid)

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

    $dialogResult = $form.ShowDialog()
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
    if (-not $selection) {
        Write-DefaultInstallSelection | Out-Null
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
    $form.TopMost = $true

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

    $form.ShowDialog() | Out-Null
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
    if (-not (Test-InstallComponentSelected -Key "codex-config")) {
        Add-InstallReport -Key "codex-config" -Status "skipped" -Message "Not selected or not available."
        return
    }

    $aePath = Resolve-McpBinaryPath -ProvidedPath $AeMcpPath -FileName "ae-mcp.exe"
    $prPath = Resolve-McpBinaryPath -ProvidedPath $PrMcpPath -FileName "pr-mcp.exe"
    $psPath = Resolve-McpBinaryPath -ProvidedPath $PsMcpPath -FileName "ps-mcp.exe"
    $aiPath = Resolve-McpBinaryPath -ProvidedPath $AiMcpPath -FileName "ai-mcp.exe"
    if (-not $aePath -and -not $prPath -and -not $psPath -and -not $aiPath) {
        Write-Warning "MCP binaries were not found. Skipped Codex config update."
        Add-InstallReport -Key "codex-config" -Status "skipped" -Message "MCP binaries were not found."
        return
    }

    $configs = Get-CodexConfigPaths
    if ($configs.Count -eq 0) {
        Write-Host "Codex config.toml was not found. Skipped Codex MCP server config update."
        Add-InstallReport -Key "codex-config" -Status "skipped" -Message "Codex config.toml was not found."
        return
    }

    $updated = 0
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
            $updated++
        } catch {
            Write-Warning "Failed to update Codex config '$config': $($_.Exception.Message)"
        }
    }
    Add-InstallReport -Key "codex-config" -Status "installed" -Message "Updated $updated Codex config file(s)."
}

if ($PlanInstall -or $InteractiveInstall) {
    Show-InstallPlanDialog | Out-Null
    if ($PlanInstall -and -not $InteractiveInstall) {
        exit 0
    }
}

if (-not $SkipHostBridgeInstall) {
    $source = Resolve-BridgeScriptPath -InputPath $BridgeScriptPath
    $targets = Get-AeInstallPaths

    if (-not (Test-InstallComponentSelected -Key "aftereffects-panel")) {
        Write-Host "After Effects bridge panel deployment skipped by install selection."
        Add-InstallReport -Key "aftereffects-panel" -Status "skipped" -Message "Not selected or not available."
    } elseif ($targets.Count -eq 0) {
        Write-Host "No After Effects installation was detected under C:\Program Files\Adobe. Skipped AE bridge deployment."
        Add-InstallReport -Key "aftereffects-panel" -Status "skipped" -Message "No After Effects installation was detected."
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
        Add-InstallReport -Key "aftereffects-panel" -Status "installed" -Message "Installed to $installed After Effects location(s)."
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
    Update-CodexMcpConfig
}

if ($FinalizeInstall -or $InteractiveInstall) {
    Show-InstallSummaryDialog
}
