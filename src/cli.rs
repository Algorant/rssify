use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "rssify")]
#[command(about = "Podcast RSS tracker and status monitor")]
pub struct Cli {
    #[arg(long, global = true, default_value = "rssify.duckdb")]
    pub db_path: PathBuf,

    #[arg(long, global = true)]
    pub slack_webhook_url: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Init,
    ImportOpml(ImportOpmlArgs),
    AddFeed(AddFeedArgs),
    ListFeeds(ListFeedsArgs),
    DisableFeed(FeedSelectorArgs),
    RemoveFeed(FeedSelectorArgs),
    Poll,
    PreviewUpdate(PreviewUpdateArgs),
    PreviewHtml(PreviewHtmlArgs),
    Status(StatusArgs),
}

#[derive(Debug, Args)]
pub struct ImportOpmlArgs {
    pub path: PathBuf,
}

#[derive(Debug, Args)]
pub struct AddFeedArgs {
    pub url: String,

    #[arg(long)]
    pub title: Option<String>,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ListFeedsArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct FeedSelectorArgs {
    #[arg(long, conflicts_with = "url")]
    pub id: Option<i64>,

    #[arg(long, conflicts_with = "id")]
    pub url: Option<String>,
}

#[derive(Debug, Args)]
pub struct PreviewUpdateArgs {
    #[arg(long, conflicts_with = "episode_id")]
    pub feed_id: Option<i64>,

    #[arg(long, conflicts_with = "feed_id")]
    pub episode_id: Option<i64>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PreviewHtmlArgs {
    #[arg(long, default_value_t = 5)]
    pub limit: usize,

    #[arg(long, default_value = "artifacts/preview/updates.html")]
    pub output: PathBuf,
}
