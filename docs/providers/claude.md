# Claude Code

UsageDeck tracks Claude subscription limits and local Claude usage history.

## What it tracks

| Metric                           | Meaning                                                      |
| -------------------------------- | ------------------------------------------------------------ |
| Session                          | Usage remaining in the current session window                |
| Weekly                           | Usage remaining in the weekly window                         |
| Sonnet / Fable                   | Model-specific limits when they are reported for the account |
| Extra Usage                      | Extra-usage allowance or spending reported by Claude         |
| Today / Yesterday / Last 30 Days | Tokens and estimated spend calculated from local usage logs  |
| Usage Trend                      | Recent local usage over time                                 |

## Sign-in and local data

Sign in with Claude Code by running `claude`. UsageDeck reuses the credentials maintained by the
CLI, including `CLAUDE_CONFIG_DIR` when it is set. On macOS, UsageDeck reads Claude Code's Keychain
item using the same Apple-signed `/usr/bin/security` reader as the CLI. This reuses that reader's
existing permission when Claude Code renews its login and drops permissions granted to other apps.
UsageDeck does not change the item's access controls.

Claude Code maintains and renews its own Keychain login on macOS and Windows. If that login expires,
UsageDeck shows the last successful limits until Claude Code renews it. File-based logins and Linux
Secret Service logins can be refreshed and saved back to their original source by UsageDeck.

## Multiple accounts

UsageDeck discovers separate Claude Code logins that use custom `CLAUDE_CONFIG_DIR` homes and shows
each account as its own card with independent limits, plan, and local usage history. Logins belonging
to the same Claude account are combined automatically.

Account cards can be renamed from Customize or from the dashboard. If a login is removed, its card
is hidden and returns with its previous customization when the login is detected again.

Live subscription limits currently require a Claude Code login. On macOS, UsageDeck can recognize
that Claude Desktop is installed, but it does not reuse Desktop's encrypted session. Run `claude`
and sign in once if Desktop is your only Claude login.

Spend history is calculated locally from Claude usage logs. It can also include compatible Claude
usage recorded by pi and, on macOS, Claude's local agent-mode sessions. These local records are not
uploaded by UsageDeck.

## Bonus limit resets

The **Rate Limit Resets** row shows remaining bonus resets and the earliest known expiry.
Open the row to see all expiry dates. Enable **Settings → Notifications → Resets expiring** for
expiry reminders, including when the row is hidden.

UsageDeck reads the grant inventory alongside usage through your existing Claude Code login.
No additional sign-in is needed. The count includes owned credits that require reaching a limit
before redemption, and excludes expired grants and already used credits. A grant with multiple
remaining credits contributes one expiry entry for each credit. Missing or filtered data shows
**Unavailable**, which is different from a confirmed zero.

These bonus resets are separate from the regular session and weekly reset schedules. See
[Claude's limit-reset guide](https://support.claude.com/en/articles/17007452-what-is-a-limit-reset)
for expiry and redemption details. UsageDeck does not read encrypted Desktop sessions to obtain them.

## Troubleshooting

- **Not logged in** — run `claude`, complete sign-in, then refresh UsageDeck.
- **Claude Desktop login found** — sign in once through the Claude Code CLI.
- **Claude Code credentials could not be read** — check access to the system credential store, then
  refresh UsageDeck. This indicates an unreadable CLI login rather than a Desktop-only login.
- **Claude Code login is incomplete** — the saved CLI credential has no usable access token. Run
  `claude`, sign in again, then refresh UsageDeck. Granting Keychain access cannot restore missing
  tokens.
- **Repeated macOS permission dialogs** — allow `security` to read the `Claude Code-credentials`
  item when macOS requests it. "Always Allow" applies to that reader and item. If Claude Code resets
  the item's permissions, macOS may ask again; UsageDeck cannot prevent the owning app from doing
  so. Avoid granting access to all applications. A dialog naming `usagedeck` for this item indicates
  an older UsageDeck build that reads it directly.
- **Session or token expired** — sign in again with `claude`.
- **No local history** — use Claude Code normally and check whether `CLAUDE_CONFIG_DIR` points to
  the directory containing your Claude data.
