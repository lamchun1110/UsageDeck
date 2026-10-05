# Codex

UsageDeck tracks Codex subscription limits and usage recorded by the Codex CLI.

## What it tracks

| Metric                           | Meaning                                                      |
| -------------------------------- | ------------------------------------------------------------ |
| Session                          | Usage remaining in the current session window                |
| Weekly                           | Usage remaining in the weekly window                         |
| Spark / Spark Weekly             | Model-specific limits when they are reported for the account |
| Extra Usage                      | Additional usage credits reported by Codex                   |
| Rate Limit Resets                | Available reset credits                                      |
| Today / Yesterday / Last 30 Days | Tokens, model usage, and estimated spend from local logs     |
| Usage Trend                      | Recent local usage over time                                 |

## Sign-in and local data

Sign in with the Codex CLI by running `codex` and choosing your ChatGPT account. UsageDeck reads the
same authentication data and respects `CODEX_HOME` when it is set. API-key-only sessions can produce
local usage history, but they cannot provide ChatGPT subscription limits.

Spend history is calculated locally from the Codex `sessions` and `archived_sessions` logs. Compatible
Codex usage recorded by pi can also be included. UsageDeck does not upload these local records.

## Reset credits and reminders

**Rate Limit Resets** shows the available count and nearest known expiry. Open the row for the
expiry timeline. Credits that expire after the last refresh drop out of the displayed count as time
passes. When the provider reports only a count, UsageDeck says the expiry times are unavailable.

Enable **Settings → Notifications → Resets expiring** and choose a lead time of 1, 24, 48, or 168
hours. Reminders work even when the reset row is hidden. Each account and deadline is notified once;
successful deliveries are remembered across app restarts, and failed deliveries are retried with
backoff. A count without an expiry cannot produce an expiry reminder. Notifications require system
permission and UsageDeck running.

Using a reset still requires explicit confirmation. Redemption is available for the default Codex
account; additional account cards show their own reset data without routing a claim to another
account. UsageDeck does not infer a lifetime total received from the remaining count.

## Troubleshooting

- **Not logged in** — run `codex`, sign in with ChatGPT, then refresh UsageDeck.
- **Subscription usage unavailable** — replace an API-key-only login with a ChatGPT login.
- **Session expired or revoked** — sign in again with `codex`.
- **No local history** — check the active Codex data directory and the value of `CODEX_HOME`.
