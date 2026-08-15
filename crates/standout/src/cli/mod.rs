//! CLI dispatch and integration for clap-based applications.
//!
//! This module bridges Standout's rendering engine with clap's argument parsing,
//! letting you focus on command logic while Standout handles output formatting,
//! help rendering, and structured output modes (JSON, YAML, etc.).
//!
//! ## When to Use This Module
//!
//! - You have a clap-based CLI and want rich, testable output
//! - You need `--output=json` support without manual serialization
//! - You want styled help with topic pages
//! - You're adopting Standout incrementally (one command at a time)
//!
//! If you only need template rendering without CLI integration, use the
//! [`render`](crate::render) functions directly.
//!
//! ## Single-Threaded Design
//!
//! CLI applications are single-threaded: parse args → run one handler → output → exit.
//! Handlers use `&mut self` and `FnMut`, allowing natural Rust patterns without
//! forcing interior mutability wrappers (`Arc<Mutex<_>>`).
//!
//! ```rust,ignore
//! use standout::cli::{App, Output};
//!
//! struct MyApi {
//!     index: HashMap<Uuid, Item>,
//! }
//!
//! impl MyApi {
//!     fn add(&mut self, item: Item) { self.index.insert(item.id, item); }
//! }
//!
//! let mut api = MyApi::new();
//!
//! // FnMut handlers can capture mutable state
//! App::new()
//!     .command("add", |m, ctx| {
//!         let item = Item::from(m);
//!         api.add(item);  // &mut self works!
//!         Ok(Output::Silent)
//!     }, "")?
//!     .build()?
//!     .run(cmd, args);
//! ```
//!
//! ## Execution Flow
//!
//! Standout follows a linear pipeline from CLI input to rendered output:
//!
//! ```text
//! Clap Parsing → Dispatch → Handler → Hooks → Rendering → Output
//! ```
//!
//! 1. Parsing: Your clap Command is augmented with Standout's flags
//!    (`--output`, `--output-file-path`) and parsed normally.
//!
//! 2. Dispatch: Standout extracts the command path from ArgMatches,
//!    navigating through subcommands to find the registered handler.
//!
//! 3. Handler: Your logic executes, returning [`Output`] (data to render,
//!    silent, or binary). Errors propagate via `?`.
//!
//! 4. Hooks: Optional hooks run at three points: pre-dispatch (validation),
//!    post-dispatch (data transformation), post-output (output transformation).
//!
//! 5. Rendering: Data flows through the template engine, applying styles.
//!    Structured modes (JSON, YAML) skip templating and serialize directly.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use standout::cli::{App, Output, HandlerResult};
//!
//! App::new()
//!     .command("list", |matches, ctx| {
//!         let items = load_items()?;
//!         Ok(Output::Render(items))
//!     }, "{% for item in items %}{{ item }}\n{% endfor %}")?
//!     .build()?
//!     .run(cmd, std::env::args());
//! ```
//!
//! ## Partial Adoption
//!
//! Standout doesn't require all-or-nothing adoption. Register only the
//! commands you want Standout to handle; unmatched commands return
//! [`RunResult::NoMatch`] with the ArgMatches for your own dispatch:
//!
//! ```rust,ignore
//! match app.run_to_string(cmd, args) {
//!     RunResult::Handled(output) => println!("{}", output),
//!     RunResult::NoMatch(matches) => legacy_dispatch(matches),
//!     RunResult::Binary(bytes, filename) => std::fs::write(filename, bytes)?,
//!     RunResult::Error(error) => {
//!         eprintln!("{}", error);
//!         std::process::exit(error.exit_status().code().into());
//!     },
//!     // RunResult is #[non_exhaustive]; cover Silent and future variants.
//!     _ => {},
//! }
//! ```
//!
//! ## Key Types
//!
//! - [`App`]: Main entry point and configuration (re-export of `AppBuilder`)
//! - [`Handler`]: Trait for command handlers (`&mut self`)
//! - [`FnHandler`]: Wrapper for `FnMut` closures
//! - [`Output`]: What handlers produce (render data, silent, binary)
//! - [`HandlerResult`]: `Result<Output<T>, Error>` — enables `?` for error handling
//! - [`RunResult`]: Typed dispatch outcome, status, and failure origin
//! - [`Hooks`]: Pre/post execution hooks for validation and transformation
//! - [`CommandContext`]: Runtime info passed to handlers (command path, app state)
//!
//! ## See Also
//!
//! - [`crate::render`]: Direct rendering without CLI integration
//! - [`handler`]: Handler types and the Handler trait
//! - [`hooks`]: Hook system for intercepting execution
//! - [`help`]: Help rendering and topic system

// Internal modules
mod default_command;
mod dispatch;
mod questionnaire;
mod result;

// Helper functions (formerly the App struct lived here)
pub(crate) mod app;

// Builder is now the single App implementation
mod builder;

// Public modules
pub mod group;
pub mod handler;
pub mod help;
pub mod hooks;
#[macro_use]
pub mod macros;

// Re-export AppBuilder as App — the single unified type
pub use builder::AppBuilder as App;

// Re-export group types for declarative dispatch
pub use group::{CommandConfig, GroupBuilder};

// Re-export result type
pub use result::HelpResult;

// Re-export help types
pub use help::{
    default_help_theme, render_help, render_help_with_topics, validate_command_groups,
    CommandGroup, HelpConfig,
};

// Re-export handler types
pub use handler::{
    Artifact, ArtifactDestination, ArtifactReceipt, ArtifactRun, CommandContext,
    CommandContextInput, ExitStatus, ExternalFailure, FnHandler, Handler, HandlerResult,
    InvalidExternalStatus, Output, OutputKind, RunError, RunErrorKind, RunOutput, RunResult,
    SuccessKind,
};

// Re-export hook types
pub use hooks::{ArtifactOutput, HookError, HookPhase, Hooks, RenderedOutput};

// Re-export derive macros from standout-macros
pub use standout_macros::Dispatch;

// Re-export error types
pub use crate::setup::{HelpWordSource, SetupError};

// Re-export dispatch utilities from standout-dispatch
pub use dispatch::{
    extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
};

// Re-export invocation-aware default-command resolution types
pub use default_command::{DefaultCommandContext, DefaultCommandResolver, UnknownDefaultCommand};

/// Parses a clap command with styled help output.
///
/// This is the simplest entry point for basic CLIs without topics.
pub fn parse(cmd: clap::Command) -> clap::ArgMatches {
    App::new().parse_with(cmd)
}

/// Like `parse`, but takes arguments from an iterator.
pub fn parse_from<I, T>(cmd: clap::Command, itr: I) -> clap::ArgMatches
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    App::new().parse_from(cmd, itr)
}
