use crate::cli::{Cli, Command, FeedSelectorArgs, PreviewUpdateArgs};
use crate::db::{Database, EpisodePreview};
use crate::opml::parse_opml;
use crate::poller::poll_all;
use crate::render::{render_episode_text, render_preview_html};
use crate::slack::SlackClient;
use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use std::env;
use std::fs;
use std::path::Path;
use tracing_subscriber::EnvFilter;

pub fn run() -> Result<()> {
    init_logging();

    let cli = Cli::parse();
    let db = Database::open(&cli.db_path)?;
    db.ensure_schema()?;

    match cli.command {
        Command::Init => {
            let created = db.initialize(Utc::now())?;
            let initialized_at = db.initialized_at()?;
            if created {
                println!(
                    "initialized database at {} with baseline {}",
                    cli.db_path.display(),
                    initialized_at.to_rfc3339()
                );
            } else {
                println!(
                    "database already initialized at {} with baseline {}",
                    cli.db_path.display(),
                    initialized_at.to_rfc3339()
                );
            }
        }
        Command::ImportOpml(args) => {
            db.initialized_at()?;
            let feeds = parse_opml(&args.path)?;
            let now = Utc::now();
            let import_rows = feeds
                .into_iter()
                .map(|feed| (feed.xml_url, feed.title))
                .collect::<Vec<_>>();
            let summary = db.import_feeds(&import_rows, now)?;
            println!(
                "imported feeds from {}: inserted={}, updated={}",
                args.path.display(),
                summary.inserted,
                summary.updated
            );
        }
        Command::AddFeed(args) => {
            db.initialized_at()?;
            let id = db.upsert_feed(&args.url, args.title.as_deref(), Utc::now())?;
            println!("stored feed {id} {}", args.url);
        }
        Command::ListFeeds(args) => {
            db.initialized_at()?;
            let feeds = db.list_feeds()?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&feeds)?);
            } else {
                for feed in feeds {
                    println!(
                        "[{}] enabled={} title={}",
                        feed.id,
                        feed.enabled,
                        feed.title
                            .or(feed.last_feed_title)
                            .unwrap_or_else(|| "<untitled>".to_string())
                    );
                    println!("url: {}", feed.url);
                    println!(
                        "last_successful_fetch_at: {}",
                        feed.last_successful_fetch_at.as_deref().unwrap_or("never")
                    );
                    println!(
                        "last_error: {}",
                        feed.last_error.as_deref().unwrap_or("none")
                    );
                    println!();
                }
            }
        }
        Command::EnableFeed(args) => {
            db.initialized_at()?;
            let enabled = enable_feed(&db, &args)?;
            if enabled {
                println!("feed enabled");
            } else {
                println!("no matching feed found");
            }
        }
        Command::DisableFeed(args) => {
            db.initialized_at()?;
            let disabled = disable_feed(&db, &args)?;
            if disabled {
                println!("feed disabled");
            } else {
                println!("no matching feed found");
            }
        }
        Command::RemoveFeed(args) => {
            db.initialized_at()?;
            let removed = remove_feed(&db, &args)?;
            if removed {
                println!("feed removed");
            } else {
                println!("no matching feed found");
            }
        }
        Command::Poll => {
            db.initialized_at()?;
            let summary = poll_all(&db)?;
            let mut delivered = 0_usize;
            let webhook_url = cli
                .slack_webhook_url
                .clone()
                .or_else(|| env::var("RSSIFY_SLACK_WEBHOOK_URL").ok());
            if let Some(webhook_url) = webhook_url.as_deref() {
                let slack = SlackClient::new(webhook_url)?;
                let pending = db.pending_episode_notifications()?;

                for episode in pending {
                    let message = render_episode_text(&episode);
                    slack.send_text(&message)?;
                    db.mark_episode_notified(episode.episode_id, Utc::now())?;
                    delivered += 1;
                }
            }

            println!(
                "poll complete: checked={}, failed={}, new_episodes={}, posted={}",
                summary.feeds_checked, summary.feeds_failed, summary.new_episodes_found, delivered
            );
        }
        Command::PreviewUpdate(args) => {
            db.initialized_at()?;
            let preview = load_preview(&db, &args)?;
            let Some(preview) = preview else {
                println!("no matching episode preview found");
                db.close()?;
                return Ok(());
            };

            if args.json {
                println!("{}", serde_json::to_string_pretty(&preview)?);
            } else {
                println!("{}", render_episode_text(&preview));
            }
        }
        Command::PreviewHtml(args) => {
            db.initialized_at()?;
            let previews = db.recent_episode_previews(args.limit)?;
            write_preview_html(&args.output, &previews)?;
            println!(
                "wrote preview HTML for {} episodes to {}",
                previews.len(),
                args.output.display()
            );
        }
        Command::Status(args) => {
            let summary = db.status_summary()?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                println!("initialized_at: {}", summary.initialized_at);
                println!("feeds: {}", summary.feed_count);
                println!("enabled_feeds: {}", summary.enabled_feed_count);
                println!("episodes: {}", summary.episode_count);
                println!(
                    "unseen_since_baseline: {}",
                    summary.unseen_since_baseline_count
                );
                println!(
                    "last_poll_started_at: {}",
                    summary.last_poll_started_at.as_deref().unwrap_or("never")
                );
                println!(
                    "last_poll_completed_at: {}",
                    summary.last_poll_completed_at.as_deref().unwrap_or("never")
                );
                println!(
                    "last_poll_new_episodes: {}",
                    summary
                        .last_poll_new_episodes
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "n/a".to_string())
                );
                println!();
                for feed in summary.feeds {
                    println!(
                        "- [{}] {}",
                        feed.id,
                        feed.title
                            .or(feed.last_feed_title)
                            .unwrap_or_else(|| "<untitled>".to_string())
                    );
                    println!("  url: {}", feed.url);
                    println!(
                        "  last_successful_fetch_at: {}",
                        feed.last_successful_fetch_at.as_deref().unwrap_or("never")
                    );
                    println!(
                        "  last_error: {}",
                        feed.last_error.as_deref().unwrap_or("none")
                    );
                }
            }
        }
    }

    db.close()?;
    Ok(())
}

fn disable_feed(db: &Database, args: &FeedSelectorArgs) -> Result<bool> {
    match (&args.id, &args.url) {
        (Some(id), None) => db.disable_feed_by_id(*id),
        (None, Some(url)) => db.disable_feed_by_url(url),
        _ => anyhow::bail!("provide exactly one of --id or --url"),
    }
}

fn enable_feed(db: &Database, args: &FeedSelectorArgs) -> Result<bool> {
    match (&args.id, &args.url) {
        (Some(id), None) => db.enable_feed_by_id(*id),
        (None, Some(url)) => db.enable_feed_by_url(url),
        _ => anyhow::bail!("provide exactly one of --id or --url"),
    }
}

fn remove_feed(db: &Database, args: &FeedSelectorArgs) -> Result<bool> {
    match (&args.id, &args.url) {
        (Some(id), None) => db.remove_feed_by_id(*id),
        (None, Some(url)) => db.remove_feed_by_url(url),
        _ => anyhow::bail!("provide exactly one of --id or --url"),
    }
}

fn load_preview(db: &Database, args: &PreviewUpdateArgs) -> Result<Option<EpisodePreview>> {
    match (args.feed_id, args.episode_id) {
        (Some(feed_id), None) => db.latest_episode_preview_for_feed(feed_id),
        (None, Some(episode_id)) => db.episode_preview_by_id(episode_id),
        (None, None) => db.latest_episode_preview(),
        _ => anyhow::bail!("provide at most one of --feed-id or --episode-id"),
    }
}

fn write_preview_html(path: &Path, previews: &[EpisodePreview]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, render_preview_html(previews))?;
    Ok(())
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
