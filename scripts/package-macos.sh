#!/usr/bin/env bash
set -euo pipefail

OUTPUT_DIR="${1:-./dist/macos}"
REQUIRE_PKG="${REQUIRE_PKG:-false}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

mkdir -p "$OUTPUT_DIR"

pushd "$REPO_ROOT" >/dev/null

echo "Building release binaries..."
cargo build --release -p ae-mcp -p pr-mcp

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
BRIDGE_PANEL_PATH="$REPO_ROOT/src/scripts/mcp-bridge-auto.jsx"
if [[ ! -f "$BRIDGE_PANEL_PATH" ]]; then
  echo "Bridge panel script not found: $BRIDGE_PANEL_PATH" >&2
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

STAGE_DIR="$OUTPUT_DIR/stage"
mkdir -p "$STAGE_DIR"
cp "$BIN_PATH_AE" "$STAGE_DIR/ae-mcp"
chmod +x "$STAGE_DIR/ae-mcp"
cp "$BIN_PATH_PR" "$STAGE_DIR/pr-mcp"
chmod +x "$STAGE_DIR/pr-mcp"
cp "$BRIDGE_PANEL_PATH" "$STAGE_DIR/mcp-bridge-auto.jsx"
mkdir -p "$STAGE_DIR/premiere-cep"
cp -R "$PREMIERE_CEP_PATH" "$STAGE_DIR/premiere-cep/mcp-bridge-premiere"
mkdir -p "$STAGE_DIR/premiere-uxp"
cp -R "$PREMIERE_UXP_PATH" "$STAGE_DIR/premiere-uxp/mcp-bridge-premiere"

ARCHIVE_PATH="$OUTPUT_DIR/after-effects-mcp-rs-macos-universal.tar.gz"
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
cp "$STAGE_DIR/mcp-bridge-auto.jsx" "$INSTALL_SHARE_DIR/mcp-bridge-auto.jsx"
mkdir -p "$INSTALL_SHARE_DIR/premiere-cep"
cp -R "$STAGE_DIR/premiere-cep/mcp-bridge-premiere" "$INSTALL_SHARE_DIR/premiere-cep/mcp-bridge-premiere"
mkdir -p "$INSTALL_SHARE_DIR/premiere-uxp"
cp -R "$STAGE_DIR/premiere-uxp/mcp-bridge-premiere" "$INSTALL_SHARE_DIR/premiere-uxp/mcp-bridge-premiere"

PKG_PATH="$OUTPUT_DIR/after-effects-mcp-rs-macos-universal.pkg"
PKG_SCRIPTS_DIR="$OUTPUT_DIR/pkgscripts"
mkdir -p "$PKG_SCRIPTS_DIR"
cat > "$PKG_SCRIPTS_DIR/postinstall" <<'POSTINSTALL'
#!/usr/bin/env bash
set -euo pipefail

SOURCE_SCRIPT="/usr/local/share/ae-mcp/mcp-bridge-auto.jsx"
PREMIERE_CEP_SOURCE="/usr/local/share/ae-mcp/premiere-cep/mcp-bridge-premiere"
PREMIERE_UXP_MANIFEST="/usr/local/share/ae-mcp/premiere-uxp/mcp-bridge-premiere/manifest.json"
if [[ ! -f "$SOURCE_SCRIPT" ]]; then
  echo "Bridge panel source not found: $SOURCE_SCRIPT"
  exit 0
fi

installed=0
for ae_path in /Applications/Adobe\ After\ Effects\ *; do
  [[ -d "$ae_path" ]] || continue
  ae_name="$(basename "$ae_path")"
  [[ "$ae_name" =~ ^Adobe\ After\ Effects\ [0-9]{4}$ ]] || continue

  dest_dir="$ae_path/Scripts/ScriptUI Panels"
  mkdir -p "$dest_dir"
  cp "$SOURCE_SCRIPT" "$dest_dir/mcp-bridge-auto.jsx"
  echo "Installed bridge panel: $dest_dir/mcp-bridge-auto.jsx"
  installed=$((installed + 1))
done

if [[ "$installed" -eq 0 ]]; then
  echo "No After Effects installation found. Bridge panel deployment skipped."
else
  echo "Bridge panel deployment completed for $installed installation(s)."
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
  exit 0
fi

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
POSTINSTALL
chmod +x "$PKG_SCRIPTS_DIR/postinstall"

pkgbuild \
  --root "$PKG_ROOT" \
  --scripts "$PKG_SCRIPTS_DIR" \
  --identifier "io.github.aodaruma.after-effects-mcp-rs" \
  --version "0.2.0" \
  --install-location "/" \
  "$PKG_PATH"

echo "Created package: $PKG_PATH"
popd >/dev/null
