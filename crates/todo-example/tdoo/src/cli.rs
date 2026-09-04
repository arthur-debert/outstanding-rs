use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use standout::cli::Dispatch;

#[derive(Parser)]
#[command(name = "tdoo", about = "A tiny todo list - the Standout sample app")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = crate::handlers)]
pub(crate) enum Commands {
    /// Add a new todo. Title comes from --title or piped stdin.
    #[dispatch(
        pure,
        inputs = crate::handlers::add_inputs,
        post_dispatch = crate::handlers::audit_hook
    )]
    Add {
        #[arg(short, long)]
        title: Option<String>,
    },
    /// List todos. By default only pending ones; pass --all for everything.
    #[dispatch(pure, pageable)]
    List {
        #[arg(short, long)]
        all: bool,
        /// Newest first; the `reverse` config key sets the default.
        #[arg(
            short,
            long,
            action = ArgAction::Set,
            num_args = 0..=1,
            default_missing_value = "true"
        )]
        reverse: Option<bool>,
    },
    /// Mark a todo done.
    #[dispatch(pure, post_dispatch = crate::handlers::audit_hook)]
    Done { id: u32 },
    /// Export todos as CSV. Writes ./todos.csv unless --stdout or
    /// --output-file-path redirects it.
    #[dispatch(pure)]
    Export {
        #[arg(short, long)]
        all: bool,
        /// Write the CSV to stdout; the report goes to stderr.
        #[arg(long)]
        stdout: bool,
    },
}

pub(crate) fn command() -> clap::Command {
    Cli::command()
}
