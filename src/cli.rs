use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "rssify")]
#[command(about = "Track podcast RSS feeds and post new episodes to Slack")]
#[command(
    long_about = "Track podcast RSS feeds, store local state in DuckDB, poll enabled feeds for new episodes, and post eligible unposted episodes to one Slack channel."
)]
#[command(after_help = "Common workflow:
  1. rssify init
  2. rssify import-opml data/PocketCasts.opml
     or rssify add-feed <url> --title <name>
  3. rssify poll
  4. rssify status

Core commands manage feeds, polling, and status.
Preview commands are optional local tools for reviewing message output.")]
pub struct Cli {
    #[arg(
        long,
        global = true,
        default_value = "rssify.duckdb",
        help = "Path to the DuckDB database file"
    )]
    pub db_path: PathBuf,

    #[arg(
        long,
        global = true,
        help = "Slack webhook URL used by `poll` when posting episode updates"
    )]
    pub slack_webhook_url: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Initialize the database and baseline timestamp")]
    Init,
    #[command(about = "Import or update feeds from an OPML file")]
    ImportOpml(ImportOpmlArgs),
    #[command(about = "Add one feed manually")]
    AddFeed(AddFeedArgs),
    #[command(about = "List stored feeds")]
    ListFeeds(ListFeedsArgs),
    #[command(about = "Enable a feed so it will be polled")]
    EnableFeed(FeedSelectorArgs),
    #[command(about = "Disable a feed so it will not be polled")]
    DisableFeed(FeedSelectorArgs),
    #[command(about = "Remove a feed from the database")]
    RemoveFeed(FeedSelectorArgs),
    #[command(about = "Poll enabled feeds and optionally post new episodes to Slack")]
    Poll,
    #[command(about = "Preview one stored episode update in the terminal")]
    PreviewUpdate(PreviewUpdateArgs),
    #[command(about = "Render recent stored episode updates as static HTML")]
    PreviewHtml(PreviewHtmlArgs),
    #[command(about = "Show overall database and feed status")]
    Status(StatusArgs),
}

#[derive(Debug, Args)]
pub struct ImportOpmlArgs {
    #[arg(help = "Path to the OPML file to import")]
    pub path: PathBuf,
}

#[derive(Debug, Args)]
pub struct AddFeedArgs {
    #[arg(help = "RSS feed URL")]
    pub url: String,

    #[arg(long, help = "Optional title to store for this feed")]
    pub title: Option<String>,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long, help = "Print status as JSON")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ListFeedsArgs {
    #[arg(long, help = "Print feeds as JSON")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct FeedSelectorArgs {
    #[arg(long, conflicts_with = "url", help = "Select a feed by numeric ID")]
    pub id: Option<i64>,

    #[arg(long, conflicts_with = "id", help = "Select a feed by URL")]
    pub url: Option<String>,
}

#[derive(Debug, Args)]
pub struct PreviewUpdateArgs {
    #[arg(
        long,
        conflicts_with = "episode_id",
        help = "Preview the latest episode for one feed ID"
    )]
    pub feed_id: Option<i64>,

    #[arg(
        long,
        conflicts_with = "feed_id",
        help = "Preview one episode by episode ID"
    )]
    pub episode_id: Option<i64>,

    #[arg(long, help = "Print the preview data as JSON")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PreviewHtmlArgs {
    #[arg(
        long,
        default_value_t = 5,
        help = "Number of recent episodes to include"
    )]
    pub limit: usize,

    #[arg(
        long,
        default_value = "artifacts/preview/updates.html",
        help = "Output HTML file path"
    )]
    pub output: PathBuf,
}
