//! Command handler types for the declarative API.
//!
//! This module re-exports handler types from `standout-dispatch` and provides
//! the core types for building command handlers:
//!
//! - [`CommandContext`]: Environment information passed to handlers (command path only)
//! - [`Output`]: What a handler produces (render data, silent, or binary)
//! - [`HandlerResult`]: The result type for handlers (`Result<Output<T>, Error>`)
//! - [`ExternalFailure`]: An intentional delegated-operation failure with an exact status/payload
//! - [`RunResult`]: The result of running the CLI dispatcher
//! - [`Handler`]: Trait for command handlers (`&mut self`)
//!
//! # Design Note
//!
//! Handler types are defined in `standout-dispatch` because dispatch orchestrates
//! handler execution. The types are render-agnostic - handlers produce serializable
//! data, and the render layer (configured by standout) handles formatting.
//!
//! Output format (JSON, YAML, terminal, etc.) is NOT passed to handlers via
//! CommandContext. This is intentional: handlers should adapt application calls
//! into view data, not make format decisions. If a handler truly needs to know
//! the output format, it can check the `--output` flag in ArgMatches directly.
//!
//! # Single-Threaded Design
//!
//! CLI applications are single-threaded: parse args → run one handler → output → exit.
//! Handlers use `&mut self` and `FnMut`, allowing natural Rust patterns without
//! forcing interior mutability wrappers (`Arc<Mutex<_>>`).
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
//! // Struct handler with mutable state
//! struct Database {
//!     connection: Connection,
//! }
//!
//! impl Database {
//!     fn query(&mut self) -> Vec<Row> { ... }
//! }
//!
//! impl Handler for Database {
//!     type Output = Vec<Row>;
//!     fn handle(&mut self, m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Row>> {
//!         Ok(Output::Render(self.query()))
//!     }
//! }
//! ```

// Re-export all handler types from standout-dispatch.
// These types are render-agnostic and focus on handler execution.
pub use standout_dispatch::{
    Artifact, ArtifactDestination, ArtifactReceipt, ArtifactRun, CommandContext, ExitStatus,
    Extensions, ExternalFailure, FnHandler, Handler, HandlerResult, InvalidExternalStatus, Output,
    OutputKind, RunError, RunErrorKind, RunOutput, RunResult, SuccessKind,
};

use standout_input::{InputSourceKind, Inputs, MissingInput};

use crate::cli::questionnaire::QUESTIONNAIRE_INPUT_NAME;

/// Extension trait for [`CommandContext`] that exposes inputs registered with
/// [`CommandConfig::input`](crate::cli::CommandConfig::input).
///
/// Handlers retrieve named, typed inputs that were resolved by the framework
/// before the handler ran. This is the read side of the declarative input
/// API; see [`CommandConfig::input`](crate::cli::CommandConfig::input) for the
/// registration side.
///
/// ```rust,ignore
/// use standout::cli::{CommandContextInput, Output};
///
/// fn handler(_m: &clap::ArgMatches, ctx: &standout::cli::CommandContext) -> standout::cli::HandlerResult<serde_json::Value> {
///     let body: &String = ctx.input("body")?;
///     Ok(Output::Render(serde_json::json!({ "body": body })))
/// }
/// ```
pub trait CommandContextInput {
    /// Returns the resolved value for `name`, or an error if no input with
    /// that name and type was registered.
    fn input<T: 'static>(&self, name: &str) -> Result<&T, MissingInput>;

    /// Returns the typed questionnaire value resolved for this command.
    ///
    /// Commands opt into this with
    /// [`CommandConfig::questionnaire`](crate::cli::CommandConfig::questionnaire),
    /// which injects the answer-sheet CLI surface, resolves the submitted or
    /// interactive answers before the handler runs, and stores the filled
    /// derived questionnaire struct in the command context.
    fn questionnaire<T: 'static>(&self) -> Result<&T, MissingInput>;

    /// Returns the source that provided `name`, if it was resolved.
    ///
    /// Useful for diagnostic output ("body came from stdin") or for branching
    /// behavior on the source kind.
    fn input_source(&self, name: &str) -> Option<InputSourceKind>;

    /// Returns the [`Inputs`] bag for this command, if any input chain ran.
    ///
    /// Most handlers should prefer [`input`](Self::input); this is for cases
    /// where the handler needs to iterate over all resolved inputs.
    fn inputs(&self) -> Option<&Inputs>;
}

impl CommandContextInput for CommandContext {
    fn input<T: 'static>(&self, name: &str) -> Result<&T, MissingInput> {
        match self.extensions.get::<Inputs>() {
            Some(bag) => bag.get_required::<T>(name),
            None => Err(MissingInput::NotRegistered {
                name: name.to_string(),
            }),
        }
    }

    fn questionnaire<T: 'static>(&self) -> Result<&T, MissingInput> {
        self.input(QUESTIONNAIRE_INPUT_NAME)
    }

    fn input_source(&self, name: &str) -> Option<InputSourceKind> {
        self.extensions.get::<Inputs>()?.source_of(name)
    }

    fn inputs(&self) -> Option<&Inputs> {
        self.extensions.get::<Inputs>()
    }
}

// Tests for these types are in the standout-dispatch crate.
