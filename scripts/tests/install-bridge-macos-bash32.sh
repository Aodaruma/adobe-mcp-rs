#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
INSTALLER="$REPO_ROOT/scripts/install-bridge.sh"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/adobe-mcp-install-bridge.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_contains() {
  local expected="$1"
  local file="$2"
  grep -F -- "$expected" "$file" >/dev/null || fail "missing '$expected' in $file"
}

assert_file_matches() {
  local expected="$1"
  local actual="$2"
  [[ -f "$actual" ]] || fail "missing file: $actual"
  cmp -s "$expected" "$actual" || fail "file differs from source: $actual"
}

run_installer() {
  local applications_dir="$1"
  local home_dir="$2"
  local codex_home="$3"
  local output="$4"
  shift 4

  HOME="$home_dir" \
    CODEX_HOME="$codex_home" \
    ADOBE_MCP_APPLICATIONS_DIR="$applications_dir" \
    ADOBE_MCP_CEP_ROOT="$home_dir/Library/Application Support/Adobe/CEP/extensions" \
    /bin/bash "$INSTALLER" "$@" >"$output" 2>&1
}

mkdir -p "$TEST_ROOT/explicit-apps" "$TEST_ROOT/explicit-home"
explicit_ae="$TEST_ROOT/Adobe After Effects Explicit"
mkdir -p "$explicit_ae"
run_installer \
  "$TEST_ROOT/explicit-apps" \
  "$TEST_ROOT/explicit-home" \
  "$TEST_ROOT/explicit-codex" \
  "$TEST_ROOT/explicit-dry-run.log" \
  --ae-path "$explicit_ae" --dry-run

assert_contains "$explicit_ae/Scripts/Startup/mcp-bridge-startup.jsx" "$TEST_ROOT/explicit-dry-run.log"
assert_contains "Dry-run mode: no copy executed." "$TEST_ROOT/explicit-dry-run.log"
[[ ! -e "$explicit_ae/Scripts" ]] || fail "dry-run created an After Effects Scripts directory"
[[ ! -e "$TEST_ROOT/explicit-codex" ]] || fail "dry-run changed the Codex config directory"

auto_apps="$TEST_ROOT/auto-apps"
auto_home="$TEST_ROOT/auto-home"
mkdir -p \
  "$auto_apps/Adobe After Effects 2026" \
  "$auto_apps/Adobe After Effects 2024" \
  "$auto_apps/Adobe After Effects Beta" \
  "$auto_apps/Adobe Premiere Pro 2026" \
  "$auto_apps/Adobe Premiere Pro 2024" \
  "$auto_apps/Adobe Premiere Pro Beta" \
  "$auto_home"

run_installer \
  "$auto_apps" \
  "$auto_home" \
  "$TEST_ROOT/auto-codex" \
  "$TEST_ROOT/auto-install.log"

assert_contains "Bridge script installed to 2 location(s)." "$TEST_ROOT/auto-install.log"
assert_contains "Detected Adobe Premiere Pro installations: 2" "$TEST_ROOT/auto-install.log"

for year in 2026 2024; do
  ae_root="$auto_apps/Adobe After Effects $year/Scripts"
  assert_file_matches \
    "$REPO_ROOT/src/scripts/mcp-bridge-auto.jsx" \
    "$ae_root/ScriptUI Panels/mcp-bridge-auto.jsx"
  assert_file_matches \
    "$REPO_ROOT/src/scripts/mcp-bridge-startup.jsx" \
    "$ae_root/Startup/mcp-bridge-startup.jsx"
  assert_file_matches \
    "$REPO_ROOT/src/scripts/mcp-bridge-shutdown.jsx" \
    "$ae_root/Shutdown/mcp-bridge-shutdown.jsx"
done

[[ ! -e "$auto_apps/Adobe After Effects Beta/Scripts" ]] || fail "invalid AE directory was installed"
assert_file_matches \
  "$REPO_ROOT/src/premiere/cep/mcp-bridge-premiere/CSXS/manifest.xml" \
  "$auto_home/Library/Application Support/Adobe/CEP/extensions/mcp-bridge-premiere/CSXS/manifest.xml"

# On-demand mode: installs the CEP companion, skips the Startup loader and moves an existing one aside.
export ADOBE_MCP_SKIP_CSXS_DEBUG=1
od_apps="$TEST_ROOT/od-apps"
od_home="$TEST_ROOT/od-home"
od_ae="$od_apps/Adobe After Effects 2026"
mkdir -p "$od_ae/Scripts/Startup" "$od_home"
echo "old timer loader" > "$od_ae/Scripts/Startup/mcp-bridge-startup.jsx"
run_installer \
  "$od_apps" \
  "$od_home" \
  "$TEST_ROOT/od-codex" \
  "$TEST_ROOT/od-install.log" \
  --on-demand

assert_contains "On-demand CEP companion installed" "$TEST_ROOT/od-install.log"
assert_file_matches \
  "$REPO_ROOT/src/scripts/mcp-bridge-auto.jsx" \
  "$od_ae/Scripts/ScriptUI Panels/mcp-bridge-auto.jsx"
assert_file_matches \
  "$REPO_ROOT/src/aftereffects/cep/mcp-bridge-ae-ondemand/js/main.js" \
  "$od_home/Library/Application Support/Adobe/CEP/extensions/mcp-bridge-ae-ondemand/js/main.js"
[[ ! -e "$od_ae/Scripts/Startup/mcp-bridge-startup.jsx" ]] || fail "on-demand install left the Startup loader in place"
[[ -f "$od_ae/Scripts/Startup_backup/mcp-bridge-startup.jsx" ]] || fail "existing Startup loader was not preserved in Startup_backup"
unset ADOBE_MCP_SKIP_CSXS_DEBUG

mkdir -p "$TEST_ROOT/empty-apps" "$TEST_ROOT/empty-home"
run_installer \
  "$TEST_ROOT/empty-apps" \
  "$TEST_ROOT/empty-home" \
  "$TEST_ROOT/empty-codex" \
  "$TEST_ROOT/empty-install.log"

assert_contains "After Effects path not found. Skipping AE bridge install." "$TEST_ROOT/empty-install.log"
assert_contains "No Adobe Premiere Pro installation detected. Skipped Premiere bridge install." "$TEST_ROOT/empty-install.log"
assert_contains "No existing InDesign preference profile was detected. Skipped InDesign deployment." "$TEST_ROOT/empty-install.log"

echo "install-bridge.sh macOS Bash regression tests passed."
