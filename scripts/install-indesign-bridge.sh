#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE="$REPO_ROOT/src/indesign/uxp/mcp-bridge-indesign.idjs"
DESTINATIONS=()
DRY_RUN="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      SOURCE="$2"
      shift 2
      ;;
    --destination)
      DESTINATIONS+=("$2")
      shift 2
      ;;
    --dry-run)
      DRY_RUN="true"
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: $0 [--source <idjs>] [--destination <Startup Scripts>] [--dry-run]" >&2
      exit 1
      ;;
  esac
done

if [[ ! -f "$SOURCE" ]]; then
  echo "InDesign bridge source not found: $SOURCE" >&2
  exit 1
fi

if [[ "${#DESTINATIONS[@]}" -eq 0 ]]; then
  PREFERENCE_ROOT="$HOME/Library/Preferences/Adobe InDesign"
  if [[ -d "$PREFERENCE_ROOT" ]]; then
    while IFS= read -r locale_dir; do
      DESTINATIONS+=("$locale_dir/Scripts/Startup Scripts")
    done < <(find "$PREFERENCE_ROOT" -mindepth 2 -maxdepth 2 -type d -path '*/Version */??_??' 2>/dev/null | sort)
  fi
fi

if [[ "${#DESTINATIONS[@]}" -eq 0 ]]; then
  echo "No InDesign preference profile found." >&2
  echo "Pass --destination, for example the application Scripts/Startup Scripts folder." >&2
  exit 1
fi

for target in "${DESTINATIONS[@]}"; do
  destination="$target/mcp-bridge-indesign.idjs"
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "Would install: $destination"
    continue
  fi
  mkdir -p "$target"
  cp "$SOURCE" "$destination"
  echo "Installed: $destination"
done

echo "Restart InDesign, then verify list-indesign-instances and run-bridge-test."
