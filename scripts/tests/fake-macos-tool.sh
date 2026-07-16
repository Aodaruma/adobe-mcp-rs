#!/usr/bin/env bash
set -euo pipefail

: "${FAKE_MACOS_TOOL_LOG:?FAKE_MACOS_TOOL_LOG is required}"

tool_name="$(basename "$0")"
{
  printf '%s' "$tool_name"
  for argument in "$@"; do
    printf '\t%s' "$argument"
  done
  printf '\n'
} >> "$FAKE_MACOS_TOOL_LOG"

if [[ "${FAKE_MACOS_FAIL_COMMAND:-}" == "$tool_name ${1:-} ${2:-}" ]]; then
  exit 42
fi

if [[ "$tool_name" == "productbuild" ]]; then
  output_path=""
  for argument in "$@"; do
    output_path="$argument"
  done
  if [[ -z "$output_path" ]]; then
    echo "fake productbuild did not receive an output path" >&2
    exit 1
  fi
  touch "$output_path"
fi

if [[ "$tool_name" == "codesign" && "${1:-}" == "--display" ]]; then
  : "${FAKE_APPLICATION_IDENTITY:?FAKE_APPLICATION_IDENTITY is required}"
  echo "Authority=$FAKE_APPLICATION_IDENTITY" >&2
fi

if [[ "$tool_name" == "pkgutil" && "${1:-}" == "--check-signature" ]]; then
  : "${FAKE_INSTALLER_IDENTITY:?FAKE_INSTALLER_IDENTITY is required}"
  echo "1. $FAKE_INSTALLER_IDENTITY"
fi
