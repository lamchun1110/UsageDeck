# Z.ai

UsageDeck tracks quota information for the Z.ai GLM Coding Plan.

## What it tracks

| Metric            | Meaning                                                        |
| ----------------- | -------------------------------------------------------------- |
| Session           | Usage remaining in the rolling 5-hour window                   |
| Weekly            | Usage remaining in the rolling 7-day window                    |
| Web Searches      | Monthly web-search allowance remaining                         |
| Rate Limit Resets | Available personal ZCode reset cards and reported expiry dates |

## Setup

Add a Z.ai API key from **Customize** in UsageDeck. Saved keys are kept in the operating system's
credential store. UsageDeck also checks `ZAI_API_KEY`, `GLM_API_KEY`,
`~/.config/usagedeck/zai.json`, `~/.config/openquota/zai.json`, and `~/.config/zai/key.json`;
a key saved in the app takes priority. These external sources apply to the default Z.ai card.
Named accounts use their own saved keys; see the [multiple-account setup](../usage.md#named-api-key-accounts).

The key must belong to an account with an active GLM Coding Plan.

## Bonus reset cards

The **Rate Limit Resets** row reads reset status from your local ZCode login. Sign in to ZCode
with the same **personal Coding Plan** account and use that account's ZCode-managed API key on
the UsageDeck card. UsageDeck matches the API key before sending a request, so a different
account's reset cards cannot appear on this card. Team and start-plan connections are not supported.

If your card uses a different API key, choose **Use ZCode API key** in the unavailable reset row.
This replaces that card's saved API key with the signed-in ZCode account's personal-plan key;
both quota usage and reset cards then refer to that account. The key stays in the operating
system credential store and never enters the frontend. You can restore your previous key in
**Customize** at any time.

UsageDeck reads `~/.zcode/v2/credentials.json` (or `ZCODE_HOME` / `ZCODE_DESKTOP_HOME_DIR`) without
changing it, and decrypts ZCode's credential format in memory. `ZCODE_CREDENTIAL_SECRET`, if
used by ZCode, must also be supplied to the UsageDeck process. It requests only reset **status**;
it does not redeem cards or mark reset history as read.

The count combines available five-hour and weekly cards, excludes expired cards, and shows
the earliest expiry. Open the row to see all dates. Enable **Settings → Notifications →
Resets expiring** for expiry reminders. An unavailable login or mismatched API key shows
**Unavailable**; a successful response with no available cards shows **0 available**.
If ZCode's login expires, reopen ZCode and refresh UsageDeck.

Reset cards are separate from the automatic five-hour and weekly quota resets. See the
[official ZCode usage guide](https://zcode.z.ai/en/docs/usage-stats) for eligibility and expiry details.

## Troubleshooting

- **Add an API key** — add a key in Customize or provide one through a supported external source.
- **API key invalid** — verify the key at [Z.ai API Keys](https://z.ai/manage-apikey/apikey-list).
- **No active coding plan** — confirm that the account has an active GLM Coding Plan.
- **Usage unavailable** — check the connection and refresh again.
