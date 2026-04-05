# RSSify Log

## 2026-04-05

- Fixed metadata extraction so the normal `feed-rs` path now merges podcast-specific RSS fields like duration, episode number, artwork URL, and summary from the raw XML pass instead of only populating them on the lossy fallback path.
- Verified the fix with a new parser test, and reran `cargo test` and `cargo build` successfully.

## 2026-04-04

- Expanded stored episode metadata to include duration, episode number, artwork URL, and summary/description.
- Added a first-pass focused test suite covering OPML parsing, DuckDB init/state persistence, episode dedupe/baseline behavior, and lossy malformed-feed recovery.
- Replaced SQLite with DuckDB as the local state store.
- Removed the old local SQLite database and verified the CLI flow against a fresh DuckDB file.
- Known issue: JRE RSS feed appears to contain malformed or corrupted XML around episode `#818 - Mike Schmidt`, even though current episodes are much newer.
- Handling: poller now retries once and falls back to a lossy recent-item parse so old tail corruption does not block current episode detection.
- Created the Rust CLI scaffold.
- Added DuckDB-backed state.
- Added `init`, `import-opml`, `add-feed`, `list-feeds`, `disable-feed`, `remove-feed`, `poll`, and `status`.
- Added OPML import from [`data/PocketCasts.opml`](/home/ivan/dev/projects/rssify/data/PocketCasts.opml).
- Added polling, episode storage, and JSON status output.
- Added poll progress output and failed-feed snapshot capture under [`artifacts/failed-feeds`](/home/ivan/dev/projects/rssify/artifacts/failed-feeds).
