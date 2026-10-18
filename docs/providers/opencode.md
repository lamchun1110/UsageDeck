# OpenCode

UsageDeck combines OpenCode Go quota information with usage recorded by local OpenCode sessions.

## What it tracks

| Metric                           | Meaning                                           |
| -------------------------------- | ------------------------------------------------- |
| Session                          | OpenCode Go rolling-window usage                  |
| Weekly                           | OpenCode Go weekly usage                          |
| Monthly                          | OpenCode Go monthly usage                         |
| Today / Yesterday / Last 30 Days | Local hosted usage and spend recorded by OpenCode |
| Usage Trend                      | Recent local usage over time                      |

Go quota rows appear when a compatible OpenCode Go login is available. Local history can still be
shown when OpenCode has been used without that plan.

The Go meters come from OpenCode's account usage endpoint, so they include usage from all devices
and reflect the limits enforced by OpenCode. Local usage history remains separate and is read from
the OpenCode data directory.

## Sign-in and local data

Sign in to OpenCode Console in OpenCode V2 with `/connect`, or use OpenCode locally first.
UsageDeck reads the active Console OAuth login or Go API-key login from OpenCode V2's SQLite
`credential` table. No API key needs to be copied into UsageDeck. The selected workspace is used
for the Go quota request; inactive saved accounts are ignored.

V1 installations remain supported through `auth.json`. When a V2 credential store exists, it is
authoritative: signing out in V2 does not reactivate an old key left in `auth.json`. UsageDeck reads
credentials without changing them or refreshing OAuth tokens. If a token has expired, open OpenCode
to let it renew the login, then refresh UsageDeck.

Local history includes hosted OpenCode and Go usage from V2's `session_message` / `session_v2`
tables and from V1 databases. Migrated sessions use V2 records so the retained V1 copies do not
inflate totals. Other providers used inside OpenCode are excluded from this card.

`OPENCODE_DATA_DIR` and `XDG_DATA_HOME` are respected when present. V2's `OPENCODE_DB` override
selects a specific database, including paths outside the data directory and custom filenames.
UsageDeck must receive the same environment overrides as OpenCode. Local databases are opened
read-only; the default location is `~/.local/share/opencode/opencode.db`.

## Multiple accounts

UsageDeck discovers additional OpenCode Go logins in sibling data directories named
`opencode-<name>` next to the active OpenCode data directory. With the default layout, examples are
`~/.local/share/opencode-work` and `~/.local/share/opencode-personal`. Each directory must contain
its own subscribed V2 Console or Go login in SQLite, or a V1 `opencode-go` login in `auth.json`,
and its own usage databases. For V2, UsageDeck follows the active saved account in each directory;
it does not create separate cards for inactive accounts in the same database.

Restart UsageDeck after setting up those directories. Each detected login gets a separate card,
quota view, local history, and customization. An empty directory or one without a Go login does
not create an account card. UsageDeck does not create or sign in to these OpenCode profiles for you.

See the [usage guide](../usage.md) for other account types and Session Kickstart.

## Troubleshooting

- **OpenCode was not detected** — sign in to OpenCode Go or complete a local OpenCode session.
- **Login data could not be read** — check database permissions, then sign in to OpenCode Console again.
- **Login expired** — open OpenCode so it can renew the OAuth token, then refresh UsageDeck.
- **Data directory could not be read** — check the configured data directory and its permissions.
- **Local usage unavailable** — check that the OpenCode data directory and database are readable,
  then refresh.

## OpenCode V2 reference

OpenCode documents the credential migration in its [V2 storage guide](https://opencode.ai/v2/docs/cli/providers#storage).
The reader follows the upstream [credential schema](https://github.com/anomalyco/opencode/blob/v2.0.26/packages/core/src/credential/sql.ts)
and [session storage schema](https://github.com/anomalyco/opencode/blob/v2.0.26/packages/core/src/session/sql.ts).
