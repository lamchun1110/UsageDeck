# Releasing UsageDeck

UsageDeck treats updater signatures and native operating-system signatures as separate trust
layers. Updater artifacts must always be signed with `TAURI_SIGNING_PRIVATE_KEY`. Windows uses an
explicit signing backend; macOS signing remains an independent opt-in because it requires an
externally provisioned certificate.
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

## Windows signing backend

Set the Actions repository variable `WINDOWS_SIGNING_BACKEND` to exactly one of:

| Value      | Behavior                                                                                                   |
| ---------- | ---------------------------------------------------------------------------------------------------------- |
| `none`     | Build and publish unsigned NSIS installers; updater signatures remain mandatory.                           |
| `esign`    | Use the existing SSL.com eSigner Tauri `signCommand` during bundling.                                      |
| `signpath` | Build NSIS first, submit an Actions artifact to SignPath, then publish only the returned signed installer. |

Invalid values fail in the validation job before any platform build. If `WINDOWS_SIGNING_BACKEND`
is unset, the legacy `ENABLE_WINDOWS_NATIVE_SIGNING=true` maps to `esign`; `false` or unset maps to
`none`. When both variables exist, the explicit backend wins and validation emits a migration
warning. Remove `ENABLE_WINDOWS_NATIVE_SIGNING` after setting the new variable.

`ENABLE_MACOS_NATIVE_SIGNING` remains independent. With it unset or `false`, macOS uses ad-hoc
signing and is not notarized. `ENABLE_LINUX_GPG_SIGNING` is also unchanged.

This default does not weaken updater verification. Tauri updater signatures are still generated,
uploaded, and verified with the bundled public key before publication.

## SSL.com eSigner fallback

Set `WINDOWS_SIGNING_BACKEND=esign` only after configuring all of the following:

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

## SignPath Foundation setup (pending approval)

UsageDeck is MIT-licensed and built from the public repository by GitHub Actions. It has no
UsageDeck-operated backend, analytics, or telemetry, but it does communicate with third-party
providers selected or authenticated by the user to retrieve usage and quota information. Project
maintainers review changes, while release approval is controlled by the maintainers and approvers
configured in GitHub and SignPath. The project retains its OpenQuota/OpenUsage lineage and
attribution.

SignPath signing is prepared but is not active merely because this workflow exists. After SignPath
Foundation approves the application, use the SignPath project integration page to configure a
trusted GitHub Actions build and add these values:

Use `https://usagedeck.app/privacy/` as the public Privacy Policy URL in the SignPath Foundation
application. The page is deployed from `website/privacy/index.html` by the repository's GitHub
Pages workflow.

| Kind             | Name                                   | Value                                                                                              |
| ---------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Actions variable | `SIGNPATH_ORGANIZATION_ID`             | Organization UUID supplied by SignPath.                                                            |
| Actions variable | `SIGNPATH_PROJECT_SLUG`                | Approved UsageDeck project slug.                                                                   |
| Actions variable | `SIGNPATH_SIGNING_POLICY_SLUG`         | Approved release policy slug.                                                                      |
| Actions variable | `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` | Artifact configuration that accepts an Actions ZIP containing exactly one UsageDeck `*-setup.exe`. |
| Actions variable | `WINDOWS_SIGNER_SUBJECT`               | Exact certificate subject after issuance; optional but strongly recommended.                       |
| Actions secret   | `SIGNPATH_API_TOKEN`                   | Token for the dedicated SignPath CI identity.                                                      |

The artifact configuration must use a ZIP root, constrain the expected NSIS filename inside it, and
Authenticode-sign that PE installer. NSIS is not one of SignPath's supported deep-signable container formats, so this
post-build path signs the final installer but does not separately sign the application executable
embedded inside it. The installer signature protects that payload. The `esign` backend retains its
stronger check that both the installer and installed executable have the same signer.

Set `WINDOWS_SIGNING_BACKEND=signpath` only after all required values exist and the SignPath trusted
build-system/origin-verification configuration points at this repository and release workflow.
Restrict the SignPath release policy to the configured maintainers/approvers and trusted tag builds.
The action waits up to one hour for policy processing or manual approval; rejection, timeout, or a
missing value fails the release and leaves the GitHub Release as a draft.

The release order is deliberately:

1. build the unsigned NSIS installer without updater artifacts;
2. upload it as a short-retention GitHub Actions artifact;
3. submit that artifact ID to SignPath and retrieve the approved result;
4. verify its valid, timestamped Authenticode signature and optional expected subject;
5. generate the Tauri updater `.sig` over the final Authenticode-signed bytes;
6. upload the verified `.exe` and matching `.sig` to the draft release and compare the released
   `.exe` SHA-256 with the verified local artifact;
7. install/start/uninstall smoke-test it, then let the existing release verification create
   `latest.json`, verify every updater signature, and publish the draft.

Authenticode changes the installer bytes. Reusing the updater `.sig` from the unsigned build would
make automatic updates fail verification, which is why the SignPath build disables Tauri updater
artifact creation until after SignPath returns the final file.

When SignPath signing is active, use the required acknowledgement: “Free code signing provided by
SignPath.io, certificate by SignPath Foundation.”

## Enabling Linux GPG signing

Linux does not rely on a public CA. UsageDeck signs the Linux installer artifacts
with a project-controlled GPG key and publishes the matching public key alongside
the release, so users can verify first-install provenance even though no platform
authority is involved.

Add repository secrets:

| Kind           | Name              | Value                                      |
| -------------- | ----------------- | ------------------------------------------ |
| Actions secret | `GPG_PRIVATE_KEY` | Contents of an armored GPG private key     |
| Actions secret | `GPG_PASSPHRASE`  | Key passphrase (omit/empty if unencrypted) |

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

The macOS and Linux boolean policy variables accept only the exact strings `true` and `false`. An
invalid value stops validation so a typo cannot silently change release trust policy. A
`verify_only` run publishes an already-built draft and therefore does not require private signing
credentials.

## Testing and fallback

Pull requests and ordinary branch pushes cannot trigger the Release workflow or SignPath. Test
application changes through CI and local packaging. To exercise release validation without signing
or publishing a production release, use a temporary semantic version/tag on a commit already merged
to `main`, keep the resulting GitHub Release as a draft, inspect all artifacts and checks, then delete
the temporary draft and tag. Use `WINDOWS_SIGNING_BACKEND=none` for a no-certificate rehearsal.
Do not point the production SignPath release policy at PR events; if SignPath provides a separate
test certificate/policy, test it with a separate temporary tag and policy slug first.

The normal fallbacks require only a repository-variable change before starting a new release run:

- `none`: no Windows native-signing secrets are read; SmartScreen may warn.
- `esign`: the four `ES_*` secrets and exact `WINDOWS_SIGNER_SUBJECT` are required.
- `signpath`: the SignPath token and four non-sensitive IDs/slugs are required; eSigner secrets are
  not read.

Do not change the backend while a release run is in progress. A failed SignPath approval leaves a
draft release; either resolve/re-run SignPath with the same backend, or remove the incomplete draft
assets and start a fresh release run using `esign` or `none`.

### Manual Windows verification

Run these commands in PowerShell on the downloaded installer:

```powershell
$signature = Get-AuthenticodeSignature -LiteralPath .\UsageDeck_0.7.0_x64-setup.exe
$signature | Format-List Status, StatusMessage, Path
$signature.SignerCertificate | Format-List Subject, Thumbprint, NotBefore, NotAfter
$signature.TimeStamperCertificate | Format-List Subject, NotBefore, NotAfter
if ($signature.Status -ne 'Valid') { throw 'Invalid Authenticode signature' }

# Optional second verification when the Windows SDK is installed:
signtool.exe verify /pa /all /tw .\UsageDeck_0.7.0_x64-setup.exe
```

Compare `SignerCertificate.Subject` with the release's documented identity or the configured
`WINDOWS_SIGNER_SUBJECT`. For `none`, `Get-AuthenticodeSignature` reports `NotSigned`; that is
expected only when the release notes explicitly identify the Windows artifacts as unsigned.

## Cutting a release

1. Bump the version in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` so
   `corepack pnpm verify:versions` passes.
2. Open a pull request, get it green, and merge to `main`.
3. Tag the release commit and push the tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.
4. The Release workflow validates the tag, builds and smoke-tests every platform target, verifies
   updater signatures against the bundled public key, then publishes the draft.
