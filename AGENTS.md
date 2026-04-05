# AGENTS

## Project Context

- RSSify is a Rust CLI for tracking podcast RSS feeds from an OPML source, polling for new episodes, and reporting feed status.
- DuckDB is the local state store.
- The baseline for "new" content is the app initialization timestamp.
- The first monitoring surface is terminal output and logs.
- Slack delivery is planned later and should be one message per new episode.

## Feed Handling Notes

- Feed lists must remain updateable over time. Manual RSS feed addition is a first-class workflow.
- The JRE feed has shown malformed or corrupted XML in its older tail around episode `#818 - Mike Schmidt`.
- Old-tail corruption should not block detection of recent episodes.
- If the DuckDB file is locked, assume the user is probably in the `duckdb` CLI or another local session using that file; tell them and ask them to exit before retrying.

## Doc Rules

- Keep planning docs under [`plan/`](/home/ivan/dev/projects/rssify/plan).
- [`plan/log.md`](/home/ivan/dev/projects/rssify/plan/log.md) is append-only in reverse chronological order: add new entries at the top, do not rewrite history unless correcting a factual mistake.
- [`plan/log.md`](/home/ivan/dev/projects/rssify/plan/log.md) is for completed changes, decisions, and notable issues.
- [`plan/todo.md`](/home/ivan/dev/projects/rssify/plan/todo.md) is for remaining work only and should be updated as tasks are completed or added.

## Usage Preference

- Prefer the built binary workflow in docs and examples: `./target/debug/rssify`, not repeated `cargo run`.
