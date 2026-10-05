# Using UsageDeck

UsageDeck shows the quota information available from each configured provider and, where supported,
token history and estimated spend. A missing metric can mean the account's plan does not report it.
Provider-specific setup and troubleshooting are in [the provider guides](providers/).

## Connect providers

For local-login providers, sign in through their CLI or supported editor, open UsageDeck, and refresh.
Claude subscription limits require a Claude Code login; Claude Desktop alone does not supply live
limits. Codex subscription limits require a ChatGPT login rather than an API-key-only session.

For OpenRouter, Z.ai, Kimi, and MiniMax, open **Customize**, select the provider, and add its API key.
Keys saved in the app use the operating system's credential store. External config files and
environment variables remain supported for the default provider cards; see each provider's guide.
Named API-key accounts use their own saved keys rather than inheriting the default card's external
key. Restart UsageDeck after changing environment variables or adding local-login profiles.

Usage refreshes contact third-party providers directly and can run automatically while UsageDeck
is open. Local logs used for token and spend estimates are processed on your device. Public model
price catalogs are refreshed about once a day; estimates may differ from provider billing.

## Multiple accounts

Each supported account gets its own card, name, layout, and quota view.

### Claude Code

Sign in to separate Claude accounts using distinct config directories. For example, on macOS or
Linux:

```sh
CLAUDE_CONFIG_DIR=~/.claude-work claude
CLAUDE_CONFIG_DIR=~/.claude-personal claude
```

In Windows PowerShell, set the directory before launching the CLI:

```powershell
$env:CLAUDE_CONFIG_DIR = "$HOME\.claude-work"
claude
$env:CLAUDE_CONFIG_DIR = "$HOME\.claude-personal"
claude
```

Restart UsageDeck to discover those profiles. Logins for the same Claude account are combined;
distinct accounts keep separate limits and local history. See [Claude Code](providers/claude.md)
for directory discovery and credential permissions.

### OpenCode Go

Additional Go logins are detected in sibling directories named `opencode-<name>` next to the active
OpenCode data directory. Each directory needs its own subscribed login and local databases.
Restart UsageDeck after setting them up. See [OpenCode](providers/opencode.md) for the data layout.

### Named API-key accounts

1. Open **Customize**, choose OpenRouter, Z.ai, Kimi, or MiniMax, enter an account name, and select
   **Add Account**.
2. Open the new card in Customize and select **Add** under **API Key**. The card appears immediately;
   no restart is required.
3. Save that account's key. For Kimi, also choose the matching endpoint in the card's connection
   settings.

Removing a named account deletes its saved key and retires that card's identifier. Other local-login
providers, including Codex, follow the currently stored login rather than discovering separate
account cards.

## Reset credits and expiry reminders

**Rate Limit Resets** is separate from the automatic reset time on a quota meter. It shows available
bonus resets for Claude, Codex, and supported personal ZCode accounts. Open the row for the reported
expiry dates. **Unavailable** means the connection could not provide reset data; it does not mean
zero credits. A count without dates is shown with **Expiry times unavailable**.

Z.ai reset cards require a personal Coding Plan login in ZCode and the matching ZCode-managed API
key. **Use ZCode API key** replaces the card's saved key with that account's key so quotas and resets
refer to the same account. See [Z.ai](providers/zai.md) for eligibility and credential setup.

To receive expiry alerts, enable **Settings → Notifications → Resets expiring** and choose 1, 24, 48,
or 168 hours of notice. Reminders work when the reset row is hidden, require notification permission
and UsageDeck running, and cannot trigger for unknown expiry times. Successful deliveries are
remembered across restarts for each account and deadline; failed deliveries are retried with backoff.

Codex reset redemption is available from the reset details and requires confirmation. Claude and
Z.ai reset inventories are read-only in UsageDeck; redeem those resets through the provider.

## Session Kickstart

Enable **Customize → provider → Session Kickstart** for supported providers to start a new rolling window after
the old window expires. UsageDeck runs a small provider CLI prompt, using a built-in command where
available or a custom command you supply. Each prompt uses provider quota and may incur charges
under that provider's plan. The feature is opt-in and requires UsageDeck running and a working CLI
login or custom command.

Select the window to restart when the provider offers multiple rolling windows. Custom commands
override built-in commands and should target the same account as the card. UsageDeck backs off
after failed attempts and suspends automatic attempts when repeated prompts do not restart the
selected window. Check the diagnostic log in Settings if a kickstart is not taking effect.

## Panel, history, and notifications

Use Customize to enable providers, rearrange cards and metrics, hide rows, and pin supported metrics
to the tray or macOS menu bar. Settings controls appearance, used or remaining quota, reset-time
display, clock format, language, global shortcut, launch at login, and optional quota alerts.

Use the floating-window mode to keep the panel open. Linux desktops without a working system tray
use a standalone window automatically. Usage data is stored separately on each device; there is no
UsageDeck account or cross-device synchronization.

History rows show today, yesterday, and the trailing 30 days of tokens and estimated spend for
providers with a supported history source. Cursor history comes from its provider usage export and
can lag behind live quotas; Claude, Codex, Grok, and OpenCode use local records. Resetting app settings
preserves provider sign-ins, API keys, and usage history.

## Updates and downloads

Use **Settings → Check for Updates** or enable automatic update checks. UsageDeck verifies update
payloads with the project's updater key before installation. Native installer signing is a separate
layer; check the selected release's signing details and available signature files.

Windows, macOS, and Linux AppImage builds support installation from the app. Linux `.deb` and `.rpm`
builds open the release page; download and install the updated package for your architecture.
The website's download buttons link to the latest published release rather than a fixed version.

See [the README](../README.md#install) for platform downloads,
[release verification](releasing.md#user-side-verification) for GPG commands, and the
[Privacy Policy](https://usagedeck.app/privacy/) for data handling.
