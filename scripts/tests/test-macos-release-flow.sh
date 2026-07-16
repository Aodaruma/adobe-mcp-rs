#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/adobe-mcp-macos-release-test.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT

fail() {
  echo "macOS release flow test failed: $*" >&2
  exit 1
}

assert_log_contains() {
  local expected="$1"
  if ! grep -Fq -- "$expected" "$FAKE_MACOS_TOOL_LOG"; then
    fail "missing log entry: $expected"
  fi
}

FAKE_BIN="$TEST_ROOT/bin"
FAKE_MACOS_TOOL_LOG="$TEST_ROOT/tool.log"
export FAKE_MACOS_TOOL_LOG
mkdir -p "$FAKE_BIN"
touch "$FAKE_MACOS_TOOL_LOG"
for tool in codesign productbuild pkgutil xcrun spctl; do
  ln -s "$SCRIPT_DIR/fake-macos-tool.sh" "$FAKE_BIN/$tool"
done
PATH="$FAKE_BIN:$PATH"
export PATH

# shellcheck source=../macos-release-tools.sh
source "$REPO_ROOT/scripts/macos-release-tools.sh"

APPLICATION_IDENTITY="Developer ID Application: Example Org (TEAMID1234)"
INSTALLER_IDENTITY="Developer ID Installer: Example Org (TEAMID1234)"
FAKE_APPLICATION_IDENTITY="$APPLICATION_IDENTITY"
FAKE_INSTALLER_IDENTITY="$INSTALLER_IDENTITY"
export FAKE_APPLICATION_IDENTITY FAKE_INSTALLER_IDENTITY
STAGE_DIR="$TEST_ROOT/stage"
mkdir -p "$STAGE_DIR"
for binary in "${MACOS_RELEASE_BINARIES[@]}"; do
  touch "$STAGE_DIR/$binary"
done

macos_validate_release_identities "$APPLICATION_IDENTITY" "$INSTALLER_IDENTITY"
macos_sign_application_binaries "$STAGE_DIR" "$APPLICATION_IDENTITY"

codesign_calls="$(grep -c '^codesign' "$FAKE_MACOS_TOOL_LOG")"
[[ "$codesign_calls" -eq 15 ]] || fail "expected 15 codesign calls, got $codesign_calls"
for binary in "${MACOS_RELEASE_BINARIES[@]}"; do
  assert_log_contains $'codesign\t--force\t--timestamp\t--options\truntime\t--sign\t'"$APPLICATION_IDENTITY"$'\t'"$STAGE_DIR/$binary"
  assert_log_contains $'codesign\t--verify\t--strict\t--verbose=2\t'"$STAGE_DIR/$binary"
  assert_log_contains $'codesign\t--display\t--verbose=4\t'"$STAGE_DIR/$binary"
done

COMPONENT_PACKAGE="$TEST_ROOT/component.pkg"
SIGNED_PACKAGE="$TEST_ROOT/signed.pkg"
UNSIGNED_PACKAGE="$TEST_ROOT/unsigned.pkg"
touch "$COMPONENT_PACKAGE"
macos_build_product_package "$COMPONENT_PACKAGE" "$SIGNED_PACKAGE" release "$INSTALLER_IDENTITY"
assert_log_contains $'productbuild\t--sign\t'"$INSTALLER_IDENTITY"$'\t--timestamp\t--package\t'"$COMPONENT_PACKAGE"$'\t'"$SIGNED_PACKAGE"
assert_log_contains $'pkgutil\t--check-signature\t'"$SIGNED_PACKAGE"

: > "$FAKE_MACOS_TOOL_LOG"
macos_build_product_package "$COMPONENT_PACKAGE" "$UNSIGNED_PACKAGE" unsigned
assert_log_contains $'productbuild\t--package\t'"$COMPONENT_PACKAGE"$'\t'"$UNSIGNED_PACKAGE"
if grep -Fq -- "--sign" "$FAKE_MACOS_TOOL_LOG"; then
  fail "unsigned productbuild path unexpectedly requested signing"
fi

if macos_validate_release_identities "$INSTALLER_IDENTITY" "$APPLICATION_IDENTITY" >/dev/null 2>&1; then
  fail "swapped identity types were accepted"
fi

: > "$FAKE_MACOS_TOOL_LOG"
ARTIFACT_DIR="$TEST_ROOT/artifacts"
FINAL_PACKAGE="$ARTIFACT_DIR/adobe-mcp-rs-macos-universal.pkg"
mkdir -p "$ARTIFACT_DIR"
touch "$FINAL_PACKAGE"
printf '%s\n' "stale submission output" > "$ARTIFACT_DIR/notarytool-submit.json"
APPLE_ID="release@example.com" \
APPLE_TEAM_ID="TEAMID1234" \
APPLE_APP_SPECIFIC_PASSWORD="test-password" \
MAC_INSTALLER_IDENTITY="$INSTALLER_IDENTITY" \
  "$REPO_ROOT/scripts/notarize-macos.sh" "$ARTIFACT_DIR"

assert_log_contains $'pkgutil\t--check-signature\t'"$FINAL_PACKAGE"
assert_log_contains $'xcrun\tnotarytool\tsubmit\t'"$FINAL_PACKAGE"
assert_log_contains $'xcrun\tstapler\tstaple\t'"$FINAL_PACKAGE"
assert_log_contains $'xcrun\tstapler\tvalidate\t'"$FINAL_PACKAGE"
assert_log_contains $'spctl\t--assess\t--type\tinstall\t--verbose=2\t'"$FINAL_PACKAGE"
[[ -f "$ARTIFACT_DIR/notarytool-submit.json" ]] || fail "notary submission output was not saved"
if grep -Fq "stale submission output" "$ARTIFACT_DIR/notarytool-submit.json"; then
  fail "notary submission output retained stale retry data"
fi

submit_line="$(grep -n $'^xcrun\tnotarytool\tsubmit' "$FAKE_MACOS_TOOL_LOG" | cut -d: -f1)"
staple_line="$(grep -n $'^xcrun\tstapler\tstaple' "$FAKE_MACOS_TOOL_LOG" | cut -d: -f1)"
validate_line="$(grep -n $'^xcrun\tstapler\tvalidate' "$FAKE_MACOS_TOOL_LOG" | cut -d: -f1)"
if [[ -z "$submit_line" || -z "$staple_line" || -z "$validate_line" || "$submit_line" -ge "$staple_line" || "$staple_line" -ge "$validate_line" ]]; then
  fail "notarization, staple, and validation order is incorrect"
fi

FAKE_MACOS_FAIL_COMMAND="xcrun notarytool submit"
export FAKE_MACOS_FAIL_COMMAND
set +e
failure_output="$(APPLE_ID="release@example.com" \
  APPLE_TEAM_ID="TEAMID1234" \
  APPLE_APP_SPECIFIC_PASSWORD="test-password" \
  MAC_INSTALLER_IDENTITY="$INSTALLER_IDENTITY" \
  "$REPO_ROOT/scripts/notarize-macos.sh" "$ARTIFACT_DIR" 2>&1)"
failure_status=$?
set -e
unset FAKE_MACOS_FAIL_COMMAND
[[ "$failure_status" -ne 0 ]] || fail "notarytool submit failure did not stop the flow"
[[ "$failure_output" == *"notarytool-submit.json"* ]] || fail "failure output omitted the saved submission path"
[[ "$failure_output" == *"xcrun notarytool log <submission-id>"* ]] || fail "failure output omitted log retrieval instructions"

echo "macOS release signing control flow passed."
