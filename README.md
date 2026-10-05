<p align="center">
  <img src="assets/usagedeck-banner.png" alt="UsageDeck logo" width="560">
</p>

<h1 align="center">UsageDeck</h1>

<p align="center">
  English · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a>
</p>

<p align="center">
  <b>Every AI coding subscription you pay for, on one panel.</b>
</p>

<p align="center">
  <a href="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml"><img src="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases/latest"><img src="https://img.shields.io/github/v/release/lamchun1110/UsageDeck" alt="Latest release"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases"><img src="https://img.shields.io/github/downloads/lamchun1110/UsageDeck/total" alt="Total downloads"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
</p>

UsageDeck is an open-source, privacy-first desktop dashboard for Windows, Linux, and macOS. It
tracks usage limits, reset times, reset-credit expiry, token history, and estimated spend across 13
AI coding providers. Available metrics depend on each provider and account plan.

It lives in your tray or menu bar and reuses credentials already stored on your machine. Everything
runs locally—there is no UsageDeck-operated account or backend, analytics, or telemetry. UsageDeck
does make network requests to the third-party providers you configure or authenticate so it can
retrieve their usage and quota information. It also refreshes its public model-price catalogs about
once a day from GitHub, models.dev, and openrouter.ai so spend estimates stay current; those
requests carry no credentials or usage data, and they happen whether or not you use those services.

## What it tracks

| Provider                                          | Credentials | What you get                                                                                      |
| ------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------- |
| **[Claude Code](docs/providers/claude.md)**       | Local       | Multiple accounts, session and weekly limits, bonus reset counts and expiry, token history, spend |
| **[Codex](docs/providers/codex.md)**              | Local       | Session and weekly limits, reset credits and expiry, token history, model breakdown, spend        |
| **[Command Code](docs/providers/commandcode.md)** | Local       | Session, weekly, and monthly limits, plus extra credits                                           |
| **[Cursor](docs/providers/cursor.md)**            | Local       | Total, Auto, and API usage, credits, token history, spend                                         |
| **[Antigravity](docs/providers/antigravity.md)**  | Local       | Shared Gemini and Claude quota pools                                                              |
| **[Copilot](docs/providers/copilot.md)**          | Local       | Premium requests or AI credits, extra usage, chat and completion quotas, org billing              |
| **[Devin](docs/providers/devin.md)**              | Local       | Daily and weekly limits, reset times, extra usage balance                                         |
| **[Grok](docs/providers/grok.md)**                | Local       | Weekly allowance, extra usage status, token history, spend                                        |
| **[OpenCode](docs/providers/opencode.md)**        | Local       | Multiple Go accounts, session, weekly, and monthly quotas, local usage and estimated spend        |
| **[OpenRouter](docs/providers/openrouter.md)**    | API key     | Credits, balance, today, this week, this month, key limit                                         |
| **[Z.ai](docs/providers/zai.md)**                 | API key     | GLM Coding Plan session, weekly, and web-search quotas; personal ZCode reset cards and expiry     |
| **[Kimi](docs/providers/kimi.md)**                | API key     | Kimi Code session and weekly quotas, on the domain you choose                                     |
| **[MiniMax](docs/providers/minimax.md)**          | API key     | Token Plan session and weekly quotas                                                              |

**Local** providers reuse the login your CLI or editor already created — nothing to configure.
**API key** providers need a key you paste into Customize once; it goes straight into your operating
system's credential store, not into a config file. Codex subscription limits need a ChatGPT login
and will not appear in an API-key-only session. See the [usage guide](docs/usage.md) for account
setup, reset reminders, Session Kickstart, and update behavior.

## Install

Grab the file for your platform from the
[latest release](https://github.com/lamchun1110/UsageDeck/releases/latest):

| Platform | File                                   | Notes                                      |
| -------- | -------------------------------------- | ------------------------------------------ |
| Windows  | `_x64-setup.exe` or `_arm64-setup.exe` | x64 and ARM64                              |
| macOS    | `_universal.dmg`                       | Intel and Apple Silicon; macOS 11 or later |
| Linux    | `.AppImage`, `.deb`, or `.rpm`         | x64 and ARM64                              |

UsageDeck can check for updates automatically. Install updates from the app on Windows, macOS,
and Linux AppImage builds. Linux `.deb` and `.rpm` installations open the release page so you can
download and install the updated package. Update payloads are cryptographically signed with the
project's updater key, separately from operating-system package signing.

### Release signatures

- **Windows:** each release's notes state that release's signing status — unsigned, or
  Authenticode-signed when a signing backend was configured for that build. Unsigned installers
  can trigger Microsoft SmartScreen warnings; do not infer signing status from this README.
- **macOS:** Developer ID signing and Apple notarization are enabled per release. Builds made
  without native signing are ad-hoc-signed and unnotarized. The release workflow verifies native
  signatures and notarization before publishing when that signing mode is enabled.
- **Linux:** when GPG signing is enabled for a release, `.AppImage`, `.deb`, and `.rpm` downloads
  have detached signatures named `<file>.asc`. RPMs also carry an embedded OpenPGP signature;
  compatibility depends on the installed RPM version and signing algorithm. Those releases include
  `SHA256SUMS`, its clearsigned copy `SHA256SUMS.asc`, and `usagedeck-gpg-public.asc`.

For a release with GPG signatures, download its public key and the matching installer signature,
then verify the Linux download:

```bash
gpg --import usagedeck-gpg-public.asc
gpg --verify UsageDeck.AppImage.asc UsageDeck.AppImage
# Replace the example filenames above with the exact files from the release.
```

> [!IMPORTANT]
> Download UsageDeck from this repository's releases page and check that release's signing details
> and available signature files. Windows SmartScreen or macOS Gatekeeper may warn for builds
> without native signing. See [docs/releasing.md](docs/releasing.md) for verification commands.

## Coming from OpenQuota?

UsageDeck began as the OpenQuota fork and is now an independent project. Your data comes with you:
on first launch, UsageDeck copies settings, usage history, the pricing cache, and Antigravity's
local data from an existing OpenQuota installation. These source files are preserved. API keys
are transferred to the `UsageDeck` service in your system credential store; an old key entry is
removed only after its replacement is saved successfully. If a transfer fails, add the key again
in Customize. Keys in `~/.config/openquota/{kimi,minimax,zai}.json` are still accepted as external
sources, as are `~/.config/usagedeck/{kimi,minimax,zai}.json`. Keys saved through the app go to the
system credential store. OpenRouter also accepts `~/.config/openrouter/key.json`.

## Living with it

- **Tray popup or floating window.** Glance and dismiss, or leave the panel open on a second monitor.
  Linux desktops without a system tray use a standalone window.
- **Pin what matters.** Promote supported metrics into the tray or macOS menu bar.
- **Multiple accounts.** Separate Claude profiles, OpenCode Go data directories, and named
  OpenRouter, Z.ai, Kimi, and MiniMax API-key accounts get their own cards and customization.
- **Used or remaining.** Whichever way round you think about quota.
- **Pacing.** Tells you whether today's burn rate lasts until the reset, before it doesn't.
- **History.** Today, yesterday, and the trailing 30 days of tokens and estimated spend.
- **Heads-up before it hurts.** Optional desktop notifications when a quota is almost out, when
  you are cutting it close, and when your pace says you will run out before the reset. Reset-credit
  expiry reminders support 1, 24, 48, or 168 hours of notice, including when the reset row is hidden.
- **Reset-credit details.** Claude, Codex, and personal ZCode connections show available resets
  and reported expiry dates. Codex reset redemption asks for confirmation. Z.ai reset cards require
  a matching personal ZCode login and API key; see the [Z.ai guide](docs/providers/zai.md).
- **Session Kickstart.** Opt in to start a new rolling usage window with a small CLI prompt after
  expiry. Each prompt uses provider quota; supported providers can use a built-in or custom command.
- **Yours to arrange.** Reorder providers and metrics, hide rows, collapse sections.
- **Yours to look at.** Light, dark, or system, five accent colours, a compact density, and 12- or
  24-hour clocks.
- **Share a card.** Copy any provider's panel as an image, ready to paste.
- **Speaks your language.** English, 繁體中文, 简体中文, 日本語, and 한국어 — or whatever your
  system is set to.
- **Stays out of the way.** Launch at login, global shortcut, follows your system theme.

Everything runs on your machine. There is no UsageDeck account or UsageDeck-operated backend,
analytics, or telemetry. Provider usage refreshes communicate directly with the third-party
services the user has configured or authenticated. Public model-price catalogs are refreshed
separately; optional Session Kickstart sends a small provider CLI prompt.

## Code signing policy

UsageDeck is open-source software released under the MIT License. Release binaries are built from
this public GitHub repository by the tag-triggered GitHub Actions release workflow. Repository
maintainers review source changes, and the maintainers and release approvers configured in the
project's GitHub access-control settings control whether a trusted release is approved.

Each release's notes state whether its Windows artifacts are signed. Tauri updater signatures are a
separate project-controlled trust layer and are required for every release regardless of the
Windows Authenticode backend.

UsageDeck has no UsageDeck-operated backend, analytics, or telemetry. It connects directly to
third-party services for configured usage, authentication, reset-credit requests, and public pricing
catalogs. Optional Session Kickstart runs provider CLI prompts. This policy preserves the lineage and attribution
to OpenQuota and OpenUsage described below. See the public
[Privacy Policy](https://usagedeck.app/privacy/) for the complete data-handling disclosure.

## Building from source

You need Node.js 24+, pnpm 11.11.0, Rust installed through rustup, and the
[Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.
The Rust version and required components are pinned in [rust-toolchain.toml](rust-toolchain.toml);
rustup selects them automatically in this repository.

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm tauri dev
```

Before opening a pull request, run the full gate — formatting, lint, types, contracts, and both
test suites:

```sh
corepack pnpm verify
```

Packaging for the current platform:

```sh
corepack pnpm build:installer             # Windows
corepack pnpm build:linux                 # Linux
corepack pnpm tauri build --bundles dmg   # macOS
```

Release and signing requirements live in [docs/releasing.md](docs/releasing.md).

## Contributing

Issues and pull requests are welcome — read [CONTRIBUTING.md](CONTRIBUTING.md) first. Please report
security problems privately, following [SECURITY.md](SECURITY.md), rather than in a public issue.

## Lineage

[OpenUsage](https://github.com/robinebers/openusage) by Robin Ebers came first, for macOS.
[OpenQuota](https://github.com/deviffyy/OpenQuota) by deviffyy rebuilt the idea as a cross-platform
Tauri app for Windows, Linux, and macOS. UsageDeck started as a fork of that project and grew into
an independent product with its own identity, release infrastructure, and roadmap. Credit for the
original design and the overwhelming majority of the early code belongs to those two projects —
thank you.

## License

[MIT](LICENSE)
