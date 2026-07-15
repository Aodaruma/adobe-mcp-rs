#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BINARY_DIR=""
CONFIG_PATH="${CODEX_HOME:-$HOME/.codex}/config.toml"
DRY_RUN="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --binary-dir)
      BINARY_DIR="${2:-}"
      shift 2
      ;;
    --config)
      CONFIG_PATH="${2:-}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN="true"
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: $0 [--binary-dir <path>] [--config <path>] [--dry-run]" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$BINARY_DIR" ]]; then
  if [[ -x "$SCRIPT_DIR/ae-mcp" || -x "$SCRIPT_DIR/id-mcp" ]]; then
    BINARY_DIR="$SCRIPT_DIR"
  elif [[ -x "$REPO_ROOT/target/release/ae-mcp" || -x "$REPO_ROOT/target/release/id-mcp" ]]; then
    BINARY_DIR="$REPO_ROOT/target/release"
  else
    BINARY_DIR="/usr/local/bin"
  fi
fi

section_exists() {
  local section="$1"
  local config="$2"
  [[ -f "$config" ]] || return 1
  awk -v expected="[$section]" '
    {
      line = $0
      sub(/[[:space:]]*#.*/, "", line)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
      if (line == expected) {
        found = 1
        exit
      }
    }
    END { exit(found ? 0 : 1) }
  ' "$config"
}

toml_quote() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '"%s"' "$value"
}

append_server() {
  local section="$1"
  local binary="$2"
  local quoted_binary
  quoted_binary="$(toml_quote "$binary")"

  if [[ -s "$CONFIG_PATH" ]]; then
    printf '\n' >> "$CONFIG_PATH"
  fi
  printf '[mcp_servers.%s]\n' "$section" >> "$CONFIG_PATH"
  printf 'command = %s\n' "$quoted_binary" >> "$CONFIG_PATH"
  printf 'args = ["serve-stdio"]\n' >> "$CONFIG_PATH"
  printf 'startup_timeout_sec = 180\n' >> "$CONFIG_PATH"
  printf 'tool_timeout_sec = 180\n' >> "$CONFIG_PATH"
}

declare -a SERVER_NAMES=("aftereffects" "indesign")
declare -a BINARY_NAMES=("ae-mcp" "id-mcp")
added=0

for index in "${!SERVER_NAMES[@]}"; do
  server="${SERVER_NAMES[$index]}"
  binary="$BINARY_DIR/${BINARY_NAMES[$index]}"
  section="mcp_servers.$server"

  if section_exists "$section" "$CONFIG_PATH"; then
    echo "Codex MCP config already contains [$section]; skipped without changes."
    continue
  fi
  if [[ ! -x "$binary" ]]; then
    echo "Codex MCP binary not found or not executable; skipped [$section]: $binary"
    continue
  fi
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "Dry-run: would append [$section] with command $binary to $CONFIG_PATH"
    continue
  fi

  if [[ ! -e "$CONFIG_PATH" ]]; then
    mkdir -p "$(dirname "$CONFIG_PATH")"
    : > "$CONFIG_PATH"
  fi
  append_server "$server" "$binary"
  echo "Added [$section] to $CONFIG_PATH"
  added=$((added + 1))
done

if [[ "$DRY_RUN" != "true" && "$added" -gt 0 ]]; then
  echo "Codex MCP config updated with $added missing server(s)."
fi
