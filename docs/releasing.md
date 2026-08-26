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
