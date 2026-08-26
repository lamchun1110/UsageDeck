# Releasing UsageDeck

UsageDeck treats updater signatures and native operating-system signatures as separate trust
layers. Updater artifacts must always be signed with `TAURI_SIGNING_PRIVATE_KEY`. Native Windows
and macOS signing are independent opt-ins because they require externally provisioned certificates.
If the updater key is encrypted, also configure `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The updater
key is project-generated and does not require a paid certificate authority or signing account.

## One-time repository setup

Releases publish from `lamchun1110/UsageDeck`. Before the first release of a fresh clone or renamed
repository:

1. Generate a dedicated updater keypair (never reuse another project's release identity):

   ```sh
   corepack pnpm tauri signer generate -w ~/.tauri/usagedeck.key
   ```

   The command writes `usagedeck.key` (private) and `usagedeck.key.pub` (public). The public key
   is already embedded in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey` as base64;
   regenerate both together if you ever rotate the key.

2. Add repository secrets:

   | Kind           | Name                                 | Value                                        |
   | -------------- | ------------------------------------ | -------------------------------------------- |
   | Actions secret | `TAURI_SIGNING_PRIVATE_KEY`          | Contents of `usagedeck.key`                  |
   | Actions secret | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The key password (omit/empty if unencrypted) |

3. Optional native-signing credentials are listed in their sections below; they stay unset until
   you deliberately enable them.

Keep the private key out of the repository and out of screenshots. Losing it means users cannot
receive verified automatic updates from your builds.

## Default release policy

Leave both native-signing repository variables unset or set them to `false`:

- `ENABLE_WINDOWS_NATIVE_SIGNING`
- `ENABLE_MACOS_NATIVE_SIGNING`

The release workflow then builds an unsigned Windows installer and an ad-hoc-signed, unnotarized
macOS application. Package installation and startup smoke tests still run, but Authenticode,
Gatekeeper, and notarization checks are skipped. The workflow emits warnings, and the download
documentation describes the unavailable native trust layers.

This default does not weaken updater verification. Tauri updater signatures are still generated,
uploaded, and verified with the bundled public key before publication.

## Enabling Windows native signing

Set `ENABLE_WINDOWS_NATIVE_SIGNING` to `true` only after configuring all of the following:

| Kind             | Name                     |
| ---------------- | ------------------------ |
| Actions secret   | `ES_USERNAME`            |
| Actions secret   | `ES_PASSWORD`            |
| Actions secret   | `ES_CREDENTIAL_ID`       |
| Actions secret   | `ES_TOTP_SECRET`         |
| Actions variable | `WINDOWS_SIGNER_SUBJECT` |

This enables the reviewed SSL.com CodeSignTool configuration. The workflow then requires a valid,
timestamped Authenticode signature on both the installer and installed executable. Missing or
incorrect values stop the release rather than silently producing an unsigned Windows artifact.

## Enabling Linux GPG signing

Linux does not rely on a public CA. UsageDeck signs the Linux installer artifacts
with a project-controlled GPG key and publishes the matching public key alongside
the release, so users can verify first-install provenance even though no platform
authority is involved.

Add repository secrets:

| Kind           | Name                | Value                                        |
| -------------- | ------------------- | -------------------------------------------- |
| Actions secret | `GPG_PRIVATE_KEY`   | Contents of an armored GPG private key       |
| Actions secret | `GPG_PASSPHRASE`    | Key passphrase (omit/empty if unencrypted)   |

Generate the keypair once:

```sh
gpg --batch --quick-generate-key 'UsageDeck Release Signing <[email protected]>' rsa4096 sign
gpg --armor --export-secret-keys [email protected]   # paste into the GPG_PRIVATE_KEY secret
gpg --armor --export [email protected]                  # publish this as the public key out of band
```

When enabled, also set the repository variable `ENABLE_LINUX_GPG_SIGNING` to `true`.
The release workflow then:

1. Detached-signs every `.deb` and `.AppImage` with the project key, verifies each
   signature round-trip, and uploads the `.asc` files next to the artifacts.
2. Builds a `SHA256SUMS` manifest covering every installer (Windows, macOS, Linux),
   clearsigns it with the same key, and uploads `SHA256SUMS`, `SHA256SUMS.asc`, and
   `usagedeck-gpg-public.asc` to the release.
3. Refuses to publish when the secret is missing or the key has no fingerprint.

Leaving `ENABLE_LINUX_GPG_SIGNING` unset (or set to `false`) keeps today's behavior:
the workflow emits a warning and uploads only the updater-signed artifacts.

### User-side verification

```sh
# 1. Trust the project key exactly once.
curl -L -o usagedeck-release.asc \
  https://github.com/lamchun1110/UsageDeck/releases/latest/download/usagedeck-gpg-public.asc
gpg --import usagedeck-release.asc

# 2. Verify the checksum manifest signature and the manifest itself.
curl -L -O https://github.com/lamchun1110/UsageDeck/releases/latest/download/SHA256SUMS.asc
curl -L -O https://github.com/lamchun1110/UsageDeck/releases/latest/download/SHA256SUMS
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum --strict --check SHA256SUMS

# 3. Verify a specific installer before installing it.
gpg --verify UsageDeck_0.5.2_amd64.deb.asc UsageDeck_0.5.2_amd64.deb
```

Cross-check the key fingerprint against the announcement published on the
project's website before trusting the key — the `.asc` file alone only proves
it was the same key as for the prior release.

## Enabling macOS native signing

Set `ENABLE_MACOS_NATIVE_SIGNING` to `true` only after configuring all of the following Actions
secrets:

- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_ID`
- `APPLE_PASSWORD`
- `APPLE_TEAM_ID`

`APPLE_PASSWORD` is an app-specific password used for notarization, not the account's normal login
password.

The direct-download DMG uses a Developer ID Application certificate, not an App Store Distribution
certificate. When enabled, the workflow requires the expected team identity, hardened runtime,
secure timestamp, Gatekeeper approval, and a valid notarization staple. Missing or incorrect values
stop the release rather than falling back to ad-hoc signing.

Both opt-ins accept only the exact strings `true` and `false`. An invalid value stops validation so a
typo cannot silently change release trust policy. A `verify_only` run publishes an already-built
draft and therefore does not require private signing credentials. The two policy variables must
still match the draft's native-signing state; a mismatch stops publication.

## Cutting a release

1. Bump the version in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` so
   `corepack pnpm verify:versions` passes.
2. Open a pull request, get it green, and merge to `main`.
3. Tag the release commit and push the tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.
4. The Release workflow validates the tag, builds and smoke-tests every platform target, verifies
   updater signatures against the bundled public key, then publishes the draft.
