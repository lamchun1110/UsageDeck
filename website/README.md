# UsageDeck website

This directory is the static website served at <https://usagedeck.app/>. GitHub Pages publishes it
through `.github/workflows/pages.yml` when website changes are pushed to `main`; there is no
separate website build.

Preview it locally from the repository root:

```sh
python3 -m http.server 4174 --bind 127.0.0.1 --directory website
```

Open <http://127.0.0.1:4174/> and check the desktop and mobile layouts, navigation, image loading,
and the privacy page before publishing changes. Verify feature descriptions against the current
application and [usage guide](../docs/usage.md).

## Product screenshots

The current previews were captured from the application's Svelte interface using example data
and an isolated local backend. They do not contain real credentials, account identifiers, or
usage records. The page captions identify the example data and configuration.

- `assets/usagedeck-dashboard.jpg`: dashboard with quota, pacing, and reset summaries (560 × 650).
- `assets/usagedeck-session-kickstart.jpg`: Codex Session Kickstart controls (560 × 330).
- `assets/usagedeck-reset-credits.jpg`: Codex reset count and expiry details (400 × 440).

Refresh these captures when the corresponding interface changes. Show the actual controls with
example data, preserve legibility, and update image dimensions and alternative text in
`index.html` when needed. Expiry dates and counts in the images are illustrative; provider
availability varies by account and plan.
