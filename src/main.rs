mod app;
mod cli;
mod db;
mod opml;
mod poller;
mod render;
mod slack;

use anyhow::Result;

fn main() -> Result<()> {
    app::run()
}
