#!/usr/bin/env bash
set -euo pipefail

ARTIFACT_DIR="${1:-./dist/macos}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG_PATH="$ARTIFACT_DIR/adobe-mcp-rs-macos-universal.pkg"
NOTARY_SUBMISSION_LOG="$ARTIFACT_DIR/notarytool-submit.json"

# shellcheck source=macos-release-tools.sh
source "$SCRIPT_DIR/macos-release-tools.sh"

: "${APPLE_ID:?APPLE_ID is required}"
: "${APPLE_TEAM_ID:?APPLE_TEAM_ID is required}"
: "${APPLE_APP_SPECIFIC_PASSWORD:?APPLE_APP_SPECIFIC_PASSWORD is required}"
: "${MAC_INSTALLER_IDENTITY:?MAC_INSTALLER_IDENTITY is required}"

if [[ ! -d "$ARTIFACT_DIR" ]]; then
  echo "Artifact directory not found: $ARTIFACT_DIR" >&2
  exit 1
fi

rm -f "$NOTARY_SUBMISSION_LOG"
macos_require_command "$MACOS_XCRUN_COMMAND"
macos_verify_installer_package "$PKG_PATH" "$MAC_INSTALLER_IDENTITY"

notarization_failed() {
  echo "macOS notarization failed. Submission output: $NOTARY_SUBMISSION_LOG" >&2
  echo "Use the submission id from that file with: xcrun notarytool log <submission-id> --apple-id <apple-id> --team-id <team-id> --password <app-password>" >&2
}
trap notarization_failed ERR

echo "Submitting final product package for notarization: $PKG_PATH"
"$MACOS_XCRUN_COMMAND" notarytool submit "$PKG_PATH" \
  --apple-id "$APPLE_ID" \
  --team-id "$APPLE_TEAM_ID" \
  --password "$APPLE_APP_SPECIFIC_PASSWORD" \
  --wait \
  --output-format json | tee "$NOTARY_SUBMISSION_LOG"
echo "Saved notarization submission output: $NOTARY_SUBMISSION_LOG"

echo "Stapling notarization ticket: $PKG_PATH"
"$MACOS_XCRUN_COMMAND" stapler staple "$PKG_PATH"

echo "Validating stapled notarization ticket: $PKG_PATH"
"$MACOS_XCRUN_COMMAND" stapler validate "$PKG_PATH"
macos_verify_installer_package "$PKG_PATH" "$MAC_INSTALLER_IDENTITY"
macos_assess_installer_package "$PKG_PATH"
trap - ERR

echo "macOS notarization completed."
