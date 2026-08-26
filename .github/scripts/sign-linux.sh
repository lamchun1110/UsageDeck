#!/usr/bin/env bash
# Signs the Linux release packages with the project GPG key and uploads the
# detached armored signatures next to the artifacts they cover.
#
# usage: sign-linux.sh <bundle-root>
# requires: RELEASE_TAG, USAGEDECK_GPG_PRIVATE_KEY, optional USAGEDECK_GPG_PASSPHRASE
set -euo pipefail

bundle_root="${1:?usage: sign-linux.sh <bundle-root>}"
test -d "$bundle_root"

export GNUPGHOME
GNUPGHOME="$(mktemp -d)"
trap 'rm -rf "$GNUPGHOME"' EXIT
chmod 700 "$GNUPGHOME"

printf '%s' "$USAGEDECK_GPG_PRIVATE_KEY" | gpg --batch --import
fingerprint="$(gpg --list-secret-keys --with-colons | awk -F: '/^fpr:/ {print $10; exit}')"
test -n "$fingerprint" || {
  echo "Imported GPG key exposes no fingerprint." >&2
  exit 1
}

passphrase_args=()
if [[ -n "${USAGEDECK_GPG_PASSPHRASE:-}" ]]; then
  passphrase_args=(--pinentry-mode loopback --passphrase "$USAGEDECK_GPG_PASSPHRASE")
fi

gpg --batch --armor --export "$fingerprint" > usagedeck-gpg-public.asc

signed=0
shopt -s nullglob
for path in "$bundle_root"/deb/*.deb "$bundle_root"/appimage/*.AppImage; do
  test -f "$path" || continue
  gpg --batch --yes --local-user "$fingerprint" \
    "${passphrase_args[@]}" --armor --detach-sign \
    --output "$path.asc" "$path"
  gpg --batch --yes --verify "$path.asc" "$path"
  gh release upload "$RELEASE_TAG" "$path.asc" "$path" \
    --repo "$GITHUB_REPOSITORY" --clobber
  signed=$((signed + 1))
done

test "$signed" -ge 2 || {
  echo "Expected at least two Linux packages (.deb and .AppImage) under $bundle_root." >&2
  exit 1
}
echo "Signed $signed Linux package(s) with key $fingerprint."
