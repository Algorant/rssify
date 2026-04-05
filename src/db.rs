use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use duckdb::{Connection, OptionalExt, params};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct FeedRecord {
    pub id: i64,
    pub url: String,
    pub title: Option<String>,
    pub enabled: bool,
    pub added_at: String,
    pub last_fetched_at: Option<String>,
    pub last_successful_fetch_at: Option<String>,
    pub last_http_status: Option<i64>,
    pub last_error: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_feed_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusSummary {
    pub initialized_at: String,
    pub feed_count: i64,
    pub enabled_feed_count: i64,
    pub episode_count: i64,
    pub unseen_since_baseline_count: i64,
    pub last_poll_started_at: Option<String>,
    pub last_poll_completed_at: Option<String>,
    pub last_poll_new_episodes: Option<i64>,
    pub feeds: Vec<FeedRecord>,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database at {}", path.display()))?;
        Ok(Self { conn })
    }

    pub fn close(self) -> Result<()> {
        self.conn
            .close()
            .map_err(|(_, err)| anyhow!(err))
    }

    pub fn ensure_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE SEQUENCE IF NOT EXISTS feeds_id_seq START 1;
            CREATE SEQUENCE IF NOT EXISTS episodes_id_seq START 1;
            CREATE SEQUENCE IF NOT EXISTS poll_runs_id_seq START 1;

            CREATE TABLE IF NOT EXISTS app_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS feeds (
                id BIGINT PRIMARY KEY DEFAULT nextval('feeds_id_seq'),
                url TEXT NOT NULL UNIQUE,
                title TEXT,
                enabled BOOLEAN NOT NULL DEFAULT TRUE,
                added_at TEXT NOT NULL,
                last_fetched_at TEXT,
                last_successful_fetch_at TEXT,
                last_http_status BIGINT,
                last_error TEXT,
                etag TEXT,
                last_modified TEXT,
                last_feed_title TEXT
            );

            CREATE TABLE IF NOT EXISTS episodes (
                id BIGINT PRIMARY KEY DEFAULT nextval('episodes_id_seq'),
                feed_id BIGINT NOT NULL REFERENCES feeds(id),
                dedupe_key TEXT NOT NULL,
                guid TEXT,
                title TEXT,
                summary TEXT,
                link TEXT,
                enclosure_url TEXT,
                artwork_url TEXT,
                duration_seconds BIGINT,
                episode_number BIGINT,
                published_at TEXT,
                first_seen_at TEXT NOT NULL,
                notified_at TEXT,
                UNIQUE(feed_id, dedupe_key)
            );

            CREATE TABLE IF NOT EXISTS poll_runs (
                id BIGINT PRIMARY KEY DEFAULT nextval('poll_runs_id_seq'),
                started_at TEXT NOT NULL,
                completed_at TEXT,
                feeds_checked BIGINT NOT NULL DEFAULT 0,
                feeds_failed BIGINT NOT NULL DEFAULT 0,
                new_episodes_found BIGINT NOT NULL DEFAULT 0
            );
            ",
        )?;

        self.add_episode_columns_if_missing()?;
        Ok(())
    }

    pub fn initialize(&self, now: DateTime<Utc>) -> Result<bool> {
        self.ensure_schema()?;

        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO app_state (key, value) VALUES ('initialized_at', ?1)",
            params![now.to_rfc3339()],
        )?;

        Ok(inserted > 0)
    }

    pub fn initialized_at(&self) -> Result<DateTime<Utc>> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM app_state WHERE key = 'initialized_at'",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let raw = value.ok_or_else(|| anyhow!("database is not initialized; run `rssify init`"))?;
        let parsed = DateTime::parse_from_rfc3339(&raw)
            .with_context(|| format!("invalid initialized_at timestamp in database: {raw}"))?;
        Ok(parsed.with_timezone(&Utc))
    }

    pub fn upsert_feed(&self, url: &str, title: Option<&str>, now: DateTime<Utc>) -> Result<i64> {
        self.conn.query_row(
            "
            INSERT INTO feeds (url, title, added_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(url) DO UPDATE SET
                title = COALESCE(excluded.title, feeds.title)
            RETURNING id
            ",
            params![url, title, now.to_rfc3339()],
            |row| row.get(0),
        ).map_err(Into::into)
    }

    pub fn disable_feed_by_id(&self, id: i64) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE feeds SET enabled = 0 WHERE id = ?1",
            params![id],
        )?;
        Ok(updated > 0)
    }

    pub fn disable_feed_by_url(&self, url: &str) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE feeds SET enabled = 0 WHERE url = ?1",
            params![url],
        )?;
        Ok(updated > 0)
    }

    pub fn remove_feed_by_id(&self, id: i64) -> Result<bool> {
        let deleted = self
            .conn
            .execute("DELETE FROM feeds WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    pub fn remove_feed_by_url(&self, url: &str) -> Result<bool> {
        let deleted = self
            .conn
            .execute("DELETE FROM feeds WHERE url = ?1", params![url])?;
        Ok(deleted > 0)
    }

    pub fn list_feeds(&self) -> Result<Vec<FeedRecord>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT
                id,
                url,
                title,
                CAST(enabled AS BIGINT) AS enabled,
                added_at,
                last_fetched_at,
                last_successful_fetch_at,
                last_http_status,
                last_error,
                etag,
                last_modified,
                last_feed_title
            FROM feeds
            ORDER BY id ASC
            ",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(FeedRecord {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                added_at: row.get(4)?,
                last_fetched_at: row.get(5)?,
                last_successful_fetch_at: row.get(6)?,
                last_http_status: row.get(7)?,
                last_error: row.get(8)?,
                etag: row.get(9)?,
                last_modified: row.get(10)?,
                last_feed_title: row.get(11)?,
            })
        })?;

        rows.collect::<duckdb::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn enabled_feeds(&self) -> Result<Vec<FeedRecord>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT
                id,
                url,
                title,
                CAST(enabled AS BIGINT) AS enabled,
                added_at,
                last_fetched_at,
                last_successful_fetch_at,
                last_http_status,
                last_error,
                etag,
                last_modified,
                last_feed_title
            FROM feeds
            WHERE enabled = 1
            ORDER BY id ASC
            ",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(FeedRecord {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                added_at: row.get(4)?,
                last_fetched_at: row.get(5)?,
                last_successful_fetch_at: row.get(6)?,
                last_http_status: row.get(7)?,
                last_error: row.get(8)?,
                etag: row.get(9)?,
                last_modified: row.get(10)?,
                last_feed_title: row.get(11)?,
            })
        })?;

        rows.collect::<duckdb::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn start_poll_run(&self, started_at: DateTime<Utc>) -> Result<i64> {
        self.conn
            .query_row(
                "INSERT INTO poll_runs (started_at) VALUES (?1) RETURNING id",
                params![started_at.to_rfc3339()],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn finish_poll_run(
        &self,
        run_id: i64,
        completed_at: DateTime<Utc>,
        feeds_checked: i64,
        feeds_failed: i64,
        new_episodes_found: i64,
    ) -> Result<()> {
        self.conn.execute(
            "
            UPDATE poll_runs
            SET
                completed_at = ?2,
                feeds_checked = ?3,
                feeds_failed = ?4,
                new_episodes_found = ?5
            WHERE id = ?1
            ",
            params![
                run_id,
                completed_at.to_rfc3339(),
                feeds_checked,
                feeds_failed,
                new_episodes_found
            ],
        )?;
        Ok(())
    }

    pub fn update_feed_fetch_success(
        &self,
        feed_id: i64,
        fetched_at: DateTime<Utc>,
        http_status: i64,
        etag: Option<&str>,
        last_modified: Option<&str>,
        feed_title: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "
            UPDATE feeds
            SET
                last_fetched_at = ?2,
                last_successful_fetch_at = ?2,
                last_http_status = ?3,
                last_error = NULL,
                etag = COALESCE(?4, etag),
                last_modified = COALESCE(?5, last_modified),
                last_feed_title = COALESCE(?6, last_feed_title)
            WHERE id = ?1
            ",
            params![
                feed_id,
                fetched_at.to_rfc3339(),
                http_status,
                etag,
                last_modified,
                feed_title
            ],
        )?;
        Ok(())
    }

    pub fn update_feed_fetch_error(
        &self,
        feed_id: i64,
        fetched_at: DateTime<Utc>,
        http_status: Option<i64>,
        error: &str,
    ) -> Result<()> {
        self.conn.execute(
            "
            UPDATE feeds
            SET
                last_fetched_at = ?2,
                last_http_status = ?3,
                last_error = ?4
            WHERE id = ?1
            ",
            params![feed_id, fetched_at.to_rfc3339(), http_status, error],
        )?;
        Ok(())
    }

    pub fn insert_episode_if_new(&self, episode: &NewEpisode<'_>) -> Result<bool> {
        let inserted = self.conn.execute(
            "
            INSERT OR IGNORE INTO episodes (
                feed_id,
                dedupe_key,
                guid,
                title,
                summary,
                link,
                enclosure_url,
                artwork_url,
                duration_seconds,
                episode_number,
                published_at,
                first_seen_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ",
            params![
                episode.feed_id,
                episode.dedupe_key,
                episode.guid,
                episode.title,
                episode.summary,
                episode.link,
                episode.enclosure_url,
                episode.artwork_url,
                episode.duration_seconds,
                episode.episode_number,
                episode.published_at,
                episode.first_seen_at.to_rfc3339()
            ],
        )?;
        Ok(inserted > 0)
    }

    pub fn status_summary(&self) -> Result<StatusSummary> {
        let initialized_at = self.initialized_at()?.to_rfc3339();
        let feed_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM feeds", [], |row| row.get(0))?;
        let enabled_feed_count =
            self.conn
                .query_row("SELECT COUNT(*) FROM feeds WHERE enabled = 1", [], |row| row.get(0))?;
        let episode_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM episodes", [], |row| row.get(0))?;
        let unseen_since_baseline_count = self.conn.query_row(
            "
            SELECT COUNT(*)
            FROM episodes
            WHERE notified_at IS NULL
              AND (
                published_at IS NULL
                OR published_at >= (SELECT value FROM app_state WHERE key = 'initialized_at')
              )
            ",
            [],
            |row| row.get(0),
        )?;

        let last_poll = self
            .conn
            .query_row(
                "
                SELECT started_at, completed_at, new_episodes_found
                FROM poll_runs
                ORDER BY id DESC
                LIMIT 1
                ",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .optional()?;

        let (last_poll_started_at, last_poll_completed_at, last_poll_new_episodes) =
            last_poll.unwrap_or((None, None, None));

        Ok(StatusSummary {
            initialized_at,
            feed_count,
            enabled_feed_count,
            episode_count,
            unseen_since_baseline_count,
            last_poll_started_at,
            last_poll_completed_at,
            last_poll_new_episodes,
            feeds: self.list_feeds()?,
        })
    }

    fn add_episode_columns_if_missing(&self) -> Result<()> {
        for ddl in [
            "ALTER TABLE episodes ADD COLUMN IF NOT EXISTS summary TEXT",
            "ALTER TABLE episodes ADD COLUMN IF NOT EXISTS artwork_url TEXT",
            "ALTER TABLE episodes ADD COLUMN IF NOT EXISTS duration_seconds BIGINT",
            "ALTER TABLE episodes ADD COLUMN IF NOT EXISTS episode_number BIGINT",
        ] {
            self.conn.execute(ddl, [])?;
        }

        Ok(())
    }
}

pub struct NewEpisode<'a> {
    pub feed_id: i64,
    pub dedupe_key: &'a str,
    pub guid: Option<&'a str>,
    pub title: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub link: Option<&'a str>,
    pub enclosure_url: Option<&'a str>,
    pub artwork_url: Option<&'a str>,
    pub duration_seconds: Option<i64>,
    pub episode_number: Option<i64>,
    pub published_at: Option<&'a str>,
    pub first_seen_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::{Database, NewEpisode};
    use chrono::Utc;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn init_persists_state_across_reopen() {
        let path = temp_db_path("init-persists");

        let db = Database::open(&path).expect("db should open");
        let now = Utc::now();
        assert!(db.initialize(now).expect("init should succeed"));
        db.close().expect("db should close");

        let reopened = Database::open(&path).expect("db should reopen");
        let summary = reopened.status_summary().expect("status should load");
        assert_eq!(summary.initialized_at, now.to_rfc3339());
        assert_eq!(summary.feed_count, 0);
        reopened.close().expect("db should close");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn upsert_and_dedupe_episode_work() {
        let path = temp_db_path("dedupe");

        let db = Database::open(&path).expect("db should open");
        db.initialize(Utc::now()).expect("init should succeed");
        let feed_id = db
            .upsert_feed("https://example.com/feed.xml", Some("Example"), Utc::now())
            .expect("feed should upsert");

        let now = Utc::now();
        let first = db
            .insert_episode_if_new(&NewEpisode {
                feed_id,
                dedupe_key: "episode-1",
                guid: Some("episode-1"),
                title: Some("Episode One"),
                summary: Some("Episode summary"),
                link: Some("https://example.com/episodes/1"),
                enclosure_url: None,
                artwork_url: Some("https://example.com/art.jpg"),
                duration_seconds: Some(3600),
                episode_number: Some(1),
                published_at: Some("2026-04-05T00:00:00Z"),
                first_seen_at: now,
            })
            .expect("first insert should succeed");
        let second = db
            .insert_episode_if_new(&NewEpisode {
                feed_id,
                dedupe_key: "episode-1",
                guid: Some("episode-1"),
                title: Some("Episode One"),
                summary: Some("Episode summary"),
                link: Some("https://example.com/episodes/1"),
                enclosure_url: None,
                artwork_url: Some("https://example.com/art.jpg"),
                duration_seconds: Some(3600),
                episode_number: Some(1),
                published_at: Some("2026-04-05T00:00:00Z"),
                first_seen_at: now,
            })
            .expect("second insert should succeed");

        assert!(first);
        assert!(!second);

        let summary = db.status_summary().expect("status should load");
        assert_eq!(summary.feed_count, 1);
        assert_eq!(summary.episode_count, 1);

        db.close().expect("db should close");
        let _ = fs::remove_file(path);
    }

    fn temp_db_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should work")
            .as_nanos();
        std::env::temp_dir().join(format!("rssify-{label}-{nanos}.duckdb"))
    }
}
