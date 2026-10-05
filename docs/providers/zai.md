# Z.ai

UsageDeck tracks quota information for the Z.ai GLM Coding Plan.

## What it tracks

| Metric       | Meaning                                      |
| ------------ | -------------------------------------------- |
| Session      | Usage remaining in the rolling 5-hour window |
| Weekly       | Usage remaining in the rolling 7-day window  |
| Web Searches | Monthly web-search allowance remaining       |

## Setup

Add a Z.ai API key from **Customize** in UsageDeck. Saved keys are kept in the operating system's
credential store. UsageDeck also checks `ZAI_API_KEY`, `GLM_API_KEY`,
`~/.config/usagedeck/zai.json`, and `~/.config/zai/key.json`; a key saved in the app takes priority.

The key must belong to an account with an active GLM Coding Plan.

## Bonus reset cards

The **Rate Limit Resets** row currently shows **Unavailable** with guidance to check **Usage Stats**
in ZCode. ZCode's reset-card status requires both a ZCode login and Coding Plan account
authentication; UsageDeck's API-key connection does not supply them. UsageDeck cannot show these
card counts or send expiry reminders through the current connection. An unavailable count is
different from a confirmed zero.

Reset cards are separate from the automatic five-hour and weekly quota resets. See the
[official ZCode usage guide](https://zcode.z.ai/en/docs/usage-stats) for eligibility and expiry details.

## Troubleshooting

- **Add an API key** — add a key in Customize or provide one through a supported external source.
- **API key invalid** — verify the key at [Z.ai API Keys](https://z.ai/manage-apikey/apikey-list).
- **No active coding plan** — confirm that the account has an active GLM Coding Plan.
- **Usage unavailable** — check the connection and refresh again.
