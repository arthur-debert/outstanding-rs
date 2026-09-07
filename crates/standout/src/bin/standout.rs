use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

mod new_project;

#[derive(Parser)]
#[command(name = "standout", about = "Standout project tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate the smallest runnable Standout workspace.
    NewProject,
}

fn main() -> Result<()> {
    new_project::build_app()?.run(Cli::command(), std::env::args());
    Ok(())
}
