//! Command dispatch and orchestration for clap-based CLIs.
//!
//! `standout-dispatch` provides command routing, handler execution, and a hook
//! system for CLI applications. It returns typed results and leaves presentation
//! to the consuming framework.
//!
//! # Architecture
//!
//! Dispatch is an orchestration layer that manages this execution flow:
//!
//! ```text
//! parsed CLI args
//!   → pre-dispatch hook (validation, setup)
//!   → handler adapter (CLI input → application call → serializable view data)
//!   → post-dispatch hook (data transformation)
//!   → framework presentation
//!   → post-output hook (output transformation)
//! ```
//!
//! ## Design Rationale
//!
//! Dispatch deliberately does not own rendering or output format logic:
//!
//! - Handler adapters have a strict input signature (`&ArgMatches`,
//!   `&CommandContext`) and return serializable data. Reusable application
//!   behavior remains behind a CLI-free library interface.
//!
//! This separation allows:
//! - Using dispatch without any rendering (just return data)
//! - Letting a consuming framework choose how returned data becomes output
//! - Keeping format/theme/template logic out of the dispatch layer
//!
//! # State Management
//!
//! [`CommandContext`] provides two mechanisms for dependency injection:
//!
//! - **`app_state`**: Immutable, app-lifetime state (database, config, API clients).
//!   Configured at app build time, shared across all dispatches via `Rc<Extensions>`.
//!
//! - **`extensions`**: Mutable, per-request state. Injected by pre-dispatch hooks
//!   for request-scoped data like user sessions or request IDs.
//!
//! ```rust,ignore
//! // App-level state (build time)
//! App::builder()
//!     .app_state(Database::connect()?)
//!     .app_state(Config::load()?)
//!
//! // In handler
//! fn handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<T> {
//!     let db = ctx.app_state.get_required::<Database>()?;   // shared
//!     let scope = ctx.extensions.get_required::<UserScope>()?; // per-request
//!     // ...
//! }
//! ```
//!
//! # Features
//!
//! - Command routing: Extract command paths from clap `ArgMatches`
//! - Handler traits: [`Handler`] trait with `&mut self` for mutable state
//! - Hook system: Pre/post dispatch and post-output hooks for cross-cutting concerns
//! - State injection: App-level state via `app_state`, per-request state via `extensions`
//! - Command results: serializable data, silent success, binary bytes, or an error
//!
//! # Usage
//!
//! ## With standout framework
//!
//! The `standout` crate provides full integration with templates and themes:
//!
//! ```rust,ignore
//! use standout::{App, embed_templates};
//!
//! App::builder()
//!     .templates(embed_templates!("src/templates"))
//!     .command("list", list_handler, "list")  // template name
//!     .build()?
//!     .run(cmd, args);
//! ```
//!
//! In this case, `standout` owns template lookup, theme selection, and output
//! mode handling after dispatch returns handler data.

// Core modules
pub mod artifact;
mod dispatch;
mod handler;
mod hooks;
pub mod verify;

// Re-export compound artifact types
pub use artifact::{Artifact, ArtifactDestination, ArtifactReceipt, ArtifactRun};

// Re-export command routing utilities
pub use dispatch::{
    extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
    path_to_string, string_to_path,
};

// Re-export handler types
pub use handler::{
    CommandContext, DispatchResult, ExitStatus, Extensions, ExternalFailure, FnHandler, Handler,
    HandlerResult, IntoHandlerResult, InvalidExternalStatus, Output, OutputKind, RunError,
    RunErrorKind, RunOutput, SimpleFnHandler, SuccessKind,
};

// Re-export hook types
pub use hooks::{
    ArtifactOutput, HookError, HookPhase, Hooks, PostDispatchFn, PostOutputFn, PreDispatchFn,
    RenderedOutput, TextOutput,
};
