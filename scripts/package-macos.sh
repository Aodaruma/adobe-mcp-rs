#!/usr/bin/env bash
set -euo pipefail

OUTPUT_DIR="${1:-./dist/macos}"
REQUIRE_PKG="${REQUIRE_PKG:-false}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

mkdir -p "$OUTPUT_DIR"

pushd "$REPO_ROOT" >/dev/null

echo "Building release binaries..."
cargo build --release -p ae-mcp -p pr-mcp -p ps-mcp -p ai-mcp -p id-mcp

BIN_PATH_AE="$REPO_ROOT/target/release/ae-mcp"
if [[ ! -f "$BIN_PATH_AE" ]]; then
  echo "Release binary not found: $BIN_PATH_AE" >&2
  exit 1
fi
BIN_PATH_PR="$REPO_ROOT/target/release/pr-mcp"
if [[ ! -f "$BIN_PATH_PR" ]]; then
  echo "Release binary not found: $BIN_PATH_PR" >&2
  exit 1
fi
BIN_PATH_PS="$REPO_ROOT/target/release/ps-mcp"
if [[ ! -f "$BIN_PATH_PS" ]]; then
  echo "Release binary not found: $BIN_PATH_PS" >&2
  exit 1
fi
BIN_PATH_AI="$REPO_ROOT/target/release/ai-mcp"
if [[ ! -f "$BIN_PATH_AI" ]]; then
  echo "Release binary not found: $BIN_PATH_AI" >&2
  exit 1
fi
BIN_PATH_ID="$REPO_ROOT/target/release/id-mcp"
if [[ ! -f "$BIN_PATH_ID" ]]; then
  echo "Release binary not found: $BIN_PATH_ID" >&2
  exit 1
fi
BRIDGE_PANEL_PATH="$REPO_ROOT/src/scripts/mcp-bridge-auto.jsx"
if [[ ! -f "$BRIDGE_PANEL_PATH" ]]; then
  echo "Bridge panel script not found: $BRIDGE_PANEL_PATH" >&2
  exit 1
fi
BRIDGE_STARTUP_PATH="$REPO_ROOT/src/scripts/mcp-bridge-startup.jsx"
if [[ ! -f "$BRIDGE_STARTUP_PATH" ]]; then
  echo "Bridge startup script not found: $BRIDGE_STARTUP_PATH" >&2
  exit 1
fi
BRIDGE_SHUTDOWN_PATH="$REPO_ROOT/src/scripts/mcp-bridge-shutdown.jsx"
if [[ ! -f "$BRIDGE_SHUTDOWN_PATH" ]]; then
  echo "Bridge shutdown script not found: $BRIDGE_SHUTDOWN_PATH" >&2
  exit 1
fi
PREMIERE_CEP_PATH="$REPO_ROOT/src/premiere/cep/mcp-bridge-premiere"
if [[ ! -d "$PREMIERE_CEP_PATH" ]]; then
  echo "Premiere CEP bridge not found: $PREMIERE_CEP_PATH" >&2
  exit 1
fi
PREMIERE_UXP_PATH="$REPO_ROOT/src/premiere/uxp/mcp-bridge-premiere"
if [[ ! -d "$PREMIERE_UXP_PATH" ]]; then
  echo "Premiere UXP bridge not found: $PREMIERE_UXP_PATH" >&2
  exit 1
fi
PHOTOSHOP_UXP_PATH="$REPO_ROOT/src/photoshop/uxp/mcp-bridge-photoshop"
if [[ ! -d "$PHOTOSHOP_UXP_PATH" ]]; then
  echo "Photoshop UXP bridge not found: $PHOTOSHOP_UXP_PATH" >&2
  exit 1
fi
ILLUSTRATOR_CEP_PATH="$REPO_ROOT/src/illustrator/cep/mcp-bridge-illustrator"
if [[ ! -d "$ILLUSTRATOR_CEP_PATH" ]]; then
  echo "Illustrator CEP bridge not found: $ILLUSTRATOR_CEP_PATH" >&2
  exit 1
fi
INDESIGN_BRIDGE_PATH="$REPO_ROOT/src/indesign/uxp/mcp-bridge-indesign.idjs"
if [[ ! -f "$INDESIGN_BRIDGE_PATH" ]]; then
  echo "InDesign Startup Script bridge not found: $INDESIGN_BRIDGE_PATH" >&2
  exit 1
fi
INDESIGN_INSTALLER_PATH="$REPO_ROOT/scripts/install-indesign-bridge.sh"
if [[ ! -f "$INDESIGN_INSTALLER_PATH" ]]; then
  echo "InDesign bridge installer not found: $INDESIGN_INSTALLER_PATH" >&2
  exit 1
fi
CODEX_CONFIG_INSTALLER_PATH="$REPO_ROOT/scripts/install-codex-mcp-config.sh"
if [[ ! -f "$CODEX_CONFIG_INSTALLER_PATH" ]]; then
  echo "Codex MCP config installer not found: $CODEX_CONFIG_INSTALLER_PATH" >&2
  exit 1
fi

STAGE_DIR="$OUTPUT_DIR/stage"
mkdir -p "$STAGE_DIR"
cp "$BIN_PATH_AE" "$STAGE_DIR/ae-mcp"
chmod +x "$STAGE_DIR/ae-mcp"
cp "$BIN_PATH_PR" "$STAGE_DIR/pr-mcp"
chmod +x "$STAGE_DIR/pr-mcp"
cp "$BIN_PATH_PS" "$STAGE_DIR/ps-mcp"
chmod +x "$STAGE_DIR/ps-mcp"
cp "$BIN_PATH_AI" "$STAGE_DIR/ai-mcp"
chmod +x "$STAGE_DIR/ai-mcp"
cp "$BIN_PATH_ID" "$STAGE_DIR/id-mcp"
chmod +x "$STAGE_DIR/id-mcp"
cp "$BRIDGE_PANEL_PATH" "$STAGE_DIR/mcp-bridge-auto.jsx"
cp "$BRIDGE_STARTUP_PATH" "$STAGE_DIR/mcp-bridge-startup.jsx"
cp "$BRIDGE_SHUTDOWN_PATH" "$STAGE_DIR/mcp-bridge-shutdown.jsx"
mkdir -p "$STAGE_DIR/premiere-cep"
cp -R "$PREMIERE_CEP_PATH" "$STAGE_DIR/premiere-cep/mcp-bridge-premiere"
mkdir -p "$STAGE_DIR/premiere-uxp"
cp -R "$PREMIERE_UXP_PATH" "$STAGE_DIR/premiere-uxp/mcp-bridge-premiere"
mkdir -p "$STAGE_DIR/photoshop-uxp"
cp -R "$PHOTOSHOP_UXP_PATH" "$STAGE_DIR/photoshop-uxp/mcp-bridge-photoshop"
mkdir -p "$STAGE_DIR/illustrator-cep"
cp -R "$ILLUSTRATOR_CEP_PATH" "$STAGE_DIR/illustrator-cep/mcp-bridge-illustrator"
mkdir -p "$STAGE_DIR/indesign"
cp "$INDESIGN_BRIDGE_PATH" "$STAGE_DIR/indesign/mcp-bridge-indesign.idjs"
cp "$INDESIGN_INSTALLER_PATH" "$STAGE_DIR/indesign/install-indesign-bridge.sh"
chmod +x "$STAGE_DIR/indesign/install-indesign-bridge.sh"
cp "$CODEX_CONFIG_INSTALLER_PATH" "$STAGE_DIR/install-codex-mcp-config.sh"
chmod +x "$STAGE_DIR/install-codex-mcp-config.sh"

ARCHIVE_PATH="$OUTPUT_DIR/adobe-mcp-rs-macos-universal.tar.gz"
tar -C "$STAGE_DIR" -czf "$ARCHIVE_PATH" .
echo "Created archive: $ARCHIVE_PATH"

if ! command -v pkgbuild >/dev/null 2>&1; then
  MSG="pkgbuild is not available; skipped pkg generation."
  if [[ "$REQUIRE_PKG" == "true" ]]; then
    echo "$MSG" >&2
    exit 1
  fi
  echo "$MSG"
  popd >/dev/null
  exit 0
fi

PKG_ROOT="$OUTPUT_DIR/pkgroot"
INSTALL_BIN_DIR="$PKG_ROOT/usr/local/bin"
INSTALL_SHARE_DIR="$PKG_ROOT/usr/local/share/ae-mcp"
mkdir -p "$INSTALL_BIN_DIR"
mkdir -p "$INSTALL_SHARE_DIR"
cp "$STAGE_DIR/ae-mcp" "$INSTALL_BIN_DIR/ae-mcp"
cp "$STAGE_DIR/pr-mcp" "$INSTALL_BIN_DIR/pr-mcp"
cp "$STAGE_DIR/ps-mcp" "$INSTALL_BIN_DIR/ps-mcp"
cp "$STAGE_DIR/ai-mcp" "$INSTALL_BIN_DIR/ai-mcp"
cp "$STAGE_DIR/id-mcp" "$INSTALL_BIN_DIR/id-mcp"
cp "$STAGE_DIR/mcp-bridge-auto.jsx" "$INSTALL_SHARE_DIR/mcp-bridge-auto.jsx"
cp "$STAGE_DIR/mcp-bridge-startup.jsx" "$INSTALL_SHARE_DIR/mcp-bridge-startup.jsx"
cp "$STAGE_DIR/mcp-bridge-shutdown.jsx" "$INSTALL_SHARE_DIR/mcp-bridge-shutdown.jsx"
mkdir -p "$INSTALL_SHARE_DIR/premiere-cep"
cp -R "$STAGE_DIR/premiere-cep/mcp-bridge-premiere" "$INSTALL_SHARE_DIR/premiere-cep/mcp-bridge-premiere"
mkdir -p "$INSTALL_SHARE_DIR/premiere-uxp"
cp -R "$STAGE_DIR/premiere-uxp/mcp-bridge-premiere" "$INSTALL_SHARE_DIR/premiere-uxp/mcp-bridge-premiere"
mkdir -p "$INSTALL_SHARE_DIR/photoshop-uxp"
cp -R "$STAGE_DIR/photoshop-uxp/mcp-bridge-photoshop" "$INSTALL_SHARE_DIR/photoshop-uxp/mcp-bridge-photoshop"
mkdir -p "$INSTALL_SHARE_DIR/illustrator-cep"
cp -R "$STAGE_DIR/illustrator-cep/mcp-bridge-illustrator" "$INSTALL_SHARE_DIR/illustrator-cep/mcp-bridge-illustrator"
mkdir -p "$INSTALL_SHARE_DIR/indesign"
cp "$STAGE_DIR/indesign/mcp-bridge-indesign.idjs" "$INSTALL_SHARE_DIR/indesign/mcp-bridge-indesign.idjs"
cp "$STAGE_DIR/indesign/install-indesign-bridge.sh" "$INSTALL_SHARE_DIR/indesign/install-indesign-bridge.sh"
chmod +x "$INSTALL_SHARE_DIR/indesign/install-indesign-bridge.sh"
cp "$STAGE_DIR/install-codex-mcp-config.sh" "$INSTALL_SHARE_DIR/install-codex-mcp-config.sh"
chmod +x "$INSTALL_SHARE_DIR/install-codex-mcp-config.sh"

PKG_PATH="$OUTPUT_DIR/adobe-mcp-rs-macos-universal.pkg"
PKG_SCRIPTS_DIR="$OUTPUT_DIR/pkgscripts"
mkdir -p "$PKG_SCRIPTS_DIR"
cat > "$PKG_SCRIPTS_DIR/postinstall" <<'POSTINSTALL'
#!/usr/bin/env bash
set -euo pipefail

SOURCE_SCRIPT="/usr/local/share/ae-mcp/mcp-bridge-auto.jsx"
SOURCE_STARTUP_SCRIPT="/usr/local/share/ae-mcp/mcp-bridge-startup.jsx"
SOURCE_SHUTDOWN_SCRIPT="/usr/local/share/ae-mcp/mcp-bridge-shutdown.jsx"
PREMIERE_CEP_SOURCE="/usr/local/share/ae-mcp/premiere-cep/mcp-bridge-premiere"
PREMIERE_UXP_MANIFEST="/usr/local/share/ae-mcp/premiere-uxp/mcp-bridge-premiere/manifest.json"
PHOTOSHOP_UXP_MANIFEST="/usr/local/share/ae-mcp/photoshop-uxp/mcp-bridge-photoshop/manifest.json"
ILLUSTRATOR_CEP_SOURCE="/usr/local/share/ae-mcp/illustrator-cep/mcp-bridge-illustrator"
INDESIGN_BUNDLE_DIR="/usr/local/share/ae-mcp/indesign"
INDESIGN_SOURCE="$INDESIGN_BUNDLE_DIR/mcp-bridge-indesign.idjs"
CODEX_CONFIG_INSTALLER="/usr/local/share/ae-mcp/install-codex-mcp-config.sh"
if [[ ! -f "$SOURCE_SCRIPT" ]]; then
  echo "Bridge runtime source not found: $SOURCE_SCRIPT"
  exit 0
fi
if [[ ! -f "$SOURCE_STARTUP_SCRIPT" ]]; then
  echo "Bridge startup source not found: $SOURCE_STARTUP_SCRIPT"
  exit 0
fi
if [[ ! -f "$SOURCE_SHUTDOWN_SCRIPT" ]]; then
  echo "Bridge shutdown source not found: $SOURCE_SHUTDOWN_SCRIPT"
  exit 0
fi

installed=0
for ae_path in /Applications/Adobe\ After\ Effects\ *; do
  [[ -d "$ae_path" ]] || continue
  ae_name="$(basename "$ae_path")"
  [[ "$ae_name" =~ ^Adobe\ After\ Effects\ [0-9]{4}$ ]] || continue

  dest_dir="$ae_path/Scripts/ScriptUI Panels"
  startup_dir="$ae_path/Scripts/Startup"
  shutdown_dir="$ae_path/Scripts/Shutdown"
  mkdir -p "$dest_dir"
  mkdir -p "$startup_dir"
  mkdir -p "$shutdown_dir"
  cp "$SOURCE_SCRIPT" "$dest_dir/mcp-bridge-auto.jsx"
  cp "$SOURCE_STARTUP_SCRIPT" "$startup_dir/mcp-bridge-startup.jsx"
  cp "$SOURCE_SHUTDOWN_SCRIPT" "$shutdown_dir/mcp-bridge-shutdown.jsx"
  echo "Installed bridge runtime: $dest_dir/mcp-bridge-auto.jsx"
  echo "Installed startup bootstrap: $startup_dir/mcp-bridge-startup.jsx"
  echo "Installed shutdown cleanup: $shutdown_dir/mcp-bridge-shutdown.jsx"
  installed=$((installed + 1))
done

if [[ "$installed" -eq 0 ]]; then
  echo "No After Effects installation found. Bridge panel deployment skipped."
else
  echo "Headless bridge deployment completed for $installed installation(s)."
fi

premiere_installed=0
for pr_path in /Applications/Adobe\ Premiere\ Pro\ *; do
  [[ -d "$pr_path" ]] || continue
  pr_name="$(basename "$pr_path")"
  [[ "$pr_name" =~ ^Adobe\ Premiere\ Pro\ [0-9]{4}$ ]] || continue
  premiere_installed=$((premiere_installed + 1))
done

if [[ "$premiere_installed" -eq 0 ]]; then
  echo "No Adobe Premiere Pro installation found. Premiere bridge deployment skipped."
else
  if [[ -d "$PREMIERE_CEP_SOURCE" ]]; then
    CEP_ROOT="/Library/Application Support/Adobe/CEP/extensions"
    mkdir -p "$CEP_ROOT"
    rm -rf "$CEP_ROOT/mcp-bridge-premiere"
    cp -R "$PREMIERE_CEP_SOURCE" "$CEP_ROOT/mcp-bridge-premiere"
    echo "Premiere bridge installed: $CEP_ROOT/mcp-bridge-premiere"
  else
    echo "Premiere CEP source not found: $PREMIERE_CEP_SOURCE"
  fi

  if [[ -f "$PREMIERE_UXP_MANIFEST" ]]; then
    echo "Premiere UXP bridge bundled. Load with Adobe UXP Developer Tool: $PREMIERE_UXP_MANIFEST"
  else
    echo "Premiere UXP manifest not found: $PREMIERE_UXP_MANIFEST"
  fi
fi

photoshop_installed=0
for ps_path in /Applications/Adobe\ Photoshop\ *; do
  [[ -d "$ps_path" ]] || continue
  ps_name="$(basename "$ps_path")"
  [[ "$ps_name" =~ ^Adobe\ Photoshop\ [0-9]{4}$ ]] || continue
  photoshop_installed=$((photoshop_installed + 1))
done

if [[ "$photoshop_installed" -eq 0 ]]; then
  echo "No Adobe Photoshop installation found. Photoshop UXP bridge deployment skipped."
elif [[ -f "$PHOTOSHOP_UXP_MANIFEST" ]]; then
  echo "Photoshop UXP bridge bundled. Load with Adobe UXP Developer Tool: $PHOTOSHOP_UXP_MANIFEST"
else
  echo "Photoshop UXP manifest not found: $PHOTOSHOP_UXP_MANIFEST"
fi

illustrator_installed=0
for ai_path in /Applications/Adobe\ Illustrator\ *; do
  [[ -d "$ai_path" ]] || continue
  ai_name="$(basename "$ai_path")"
  [[ "$ai_name" =~ ^Adobe\ Illustrator\ [0-9]{4}$ ]] || continue
  illustrator_installed=$((illustrator_installed + 1))
done

if [[ "$illustrator_installed" -eq 0 ]]; then
  echo "No Adobe Illustrator installation found. Illustrator CEP bridge deployment skipped."
elif [[ -d "$ILLUSTRATOR_CEP_SOURCE" ]]; then
  CEP_ROOT="/Library/Application Support/Adobe/CEP/extensions"
  mkdir -p "$CEP_ROOT"
  rm -rf "$CEP_ROOT/mcp-bridge-illustrator"
  cp -R "$ILLUSTRATOR_CEP_SOURCE" "$CEP_ROOT/mcp-bridge-illustrator"
  echo "Illustrator CEP bridge installed: $CEP_ROOT/mcp-bridge-illustrator"
else
  echo "Illustrator CEP source not found: $ILLUSTRATOR_CEP_SOURCE"
fi

indesign_installed=0
if [[ -f "$INDESIGN_SOURCE" ]]; then
  for id_path in /Applications/Adobe\ InDesign\ *; do
    [[ -d "$id_path" ]] || continue
    id_name="$(basename "$id_path")"
    [[ "$id_name" =~ ^Adobe\ InDesign\ [0-9]{4}$ ]] || continue

    indesign_dest="$id_path/Scripts/Startup Scripts"
    mkdir -p "$indesign_dest"
    cp "$INDESIGN_SOURCE" "$indesign_dest/mcp-bridge-indesign.idjs"
    echo "InDesign startup bridge installed: $indesign_dest/mcp-bridge-indesign.idjs"
    indesign_installed=$((indesign_installed + 1))
  done
  if [[ "$indesign_installed" -eq 0 ]]; then
    echo "No Adobe InDesign installation found. InDesign bridge deployment skipped."
  fi
else
  echo "InDesign Startup Script source not found: $INDESIGN_SOURCE"
fi

console_user="$(/usr/bin/stat -f '%Su' /dev/console 2>/dev/null || true)"
case "$console_user" in
  ""|root|loginwindow|_mbsetupuser)
    echo "No eligible console user found. Codex MCP config update skipped."
    ;;
  *)
    console_home="$(/usr/bin/dscl . -read "/Users/$console_user" NFSHomeDirectory 2>/dev/null | /usr/bin/awk '{ $1 = ""; sub(/^ /, ""); print }' || true)"
    if [[ -z "$console_home" || ! -d "$console_home" ]]; then
      echo "Home directory for console user '$console_user' was not found. Codex MCP config update skipped."
    elif [[ ! -x "$CODEX_CONFIG_INSTALLER" ]]; then
      echo "Codex MCP config installer not found or not executable: $CODEX_CONFIG_INSTALLER"
    else
      codex_config="$console_home/.codex/config.toml"
      echo "Updating missing Codex MCP entries for console user: $console_user"
      if ! /usr/bin/sudo -H -u "$console_user" /usr/bin/env HOME="$console_home" \
          "$CODEX_CONFIG_INSTALLER" \
          --binary-dir "/usr/local/bin" \
          --config "$codex_config"; then
        echo "Codex MCP config update failed for console user '$console_user'. Installed binaries were left intact."
      fi
    fi
    ;;
esac
POSTINSTALL
chmod +x "$PKG_SCRIPTS_DIR/postinstall"

pkgbuild \
  --root "$PKG_ROOT" \
  --scripts "$PKG_SCRIPTS_DIR" \
  --identifier "io.github.aodaruma.adobe-mcp-rs" \
  --version "0.5.0" \
  --install-location "/" \
  "$PKG_PATH"

echo "Created package: $PKG_PATH"
popd >/dev/null
