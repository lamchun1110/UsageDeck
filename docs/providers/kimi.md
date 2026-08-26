# Kimi

UsageDeck tracks the Session (rolling five-hour) and Weekly quotas of a Kimi Code membership.

## What it tracks

| Metric  | Meaning                                      |
| ------- | -------------------------------------------- |
| Session | Usage remaining in the rolling 5-hour window |
| Weekly  | Usage remaining in the rolling 7-day window  |

## Setup

Create a Kimi Code API key in the [Kimi Code Console](https://www.kimi.com/code/console), then add
it in **Customize** in UsageDeck. Saved keys are stored in the operating system's credential store.
UsageDeck also checks `KIMI_API_KEY` and `~/.config/usagedeck/kimi.json`; a key saved in the app
takes priority.

## Choosing an endpoint

Kimi Code serves the same coding API from two domains. Pick the one your key belongs to under
**Connection → Endpoint** in Customize:

| Choice     | Endpoint                                   |
| ---------- | ------------------------------------------ |
| `kimi.com` | `https://api.kimi.com/coding/v1` (default) |
| `kimi.ai`  | `https://api.kimi.ai/coding/v1`            |

A key issued for one domain is not accepted by the other, so switch the endpoint if a key you know
is valid reports **API key invalid**. Kimi Code keys are also not interchangeable with Kimi Open
Platform keys.

## Troubleshooting

- **Add an API key** — add a Kimi Code key in Customize or provide it through a supported external source.
- **API key invalid** — confirm the **Endpoint** matches the domain the key came from, then create
  or verify the key in the [Kimi Code Console](https://www.kimi.com/code/console).
- **Usage unavailable** — check the connection and refresh again.
