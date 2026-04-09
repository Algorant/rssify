# RSSify Plan

## Goal

Build a small Rust app that tracks podcast RSS feeds, polls them regularly, stores local state in DuckDB, and posts newly discovered episodes to one Slack channel.

## Core Requirements

1. Feed management
- Import feeds from OPML
- Add feeds manually
- List feeds
- Enable or disable feeds
- Remove feeds
- Keep feed state easy to update over time

2. Polling and state
- Periodically fetch enabled feeds
- Store discovered episodes locally
- Deduplicate episodes reliably
- Use a one-time initialization timestamp as the baseline for what counts as new
- If `published_at` is missing, treat an episode as new only if first seen after initialization

3. Slack delivery
- Post each eligible unposted episode to one Slack channel
- Mark episodes as posted only after successful delivery
- Leave failed deliveries retryable

4. Runtime
- Run locally as a CLI during development
- Run regularly in production from a simple scheduled environment such as a cron job, cloud worker, or Lambda-style function

## Minimal Architecture

- Rust CLI app
- DuckDB for local durable state
- RSS and OPML parsing in-process
- Slack webhook delivery
- One executable that can be run on a schedule

## Main Workflows

### Setup
- Initialize the database
- Import an OPML file or add feeds manually
- Configure Slack webhook
- Run the poll command on a schedule

### Ongoing management
- Re-import OPML to add or update feeds
- Manually add, remove, enable, or disable feeds as needed
- Check status when debugging feed or delivery issues

### Poll cycle
- Fetch enabled feeds
- Store discovered episodes
- Select unposted episodes that are eligible relative to the baseline
- Post them to Slack
- Mark successful posts as posted

## Commands

- `init`
- `import-opml`
- `add-feed`
- `list-feeds`
- `disable-feed`
- `remove-feed`
- `poll`
- `status`

Optional preview commands can remain as local tooling, but they are not part of the core product.

## Deployment Direction

Keep the app runnable as a normal binary:
- local development: run manually
- production: run on a schedule

Good enough deployment targets:
- small VM with cron
- GitHub Actions on a schedule if state handling is acceptable
- cloud worker or Lambda-style runner if DuckDB file access and persistence are handled cleanly

## Constraints

- Prefer Rust and DuckDB unless a concrete limitation appears
- Keep the system small and operationally simple
- Avoid unnecessary architecture or services
