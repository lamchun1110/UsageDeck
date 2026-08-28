#!/usr/bin/env bash
set -euo pipefail

required=(TAURI_SIGNING_PRIVATE_KEY)
case "${WINDOWS_SIGNING_BACKEND:?WINDOWS_SIGNING_BACKEND is required}" in
  esign)
    required+=(ES_USERNAME ES_PASSWORD ES_CREDENTIAL_ID ES_TOTP_SECRET WINDOWS_SIGNER_SUBJECT)
    ;;
  signpath)
    required+=(SIGNPATH_API_TOKEN SIGNPATH_ORGANIZATION_ID SIGNPATH_PROJECT_SLUG SIGNPATH_SIGNING_POLICY_SLUG SIGNPATH_ARTIFACT_CONFIGURATION_SLUG)
    if [[ -z "${WINDOWS_SIGNER_SUBJECT:-}" ]]; then
      echo '::warning::WINDOWS_SIGNER_SUBJECT is not configured; SignPath verification will accept any otherwise-valid trusted signer.'
    fi
    ;;
  none)
    echo '::warning::Windows artifacts will not be Authenticode-signed.'
    ;;
  *)
    echo 'Invalid resolved WINDOWS_SIGNING_BACKEND.' >&2
    exit 1
    ;;
esac

if [[ "${MACOS_SIGNING_ENABLED:?MACOS_SIGNING_ENABLED is required}" = true ]]; then
  required+=(APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID)
else
  echo '::warning::macOS artifacts will use ad-hoc signing and will not be notarized.'
fi
if [[ "${LINUX_GPG_SIGNING_ENABLED:?LINUX_GPG_SIGNING_ENABLED is required}" = true ]]; then
  required+=(GPG_PRIVATE_KEY)
else
  echo '::warning::Linux artifacts will not be GPG-signed.'
fi

for name in "${required[@]}"; do
  test -n "${!name:-}" || {
    echo "Missing required release signing value: $name" >&2
    exit 1
  }
done
