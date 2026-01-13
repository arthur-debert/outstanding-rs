//! # Dispatch Derive Macro Example
//!
//! This example demonstrates the `#[derive(Dispatch)]` macro for
//! declarative command-to-handler mapping.
//!
//! The derive macro generates a `dispatch_config()` method that maps
//! enum variants to handler functions by naming convention.
//!
//! Run with: cargo run --example dispatch_derive_example -- <command>
//!
//! Try:
//!   cargo run --example dispatch_derive_example -- --help
//!   cargo run --example dispatch_derive_example -- add "Buy milk"
//!   cargo run --example dispatch_derive_example -- list
//!   cargo run --example dispatch_derive_example -- list --output=json

use clap::{ArgMatches, CommandFactory, Parser, Subcommand};
use outstanding_clap::{CommandContext, CommandResult, Dispatch, Outstanding};
use serde::Serialize;

// ============================================================================
// CLI DEFINITION (using clap derive)
// ============================================================================

/// A simple task manager demonstrating the Dispatch derive macro
#[derive(Parser)]
#[command(name = "taskr")]
#[command(about = "A task manager demonstrating #[derive(Dispatch)]")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Available commands.
///
/// The `#[derive(Dispatch)]` macro generates `dispatch_config()` which maps:
/// - `Add` → `handlers::add`
/// - `List` → `handlers::list`
/// - `Complete` → `handlers::complete`
#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    /// Add a new task
    Add {
        /// Task description
        text: String,
    },

    /// List all tasks
    List,

    /// Mark a task as complete
    Complete {
        /// Task ID to complete
        id: u32,
    },
}

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Serialize)]
struct TaskResult {
    message: Option<String>,
    tasks: Vec<Task>,
}

#[derive(Serialize, Clone)]
struct Task {
    id: u32,
    text: String,
    done: bool,
}

// ============================================================================
// HANDLERS MODULE
// ============================================================================
// The Dispatch derive expects handlers at `handlers::{command_snake_case}`

mod handlers {
    use super::*;

    pub fn add(matches: &ArgMatches, _ctx: &CommandContext) -> CommandResult<TaskResult> {
        let text = matches.get_one::<String>("text").unwrap().clone();
        CommandResult::Ok(TaskResult {
            message: Some(format!("Added: {}", text)),
            tasks: vec![Task {
                id: 1,
                text,
                done: false,
            }],
        })
    }

    pub fn list(_matches: &ArgMatches, _ctx: &CommandContext) -> CommandResult<TaskResult> {
        CommandResult::Ok(TaskResult {
            message: None,
            tasks: vec![
                Task {
                    id: 1,
                    text: "Buy milk".into(),
                    done: false,
                },
                Task {
                    id: 2,
                    text: "Walk dog".into(),
                    done: true,
                },
            ],
        })
    }

    pub fn complete(matches: &ArgMatches, _ctx: &CommandContext) -> CommandResult<TaskResult> {
        let id = *matches.get_one::<u32>("id").unwrap();
        CommandResult::Ok(TaskResult {
            message: Some(format!("Completed task {}", id)),
            tasks: vec![Task {
                id,
                text: "Completed".into(),
                done: true,
            }],
        })
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    let cmd = Cli::command();

    // The generated dispatch_config() returns a closure that registers
    // all commands with their handlers automatically.
    //
    // Without explicit templates, output falls back to JSON serialization.
    Outstanding::builder()
        .commands(Commands::dispatch_config())
        .run_and_print(cmd, std::env::args());
}
