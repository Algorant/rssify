# RSSify

Rust CLI for tracking podcast RSS feeds and detecting new episodes.

## Build

```bash
cargo build
```

Binary:

```bash
./target/debug/rssify
```

## Basic Usage

Initialize the app and create the local database:

```bash
./target/debug/rssify init
```

Default database file:

```bash
rssify.duckdb
```

Import feeds from the Pocket Casts OPML export:

```bash
./target/debug/rssify import-opml data/PocketCasts.opml
```

Add a feed manually:

```bash
./target/debug/rssify add-feed https://example.com/feed.xml --title "Example Feed"
```

List feeds:

```bash
./target/debug/rssify list-feeds
./target/debug/rssify list-feeds --json | jq
```

Disable a feed:

```bash
./target/debug/rssify disable-feed --id 3
./target/debug/rssify disable-feed --url https://example.com/feed.xml
```

Remove a feed:

```bash
./target/debug/rssify remove-feed --id 3
./target/debug/rssify remove-feed --url https://example.com/feed.xml
```

Poll feeds:

```bash
./target/debug/rssify poll
```

Post new eligible episodes to Slack while polling:

```bash
./target/debug/rssify poll --slack-webhook-url https://hooks.slack.com/services/...
```

Or set the webhook in the environment:

```bash
export RSSIFY_SLACK_WEBHOOK_URL=https://hooks.slack.com/services/...
./target/debug/rssify poll
```

Notification behavior:

- `init` sets a one-time baseline timestamp.
- RSSify stores all discovered episodes locally.
- RSSify only posts episodes that are unposted and eligible relative to that baseline.
- If an episode has no reliable `published_at`, it is only treated as new when first seen after initialization.
- Episodes are marked as posted only after Slack delivery succeeds, so failed sends remain retryable.

Check status:

```bash
./target/debug/rssify status
./target/debug/rssify status --json | jq
```

Preview a single stored episode update:

```bash
./target/debug/rssify preview-update
./target/debug/rssify preview-update --feed-id 3
./target/debug/rssify preview-update --json | jq
```

Render recent update previews as static HTML:

```bash
./target/debug/rssify preview-html
./target/debug/rssify preview-html --limit 10 --output artifacts/preview/my-updates.html
```
