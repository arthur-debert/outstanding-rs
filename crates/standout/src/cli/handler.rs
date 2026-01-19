//! Command handler types for the declarative API.
//!
//! This module re-exports handler types from `standout-dispatch` and provides
//! the core types for building command handlers:
//!
//! - [`CommandContext`]: Environment information passed to handlers (command path only)
//! - [`Output`]: What a handler produces (render data, silent, or binary)
//! - [`HandlerResult`]: The result type for handlers (`Result<Output<T>, Error>`)
//! - [`RunResult`]: The result of running the CLI dispatcher
//! - [`Handler`]: Trait for thread-safe command handlers (`Send + Sync`, `&self`)
//! - [`LocalHandler`]: Trait for local command handlers (no `Send + Sync`, `&mut self`)
//!
//! # Design Note
//!
//! Handler types are defined in `standout-dispatch` because dispatch orchestrates
//! handler execution. The types are render-agnostic - handlers produce serializable
//! data, and the render layer (configured by standout) handles formatting.
//!
//! Output format (JSON, YAML, terminal, etc.) is NOT passed to handlers via
//! CommandContext. This is intentional: handlers should focus on business logic
//! and produce data, not make format decisions. If a handler truly needs to know
//! the output format, it can check the `--output` flag in ArgMatches directly.
//!
//! # Handler Modes
//!
//! Standout supports two handler modes:
//!
//! ## Thread-safe handlers (default)
//!
//! Use [`Handler`] and [`FnHandler`] for the default `App`. These require
//! `Send + Sync` and immutable `&self`:
//!
//! ```rust,ignore
//! use standout::cli::{App, Handler, Output, HandlerResult, CommandContext};
//!
//! // Closure handler (most common)
//! App::builder()
//!     .command("list", |matches, ctx| {
//!         Ok(Output::Render(get_items()?))
//!     }, "{{ items }}")
//!
//! // Struct handler with interior mutability
//! struct CachedHandler {
//!     cache: Arc<Mutex<HashMap<String, Data>>>,
//! }
//!
//! impl Handler for CachedHandler {
//!     type Output = Data;
//!     fn handle(&self, m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Data> {
//!         let mut cache = self.cache.lock().unwrap();
//!         // ...
//!     }
//! }
//! ```
//!
//! ## Local handlers (mutable state)
//!
//! Use [`LocalHandler`] and [`LocalFnHandler`] with `LocalApp`. These allow
//! `&mut self` without `Send + Sync`:
//!
//! ```rust,ignore
//! use standout::cli::{LocalApp, LocalHandler, Output, HandlerResult, CommandContext};
//!
//! struct MyDatabase {
//!     connection: Connection,
//! }
//!
//! impl MyDatabase {
//!     fn query_mut(&mut self) -> Vec<Row> { ... }
//! }
//!
//! // LocalHandler allows &mut self
//! impl LocalHandler for MyDatabase {
//!     type Output = Vec<Row>;
//!     fn handle(&mut self, m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Row>> {
//!         Ok(Output::Render(self.query_mut()))
//!     }
//! }
//!
//! // Or use FnMut closures
//! let mut db = MyDatabase::connect()?;
//! LocalApp::builder()
//!     .command("query", move |m, ctx| {
//!         Ok(Output::Render(db.query_mut()))
//!     }, "{{ rows }}")
//! ```

// Re-export all handler types from standout-dispatch.
// These types are render-agnostic and focus on handler execution.
pub use standout_dispatch::{
    CommandContext, FnHandler, Handler, HandlerResult, LocalFnHandler, LocalHandler, Output,
    RunResult,
};

// Tests for these types are in the standout-dispatch crate.
