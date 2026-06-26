param(
    [string]$OutputDir = ".\dist\windows",
    [switch]$RequireMsi
)

$ErrorActionPreference = "Stop"

function Ensure-Directory {
    param([string]$Path)
    if (!(Test-Path $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Find-WixCommand {
    $wix = Get-Command wix -ErrorAction SilentlyContinue
    if ($wix) {
        return $wix.Source
    }

    $candidates = @(
        (Join-Path $env:USERPROFILE ".dotnet\tools\wix.exe"),
        "C:\Program Files\WiX Toolset v7.0\bin\wix.exe",
        "C:\Program Files\WiX Toolset v6.0\bin\wix.exe",
        "C:\Program Files\WiX Toolset v5.0\bin\wix.exe"
    )

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    return $null
}

function ConvertTo-RtfEscapedText {
    param([string]$Text)

    $escaped = $Text -replace '\\', '\\' -replace '\{', '\{' -replace '\}', '\}'
    $escaped = $escaped -replace "`r`n", '\par ' -replace "`n", '\par ' -replace "`r", '\par '
    return "{\rtf1\ansi\deff0 $escaped}"
}

function Invoke-NativeChecked {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$ArgumentList
    )

    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code ${LASTEXITCODE}: $FilePath $($ArgumentList -join ' ')"
    }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$output = Resolve-Path -Path $OutputDir -ErrorAction SilentlyContinue
if (-not $output) {
    Ensure-Directory $OutputDir
    $output = Resolve-Path $OutputDir
}

Push-Location $repoRoot
try {
    Write-Host "Building release binaries..."
    Invoke-NativeChecked -FilePath "cargo" -ArgumentList @("build", "--release", "-p", "ae-mcp", "-p", "pr-mcp", "-p", "ps-mcp", "-p", "ai-mcp")

    $exePath = Join-Path $repoRoot "target\release\ae-mcp.exe"
    if (!(Test-Path $exePath)) {
        throw "Release binary not found: $exePath"
    }
    $prExePath = Join-Path $repoRoot "target\release\pr-mcp.exe"
    if (!(Test-Path $prExePath)) {
        throw "Release binary not found: $prExePath"
    }
    $psExePath = Join-Path $repoRoot "target\release\ps-mcp.exe"
    if (!(Test-Path $psExePath)) {
        throw "Release binary not found: $psExePath"
    }
    $aiExePath = Join-Path $repoRoot "target\release\ai-mcp.exe"
    if (!(Test-Path $aiExePath)) {
        throw "Release binary not found: $aiExePath"
    }
    $bridgePanelPath = Join-Path $repoRoot "src\scripts\mcp-bridge-auto.jsx"
    if (!(Test-Path $bridgePanelPath)) {
        throw "Bridge panel script not found: $bridgePanelPath"
    }
    $premiereCepPath = Join-Path $repoRoot "src\premiere\cep\mcp-bridge-premiere"
    if (!(Test-Path $premiereCepPath)) {
        throw "Premiere CEP bridge not found: $premiereCepPath"
    }
    $premiereUxpPath = Join-Path $repoRoot "src\premiere\uxp\mcp-bridge-premiere"
    if (!(Test-Path $premiereUxpPath)) {
        throw "Premiere UXP bridge not found: $premiereUxpPath"
    }
    $photoshopUxpPath = Join-Path $repoRoot "src\photoshop\uxp\mcp-bridge-photoshop"
    if (!(Test-Path $photoshopUxpPath)) {
        throw "Photoshop UXP bridge not found: $photoshopUxpPath"
    }
    $illustratorCepPath = Join-Path $repoRoot "src\illustrator\cep\mcp-bridge-illustrator"
    if (!(Test-Path $illustratorCepPath)) {
        throw "Illustrator CEP bridge not found: $illustratorCepPath"
    }
    $installerBridgeScriptPath = Join-Path $repoRoot "scripts\install-bridge-installer.ps1"
    if (!(Test-Path $installerBridgeScriptPath)) {
        throw "Installer bridge deployment script not found: $installerBridgeScriptPath"
    }

    $stageDir = Join-Path $output "stage"
    if (Test-Path -LiteralPath $stageDir) {
        Remove-Item -LiteralPath $stageDir -Recurse -Force
    }
    Ensure-Directory $stageDir
    Copy-Item $exePath (Join-Path $stageDir "ae-mcp.exe") -Force
    Copy-Item $prExePath (Join-Path $stageDir "pr-mcp.exe") -Force
    Copy-Item $psExePath (Join-Path $stageDir "ps-mcp.exe") -Force
    Copy-Item $aiExePath (Join-Path $stageDir "ai-mcp.exe") -Force
    Copy-Item $bridgePanelPath (Join-Path $stageDir "mcp-bridge-auto.jsx") -Force
    $premiereStageDir = Join-Path $stageDir "premiere-cep"
    Ensure-Directory $premiereStageDir
    Copy-Item $premiereCepPath (Join-Path $premiereStageDir "mcp-bridge-premiere") -Recurse -Force
    $premiereUxpStageDir = Join-Path $stageDir "premiere-uxp"
    Ensure-Directory $premiereUxpStageDir
    Copy-Item $premiereUxpPath (Join-Path $premiereUxpStageDir "mcp-bridge-premiere") -Recurse -Force
    $photoshopUxpStageDir = Join-Path $stageDir "photoshop-uxp"
    Ensure-Directory $photoshopUxpStageDir
    Copy-Item $photoshopUxpPath (Join-Path $photoshopUxpStageDir "mcp-bridge-photoshop") -Recurse -Force
    $illustratorCepStageDir = Join-Path $stageDir "illustrator-cep"
    Ensure-Directory $illustratorCepStageDir
    Copy-Item $illustratorCepPath (Join-Path $illustratorCepStageDir "mcp-bridge-illustrator") -Recurse -Force
    Copy-Item $installerBridgeScriptPath (Join-Path $stageDir "install-bridge-installer.ps1") -Force

    $zipPath = Join-Path $output "adobe-mcp-rs-windows-x86_64.zip"
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Compress-Archive -Path (Join-Path $stageDir "*") -DestinationPath $zipPath -Force
    Write-Host "Created archive: $zipPath"

    $wixCmd = Find-WixCommand
    if (-not $wixCmd) {
        $msg = "WiX CLI (wix) is not installed; skipped MSI generation."
        if ($RequireMsi) {
            throw $msg
        }
        Write-Warning $msg
        return
    }

    $wxsPath = Join-Path $output "ae-mcp.wxs"
    $msiPath = Join-Path $output "adobe-mcp-rs-windows-x86_64.msi"
    $licenseRtfPath = Join-Path $output "license.rtf"
    $licenseText = Get-Content -Raw -LiteralPath (Join-Path $repoRoot "LICENSE")
    ConvertTo-RtfEscapedText -Text $licenseText | Set-Content -LiteralPath $licenseRtfPath -Encoding ASCII

    $escapedExe = (Join-Path $stageDir "ae-mcp.exe").Replace("\", "\\")
    $escapedPrExe = (Join-Path $stageDir "pr-mcp.exe").Replace("\", "\\")
    $escapedPsExe = (Join-Path $stageDir "ps-mcp.exe").Replace("\", "\\")
    $escapedAiExe = (Join-Path $stageDir "ai-mcp.exe").Replace("\", "\\")
    $escapedBridgePanel = (Join-Path $stageDir "mcp-bridge-auto.jsx").Replace("\", "\\")
    $escapedBridgeInstallerPs1 = (Join-Path $stageDir "install-bridge-installer.ps1").Replace("\", "\\")
    $escapedLicenseRtf = $licenseRtfPath.Replace("\", "\\")
    $premiereRoot = Join-Path $stageDir "premiere-cep\mcp-bridge-premiere"
    $escapedPremiereManifest = (Join-Path $premiereRoot "CSXS\manifest.xml").Replace("\", "\\")
    $escapedPremiereIndex = (Join-Path $premiereRoot "index.html").Replace("\", "\\")
    $escapedPremiereCss = (Join-Path $premiereRoot "css\styles.css").Replace("\", "\\")
    $escapedPremiereJs = (Join-Path $premiereRoot "js\main.js").Replace("\", "\\")
    $escapedPremiereJsx = (Join-Path $premiereRoot "jsx\bridge.jsx").Replace("\", "\\")
    $premiereUxpRoot = Join-Path $stageDir "premiere-uxp\mcp-bridge-premiere"
    $escapedPremiereUxpManifest = (Join-Path $premiereUxpRoot "manifest.json").Replace("\", "\\")
    $escapedPremiereUxpIndex = (Join-Path $premiereUxpRoot "index.html").Replace("\", "\\")
    $escapedPremiereUxpReadme = (Join-Path $premiereUxpRoot "README.md").Replace("\", "\\")
    $escapedPremiereUxpCss = (Join-Path $premiereUxpRoot "css\styles.css").Replace("\", "\\")
    $escapedPremiereUxpJs = (Join-Path $premiereUxpRoot "js\main.js").Replace("\", "\\")
    $photoshopUxpRoot = Join-Path $stageDir "photoshop-uxp\mcp-bridge-photoshop"
    $escapedPhotoshopUxpManifest = (Join-Path $photoshopUxpRoot "manifest.json").Replace("\", "\\")
    $escapedPhotoshopUxpIndex = (Join-Path $photoshopUxpRoot "index.html").Replace("\", "\\")
    $escapedPhotoshopUxpReadme = (Join-Path $photoshopUxpRoot "README.md").Replace("\", "\\")
    $escapedPhotoshopUxpCss = (Join-Path $photoshopUxpRoot "css\styles.css").Replace("\", "\\")
    $escapedPhotoshopUxpJs = (Join-Path $photoshopUxpRoot "js\main.js").Replace("\", "\\")
    $illustratorCepRoot = Join-Path $stageDir "illustrator-cep\mcp-bridge-illustrator"
    $escapedIllustratorManifest = (Join-Path $illustratorCepRoot "CSXS\manifest.xml").Replace("\", "\\")
    $escapedIllustratorIndex = (Join-Path $illustratorCepRoot "index.html").Replace("\", "\\")
    $escapedIllustratorCss = (Join-Path $illustratorCepRoot "css\styles.css").Replace("\", "\\")
    $escapedIllustratorJs = (Join-Path $illustratorCepRoot "js\main.js").Replace("\", "\\")
    $escapedIllustratorJsx = (Join-Path $illustratorCepRoot "jsx\bridge.jsx").Replace("\", "\\")

    @"
<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs" xmlns:ui="http://wixtoolset.org/schemas/v4/wxs/ui">
  <Package Name="Adobe MCP (Rust)"
           Manufacturer="adobe-mcp-rs contributors"
           Version="0.4.4.0"
           UpgradeCode="D7C1D860-4DA9-4E1E-B64A-8F64B7D9CC6E"
           Compressed="yes">
    <MediaTemplate EmbedCab="yes" />
    <MajorUpgrade AllowDowngrades="yes" />
    <WixVariable Id="WixUILicenseRtf" Value="$escapedLicenseRtf" />
    <ui:WixUI Id="WixUI_FeatureTree" />
    <StandardDirectory Id="ProgramFiles64Folder">
      <Directory Id="INSTALLFOLDER" Name="AfterEffectsMcp">
        <Component Id="AeMcpExeComponent" Guid="F94E8CF7-36DE-4E55-8FE5-C86069A6A4F9">
          <File Id="AeMcpExeFile" Source="$escapedExe" KeyPath="yes" />
        </Component>
        <Component Id="PrMcpExeComponent" Guid="E1E8E4F4-3D8C-4C8D-A44B-5D2BB9F5D311">
          <File Id="PrMcpExeFile" Source="$escapedPrExe" KeyPath="yes" />
        </Component>
        <Component Id="PsMcpExeComponent" Guid="6B0F686E-D199-4F9B-9B0E-643FA63F1C30">
          <File Id="PsMcpExeFile" Source="$escapedPsExe" KeyPath="yes" />
        </Component>
        <Component Id="AiMcpExeComponent" Guid="B9E58B92-1F55-4C5F-9699-4AF70DE7012A">
          <File Id="AiMcpExeFile" Source="$escapedAiExe" KeyPath="yes" />
        </Component>
        <Component Id="BridgePanelComponent" Guid="6EFCE0CF-7EFD-4A28-9DF9-9A4B1A16F9D4">
          <File Id="BridgePanelFile" Source="$escapedBridgePanel" KeyPath="yes" />
        </Component>
        <Component Id="BridgeInstallerScriptComponent" Guid="6D25E6A8-A7F3-4C42-AEF9-A1756BC85701">
          <File Id="BridgeInstallerScriptFile" Source="$escapedBridgeInstallerPs1" KeyPath="yes" />
        </Component>
        <Directory Id="PremiereCepRoot" Name="premiere-cep">
          <Directory Id="PremiereCepExtension" Name="mcp-bridge-premiere">
            <Directory Id="PremiereCepCsxs" Name="CSXS">
              <Component Id="PremiereBridgeManifestComponent" Guid="B6F8D17F-1D0E-42B8-B13E-17F6C8D4E5B1">
                <File Id="PremiereBridgeManifestFile" Source="$escapedPremiereManifest" KeyPath="yes" />
              </Component>
            </Directory>
            <Directory Id="PremiereCepCss" Name="css">
              <Component Id="PremiereBridgeCssComponent" Guid="D8C77E1B-9D3E-4D74-91E7-98A9D1F9B8B1">
                <File Id="PremiereBridgeCssFile" Source="$escapedPremiereCss" KeyPath="yes" />
              </Component>
            </Directory>
            <Directory Id="PremiereCepJs" Name="js">
              <Component Id="PremiereBridgeJsComponent" Guid="0E5C32E5-5E2B-4AF0-A3C2-7BE2C2B6A6E7">
                <File Id="PremiereBridgeJsFile" Source="$escapedPremiereJs" KeyPath="yes" />
              </Component>
            </Directory>
            <Directory Id="PremiereCepJsx" Name="jsx">
              <Component Id="PremiereBridgeJsxComponent" Guid="A7E7F6A2-52A9-4E9A-8B1F-9F7855DD6F4B">
                <File Id="PremiereBridgeJsxFile" Source="$escapedPremiereJsx" KeyPath="yes" />
              </Component>
            </Directory>
            <Component Id="PremiereBridgeIndexComponent" Guid="5D1D4F77-98D5-4A6B-9F93-3D60B610B1F0">
              <File Id="PremiereBridgeIndexFile" Source="$escapedPremiereIndex" KeyPath="yes" />
            </Component>
          </Directory>
        </Directory>
        <Directory Id="PremiereUxpRoot" Name="premiere-uxp">
          <Directory Id="PremiereUxpExtension" Name="mcp-bridge-premiere">
            <Directory Id="PremiereUxpCss" Name="css">
              <Component Id="PremiereUxpCssComponent" Guid="6C70D6C3-8F6F-4F53-BC00-89E60592743B">
                <File Id="PremiereUxpCssFile" Source="$escapedPremiereUxpCss" KeyPath="yes" />
              </Component>
            </Directory>
            <Directory Id="PremiereUxpJs" Name="js">
              <Component Id="PremiereUxpJsComponent" Guid="8C20F181-F4AC-45B9-A6E9-05ED4322A774">
                <File Id="PremiereUxpJsFile" Source="$escapedPremiereUxpJs" KeyPath="yes" />
              </Component>
            </Directory>
            <Component Id="PremiereUxpManifestComponent" Guid="B8F3412B-91CE-47C6-AB6A-6329E6D89C87">
              <File Id="PremiereUxpManifestFile" Source="$escapedPremiereUxpManifest" KeyPath="yes" />
            </Component>
            <Component Id="PremiereUxpIndexComponent" Guid="C2DB879E-68E7-45DB-90B4-35E5E772834B">
              <File Id="PremiereUxpIndexFile" Source="$escapedPremiereUxpIndex" KeyPath="yes" />
            </Component>
            <Component Id="PremiereUxpReadmeComponent" Guid="01D18F9B-D928-435D-A1AB-4DD40A287F4F">
              <File Id="PremiereUxpReadmeFile" Source="$escapedPremiereUxpReadme" KeyPath="yes" />
            </Component>
          </Directory>
        </Directory>
        <Directory Id="PhotoshopUxpRoot" Name="photoshop-uxp">
          <Directory Id="PhotoshopUxpExtension" Name="mcp-bridge-photoshop">
            <Directory Id="PhotoshopUxpCss" Name="css">
              <Component Id="PhotoshopUxpCssComponent" Guid="8F568B8E-DB60-4653-B6F3-FEE033F9F92E">
                <File Id="PhotoshopUxpCssFile" Source="$escapedPhotoshopUxpCss" KeyPath="yes" />
              </Component>
            </Directory>
            <Directory Id="PhotoshopUxpJs" Name="js">
              <Component Id="PhotoshopUxpJsComponent" Guid="4F96FD3D-E305-461B-8582-1CB5B00D42BA">
                <File Id="PhotoshopUxpJsFile" Source="$escapedPhotoshopUxpJs" KeyPath="yes" />
              </Component>
            </Directory>
            <Component Id="PhotoshopUxpManifestComponent" Guid="4C00817C-CB15-4240-BA24-6E7983BB0370">
              <File Id="PhotoshopUxpManifestFile" Source="$escapedPhotoshopUxpManifest" KeyPath="yes" />
            </Component>
            <Component Id="PhotoshopUxpIndexComponent" Guid="6371FCA7-FE43-474B-8360-C11F0E9FC939">
              <File Id="PhotoshopUxpIndexFile" Source="$escapedPhotoshopUxpIndex" KeyPath="yes" />
            </Component>
            <Component Id="PhotoshopUxpReadmeComponent" Guid="0932A804-48A2-4FF6-B763-69A2E3019E41">
              <File Id="PhotoshopUxpReadmeFile" Source="$escapedPhotoshopUxpReadme" KeyPath="yes" />
            </Component>
          </Directory>
        </Directory>
        <Directory Id="IllustratorCepRoot" Name="illustrator-cep">
          <Directory Id="IllustratorCepExtension" Name="mcp-bridge-illustrator">
            <Directory Id="IllustratorCepCsxs" Name="CSXS">
              <Component Id="IllustratorBridgeManifestComponent" Guid="DAAE6E00-1A8C-4EC2-94D7-1CD32B636D90">
                <File Id="IllustratorBridgeManifestFile" Source="$escapedIllustratorManifest" KeyPath="yes" />
              </Component>
            </Directory>
            <Directory Id="IllustratorCepCss" Name="css">
              <Component Id="IllustratorBridgeCssComponent" Guid="6317D1C4-E175-497F-989E-F773E1D8B0E4">
                <File Id="IllustratorBridgeCssFile" Source="$escapedIllustratorCss" KeyPath="yes" />
              </Component>
            </Directory>
            <Directory Id="IllustratorCepJs" Name="js">
              <Component Id="IllustratorBridgeJsComponent" Guid="30C54BA9-05D4-4D12-95E0-9A60418B4654">
                <File Id="IllustratorBridgeJsFile" Source="$escapedIllustratorJs" KeyPath="yes" />
              </Component>
            </Directory>
            <Directory Id="IllustratorCepJsx" Name="jsx">
              <Component Id="IllustratorBridgeJsxComponent" Guid="E181F4A2-0D49-4CE0-9A7A-A21D90318EA1">
                <File Id="IllustratorBridgeJsxFile" Source="$escapedIllustratorJsx" KeyPath="yes" />
              </Component>
            </Directory>
            <Component Id="IllustratorBridgeIndexComponent" Guid="922356A2-96F0-469D-B605-B584F1E0B8DE">
              <File Id="IllustratorBridgeIndexFile" Source="$escapedIllustratorIndex" KeyPath="yes" />
            </Component>
          </Directory>
        </Directory>
      </Directory>
    </StandardDirectory>
    <SetProperty Id="InstallMachineHostIntegration"
                 Value="&quot;[System64Folder]WindowsPowerShell\v1.0\powershell.exe&quot; -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File &quot;[INSTALLFOLDER]install-bridge-installer.ps1&quot; -BridgeScriptPath &quot;[INSTALLFOLDER]mcp-bridge-auto.jsx&quot; -AeMcpPath &quot;[INSTALLFOLDER]ae-mcp.exe&quot; -PrMcpPath &quot;[INSTALLFOLDER]pr-mcp.exe&quot; -PsMcpPath &quot;[INSTALLFOLDER]ps-mcp.exe&quot; -AiMcpPath &quot;[INSTALLFOLDER]ai-mcp.exe&quot; -NonInteractive -SkipUserInstall"
                 Before="InstallMachineHostIntegration"
                 Sequence="execute" />
    <CustomAction Id="InstallMachineHostIntegration"
                  BinaryRef="Wix4UtilCA_X64"
                  DllEntry="WixQuietExec"
                  Execute="deferred"
                  Impersonate="no"
                  Return="ignore" />
    <SetProperty Id="WixQuietExecCmdLine"
                 Value="&quot;[System64Folder]WindowsPowerShell\v1.0\powershell.exe&quot; -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File &quot;[INSTALLFOLDER]install-bridge-installer.ps1&quot; -BridgeScriptPath &quot;[INSTALLFOLDER]mcp-bridge-auto.jsx&quot; -AeMcpPath &quot;[INSTALLFOLDER]ae-mcp.exe&quot; -PrMcpPath &quot;[INSTALLFOLDER]pr-mcp.exe&quot; -PsMcpPath &quot;[INSTALLFOLDER]ps-mcp.exe&quot; -AiMcpPath &quot;[INSTALLFOLDER]ai-mcp.exe&quot; -NonInteractive -SkipHostBridgeInstall"
                 Before="InstallUserHostIntegration"
                 Sequence="execute" />
    <CustomAction Id="InstallUserHostIntegration"
                  BinaryRef="Wix4UtilCA_X64"
                  DllEntry="WixQuietExec"
                  Execute="immediate"
                  Return="ignore" />
    <InstallExecuteSequence>
      <Custom Action="InstallMachineHostIntegration" After="InstallFiles" Condition="NOT Installed AND NOT REMOVE" />
      <Custom Action="InstallUserHostIntegration" After="InstallFinalize" Condition="NOT Installed AND NOT REMOVE" />
    </InstallExecuteSequence>
    <Feature Id="MainFeature" Title="Adobe MCP Core" Description="Installs the MCP command-line binaries and installer helper." Level="1" Display="expand">
      <ComponentRef Id="AeMcpExeComponent" />
      <ComponentRef Id="PrMcpExeComponent" />
      <ComponentRef Id="PsMcpExeComponent" />
      <ComponentRef Id="AiMcpExeComponent" />
      <ComponentRef Id="BridgeInstallerScriptComponent" />
      <Feature Id="AfterEffectsPanelFeature" Title="After Effects ScriptUI panel" Description="Deploys mcp-bridge-auto.jsx and startup loader to detected After Effects installations." Level="1">
        <ComponentRef Id="BridgePanelComponent" />
      </Feature>
      <Feature Id="PremiereUxpFeature" Title="Premiere Pro UXP bridge" Description="Installs the Premiere Pro UXP bridge package when supported." Level="1">
        <ComponentRef Id="PremiereUxpManifestComponent" />
        <ComponentRef Id="PremiereUxpIndexComponent" />
        <ComponentRef Id="PremiereUxpReadmeComponent" />
        <ComponentRef Id="PremiereUxpCssComponent" />
        <ComponentRef Id="PremiereUxpJsComponent" />
      </Feature>
      <Feature Id="PremiereCepFeature" Title="Premiere Pro CEP fallback" Description="Installs the Premiere Pro CEP fallback panel for older Premiere Pro versions." Level="1">
        <ComponentRef Id="PremiereBridgeManifestComponent" />
        <ComponentRef Id="PremiereBridgeCssComponent" />
        <ComponentRef Id="PremiereBridgeJsComponent" />
        <ComponentRef Id="PremiereBridgeJsxComponent" />
        <ComponentRef Id="PremiereBridgeIndexComponent" />
      </Feature>
      <Feature Id="PhotoshopUxpFeature" Title="Photoshop UXP bridge" Description="Installs the Photoshop UXP bridge package." Level="1">
        <ComponentRef Id="PhotoshopUxpManifestComponent" />
        <ComponentRef Id="PhotoshopUxpIndexComponent" />
        <ComponentRef Id="PhotoshopUxpReadmeComponent" />
        <ComponentRef Id="PhotoshopUxpCssComponent" />
        <ComponentRef Id="PhotoshopUxpJsComponent" />
      </Feature>
      <Feature Id="IllustratorCepFeature" Title="Illustrator CEP bridge" Description="Installs the Illustrator CEP panel." Level="1">
        <ComponentRef Id="IllustratorBridgeManifestComponent" />
        <ComponentRef Id="IllustratorBridgeCssComponent" />
        <ComponentRef Id="IllustratorBridgeJsComponent" />
        <ComponentRef Id="IllustratorBridgeJsxComponent" />
        <ComponentRef Id="IllustratorBridgeIndexComponent" />
      </Feature>
    </Feature>
  </Package>
</Wix>
"@ | Set-Content -Encoding UTF8 $wxsPath

    if (Test-Path -LiteralPath $msiPath) {
        Remove-Item -LiteralPath $msiPath -Force
    }
    Invoke-NativeChecked -FilePath $wixCmd -ArgumentList @("extension", "add", "WixToolset.UI.wixext/5.0.2", "--global")
    Invoke-NativeChecked -FilePath $wixCmd -ArgumentList @("extension", "add", "WixToolset.Util.wixext/5.0.2", "--global")
    Invoke-NativeChecked -FilePath $wixCmd -ArgumentList @("build", $wxsPath, "-arch", "x64", "-ext", "WixToolset.UI.wixext", "-ext", "WixToolset.Util.wixext", "-out", $msiPath)
    if (!(Test-Path $msiPath)) {
        throw "MSI generation failed. See WiX output above."
    }
    Write-Host "Created MSI: $msiPath"

    $tmpDropDir = "D:\GoogleDrive\tmp"
    if (Test-Path -LiteralPath $tmpDropDir) {
        $tmpMsiPath = Join-Path $tmpDropDir "adobe-mcp-rs-windows-x86_64.msi"
        Copy-Item -LiteralPath $msiPath -Destination $tmpMsiPath -Force
        Write-Host "Copied MSI: $tmpMsiPath"
    } else {
        Write-Warning "MSI copy target not found: $tmpDropDir"
    }
}
finally {
    Pop-Location
}
