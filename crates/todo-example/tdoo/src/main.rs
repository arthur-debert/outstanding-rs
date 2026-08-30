//! `tdoo` owns the shell-facing application: clap, Standout, environment
//! lookup, handlers, presentation assets, and process execution. The
//! sibling `todo-core` crate owns reusable todo behavior and persistence.

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
