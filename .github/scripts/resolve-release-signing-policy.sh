#!/usr/bin/env bash
set -euo pipefail

normalize_policy() {
  local name="$1"
  local value="$2"
  case "$value" in
    true | false) printf '%s' "$value" ;;
    *)
      echo "::error::$name must be either true or false." >&2
      return 1
      ;;
  esac
}

legacy_windows_signing="$(normalize_policy ENABLE_WINDOWS_NATIVE_SIGNING "${ENABLE_WINDOWS_NATIVE_SIGNING:-false}")"
windows_backend="${WINDOWS_SIGNING_BACKEND:-}"
if [[ -z "$windows_backend" ]]; then
  if [[ "$legacy_windows_signing" = true ]]; then
    windows_backend=esign
  else
    windows_backend=none
  fi
else
  case "$windows_backend" in
    none | esign | signpath) ;;
    *)
      echo '::error::WINDOWS_SIGNING_BACKEND must be none, esign, or signpath.' >&2
      exit 1
      ;;
  esac
  legacy_backend=$([[ "$legacy_windows_signing" = true ]] && printf esign || printf none)
  if [[ "${LEGACY_WINDOWS_SIGNING_CONFIGURED:-false}" = true && "$legacy_backend" != "$windows_backend" ]]; then
    echo "::warning::WINDOWS_SIGNING_BACKEND=$windows_backend overrides legacy ENABLE_WINDOWS_NATIVE_SIGNING=$legacy_windows_signing. Remove the legacy variable after migration."
  fi
fi

macos_signing="$(normalize_policy ENABLE_MACOS_NATIVE_SIGNING "${ENABLE_MACOS_NATIVE_SIGNING:-false}")"
linux_gpg_signing="$(normalize_policy ENABLE_LINUX_GPG_SIGNING "${ENABLE_LINUX_GPG_SIGNING:-false}")"

output_file="${GITHUB_OUTPUT:-/dev/stdout}"
{
  echo "windows_signing_backend=$windows_backend"
  echo "macos_signing=$macos_signing"
  echo "linux_gpg_signing=$linux_gpg_signing"
} >> "$output_file"
