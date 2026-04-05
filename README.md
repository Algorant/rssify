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

Check status:

```bash
./target/debug/rssify status
./target/debug/rssify status --json | jq
```
