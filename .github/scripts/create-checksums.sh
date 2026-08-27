#!/usr/bin/env bash
# Builds a SHA256SUMS manifest over every installer asset on the draft release,
# clearsigns it with the project GPG key, and uploads the manifest, signature,
# and the public key users need for verification.
#
# requires: RELEASE_TAG, USAGEDECK_GPG_PRIVATE_KEY, optional USAGEDECK_GPG_PASSPHRASE
set -euo pipefail

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

# Draft releases are only addressable by id, never by tag.
release_id="$(gh release view "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY" --json databaseId --jq '.databaseId')"
test -n "$release_id"
gh api "repos/$GITHUB_REPOSITORY/releases/$release_id" > checksums-release.json

mkdir -p checksums
jq -r '.assets[].name
  | select(test("(-setup\\.exe|\\.AppImage|\\.deb|\\.app\\.tar\\.gz|\\.dmg)$"))' \
  checksums-release.json | sort -u > checksums/installers.txt
test -s checksums/installers.txt || {
  echo "No installer assets found on $RELEASE_TAG to hash." >&2
  exit 1
}

: > checksums/SHA256SUMS
while IFS= read -r asset_name; do
  asset_id="$(jq -r --arg name "$asset_name" \
    '.assets[] | select(.name == $name) | .id' checksums-release.json)"
  test -n "$asset_id"
  gh api \
    -H 'Accept: application/octet-stream' \
    "repos/$GITHUB_REPOSITORY/releases/assets/$asset_id" \
    > "checksums/$asset_name"
  (cd checksums && sha256sum "$asset_name") >> checksums/SHA256SUMS
done < checksums/installers.txt

gpg --batch --yes --local-user "$fingerprint" \
  "${passphrase_args[@]}" --clearsign \
  --output checksums/SHA256SUMS.asc checksums/SHA256SUMS
# The clearsign was produced by the same key just above; trust it on the verify line so
# gpg does not reject a freshly imported key as unsigned/unknown signer.
# `gpg --verify` consults the local trustdb, which has no ultimate trust for a key imported
# into the ephemeral keyring during this step. `--trusted-key` pins the verifier to that
# fingerprint regardless of trustdb state, so the clearsign the script just produced is
# accepted without weakening the public key uploaded alongside it.
gpg --batch --yes --trusted-key "$fingerprint" --verify checksums/SHA256SUMS.asc checksums/SHA256SUMS
(cd checksums && sha256sum --check --strict SHA256SUMS)

gpg --batch --armor --export "$fingerprint" > checksums/usagedeck-gpg-public.asc
gh release upload "$RELEASE_TAG" \
  checksums/SHA256SUMS checksums/SHA256SUMS.asc checksums/usagedeck-gpg-public.asc \
  --repo "$GITHUB_REPOSITORY" --clobber
echo "Published SHA256SUMS over $(wc -l < checksums/installers.txt) installers, signed with $fingerprint."
