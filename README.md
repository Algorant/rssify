# RSSify

Small Rust app for managing podcast RSS feeds, polling them for new episodes, and posting new episodes to Slack.

## Scope

RSSify does four things:

- track podcast RSS subscriptions in DuckDB
- let you import, add, list, enable, disable, and remove feeds
- poll enabled feeds and store discovered episodes
- post eligible unposted episodes to one Slack channel

## Build

```bash
cargo build
./target/debug/rssify --help
```

## Core Commands

Initialize the database:

```bash
./target/debug/rssify init
```

Import feeds from OPML:

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

Enable a feed:

```bash
./target/debug/rssify enable-feed --id 3
./target/debug/rssify enable-feed --url https://example.com/feed.xml
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

Poll feeds and post to Slack:

```bash
./target/debug/rssify poll --slack-webhook-url https://hooks.slack.com/services/...
```

Or:

```bash
export RSSIFY_SLACK_WEBHOOK_URL=https://hooks.slack.com/services/...
./target/debug/rssify poll
```

Check status:

```bash
./target/debug/rssify status
./target/debug/rssify status --json | jq
```

## How It Works

- `init` sets a one-time baseline timestamp.
- RSSify stores all discovered episodes locally.
- RSSify posts episodes that are both unposted and eligible relative to that baseline.
- If an episode has no reliable `published_at`, it is only treated as new when first seen after initialization.
- Episodes are marked as posted only after Slack delivery succeeds.
- If Slack delivery fails, RSSify logs the failure and leaves the episode unposted for a later scheduled run.

## Optional Preview Tools

Preview a single stored episode update in the terminal:

```bash
./target/debug/rssify preview-update
./target/debug/rssify preview-update --feed-id 3
./target/debug/rssify preview-update --json | jq
```

Render recent stored episode updates as static HTML:

```bash
./target/debug/rssify preview-html
./target/debug/rssify preview-html --limit 10 --output artifacts/preview/my-updates.html
```
