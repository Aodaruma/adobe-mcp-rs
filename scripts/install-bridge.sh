#!/usr/bin/env bash
set -euo pipefail

AE_PATH=""
DRY_RUN="false"
AE_PATHS=()
AE_PATH_COUNT=0
PREMIERE_PATHS=()
PREMIERE_PATH_COUNT=0
# Used by the isolated regression test; normal installs keep the macOS defaults.
APPLICATIONS_DIR="${ADOBE_MCP_APPLICATIONS_DIR:-/Applications}"

add_unique_path() {
  local candidate="$1"
  local existing
  if [[ "$AE_PATH_COUNT" -gt 0 ]]; then
    for existing in "${AE_PATHS[@]}"; do
      if [[ "$existing" == "$candidate" ]]; then
        return
      fi
    done
  fi
  AE_PATHS+=("$candidate")
  AE_PATH_COUNT=$((AE_PATH_COUNT + 1))
}

add_unique_premiere_path() {
  local candidate="$1"
  local existing
  if [[ "$PREMIERE_PATH_COUNT" -gt 0 ]]; then
    for existing in "${PREMIERE_PATHS[@]}"; do
      if [[ "$existing" == "$candidate" ]]; then
        return
      fi
    done
  fi
  PREMIERE_PATHS+=("$candidate")
  PREMIERE_PATH_COUNT=$((PREMIERE_PATH_COUNT + 1))
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ae-path)
      AE_PATH="${2:-}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN="true"
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: $0 [--ae-path <path>] [--dry-run]" >&2
      echo "If --ae-path is omitted, installs to all detected After Effects versions." >&2
      exit 1
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_SCRIPT="$REPO_ROOT/src/scripts/mcp-bridge-auto.jsx"
SOURCE_STARTUP_SCRIPT="$REPO_ROOT/src/scripts/mcp-bridge-startup.jsx"
SOURCE_SHUTDOWN_SCRIPT="$REPO_ROOT/src/scripts/mcp-bridge-shutdown.jsx"
CODEX_CONFIG_INSTALLER="$SCRIPT_DIR/install-codex-mcp-config.sh"

if [[ ! -f "$SOURCE_SCRIPT" ]]; then
  echo "Bridge script not found: $SOURCE_SCRIPT" >&2
  exit 1
fi
if [[ ! -f "$SOURCE_STARTUP_SCRIPT" ]]; then
  echo "Bridge startup script not found: $SOURCE_STARTUP_SCRIPT" >&2
  exit 1
fi
if [[ ! -f "$SOURCE_SHUTDOWN_SCRIPT" ]]; then
  echo "Bridge shutdown script not found: $SOURCE_SHUTDOWN_SCRIPT" >&2
  exit 1
fi

if [[ -n "$AE_PATH" ]]; then
  if [[ ! -d "$AE_PATH" ]]; then
    echo "After Effects path not found: $AE_PATH" >&2
    exit 1
  fi
  add_unique_path "$AE_PATH"
else
  CANDIDATES=(
    "$APPLICATIONS_DIR/Adobe After Effects 2030"
    "$APPLICATIONS_DIR/Adobe After Effects 2029"
    "$APPLICATIONS_DIR/Adobe After Effects 2028"
    "$APPLICATIONS_DIR/Adobe After Effects 2027"
    "$APPLICATIONS_DIR/Adobe After Effects 2026"
    "$APPLICATIONS_DIR/Adobe After Effects 2025"
    "$APPLICATIONS_DIR/Adobe After Effects 2024"
    "$APPLICATIONS_DIR/Adobe After Effects 2023"
    "$APPLICATIONS_DIR/Adobe After Effects 2022"
    "$APPLICATIONS_DIR/Adobe After Effects 2021"
  )

  for path in "${CANDIDATES[@]}"; do
    if [[ -d "$path" ]]; then
      add_unique_path "$path"
    fi
  done

  while IFS= read -r path; do
    case "${path##*/}" in
      Adobe\ After\ Effects\ [0-9][0-9][0-9][0-9])
        add_unique_path "$path"
        ;;
    esac
  done < <(find "$APPLICATIONS_DIR" -maxdepth 1 -type d -name "Adobe After Effects *" 2>/dev/null | sort -r)
fi

if [[ "$AE_PATH_COUNT" -eq 0 ]]; then
  echo "After Effects path not found. Skipping AE bridge install."
else
  echo "Source      : $SOURCE_SCRIPT"
  echo "Destinations:"
  for ae in "${AE_PATHS[@]}"; do
    echo "  - $ae/Scripts/ScriptUI Panels/mcp-bridge-auto.jsx"
    echo "  - $ae/Scripts/Startup/mcp-bridge-startup.jsx"
    echo "  - $ae/Scripts/Shutdown/mcp-bridge-shutdown.jsx"
  done
fi

if [[ "$DRY_RUN" == "true" ]]; then
  if [[ -f "$CODEX_CONFIG_INSTALLER" ]]; then
    bash "$CODEX_CONFIG_INSTALLER" --binary-dir "$REPO_ROOT/target/release" --dry-run
  else
    echo "Codex MCP config installer not found or not executable: $CODEX_CONFIG_INSTALLER"
  fi
  echo "Dry-run mode: no copy executed."
  exit 0
fi

if [[ "$AE_PATH_COUNT" -gt 0 ]]; then
  for ae in "${AE_PATHS[@]}"; do
    DEST_DIR="$ae/Scripts/ScriptUI Panels"
    DEST_FILE="$DEST_DIR/mcp-bridge-auto.jsx"
    STARTUP_DIR="$ae/Scripts/Startup"
    STARTUP_FILE="$STARTUP_DIR/mcp-bridge-startup.jsx"
    SHUTDOWN_DIR="$ae/Scripts/Shutdown"
    SHUTDOWN_FILE="$SHUTDOWN_DIR/mcp-bridge-shutdown.jsx"

    if [[ -w "$ae" || ( -d "$DEST_DIR" && -w "$DEST_DIR" && -d "$STARTUP_DIR" && -w "$STARTUP_DIR" && -d "$SHUTDOWN_DIR" && -w "$SHUTDOWN_DIR" ) ]]; then
      mkdir -p "$DEST_DIR"
      mkdir -p "$STARTUP_DIR"
      mkdir -p "$SHUTDOWN_DIR"
      cp "$SOURCE_SCRIPT" "$DEST_FILE"
      cp "$SOURCE_STARTUP_SCRIPT" "$STARTUP_FILE"
      cp "$SOURCE_SHUTDOWN_SCRIPT" "$SHUTDOWN_FILE"
    else
      echo "Destination may require sudo. Installing with sudo for: $ae"
      sudo mkdir -p "$DEST_DIR"
      sudo mkdir -p "$STARTUP_DIR"
      sudo mkdir -p "$SHUTDOWN_DIR"
      sudo cp "$SOURCE_SCRIPT" "$DEST_FILE"
      sudo cp "$SOURCE_STARTUP_SCRIPT" "$STARTUP_FILE"
      sudo cp "$SOURCE_SHUTDOWN_SCRIPT" "$SHUTDOWN_FILE"
    fi
  done
fi

if [[ "$AE_PATH_COUNT" -gt 0 ]]; then
  echo
  echo "Bridge script installed to $AE_PATH_COUNT location(s)."
  for ae in "${AE_PATHS[@]}"; do
    echo "  - $ae/Scripts/ScriptUI Panels/mcp-bridge-auto.jsx"
    echo "  - $ae/Scripts/Startup/mcp-bridge-startup.jsx"
    echo "  - $ae/Scripts/Shutdown/mcp-bridge-shutdown.jsx"
  done
  echo "Next steps:"
  echo "1. Open After Effects"
  echo "2. After Effects > Settings > Scripting & Expressions"
  echo "3. Enable \"Allow Scripts to Write Files and Access Network\""
  echo "4. Restart After Effects"
  echo "5. The MCP bridge starts headlessly; no panel or Auto-run checkbox is required"
fi

PREMIERE_SOURCE="$REPO_ROOT/src/premiere/cep/mcp-bridge-premiere"
PREMIERE_UXP_MANIFEST="$REPO_ROOT/src/premiere/uxp/mcp-bridge-premiere/manifest.json"
if [[ -d "$PREMIERE_SOURCE" ]]; then
  PREMIERE_CANDIDATES=(
    "$APPLICATIONS_DIR/Adobe Premiere Pro 2030"
    "$APPLICATIONS_DIR/Adobe Premiere Pro 2029"
    "$APPLICATIONS_DIR/Adobe Premiere Pro 2028"
    "$APPLICATIONS_DIR/Adobe Premiere Pro 2027"
    "$APPLICATIONS_DIR/Adobe Premiere Pro 2026"
    "$APPLICATIONS_DIR/Adobe Premiere Pro 2025"
    "$APPLICATIONS_DIR/Adobe Premiere Pro 2024"
  )

  for path in "${PREMIERE_CANDIDATES[@]}"; do
    if [[ -d "$path" ]]; then
      add_unique_premiere_path "$path"
    fi
  done

  while IFS= read -r path; do
    case "${path##*/}" in
      Adobe\ Premiere\ Pro\ [0-9][0-9][0-9][0-9])
        add_unique_premiere_path "$path"
        ;;
    esac
  done < <(find "$APPLICATIONS_DIR" -maxdepth 1 -type d -name "Adobe Premiere Pro *" 2>/dev/null | sort -r)

  if [[ "$PREMIERE_PATH_COUNT" -eq 0 ]]; then
    echo
    echo "No Adobe Premiere Pro installation detected. Skipped Premiere bridge install."
  else
    echo
    echo "Detected Adobe Premiere Pro installations: $PREMIERE_PATH_COUNT"
    if [[ -n "${ADOBE_MCP_CEP_ROOT:-}" ]]; then
      CEP_ROOT="$ADOBE_MCP_CEP_ROOT"
    elif [[ "$(id -u)" -eq 0 ]]; then
      CEP_ROOT="/Library/Application Support/Adobe/CEP/extensions"
    else
      CEP_ROOT="$HOME/Library/Application Support/Adobe/CEP/extensions"
    fi
    PREMIERE_DEST="$CEP_ROOT/mcp-bridge-premiere"
    mkdir -p "$CEP_ROOT"
    rm -rf "$PREMIERE_DEST"
    cp -R "$PREMIERE_SOURCE" "$PREMIERE_DEST"
    echo
    echo "Premiere bridge installed: $PREMIERE_DEST"
    echo "Next steps (Premiere Pro):"
    echo "1. Open Adobe Premiere Pro"
    if [[ -f "$PREMIERE_UXP_MANIFEST" ]]; then
      echo "2. Load the UXP plugin with Adobe UXP Developer Tool:"
      echo "   $PREMIERE_UXP_MANIFEST"
      echo "3. Window > UXP Plugins > Premiere MCP Bridge"
      echo "4. Enable Auto-run commands"
      echo "Legacy CEP fallback is also installed:"
      echo "   Window > Extensions > Premiere MCP Bridge"
    else
      echo "2. Window > Extensions > Premiere MCP Bridge"
      echo "3. Enable Auto-run commands"
    fi
  fi
fi

INDESIGN_SOURCE="$REPO_ROOT/src/indesign/uxp/mcp-bridge-indesign.idjs"
INDESIGN_PREFERENCE_ROOT="$HOME/Library/Preferences/Adobe InDesign"
INDESIGN_TARGETS=()
INDESIGN_TARGET_COUNT=0
if [[ -f "$INDESIGN_SOURCE" && -d "$INDESIGN_PREFERENCE_ROOT" ]]; then
  while IFS= read -r locale_dir; do
    INDESIGN_TARGETS+=("$locale_dir/Scripts/Startup Scripts")
    INDESIGN_TARGET_COUNT=$((INDESIGN_TARGET_COUNT + 1))
  done < <(find "$INDESIGN_PREFERENCE_ROOT" -mindepth 2 -maxdepth 2 -type d -path '*/Version */??_??' 2>/dev/null | sort)
fi

echo
if [[ ! -f "$INDESIGN_SOURCE" ]]; then
  echo "InDesign UXP startup bridge source not found. Skipped InDesign deployment."
elif [[ "$INDESIGN_TARGET_COUNT" -eq 0 ]]; then
  echo "No existing InDesign preference profile was detected. Skipped InDesign deployment."
  echo "See docs/setup-codex-mcp.md for the manual Startup Scripts path."
else
  for target in "${INDESIGN_TARGETS[@]}"; do
    destination="$target/mcp-bridge-indesign.idjs"
    if [[ "$DRY_RUN" == "true" ]]; then
      echo "Dry-run: InDesign bridge would be installed to $destination"
    else
      mkdir -p "$target"
      cp "$INDESIGN_SOURCE" "$destination"
      echo "InDesign startup bridge installed: $destination"
    fi
  done
  echo "Restart InDesign; no panel or Auto-run toggle is required."
fi

echo
if [[ -f "$CODEX_CONFIG_INSTALLER" ]]; then
  bash "$CODEX_CONFIG_INSTALLER" --binary-dir "$REPO_ROOT/target/release"
else
  echo "Codex MCP config installer not found or not executable: $CODEX_CONFIG_INSTALLER"
fi
