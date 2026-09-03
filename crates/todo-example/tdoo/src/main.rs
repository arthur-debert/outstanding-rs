//! `tdoo` owns the shell-facing application: clap, Standout, configuration,
//! handlers, presentation assets, and process execution. The sibling
//! `todo-core` crate owns reusable todo behavior and persistence.

mod app;
mod cli;
mod config;
mod handlers;

use anyhow::Result;
use clapfig::SearchPath;

fn main() -> Result<()> {
    let app = app::build(SearchPath::Platform)?;
    app.run(cli::command(), std::env::args());
    Ok(())
}
