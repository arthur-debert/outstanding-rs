//! CLI dispatch and integration for clap-based applications.
//!
//! This module bridges Outstanding's rendering engine with clap's argument parsing,
//! letting you focus on command logic while Outstanding handles output formatting,
//! help rendering, and structured output modes (JSON, YAML, etc.).
//!
//! ## When to Use This Module
//!
//! - You have a clap-based CLI and want rich, testable output
//! - You need `--output=json` support without manual serialization
//! - You want styled help with topic pages
//! - You're adopting Outstanding incrementally (one command at a time)
//!
//! If you only need template rendering without CLI integration, use the
//! [`render`](crate::render) functions directly.
//!
//! ## Execution Flow
//!
//! Outstanding follows a linear pipeline from CLI input to rendered output:
//!
//! ```text
//! Clap Parsing → Dispatch → Handler → Hooks → Rendering → Output
//! ```
//!
//! 1. **Parsing**: Your clap Command is augmented with Outstanding's flags
//!    (`--output`, `--output-file-path`) and parsed normally.
//!
//! 2. **Dispatch**: Outstanding extracts the command path from ArgMatches,
//!    navigating through subcommands to find the registered handler.
//!
//! 3. **Handler**: Your logic executes, returning [`Output`] (data to render,
//!    silent, or binary). Errors propagate via `?`.
//!
//! 4. **Hooks**: Optional hooks run at three points: pre-dispatch (validation),
//!    post-dispatch (data transformation), post-output (output transformation).
//!
//! 5. **Rendering**: Data flows through the template engine, applying styles.
//!    Structured modes (JSON, YAML) skip templating and serialize directly.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use outstanding::cli::{App, Output, HandlerResult};
//!
//! App::builder()
//!     .command("list", |matches, ctx| {
//!         let items = load_items()?;
//!         Ok(Output::Render(items))
//!     })
//!     .template("list", "{% for item in items %}{{ item }}\n{% endfor %}")
//!     .build()?
//!     .run(cmd, std::env::args());
//! ```
//!
//! ## Partial Adoption
//!
//! Outstanding doesn't require all-or-nothing adoption. Register only the
//! commands you want Outstanding to handle; unmatched commands return
//! [`RunResult::NoMatch`] with the ArgMatches for your own dispatch:
//!
//! ```rust,ignore
//! match app.run_to_string(cmd, args) {
//!     RunResult::Handled(output) => println!("{}", output),
//!     RunResult::NoMatch(matches) => legacy_dispatch(matches),
//!     RunResult::Binary(bytes, filename) => std::fs::write(filename, bytes)?,
//! }
//! ```
//!
//! ## Key Types
//!
//! - [`App`] / [`AppBuilder`]: Main entry point and configuration
//! - [`Handler`]: Trait for command handlers (closures work via [`FnHandler`])
//! - [`Output`]: What handlers produce (render data, silent, binary)
//! - [`HandlerResult`]: `Result<Output<T>, Error>` — enables `?` for error handling
//! - [`RunResult`]: Dispatch outcome (handled, binary, or no match)
//! - [`Hooks`]: Pre/post execution hooks for validation and transformation
//! - [`CommandContext`]: Runtime info passed to handlers (output mode, command path)
//!
//! ## See Also
//!
//! - [`crate::render`]: Direct rendering without CLI integration
//! - [`handler`]: Handler types and the Handler trait
//! - [`hooks`]: Hook system for intercepting execution
//! - [`help`]: Help rendering and topic system

// Internal modules
mod dispatch;
mod result;

// Split from former outstanding module
mod app;
mod builder;

// Public modules
pub mod group;
pub mod handler;
pub mod help;
pub mod hooks;
#[macro_use]
pub mod macros;

// Re-export main types from app and builder modules
pub use app::App;
pub use builder::AppBuilder;

// Re-export group types for declarative dispatch
pub use group::{CommandConfig, GroupBuilder};

// Re-export result type
pub use result::HelpResult;

// Re-export help types
pub use help::{default_help_theme, render_help, render_help_with_topics, HelpConfig};

// Re-export handler types
pub use handler::{CommandContext, FnHandler, Handler, HandlerResult, Output, RunResult};

// Re-export hook types
pub use hooks::{HookError, HookPhase, Hooks, RenderedOutput};

// Re-export derive macros from outstanding-macros
pub use outstanding_macros::Dispatch;

// Re-export error types
pub use crate::setup::SetupError;

/// Parses a clap command with styled help output.
///
/// This is the simplest entry point for basic CLIs without topics.
pub fn parse(cmd: clap::Command) -> clap::ArgMatches {
    App::parse(cmd)
}

/// Like `parse`, but takes arguments from an iterator.
pub fn parse_from<I, T>(cmd: clap::Command, itr: I) -> clap::ArgMatches
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    App::new().parse_from(cmd, itr)
}
