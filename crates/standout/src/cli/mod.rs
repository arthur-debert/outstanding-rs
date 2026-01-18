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
//! ## Handler Modes
//!
//! Standout supports two handler modes:
//!
//! ### Thread-safe handlers (default)
//!
//! Use [`App`] with [`Handler`] for the default mode. Handlers must implement
//! `Send + Sync` and use `&self` (not `&mut self`):
//!
//! ```rust,ignore
//! use standout::cli::{App, Output};
//!
//! // Stateless closure - works naturally
//! App::builder()
//!     .command("list", |m, ctx| Ok(Output::Render(get_items()?)), "{{ items }}")
//!
//! // Stateful handler requires interior mutability
//! let cache = Arc::new(Mutex::new(HashMap::new()));
//! ```
//!
//! ### Local handlers (mutable state)
//!
//! Use [`LocalApp`] with [`LocalHandler`] when handlers need `&mut self` access
//! without interior mutability wrappers:
//!
//! ```rust,ignore
//! use standout::cli::{LocalApp, Output};
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
//! // LocalApp allows FnMut handlers that capture &mut api
//! LocalApp::builder()
//!     .command("add", |m, ctx| {
//!         let item = Item::from(m);
//!         api.add(item);  // &mut self works!
//!         Ok(Output::Silent)
//!     }, "")
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
//! 1. **Parsing**: Your clap Command is augmented with Standout's flags
//!    (`--output`, `--output-file-path`) and parsed normally.
//!
//! 2. **Dispatch**: Standout extracts the command path from ArgMatches,
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
//! use standout::cli::{App, Output, HandlerResult};
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
//! Standout doesn't require all-or-nothing adoption. Register only the
//! commands you want Standout to handle; unmatched commands return
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
//! ### Thread-safe (default)
//!
//! - [`App`] / [`AppBuilder`]: Main entry point and configuration
//! - [`Handler`]: Trait for thread-safe handlers (`Send + Sync`, `&self`)
//! - [`FnHandler`]: Wrapper for `Fn` closures
//!
//! ### Local (mutable state)
//!
//! - [`LocalApp`] / [`LocalAppBuilder`]: Single-threaded app with `FnMut` handlers
//! - [`LocalHandler`]: Trait for local handlers (no `Send + Sync`, `&mut self`)
//! - [`LocalFnHandler`]: Wrapper for `FnMut` closures
//!
//! ### Shared
//!
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
//! - [`mode`]: Handler execution modes (ThreadSafe, Local)

// Internal modules
mod dispatch;
mod result;

// Split from former standout module
mod app;
mod builder;

// Local (mutable) handler support
mod local_app;
mod local_builder;

// Public modules
pub mod group;
pub mod handler;
pub mod help;
pub mod hooks;
pub mod mode;
#[macro_use]
pub mod macros;

// Re-export main types from app and builder modules
pub use app::App;
pub use builder::AppBuilder;

// Re-export local app types
pub use local_app::LocalApp;
pub use local_builder::LocalAppBuilder;

// Re-export group types for declarative dispatch
pub use group::{CommandConfig, GroupBuilder};

// Re-export result type
pub use result::HelpResult;

// Re-export help types
pub use help::{default_help_theme, render_help, render_help_with_topics, HelpConfig};

// Re-export handler types (thread-safe)
pub use handler::{CommandContext, FnHandler, Handler, HandlerResult, Output, RunResult};

// Re-export local handler types
pub use handler::{LocalFnHandler, LocalHandler};

// Re-export mode types
pub use mode::{HandlerMode, Local, ThreadSafe};

// Re-export hook types
pub use hooks::{HookError, HookPhase, Hooks, RenderedOutput};

// Re-export derive macros from standout-macros
pub use standout_macros::Dispatch;

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
