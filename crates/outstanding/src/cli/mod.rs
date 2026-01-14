//! CLI integration for clap-based applications.
//!
//! This module provides batteries-included integration with clap:
//!
//! - Styled help output using outstanding templates
//! - Help topics system (`help <topic>`, `help topics`)
//! - `--output` flag for user output control
//! - Pager support for long help content
//! - Command dispatch with hooks
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use clap::Command;
//! use outstanding::cli::Outstanding;
//!
//! // Simplest usage - styled help with --output flag
//! let matches = Outstanding::parse(Command::new("my-app"));
//! ```
//!
//! # With Command Handlers
//!
//! ```rust,ignore
//! use outstanding::cli::{Outstanding, CommandResult};
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct ListOutput { items: Vec<String> }
//!
//! Outstanding::builder()
//!     .command("list", |_m, _ctx| {
//!         CommandResult::Ok(ListOutput { items: vec!["one".into()] })
//!     }, "{% for item in items %}{{ item }}\n{% endfor %}")
//!     .build()?
//!     .run(cmd, std::env::args());
//! ```

// Internal modules
mod dispatch;
mod outstanding;
mod result;

// Public modules
pub mod group;
pub mod handler;
pub mod help;
pub mod hooks;
#[macro_use]
pub mod macros;

// Re-export main types from outstanding module
pub use outstanding::{Outstanding, OutstandingBuilder};

// Re-export group types for declarative dispatch
pub use group::{CommandConfig, GroupBuilder};

// Re-export result type
pub use result::HelpResult;

// Re-export help types
pub use help::{default_help_theme, render_help, render_help_with_topics, HelpConfig};

// Re-export handler types
pub use handler::{CommandContext, CommandResult, FnHandler, Handler, RunResult};

// Re-export hook types
pub use hooks::{HookError, HookPhase, Hooks, Output};

// Re-export derive macros from outstanding-macros
pub use outstanding_macros::Dispatch;

// Re-export error types
pub use crate::setup::SetupError;

/// Parses a clap command with styled help output.
///
/// This is the simplest entry point for basic CLIs without topics.
pub fn parse(cmd: clap::Command) -> clap::ArgMatches {
    Outstanding::parse(cmd)
}

/// Like `parse`, but takes arguments from an iterator.
pub fn parse_from<I, T>(cmd: clap::Command, itr: I) -> clap::ArgMatches
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Outstanding::new().parse_from(cmd, itr)
}
