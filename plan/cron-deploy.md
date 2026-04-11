# Cron Deployment Plan

## Goal

Run RSSify from a homelab server as a simple hourly cron job that polls podcast feeds and posts eligible new episodes to Slack.

## Deployment Shape

- Keep RSSify as a normal Rust binary
- Keep DuckDB as the local state store
- Run `poll` every hour with cron
- Store the Slack webhook as an environment variable on the server
- Keep logs on disk for quick debugging

## Server Layout

Recommended layout on the homelab server:

```text
/srv/rssify/
  rssify                 # built binary
  rssify.duckdb          # local DuckDB state
  .env                   # local secrets, not committed
  logs/
    cron.log
  data/
    feeds.opml           # optional source OPML file
```

## Setup Steps

1. Copy the repo to the homelab server
- clone the repo or copy the working tree

2. Build the binary on the server

```bash
cargo build --release
```

3. Place the binary in a stable path

```bash
cp target/release/rssify /srv/rssify/rssify
```

4. Create the runtime directory structure

```bash
mkdir -p /srv/rssify/logs
```

5. Put the Slack webhook into `/srv/rssify/.env`

```bash
RSSIFY_SLACK_WEBHOOK_URL=https://hooks.slack.com/services/...
```

6. Initialize the database once

```bash
/srv/rssify/rssify --db-path /srv/rssify/rssify.duckdb init
```

7. Import feeds once

```bash
/srv/rssify/rssify --db-path /srv/rssify/rssify.duckdb import-opml /srv/rssify/data/feeds.opml
```

8. Test one manual poll before scheduling

```bash
set -a
. /srv/rssify/.env
set +a
/srv/rssify/rssify --db-path /srv/rssify/rssify.duckdb poll
```

## Cron Job

Run the poll at the top of every hour:

```cron
0 * * * * /bin/sh -lc 'set -a; . /srv/rssify/.env; set +a; /srv/rssify/rssify --db-path /srv/rssify/rssify.duckdb poll >> /srv/rssify/logs/cron.log 2>&1'
```

## Operations

Useful manual commands:

Check status:

```bash
/srv/rssify/rssify --db-path /srv/rssify/rssify.duckdb status
```

Review logs:

```bash
tail -f /srv/rssify/logs/cron.log
```

Re-import feeds after OPML changes:

```bash
/srv/rssify/rssify --db-path /srv/rssify/rssify.duckdb import-opml /srv/rssify/data/feeds.opml
```

## Notes

- Keep the server clock and timezone configured correctly.
- DuckDB should live on persistent disk, not temporary storage.
- Only one cron poll should run at a time against the same DuckDB file.
- If the DB is locked, check for another running RSSify process or an open `duckdb` shell session.
- Back up `rssify.duckdb` if the feed and episode history matter.

## Future Upgrade Path

If cron becomes too limited later, the easiest upgrade is probably:

- `systemd` service + timer on the same server

That keeps the same binary, `.env`, and DuckDB layout while giving better supervision and logs.
