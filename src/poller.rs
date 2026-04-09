use crate::db::{Database, FeedRecord, NewEpisode};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use feed_rs::model::Entry;
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::blocking::Client;
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, USER_AGENT};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

const LOSSY_ENTRY_LIMIT: usize = 200;

pub struct PollSummary {
    pub feeds_checked: i64,
    pub feeds_failed: i64,
    pub new_episodes_found: i64,
}

enum FeedPollOutcome {
    NotModified,
    Updated {
        new_episodes: i64,
        warning: Option<String>,
    },
}

struct ParsedFeed {
    title: Option<String>,
    entries: Vec<ParsedEntry>,
    warning: Option<String>,
}

struct ParsedEntry {
    guid: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    link: Option<String>,
    enclosure_url: Option<String>,
    artwork_url: Option<String>,
    duration_seconds: Option<i64>,
    episode_number: Option<i64>,
    published_at: Option<DateTime<Utc>>,
}

pub fn poll_all(db: &Database) -> Result<PollSummary> {
    let started_at = Utc::now();
    let baseline = db.initialized_at()?;
    let run_id = db.start_poll_run(started_at)?;
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .context("failed to build HTTP client")?;

    let feeds = db.enabled_feeds()?;
    let mut feeds_failed = 0_i64;
    let mut new_episodes_found = 0_i64;

    println!(
        "starting poll: feeds={}, baseline={}",
        feeds.len(),
        baseline.to_rfc3339()
    );

    for (index, feed) in feeds.iter().enumerate() {
        println!(
            "[{}/{}] polling [{}] {}",
            index + 1,
            feeds.len(),
            feed.id,
            display_feed_name(feed)
        );

        match poll_feed(db, &client, feed, baseline) {
            Ok(FeedPollOutcome::NotModified) => {
                println!("  not modified");
            }
            Ok(FeedPollOutcome::Updated {
                new_episodes,
                warning,
            }) => {
                new_episodes_found += new_episodes;
                println!("  ok: new_episodes={new_episodes}");
                if let Some(warning) = warning {
                    println!("  warning: {warning}");
                }
            }
            Err(err) => {
                feeds_failed += 1;
                println!("  error: {err:#}");
                warn!(feed_id = feed.id, url = %feed.url, error = %err, "feed poll failed");
            }
        }
    }

    let completed_at = Utc::now();
    db.finish_poll_run(
        run_id,
        completed_at,
        feeds.len() as i64,
        feeds_failed,
        new_episodes_found,
    )?;

    Ok(PollSummary {
        feeds_checked: feeds.len() as i64,
        feeds_failed,
        new_episodes_found,
    })
}

fn poll_feed(
    db: &Database,
    client: &Client,
    feed: &FeedRecord,
    baseline: DateTime<Utc>,
) -> Result<FeedPollOutcome> {
    let mut last_error = None;

    for attempt in 1..=2 {
        match poll_feed_once(db, client, feed, baseline) {
            Ok(outcome) => {
                if attempt > 1 {
                    println!("  recovered on retry");
                }
                return Ok(outcome);
            }
            Err(err) => {
                if attempt == 1 {
                    println!("  warning: first attempt failed, retrying");
                }
                last_error = Some(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("feed poll failed without an error")))
}

fn poll_feed_once(
    db: &Database,
    client: &Client,
    feed: &FeedRecord,
    baseline: DateTime<Utc>,
) -> Result<FeedPollOutcome> {
    let fetched_at = Utc::now();
    let mut request = client.get(&feed.url).header(USER_AGENT, "rssify/0.1");

    if let Some(etag) = &feed.etag {
        request = request.header(IF_NONE_MATCH, etag);
    }
    if let Some(last_modified) = &feed.last_modified {
        request = request.header(IF_MODIFIED_SINCE, last_modified);
    }

    let response = request
        .send()
        .with_context(|| format!("request failed for {}", feed.url))?;

    let status = response.status();
    let status_code = i64::from(status.as_u16());

    if status == StatusCode::NOT_MODIFIED {
        db.update_feed_fetch_success(
            feed.id,
            fetched_at,
            status_code,
            response.headers().get(ETAG).and_then(|v| v.to_str().ok()),
            response
                .headers()
                .get(LAST_MODIFIED)
                .and_then(|v| v.to_str().ok()),
            None,
        )?;
        return Ok(FeedPollOutcome::NotModified);
    }

    if !status.is_success() {
        let error = format!("unexpected HTTP status {}", status);
        db.update_feed_fetch_error(feed.id, fetched_at, Some(status_code), &error)?;
        return Err(anyhow!(error));
    }

    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let last_modified = response
        .headers()
        .get(LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = response
        .bytes()
        .with_context(|| format!("failed to read response body for {}", feed.url))?;
    let feed_doc = parse_feed_document(&bytes, feed)
        .with_context(|| format!("failed to parse feed document for {}", feed.url))
        .map_err(|err| {
            let snapshot_path = save_failed_feed_body(feed, &bytes).ok();
            let error = match snapshot_path {
                Some(path) => format!("{err:#}; saved response snapshot to {}", path.display()),
                None => format!("{err:#}"),
            };
            let _ = db.update_feed_fetch_error(feed.id, fetched_at, Some(status_code), &error);
            anyhow!(error)
        })?;

    db.update_feed_fetch_success(
        feed.id,
        fetched_at,
        status_code,
        etag.as_deref(),
        last_modified.as_deref(),
        feed_doc.title.as_deref(),
    )?;

    let mut new_count = 0_i64;
    for entry in &feed_doc.entries {
        if store_entry(db, feed.id, entry, baseline, fetched_at)? {
            new_count += 1;
        }
    }

    Ok(FeedPollOutcome::Updated {
        new_episodes: new_count,
        warning: feed_doc.warning,
    })
}

fn store_entry(
    db: &Database,
    feed_id: i64,
    entry: &ParsedEntry,
    baseline: DateTime<Utc>,
    fetched_at: DateTime<Utc>,
) -> Result<bool> {
    let guid = entry.guid.as_ref().map(|value| value.trim().to_owned());
    let title = entry.title.as_ref().map(|value| value.trim().to_owned());
    let summary = entry.summary.as_ref().map(|value| value.trim().to_owned());
    let link = entry.link.as_ref().map(|value| value.trim().to_owned());
    let enclosure_url = entry
        .enclosure_url
        .as_ref()
        .map(|value| value.trim().to_owned());
    let artwork_url = entry
        .artwork_url
        .as_ref()
        .map(|value| value.trim().to_owned());
    let published_at = entry.published_at;
    let published_at_raw = published_at.map(|value| value.to_rfc3339());

    let dedupe_key = guid
        .clone()
        .or_else(|| link.clone())
        .or_else(|| {
            let title_value = title.clone()?;
            let published_value = published_at_raw.clone().unwrap_or_default();
            Some(format!("{title_value}:{published_value}"))
        })
        .ok_or_else(|| anyhow!("feed entry is missing guid, link, and title"))?;

    let inserted = db.insert_episode_if_new(&NewEpisode {
        feed_id,
        dedupe_key: &dedupe_key,
        guid: guid.as_deref(),
        title: title.as_deref(),
        summary: summary.as_deref(),
        link: link.as_deref(),
        enclosure_url: enclosure_url.as_deref(),
        artwork_url: artwork_url.as_deref(),
        duration_seconds: entry.duration_seconds,
        episode_number: entry.episode_number,
        published_at: published_at_raw.as_deref(),
        first_seen_at: fetched_at,
    })?;

    if !inserted {
        return Ok(false);
    }

    Ok(match published_at {
        Some(published_at) => published_at >= baseline,
        None => fetched_at >= baseline,
    })
}

fn display_feed_name(feed: &FeedRecord) -> String {
    feed.title
        .clone()
        .or(feed.last_feed_title.clone())
        .unwrap_or_else(|| feed.url.clone())
}

fn parse_feed_document(bytes: &[u8], feed: &FeedRecord) -> Result<ParsedFeed> {
    let raw_feed = parse_rss_lossy(bytes).ok();

    match feed_rs::parser::parse(bytes) {
        Ok(parsed) => Ok(merge_raw_metadata(
            parsed_feed_from_feed_rs(parsed),
            raw_feed,
        )),
        Err(primary_err) => {
            let lossy = raw_feed
                .ok_or_else(|| anyhow!(primary_err.to_string()))
                .with_context(|| {
                    format!("feed-rs failed first for {}: {}", feed.url, primary_err)
                })?;

            if lossy.entries.is_empty() {
                Err(anyhow!(primary_err))
            } else {
                Ok(lossy)
            }
        }
    }
}

fn merge_raw_metadata(mut parsed: ParsedFeed, raw: Option<ParsedFeed>) -> ParsedFeed {
    let Some(raw) = raw else {
        return parsed;
    };

    if parsed.title.is_none() {
        parsed.title = raw.title;
    }
    if parsed.warning.is_none() {
        parsed.warning = raw.warning;
    }

    let raw_by_key: HashMap<String, ParsedEntry> = raw
        .entries
        .into_iter()
        .filter_map(|entry| entry_merge_key(&entry).map(|key| (key, entry)))
        .collect();

    for entry in &mut parsed.entries {
        let Some(key) = entry_merge_key(entry) else {
            continue;
        };
        let Some(raw_entry) = raw_by_key.get(&key) else {
            continue;
        };

        if entry.summary.is_none() {
            entry.summary = raw_entry.summary.clone();
        }
        if entry.artwork_url.is_none() {
            entry.artwork_url = raw_entry.artwork_url.clone();
        }
        if entry.duration_seconds.is_none() {
            entry.duration_seconds = raw_entry.duration_seconds;
        }
        if entry.episode_number.is_none() {
            entry.episode_number = raw_entry.episode_number;
        }
        if entry.enclosure_url.is_none() {
            entry.enclosure_url = raw_entry.enclosure_url.clone();
        }
    }

    parsed
}

fn entry_merge_key(entry: &ParsedEntry) -> Option<String> {
    if let Some(guid) = entry
        .guid
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Some(format!("guid:{guid}"));
    }

    if let Some(link) = entry
        .link
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Some(format!("link:{link}"));
    }

    let title = entry
        .title
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())?;
    let published_at = entry
        .published_at
        .map(|value| value.to_rfc3339())
        .unwrap_or_default();
    Some(format!("title:{title}|published:{published_at}"))
}

fn parsed_feed_from_feed_rs(feed: feed_rs::model::Feed) -> ParsedFeed {
    ParsedFeed {
        title: feed.title.map(|value| value.content),
        entries: feed
            .entries
            .into_iter()
            .map(parsed_entry_from_feed_rs)
            .collect(),
        warning: None,
    }
}

fn parsed_entry_from_feed_rs(entry: Entry) -> ParsedEntry {
    ParsedEntry {
        guid: if entry.id.trim().is_empty() {
            None
        } else {
            Some(entry.id)
        },
        title: entry.title.map(|value| value.content),
        summary: entry.summary.clone().map(|value| value.content),
        link: entry.links.first().map(|link| link.href.clone()),
        enclosure_url: entry.media.first().and_then(|media| {
            media
                .content
                .first()
                .and_then(|content| content.url.as_ref().map(ToString::to_string))
        }),
        artwork_url: entry
            .media
            .first()
            .and_then(|media| media.thumbnails.first())
            .map(|thumbnail| thumbnail.image.uri.clone()),
        duration_seconds: None,
        episode_number: None,
        published_at: entry
            .published
            .or(entry.updated)
            .map(|value| value.with_timezone(&Utc)),
    }
}

fn parse_rss_lossy(bytes: &[u8]) -> Result<ParsedFeed> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut feed_title = None;
    let mut entries = Vec::new();
    let mut current_item: Option<ParsedEntry> = None;
    let mut current_tag: Option<Vec<u8>> = None;
    let mut inside_item = false;
    let mut warning = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = event.name().as_ref().to_vec();
                match name.as_slice() {
                    b"item" => {
                        inside_item = true;
                        current_item = Some(ParsedEntry {
                            guid: None,
                            title: None,
                            summary: None,
                            link: None,
                            enclosure_url: None,
                            artwork_url: None,
                            duration_seconds: None,
                            episode_number: None,
                            published_at: None,
                        });
                    }
                    b"enclosure" if inside_item => {
                        if let Some(item) = current_item.as_mut() {
                            for attribute in event.attributes().flatten() {
                                if attribute.key.as_ref() == b"url" {
                                    item.enclosure_url = Some(
                                        attribute
                                            .decode_and_unescape_value(reader.decoder())?
                                            .into_owned(),
                                    );
                                }
                            }
                        }
                    }
                    b"itunes:image" if inside_item => {
                        if let Some(item) = current_item.as_mut() {
                            for attribute in event.attributes().flatten() {
                                if attribute.key.as_ref() == b"href" {
                                    item.artwork_url = Some(
                                        attribute
                                            .decode_and_unescape_value(reader.decoder())?
                                            .into_owned(),
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
                current_tag = Some(name);
            }
            Ok(Event::Empty(event)) => {
                let name = event.name().as_ref().to_vec();
                match name.as_slice() {
                    b"enclosure" if inside_item => {
                        if let Some(item) = current_item.as_mut() {
                            for attribute in event.attributes().flatten() {
                                if attribute.key.as_ref() == b"url" {
                                    item.enclosure_url = Some(
                                        attribute
                                            .decode_and_unescape_value(reader.decoder())?
                                            .into_owned(),
                                    );
                                }
                            }
                        }
                    }
                    b"itunes:image" if inside_item => {
                        if let Some(item) = current_item.as_mut() {
                            for attribute in event.attributes().flatten() {
                                if attribute.key.as_ref() == b"href" {
                                    item.artwork_url = Some(
                                        attribute
                                            .decode_and_unescape_value(reader.decoder())?
                                            .into_owned(),
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(event)) => {
                let value = event.decode()?.into_owned();
                assign_text(
                    &mut feed_title,
                    current_item.as_mut(),
                    current_tag.as_deref(),
                    inside_item,
                    &value,
                );
            }
            Ok(Event::CData(event)) => {
                let value = event.decode()?.into_owned();
                assign_text(
                    &mut feed_title,
                    current_item.as_mut(),
                    current_tag.as_deref(),
                    inside_item,
                    &value,
                );
            }
            Ok(Event::End(event)) => {
                match event.name().as_ref() {
                    b"item" => {
                        inside_item = false;
                        if let Some(item) = current_item.take() {
                            entries.push(item);
                            if entries.len() >= LOSSY_ENTRY_LIMIT {
                                warning = Some(format!(
                                    "lossy parser stopped after {} recent entries",
                                    entries.len()
                                ));
                                break;
                            }
                        }
                    }
                    _ => {}
                }
                current_tag = None;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => {
                warning = Some(format!(
                    "parsed {} entries before XML error: {}",
                    entries.len(),
                    err
                ));
                break;
            }
        }
    }

    Ok(ParsedFeed {
        title: feed_title,
        entries,
        warning,
    })
}

fn assign_text(
    feed_title: &mut Option<String>,
    current_item: Option<&mut ParsedEntry>,
    current_tag: Option<&[u8]>,
    inside_item: bool,
    value: &str,
) {
    let Some(tag) = current_tag else {
        return;
    };

    if inside_item {
        if let Some(item) = current_item {
            match tag {
                b"title" => item.title = Some(value.to_owned()),
                b"guid" => item.guid = Some(value.to_owned()),
                b"description" | b"itunes:summary" => {
                    if item.summary.is_none() {
                        item.summary = Some(value.to_owned());
                    }
                }
                b"link" => item.link = Some(value.to_owned()),
                b"itunes:duration" => item.duration_seconds = parse_duration_seconds(value),
                b"itunes:episode" => item.episode_number = value.trim().parse::<i64>().ok(),
                b"pubDate" => {
                    item.published_at = chrono::DateTime::parse_from_rfc2822(value)
                        .ok()
                        .map(|value| value.with_timezone(&Utc));
                }
                _ => {}
            }
        }
    } else if tag == b"title" && feed_title.is_none() {
        *feed_title = Some(value.to_owned());
    }
}

fn parse_duration_seconds(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if let Ok(seconds) = trimmed.parse::<i64>() {
        return Some(seconds);
    }

    let parts: Vec<_> = trimmed.split(':').collect();
    match parts.as_slice() {
        [hours, minutes, seconds] => {
            let hours = hours.parse::<i64>().ok()?;
            let minutes = minutes.parse::<i64>().ok()?;
            let seconds = seconds.parse::<i64>().ok()?;
            Some(hours * 3600 + minutes * 60 + seconds)
        }
        [minutes, seconds] => {
            let minutes = minutes.parse::<i64>().ok()?;
            let seconds = seconds.parse::<i64>().ok()?;
            Some(minutes * 60 + seconds)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_duration_seconds, parse_feed_document, store_entry, ParsedEntry};
    use crate::db::{Database, FeedRecord};
    use chrono::{DateTime, Utc};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn lossy_parser_recovers_recent_entries_from_malformed_tail() {
        let feed = dummy_feed();
        let malformed = br#"
<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example Feed</title>
    <item>
      <title>#2479 - Current Episode</title>
      <guid>ep-2479</guid>
      <pubDate>Fri, 03 Apr 2026 17:00:00 -0000</pubDate>
      <link>https://example.com/2479</link>
    </item>
    <item>
      <title>#2478 - Previous Episode</title>
      <guid>ep-2478</guid>
      <pubDate>Thu, 02 Apr 2026 17:00:00 -0000</pubDate>
      <link>https://example.com/2478</link>
    </item>
    <item>
      <title>#818 - Mike Schmidt</title>
      <description>Before becoming a door guy at The Comedy Store,becoming a don39tn"efucf5o; pro f8otbSi</>
  </channel>
</rss>
        "#;

        let parsed = parse_feed_document(malformed, &feed).expect("lossy parse should succeed");

        assert_eq!(parsed.title.as_deref(), Some("Example Feed"));
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].guid.as_deref(), Some("ep-2479"));
        assert_eq!(parsed.entries[1].guid.as_deref(), Some("ep-2478"));
        assert!(parsed.warning.is_some());
    }

    #[test]
    fn baseline_filters_old_entries_but_stores_them() {
        let path = temp_db_path("baseline");
        let db = Database::open(&path).expect("db should open");
        let baseline = DateTime::parse_from_rfc3339("2026-04-05T00:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&Utc);
        db.initialize(baseline).expect("init should succeed");
        let feed_id = db
            .upsert_feed("https://example.com/feed.xml", Some("Example"), Utc::now())
            .expect("feed should upsert");

        let old_entry = ParsedEntry {
            guid: Some("old-entry".to_string()),
            title: Some("Old Entry".to_string()),
            summary: Some("Old summary".to_string()),
            link: Some("https://example.com/old".to_string()),
            enclosure_url: None,
            artwork_url: Some("https://example.com/old.jpg".to_string()),
            duration_seconds: Some(123),
            episode_number: Some(7),
            published_at: Some(
                DateTime::parse_from_rfc3339("2026-04-01T00:00:00Z")
                    .expect("timestamp should parse")
                    .with_timezone(&Utc),
            ),
        };

        let is_new = store_entry(&db, feed_id, &old_entry, baseline, Utc::now())
            .expect("store should succeed");

        assert!(!is_new);
        let summary = db.status_summary().expect("status should load");
        assert_eq!(summary.episode_count, 1);
        assert_eq!(summary.unseen_since_baseline_count, 0);

        db.close().expect("db should close");
        let _ = fs::remove_file(path);
    }

    fn dummy_feed() -> FeedRecord {
        FeedRecord {
            id: 11,
            url: "https://feeds.megaphone.fm/GLT1412515089".to_string(),
            title: Some("The Joe Rogan Experience".to_string()),
            enabled: true,
            added_at: "2026-04-05T00:00:00Z".to_string(),
            last_fetched_at: None,
            last_successful_fetch_at: None,
            last_http_status: None,
            last_error: None,
            etag: None,
            last_modified: None,
            last_feed_title: None,
        }
    }

    fn temp_db_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should work")
            .as_nanos();
        std::env::temp_dir().join(format!("rssify-{label}-{nanos}.duckdb"))
    }

    #[test]
    fn parses_duration_formats() {
        assert_eq!(parse_duration_seconds("3600"), Some(3600));
        assert_eq!(parse_duration_seconds("01:02:03"), Some(3723));
        assert_eq!(parse_duration_seconds("42:17"), Some(2537));
        assert_eq!(parse_duration_seconds("not-a-duration"), None);
    }

    #[test]
    fn merges_podcast_metadata_into_normal_feed_parse() {
        let feed = dummy_feed();
        let rss = br#"
<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
  <channel>
    <title>Example Feed</title>
    <item>
      <title>#2479 - Current Episode</title>
      <guid>ep-2479</guid>
      <pubDate>Fri, 03 Apr 2026 17:00:00 -0000</pubDate>
      <link>https://example.com/2479</link>
      <description>Current episode summary.</description>
      <itunes:duration>01:02:03</itunes:duration>
      <itunes:episode>2479</itunes:episode>
      <itunes:image href="https://example.com/2479.jpg" />
    </item>
  </channel>
</rss>
        "#;

        let parsed = parse_feed_document(rss, &feed).expect("feed should parse");
        let entry = parsed.entries.first().expect("entry should exist");

        assert_eq!(entry.guid.as_deref(), Some("ep-2479"));
        assert_eq!(entry.summary.as_deref(), Some("Current episode summary."));
        assert_eq!(entry.duration_seconds, Some(3723));
        assert_eq!(entry.episode_number, Some(2479));
        assert_eq!(
            entry.artwork_url.as_deref(),
            Some("https://example.com/2479.jpg")
        );
    }
}

fn save_failed_feed_body(feed: &FeedRecord, bytes: &[u8]) -> Result<PathBuf> {
    let dir = PathBuf::from("artifacts/failed-feeds");
    fs::create_dir_all(&dir).context("failed to create failed feed artifact directory")?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("feed-{}-{}.xml", feed.id, timestamp));
    fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}
