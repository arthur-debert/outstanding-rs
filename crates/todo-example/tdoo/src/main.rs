//! `tdoo` owns the shell-facing application.
//!
//! The sibling `todo-core` package owns reusable todo behavior and persistence;
//! this binary package owns clap, Standout, environment lookup, handlers,
//! presentation assets, and final process execution.

mod app;
mod cli;
mod handlers;

use anyhow::Result;
use todo_core::TodoStore;

fn main() -> Result<()> {
    let store = TodoStore::load(cli::resolve_store_path())?;
    let app = app::build(store)?;
    app.run(cli::command(), std::env::args());
    Ok(())
}
