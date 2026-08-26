# Command Code

UsageDeck reads the local session created by the Command Code CLI and tracks the subscription
windows reported by Command Code.

## What it tracks

| Metric        | Meaning                                                        |
| ------------- | -------------------------------------------------------------- |
| Session       | Credit usage in the rolling 5-hour subscription window         |
| Weekly        | Credit usage in the rolling 7-day subscription window          |
| Monthly       | Subscription usage and remaining credits for the billing cycle |
| Extra Credits | Remaining purchased or top-up credits, when available          |

## Setup

Install the Command Code CLI and sign in:

```sh
command-code login
```

UsageDeck reads `~/.commandcode/auth.json` locally. The session key remains on your device and is
used only to request Command Code usage data.

For current individual plans, the monthly meter uses the plan allocation published by Command Code
and resets at the end of the subscription billing cycle. If a custom or future plan does not expose
a known allocation, UsageDeck shows the remaining monthly balance instead of guessing a limit.

## Troubleshooting

- **Not logged in** — run `command-code login`, then refresh UsageDeck.
- **Login expired** — sign in again with `command-code login`.
- **Usage unavailable** — check the connection and refresh again.
