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
    return $null
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
    cargo build --release -p ae-mcp -p pr-mcp -p ps-mcp

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
    $escapedExe = (Join-Path $stageDir "ae-mcp.exe").Replace("\", "\\")
    $escapedPrExe = (Join-Path $stageDir "pr-mcp.exe").Replace("\", "\\")
    $escapedPsExe = (Join-Path $stageDir "ps-mcp.exe").Replace("\", "\\")
    $escapedBridgePanel = (Join-Path $stageDir "mcp-bridge-auto.jsx").Replace("\", "\\")
    $escapedBridgeInstallerPs1 = (Join-Path $stageDir "install-bridge-installer.ps1").Replace("\", "\\")
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

    @"
<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="Adobe MCP (Rust)"
           Manufacturer="adobe-mcp-rs contributors"
           Version="0.2.0.0"
           UpgradeCode="D7C1D860-4DA9-4E1E-B64A-8F64B7D9CC6E"
           Compressed="yes">
    <MediaTemplate EmbedCab="yes" />
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
        <Component Id="BridgeAssetsComponent" Guid="6EFCE0CF-7EFD-4A28-9DF9-9A4B1A16F9D4">
          <File Id="BridgePanelFile" Source="$escapedBridgePanel" KeyPath="yes" />
          <File Id="BridgeInstallerScriptFile" Source="$escapedBridgeInstallerPs1" />
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
      </Directory>
    </StandardDirectory>
    <CustomAction Id="InstallAeBridgePanels"
                  Directory="INSTALLFOLDER"
                  ExeCommand="&quot;[System64Folder]WindowsPowerShell\v1.0\powershell.exe&quot; -NoProfile -ExecutionPolicy Bypass -File &quot;[INSTALLFOLDER]install-bridge-installer.ps1&quot; -BridgeScriptPath &quot;[INSTALLFOLDER]mcp-bridge-auto.jsx&quot; -AeMcpPath &quot;[INSTALLFOLDER]ae-mcp.exe&quot; -PrMcpPath &quot;[INSTALLFOLDER]pr-mcp.exe&quot; -SkipUserInstall"
                  Execute="deferred"
                  Impersonate="no"
                  Return="ignore" />
    <CustomAction Id="InstallUserPremiereUxpAndCodexConfig"
                  Directory="INSTALLFOLDER"
                  ExeCommand="&quot;[System64Folder]WindowsPowerShell\v1.0\powershell.exe&quot; -NoProfile -ExecutionPolicy Bypass -File &quot;[INSTALLFOLDER]install-bridge-installer.ps1&quot; -AeMcpPath &quot;[INSTALLFOLDER]ae-mcp.exe&quot; -PrMcpPath &quot;[INSTALLFOLDER]pr-mcp.exe&quot; -SkipHostBridgeInstall"
                  Execute="deferred"
                  Impersonate="yes"
                  Return="ignore" />
    <InstallExecuteSequence>
      <Custom Action="InstallAeBridgePanels" After="InstallFiles" Condition="NOT Installed AND NOT REMOVE" />
      <Custom Action="InstallUserPremiereUxpAndCodexConfig" After="InstallAeBridgePanels" Condition="NOT Installed AND NOT REMOVE" />
    </InstallExecuteSequence>
    <Feature Id="MainFeature" Title="Adobe MCP" Level="1">
      <ComponentRef Id="AeMcpExeComponent" />
      <ComponentRef Id="PrMcpExeComponent" />
      <ComponentRef Id="PsMcpExeComponent" />
      <ComponentRef Id="BridgeAssetsComponent" />
      <ComponentRef Id="PremiereBridgeManifestComponent" />
      <ComponentRef Id="PremiereBridgeCssComponent" />
      <ComponentRef Id="PremiereBridgeJsComponent" />
      <ComponentRef Id="PremiereBridgeJsxComponent" />
      <ComponentRef Id="PremiereBridgeIndexComponent" />
      <ComponentRef Id="PremiereUxpManifestComponent" />
      <ComponentRef Id="PremiereUxpIndexComponent" />
      <ComponentRef Id="PremiereUxpReadmeComponent" />
      <ComponentRef Id="PremiereUxpCssComponent" />
      <ComponentRef Id="PremiereUxpJsComponent" />
      <ComponentRef Id="PhotoshopUxpManifestComponent" />
      <ComponentRef Id="PhotoshopUxpIndexComponent" />
      <ComponentRef Id="PhotoshopUxpReadmeComponent" />
      <ComponentRef Id="PhotoshopUxpCssComponent" />
      <ComponentRef Id="PhotoshopUxpJsComponent" />
    </Feature>
  </Package>
</Wix>
"@ | Set-Content -Encoding UTF8 $wxsPath

    & $wixCmd build $wxsPath -arch x64 -out $msiPath
    if (!(Test-Path $msiPath)) {
        throw "MSI generation failed. See WiX output above."
    }
    Write-Host "Created MSI: $msiPath"
}
finally {
    Pop-Location
}
