# RSSify Log

## 2026-04-08

- Refactored feed upserts to use explicit lookup/update/insert instead of DuckDB `ON CONFLICT` on `feeds`, preserving stable feed IDs when episodes already reference a feed.
- Added transactional OPML feed import so repeated imports update existing feeds safely and return inserted vs updated counts.
- Added regression tests covering repeated feed imports with existing episode foreign-key references.

## 2026-04-07

- Added Slack webhook delivery to `poll` via `--slack-webhook-url` or `RSSIFY_SLACK_WEBHOOK_URL`.
- Added explicit notification candidate selection and posted-state updates in DuckDB using the initialization baseline and `first_seen_at` fallback when `published_at` is missing.
- Extracted shared episode rendering into a dedicated module reused by preview output and Slack message text.
- Added tests covering pending notification eligibility, mark-as-posted behavior, and Slack webhook posting.

## 2026-04-05

- Added a `preview-html` CLI command that writes a small static HTML page with recent episode update cards to `artifacts/preview/updates.html` for local review.
- Added a `preview-update` CLI command that renders a local mockup of a single episode notification from DuckDB, defaulting to the latest stored episode and supporting feed- or episode-specific targeting.
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
