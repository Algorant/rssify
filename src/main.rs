mod app;
mod cli;
mod db;
mod opml;
mod poller;

use anyhow::Result;

fn main() -> Result<()> {
    app::run()
}
