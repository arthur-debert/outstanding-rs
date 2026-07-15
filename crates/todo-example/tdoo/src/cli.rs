use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tdoo", about = "A tiny todo list - the Standout sample app")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Add a new todo. Title comes from --title or piped stdin.
    Add {
        #[arg(short, long)]
        title: Option<String>,
    },
    /// List todos. By default only pending ones; pass --all for everything.
    List {
        #[arg(short, long)]
        all: bool,
    },
    /// Mark a todo done.
    Done { id: u32 },
}

pub(crate) fn command() -> clap::Command {
    Cli::command()
}

/// Resolves CLI configuration before constructing the core store.
pub(crate) fn resolve_store_path() -> PathBuf {
    if let Ok(path) = std::env::var("TODO_FILE") {
        return path.into();
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".todos.json")
}
