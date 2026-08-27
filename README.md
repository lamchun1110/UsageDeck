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
tracks usage limits, reset times, token history, and estimated spend across 13 AI coding assistants.

It lives in your tray or menu bar and reuses credentials already stored on your machine. Everything
runs locally—there is no UsageDeck account, backend, analytics, or telemetry.

## What it tracks

| Provider                                          | Credentials | What you get                                                                                 |
| ------------------------------------------------- | ----------- | -------------------------------------------------------------------------------------------- |
| **[Claude Code](docs/providers/claude.md)**       | Local       | Multiple accounts, session and weekly limits, per-model usage, token history, spend          |
| **[Codex](docs/providers/codex.md)**              | Local       | Session and weekly limits, rate limit resets, credits, token history, model breakdown, spend |
| **[Command Code](docs/providers/commandcode.md)** | Local       | Session, weekly, and monthly limits, plus extra credits                                      |
| **[Cursor](docs/providers/cursor.md)**            | Local       | Total, Auto, and API usage, credits, token history, spend                                    |
| **[Antigravity](docs/providers/antigravity.md)**  | Local       | Shared Gemini and Claude quota pools                                                         |
| **[Copilot](docs/providers/copilot.md)**          | Local       | Premium requests, extra usage, chat and completion quotas, org billing                       |
| **[Devin](docs/providers/devin.md)**              | Local       | Daily and weekly limits, reset times, extra usage balance                                    |
| **[Grok](docs/providers/grok.md)**                | Local       | Weekly allowance, extra usage status, token history, spend                                   |
| **[OpenCode](docs/providers/opencode.md)**        | Local       | Go session, weekly, and monthly quotas, plus local usage and estimated spend                 |
| **[OpenRouter](docs/providers/openrouter.md)**    | API key     | Credits, balance, today, this week, this month, key limit                                    |
| **[Z.ai](docs/providers/zai.md)**                 | API key     | GLM Coding Plan session, weekly, and web-search quotas                                       |
| **[Kimi](docs/providers/kimi.md)**                | API key     | Kimi Code session and weekly quotas, on the domain you choose                                |
| **[MiniMax](docs/providers/minimax.md)**          | API key     | Token Plan session and weekly quotas                                                         |

**Local** providers reuse the login your CLI or editor already created — nothing to configure.
**API key** providers need a key you paste into Customize once; it goes straight into your operating
system's credential store, not into a config file. Codex subscription limits need a ChatGPT login
and will not appear in an API-key-only session.

## Install

Grab the file for your platform from the
[latest release](https://github.com/lamchun1110/UsageDeck/releases/latest):

| Platform | File                                   | Notes                                              |
| -------- | -------------------------------------- | -------------------------------------------------- |
| Windows  | `_x64-setup.exe` or `_arm64-setup.exe` | x64 and ARM64                                      |
| macOS    | `_universal.dmg`                       | Universal, Developer ID signed and Apple notarized |
| Linux    | `.AppImage` or `.deb`                  | x64 and ARM64, with detached GPG signatures        |

The app updates itself. Update payloads are cryptographically signed with the project's own updater
key, which is a separate thing from operating-system package signing.

### Release signatures

- **macOS:** official releases are signed with an Apple Developer ID certificate and notarized by
  Apple when the `ENABLE_MACOS_NATIVE_SIGNING` repository variable is set to `true`. Signed releases
  use the bundle ID `com.lamchun1110.usagedeck` and are stapled. The release workflow verifies the
  code signature, Gatekeeper assessment, notarization ticket, and hardened runtime before publishing.
- **Linux:** every `.AppImage` and `.deb` has a matching ASCII-armored detached signature named
  `<file>.asc`. Releases also include `SHA256SUMS`, its GPG-signed copy `SHA256SUMS.asc`, and
  `usagedeck-gpg-public.asc`, the public key needed for verification.

To verify a Linux download:

```bash
gpg --import usagedeck-gpg-public.asc
gpg --verify UsageDeck.AppImage.asc UsageDeck.AppImage
# Replace the example filenames above with the exact files from the release.
```

> [!IMPORTANT]
> Windows installers are not currently Authenticode-signed, so SmartScreen may ask you to confirm.
> On macOS, builds produced without `ENABLE_MACOS_NATIVE_SIGNING` are ad-hoc-signed and unnotarized;
> Gatekeeper will block the first launch unless you right-click → Open. Only download UsageDeck from
> this repository's releases page. Every release states its exact signing status in the notes.

## Coming from OpenQuota?

UsageDeck began as the OpenQuota fork and is now an independent project. Your data comes with you:
on first launch, UsageDeck migrates settings, usage history, the pricing cache, and Antigravity's
local data from an existing OpenQuota installation automatically, and API keys saved in your system
credential store are moved to UsageDeck's own entry. Nothing is deleted from the old location. Keys
stored in `~/.config/openquota/{kimi,minimax,zai}.json` are still read; new keys live in
`~/.config/usagedeck/`. OpenRouter reads its own legacy path at `~/.config/openrouter/key.json`.

## Living with it

- **Tray popup or floating window.** Glance and dismiss, or leave the panel open on a second monitor.
- **Pin what matters.** Promote any metric into the tray or macOS menu bar.
- **Used or remaining.** Whichever way round you think about quota.
- **Pacing.** Tells you whether today's burn rate lasts until the reset, before it doesn't.
- **History.** Today, yesterday, and the trailing 30 days of tokens and estimated spend.
- **Heads-up before it hurts.** Optional desktop notifications when a quota is almost out, when
  you are cutting it close, and when your pace says you will run out before the reset.
- **Yours to arrange.** Reorder providers and metrics, hide rows, collapse sections.
- **Yours to look at.** Light, dark, or system, five accent colours, a compact density, and 12- or
  24-hour clocks.
- **Share a card.** Copy any provider's panel as an image, ready to paste.
- **Speaks your language.** English, 繁體中文, 简体中文, 日本語, and 한국어 — or whatever your
  system is set to.
- **Stays out of the way.** Launch at login, global shortcut, follows your system theme.

Everything runs on your machine. No account, no backend, no analytics, no telemetry.

## Building from source

You need Node.js 22+, pnpm 11.11.0, a stable Rust toolchain, and the
[Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

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
