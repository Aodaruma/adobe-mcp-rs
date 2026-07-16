#!/usr/bin/env bash

MACOS_CODESIGN_COMMAND="${MACOS_CODESIGN_COMMAND:-codesign}"
MACOS_PRODUCTBUILD_COMMAND="${MACOS_PRODUCTBUILD_COMMAND:-productbuild}"
MACOS_PKGUTIL_COMMAND="${MACOS_PKGUTIL_COMMAND:-pkgutil}"
MACOS_XCRUN_COMMAND="${MACOS_XCRUN_COMMAND:-xcrun}"
MACOS_SPCTL_COMMAND="${MACOS_SPCTL_COMMAND:-spctl}"

MACOS_RELEASE_BINARIES=(
  "ae-mcp"
  "pr-mcp"
  "ps-mcp"
  "ai-mcp"
  "id-mcp"
)

macos_require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command not found: $command_name" >&2
    return 1
  fi
}

macos_validate_release_identities() {
  local application_identity="$1"
  local installer_identity="$2"

  case "$application_identity" in
    "Developer ID Application: "*) ;;
    *)
      echo "MAC_APPLICATION_IDENTITY must be a Developer ID Application identity." >&2
      return 1
      ;;
  esac
  case "$installer_identity" in
    "Developer ID Installer: "*) ;;
    *)
      echo "MAC_INSTALLER_IDENTITY must be a Developer ID Installer identity." >&2
      return 1
      ;;
  esac
  if [[ "$application_identity" == "$installer_identity" ]]; then
    echo "Application and Installer identities must be different." >&2
    return 1
  fi
}

macos_verify_application_binaries() {
  local directory="$1"
  local application_identity="$2"
  local binary
  local binary_path
  local signature_details

  case "$application_identity" in
    "Developer ID Application: "*) ;;
    *)
      echo "Application verification requires a Developer ID Application identity." >&2
      return 1
      ;;
  esac

  macos_require_command "$MACOS_CODESIGN_COMMAND"
  for binary in "${MACOS_RELEASE_BINARIES[@]}"; do
    binary_path="$directory/$binary"
    if [[ ! -f "$binary_path" ]]; then
      echo "Application binary not found: $binary_path" >&2
      return 1
    fi
    "$MACOS_CODESIGN_COMMAND" --verify --strict --verbose=2 "$binary_path"
    if ! signature_details="$("$MACOS_CODESIGN_COMMAND" --display --verbose=4 "$binary_path" 2>&1)"; then
      printf '%s\n' "$signature_details" >&2
      return 1
    fi
    printf '%s\n' "$signature_details"
    if [[ "$signature_details" != *"Authority=$application_identity"* ]]; then
      echo "Unexpected application signature authority for $binary_path; expected $application_identity" >&2
      return 1
    fi
    echo "Verified Developer ID Application signature: $binary_path"
  done
}

macos_sign_application_binaries() {
  local directory="$1"
  local application_identity="$2"
  local binary
  local binary_path

  case "$application_identity" in
    "Developer ID Application: "*) ;;
    *)
      echo "Application signing requires a Developer ID Application identity." >&2
      return 1
      ;;
  esac

  macos_require_command "$MACOS_CODESIGN_COMMAND"
  for binary in "${MACOS_RELEASE_BINARIES[@]}"; do
    binary_path="$directory/$binary"
    if [[ ! -f "$binary_path" ]]; then
      echo "Application binary not found: $binary_path" >&2
      return 1
    fi
    echo "Signing application binary: $binary_path"
    "$MACOS_CODESIGN_COMMAND" \
      --force \
      --timestamp \
      --options runtime \
      --sign "$application_identity" \
      "$binary_path"
  done
  macos_verify_application_binaries "$directory" "$application_identity"
}

macos_verify_installer_package() {
  local package_path="$1"
  local installer_identity="$2"
  local signature_details

  case "$installer_identity" in
    "Developer ID Installer: "*) ;;
    *)
      echo "Installer verification requires a Developer ID Installer identity." >&2
      return 1
      ;;
  esac

  if [[ ! -f "$package_path" ]]; then
    echo "Installer package not found: $package_path" >&2
    return 1
  fi
  macos_require_command "$MACOS_PKGUTIL_COMMAND"
  if ! signature_details="$("$MACOS_PKGUTIL_COMMAND" --check-signature "$package_path" 2>&1)"; then
    printf '%s\n' "$signature_details" >&2
    return 1
  fi
  printf '%s\n' "$signature_details"
  if [[ "$signature_details" != *"$installer_identity"* ]]; then
    echo "Unexpected installer signature authority for $package_path; expected $installer_identity" >&2
    return 1
  fi
  echo "Verified Developer ID Installer signature: $package_path"
}

macos_assess_installer_package() {
  local package_path="$1"

  if [[ ! -f "$package_path" ]]; then
    echo "Installer package not found: $package_path" >&2
    return 1
  fi
  macos_require_command "$MACOS_SPCTL_COMMAND"
  "$MACOS_SPCTL_COMMAND" --assess --type install --verbose=2 "$package_path"
  echo "Gatekeeper accepted notarized installer package: $package_path"
}

macos_build_product_package() {
  local component_package="$1"
  local product_package="$2"
  local signing_mode="$3"
  local installer_identity="${4:-}"

  if [[ ! -f "$component_package" ]]; then
    echo "Component package not found: $component_package" >&2
    return 1
  fi
  macos_require_command "$MACOS_PRODUCTBUILD_COMMAND"

  case "$signing_mode" in
    release)
      case "$installer_identity" in
        "Developer ID Installer: "*) ;;
        *)
          echo "Release package signing requires a Developer ID Installer identity." >&2
          return 1
          ;;
      esac
      echo "Building Developer ID Installer signed product package: $product_package"
      "$MACOS_PRODUCTBUILD_COMMAND" \
        --sign "$installer_identity" \
        --timestamp \
        --package "$component_package" \
        "$product_package"
      macos_verify_installer_package "$product_package" "$installer_identity"
      ;;
    unsigned)
      echo "Building explicitly unsigned product package: $product_package"
      "$MACOS_PRODUCTBUILD_COMMAND" \
        --package "$component_package" \
        "$product_package"
      ;;
    *)
      echo "Unsupported MACOS_SIGNING_MODE: $signing_mode (expected release or unsigned)" >&2
      return 1
      ;;
  esac
}
