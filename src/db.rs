use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use duckdb::{params, Connection, OptionalExt};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedUpsertOutcome {
    Inserted(i64),
    Updated(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeedImportSummary {
    pub inserted: i64,
    pub updated: i64,
}

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

#[derive(Debug, Clone, Serialize)]
pub struct EpisodePreview {
    pub episode_id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub feed_url: String,
    pub episode_title: Option<String>,
    pub summary: Option<String>,
    pub link: Option<String>,
    pub enclosure_url: Option<String>,
    pub artwork_url: Option<String>,
    pub duration_seconds: Option<i64>,
    pub episode_number: Option<i64>,
    pub published_at: Option<String>,
    pub first_seen_at: String,
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
        self.conn.close().map_err(|(_, err)| anyhow!(err))
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
        Ok(self.upsert_feed_with_outcome(url, title, now)?.id())
    }

    pub fn import_feeds(
        &self,
        feeds: &[(String, Option<String>)],
        now: DateTime<Utc>,
    ) -> Result<FeedImportSummary> {
        self.conn.execute_batch("BEGIN TRANSACTION")?;

        let result = (|| {
            let mut inserted = 0_i64;
            let mut updated = 0_i64;

            for (url, title) in feeds {
                match self.upsert_feed_with_outcome(url, title.as_deref(), now)? {
                    FeedUpsertOutcome::Inserted(_) => inserted += 1,
                    FeedUpsertOutcome::Updated(_) => updated += 1,
                }
            }

            Ok(FeedImportSummary { inserted, updated })
        })();

        match result {
            Ok(summary) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(summary)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    fn upsert_feed_with_outcome(
        &self,
        url: &str,
        title: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<FeedUpsertOutcome> {
        let existing_id: Option<i64> = self
            .conn
            .query_row("SELECT id FROM feeds WHERE url = ?1", params![url], |row| {
                row.get(0)
            })
            .optional()?;

        if let Some(id) = existing_id {
            self.conn.execute(
                "UPDATE feeds SET title = COALESCE(?2, title) WHERE id = ?1",
                params![id, title],
            )?;
            Ok(FeedUpsertOutcome::Updated(id))
        } else {
            let id = self.conn.query_row(
                "INSERT INTO feeds (url, title, added_at) VALUES (?1, ?2, ?3) RETURNING id",
                params![url, title, now.to_rfc3339()],
                |row| row.get(0),
            )?;
            Ok(FeedUpsertOutcome::Inserted(id))
        }
    }

    pub fn disable_feed_by_id(&self, id: i64) -> Result<bool> {
        let updated = self
            .conn
            .execute("UPDATE feeds SET enabled = 0 WHERE id = ?1", params![id])?;
        Ok(updated > 0)
    }

    pub fn enable_feed_by_id(&self, id: i64) -> Result<bool> {
        let updated = self
            .conn
            .execute("UPDATE feeds SET enabled = 1 WHERE id = ?1", params![id])?;
        Ok(updated > 0)
    }

    pub fn disable_feed_by_url(&self, url: &str) -> Result<bool> {
        let updated = self
            .conn
            .execute("UPDATE feeds SET enabled = 0 WHERE url = ?1", params![url])?;
        Ok(updated > 0)
    }

    pub fn enable_feed_by_url(&self, url: &str) -> Result<bool> {
        let updated = self
            .conn
            .execute("UPDATE feeds SET enabled = 1 WHERE url = ?1", params![url])?;
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

        rows.collect::<duckdb::Result<Vec<_>>>().map_err(Into::into)
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

        rows.collect::<duckdb::Result<Vec<_>>>().map_err(Into::into)
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
                .query_row("SELECT COUNT(*) FROM feeds WHERE enabled = 1", [], |row| {
                    row.get(0)
                })?;
        let episode_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM episodes", [], |row| row.get(0))?;
        let unseen_since_baseline_count = self.conn.query_row(
            "
            SELECT COUNT(*)
            FROM episodes
            WHERE notified_at IS NULL
              AND (
                published_at >= (SELECT value FROM app_state WHERE key = 'initialized_at')
                OR (
                    published_at IS NULL
                    AND first_seen_at >= (SELECT value FROM app_state WHERE key = 'initialized_at')
                )
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

    pub fn latest_episode_preview(&self) -> Result<Option<EpisodePreview>> {
        self.preview_query(
            &format!(
                "{} ORDER BY e.published_at DESC NULLS LAST, e.first_seen_at DESC, e.id DESC LIMIT 1",
                episode_preview_select_sql()
            ),
            [],
        )
    }

    pub fn latest_episode_preview_for_feed(&self, feed_id: i64) -> Result<Option<EpisodePreview>> {
        self.preview_query(
            &format!(
                "{} WHERE f.id = ?1 ORDER BY e.published_at DESC NULLS LAST, e.first_seen_at DESC, e.id DESC LIMIT 1",
                episode_preview_select_sql()
            ),
            params![feed_id],
        )
    }

    pub fn episode_preview_by_id(&self, episode_id: i64) -> Result<Option<EpisodePreview>> {
        self.preview_query(
            &format!("{} WHERE e.id = ?1 LIMIT 1", episode_preview_select_sql()),
            params![episode_id],
        )
    }

    pub fn recent_episode_previews(&self, limit: usize) -> Result<Vec<EpisodePreview>> {
        let mut stmt = self.conn.prepare(&format!(
            "{} ORDER BY e.published_at DESC NULLS LAST, e.first_seen_at DESC, e.id DESC LIMIT ?1",
            episode_preview_select_sql()
        ))?;

        let rows = stmt.query_map(params![limit as i64], map_episode_preview_row)?;

        rows.collect::<duckdb::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn pending_episode_notifications(&self) -> Result<Vec<EpisodePreview>> {
        let mut stmt = self.conn.prepare(
            &format!(
                "{}
                WHERE e.notified_at IS NULL
                  AND (
                    e.published_at >= (SELECT value FROM app_state WHERE key = 'initialized_at')
                    OR (
                        e.published_at IS NULL
                        AND e.first_seen_at >= (SELECT value FROM app_state WHERE key = 'initialized_at')
                    )
                  )
                ORDER BY COALESCE(e.published_at, e.first_seen_at) ASC, e.first_seen_at ASC, e.id ASC",
                episode_preview_select_sql()
            ),
        )?;

        let rows = stmt.query_map([], map_episode_preview_row)?;

        rows.collect::<duckdb::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn mark_episode_notified(&self, episode_id: i64, notified_at: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE episodes SET notified_at = ?2 WHERE id = ?1",
            params![episode_id, notified_at.to_rfc3339()],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn episode_notified_at(&self, episode_id: i64) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT notified_at FROM episodes WHERE id = ?1",
                params![episode_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
            .map(|value| value.flatten())
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

    fn preview_query<P>(&self, sql: &str, params: P) -> Result<Option<EpisodePreview>>
    where
        P: duckdb::Params,
    {
        self.conn
            .query_row(sql, params, map_episode_preview_row)
            .optional()
            .map_err(Into::into)
    }
}

impl FeedUpsertOutcome {
    fn id(self) -> i64 {
        match self {
            FeedUpsertOutcome::Inserted(id) | FeedUpsertOutcome::Updated(id) => id,
        }
    }
}

fn episode_preview_select_sql() -> &'static str {
    "
    SELECT
        e.id,
        f.id,
        COALESCE(f.title, f.last_feed_title, f.url) AS feed_title,
        f.url,
        e.title,
        e.summary,
        e.link,
        e.enclosure_url,
        e.artwork_url,
        e.duration_seconds,
        e.episode_number,
        e.published_at,
        e.first_seen_at
    FROM episodes e
    JOIN feeds f ON f.id = e.feed_id
    "
}

fn map_episode_preview_row(row: &duckdb::Row<'_>) -> duckdb::Result<EpisodePreview> {
    Ok(EpisodePreview {
        episode_id: row.get(0)?,
        feed_id: row.get(1)?,
        feed_title: row.get(2)?,
        feed_url: row.get(3)?,
        episode_title: row.get(4)?,
        summary: row.get(5)?,
        link: row.get(6)?,
        enclosure_url: row.get(7)?,
        artwork_url: row.get(8)?,
        duration_seconds: row.get(9)?,
        episode_number: row.get(10)?,
        published_at: row.get(11)?,
        first_seen_at: row.get(12)?,
    })
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
    use chrono::{DateTime, Utc};
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

    #[test]
    fn latest_episode_preview_returns_most_recent_episode() {
        let path = temp_db_path("preview");

        let db = Database::open(&path).expect("db should open");
        db.initialize(Utc::now()).expect("init should succeed");
        let feed_id = db
            .upsert_feed("https://example.com/feed.xml", Some("Example"), Utc::now())
            .expect("feed should upsert");

        db.insert_episode_if_new(&NewEpisode {
            feed_id,
            dedupe_key: "episode-1",
            guid: Some("episode-1"),
            title: Some("Episode One"),
            summary: Some("Older episode"),
            link: Some("https://example.com/episodes/1"),
            enclosure_url: None,
            artwork_url: Some("https://example.com/1.jpg"),
            duration_seconds: Some(1800),
            episode_number: Some(1),
            published_at: Some("2026-04-04T00:00:00Z"),
            first_seen_at: Utc::now(),
        })
        .expect("first episode should insert");

        db.insert_episode_if_new(&NewEpisode {
            feed_id,
            dedupe_key: "episode-2",
            guid: Some("episode-2"),
            title: Some("Episode Two"),
            summary: Some("Newer episode"),
            link: Some("https://example.com/episodes/2"),
            enclosure_url: None,
            artwork_url: Some("https://example.com/2.jpg"),
            duration_seconds: Some(3600),
            episode_number: Some(2),
            published_at: Some("2026-04-05T00:00:00Z"),
            first_seen_at: Utc::now(),
        })
        .expect("second episode should insert");

        let preview = db
            .latest_episode_preview()
            .expect("preview should load")
            .expect("preview should exist");

        assert_eq!(preview.feed_id, feed_id);
        assert_eq!(preview.feed_title, "Example");
        assert_eq!(preview.episode_title.as_deref(), Some("Episode Two"));
        assert_eq!(preview.duration_seconds, Some(3600));
        assert_eq!(preview.episode_number, Some(2));

        db.close().expect("db should close");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn pending_notifications_follow_baseline_and_notified_state() {
        let path = temp_db_path("pending-notifications");

        let db = Database::open(&path).expect("db should open");
        let baseline = DateTime::parse_from_rfc3339("2026-04-05T00:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&Utc);
        db.initialize(baseline).expect("init should succeed");
        let feed_id = db
            .upsert_feed("https://example.com/feed.xml", Some("Example"), Utc::now())
            .expect("feed should upsert");

        db.insert_episode_if_new(&NewEpisode {
            feed_id,
            dedupe_key: "old-published",
            guid: Some("old-published"),
            title: Some("Old Published"),
            summary: None,
            link: Some("https://example.com/old-published"),
            enclosure_url: None,
            artwork_url: None,
            duration_seconds: None,
            episode_number: None,
            published_at: Some("2026-04-04T00:00:00Z"),
            first_seen_at: baseline,
        })
        .expect("old published episode should insert");

        db.insert_episode_if_new(&NewEpisode {
            feed_id,
            dedupe_key: "new-published",
            guid: Some("new-published"),
            title: Some("New Published"),
            summary: None,
            link: Some("https://example.com/new-published"),
            enclosure_url: None,
            artwork_url: None,
            duration_seconds: None,
            episode_number: None,
            published_at: Some("2026-04-06T00:00:00Z"),
            first_seen_at: baseline,
        })
        .expect("new published episode should insert");

        db.insert_episode_if_new(&NewEpisode {
            feed_id,
            dedupe_key: "missing-published-before-baseline",
            guid: Some("missing-published-before-baseline"),
            title: Some("Missing Published Before Baseline"),
            summary: None,
            link: Some("https://example.com/missing-before"),
            enclosure_url: None,
            artwork_url: None,
            duration_seconds: None,
            episode_number: None,
            published_at: None,
            first_seen_at: DateTime::parse_from_rfc3339("2026-04-04T23:00:00Z")
                .expect("timestamp should parse")
                .with_timezone(&Utc),
        })
        .expect("missing published old episode should insert");

        db.insert_episode_if_new(&NewEpisode {
            feed_id,
            dedupe_key: "missing-published-after-baseline",
            guid: Some("missing-published-after-baseline"),
            title: Some("Missing Published After Baseline"),
            summary: None,
            link: Some("https://example.com/missing-after"),
            enclosure_url: None,
            artwork_url: None,
            duration_seconds: None,
            episode_number: None,
            published_at: None,
            first_seen_at: DateTime::parse_from_rfc3339("2026-04-05T01:00:00Z")
                .expect("timestamp should parse")
                .with_timezone(&Utc),
        })
        .expect("missing published new episode should insert");

        let mut pending = db
            .pending_episode_notifications()
            .expect("pending notifications should load");
        let mut titles = pending
            .iter_mut()
            .map(|episode| episode.episode_title.take().expect("title should exist"))
            .collect::<Vec<_>>();
        titles.sort();

        assert_eq!(
            titles,
            vec![
                "Missing Published After Baseline".to_string(),
                "New Published".to_string()
            ]
        );

        let summary = db.status_summary().expect("status should load");
        assert_eq!(summary.unseen_since_baseline_count, 2);

        db.close().expect("db should close");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mark_episode_notified_removes_episode_from_pending_notifications() {
        let path = temp_db_path("mark-notified");

        let db = Database::open(&path).expect("db should open");
        let baseline = DateTime::parse_from_rfc3339("2026-04-05T00:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&Utc);
        db.initialize(baseline).expect("init should succeed");
        let feed_id = db
            .upsert_feed("https://example.com/feed.xml", Some("Example"), Utc::now())
            .expect("feed should upsert");

        db.insert_episode_if_new(&NewEpisode {
            feed_id,
            dedupe_key: "episode-1",
            guid: Some("episode-1"),
            title: Some("Episode One"),
            summary: None,
            link: Some("https://example.com/episodes/1"),
            enclosure_url: None,
            artwork_url: None,
            duration_seconds: None,
            episode_number: None,
            published_at: Some("2026-04-05T12:00:00Z"),
            first_seen_at: baseline,
        })
        .expect("episode should insert");

        let pending = db
            .pending_episode_notifications()
            .expect("pending notifications should load");
        assert_eq!(pending.len(), 1);

        db.mark_episode_notified(pending[0].episode_id, Utc::now())
            .expect("mark notified should succeed");

        assert!(db
            .episode_notified_at(pending[0].episode_id)
            .expect("notified_at should load")
            .is_some());
        assert!(db
            .pending_episode_notifications()
            .expect("pending notifications should reload")
            .is_empty());

        db.close().expect("db should close");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn upsert_feed_preserves_existing_id_when_episodes_reference_feed() {
        let path = temp_db_path("upsert-feed-preserves-id");

        let db = Database::open(&path).expect("db should open");
        db.initialize(Utc::now()).expect("init should succeed");
        let first_id = db
            .upsert_feed("https://example.com/feed.xml", Some("Original"), Utc::now())
            .expect("feed should insert");

        db.insert_episode_if_new(&NewEpisode {
            feed_id: first_id,
            dedupe_key: "episode-1",
            guid: Some("episode-1"),
            title: Some("Episode One"),
            summary: None,
            link: Some("https://example.com/episodes/1"),
            enclosure_url: None,
            artwork_url: None,
            duration_seconds: None,
            episode_number: None,
            published_at: Some("2026-04-06T00:00:00Z"),
            first_seen_at: Utc::now(),
        })
        .expect("episode should insert");

        let second_id = db
            .upsert_feed("https://example.com/feed.xml", Some("Updated"), Utc::now())
            .expect("feed should update");

        assert_eq!(first_id, second_id);
        let feeds = db.list_feeds().expect("feeds should load");
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].id, first_id);
        assert_eq!(feeds[0].title.as_deref(), Some("Updated"));

        db.close().expect("db should close");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn enable_and_disable_feed_toggle_enabled_state() {
        let path = temp_db_path("enable-disable-feed");

        let db = Database::open(&path).expect("db should open");
        db.initialize(Utc::now()).expect("init should succeed");
        let feed_id = db
            .upsert_feed("https://example.com/feed.xml", Some("Example"), Utc::now())
            .expect("feed should insert");

        assert!(db
            .disable_feed_by_id(feed_id)
            .expect("disable should succeed"));
        let feeds = db.list_feeds().expect("feeds should load");
        assert!(!feeds[0].enabled);

        assert!(db
            .enable_feed_by_url("https://example.com/feed.xml")
            .expect("enable should succeed"));
        let feeds = db.list_feeds().expect("feeds should reload");
        assert!(feeds[0].enabled);

        db.close().expect("db should close");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn import_feeds_updates_existing_rows_without_breaking_episode_references() {
        let path = temp_db_path("import-feeds-updates");

        let db = Database::open(&path).expect("db should open");
        db.initialize(Utc::now()).expect("init should succeed");
        let feed_id = db
            .upsert_feed("https://example.com/feed.xml", Some("Original"), Utc::now())
            .expect("feed should insert");

        db.insert_episode_if_new(&NewEpisode {
            feed_id,
            dedupe_key: "episode-1",
            guid: Some("episode-1"),
            title: Some("Episode One"),
            summary: None,
            link: Some("https://example.com/episodes/1"),
            enclosure_url: None,
            artwork_url: None,
            duration_seconds: None,
            episode_number: None,
            published_at: Some("2026-04-06T00:00:00Z"),
            first_seen_at: Utc::now(),
        })
        .expect("episode should insert");

        let feeds = vec![
            (
                "https://example.com/feed.xml".to_string(),
                Some("Updated".to_string()),
            ),
            (
                "https://example.com/new-feed.xml".to_string(),
                Some("Brand New".to_string()),
            ),
        ];

        let summary = db
            .import_feeds(&feeds, Utc::now())
            .expect("import should succeed");

        assert_eq!(summary.inserted, 1);
        assert_eq!(summary.updated, 1);

        let stored_feeds = db.list_feeds().expect("feeds should load");
        assert_eq!(stored_feeds.len(), 2);
        assert!(stored_feeds
            .iter()
            .any(|feed| { feed.id == feed_id && feed.title.as_deref() == Some("Updated") }));

        let preview = db
            .latest_episode_preview_for_feed(feed_id)
            .expect("preview should load")
            .expect("preview should exist");
        assert_eq!(preview.feed_id, feed_id);
        assert_eq!(preview.feed_title, "Updated");

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
