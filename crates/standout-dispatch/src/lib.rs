//! Command dispatch and orchestration for clap-based CLIs.
//!
//! `standout-dispatch` provides command routing, handler execution, and a hook
//! system for CLI applications. It orchestrates the execution flow while remaining
//! agnostic to rendering implementation.
//!
//! # Architecture
//!
//! Dispatch is an orchestration layer that manages this execution flow:
//!
//! ```text
//! parsed CLI args
//!   → pre-dispatch hook (validation, setup)
//!   → logic handler (business logic → serializable data)
//!   → post-dispatch hook (data transformation)
//!   → render handler (view + data → string output)
//!   → post-output hook (output transformation)
//! ```
//!
//! ## Design Rationale
//!
//! Dispatch deliberately does not own rendering or output format logic:
//!
//! - Logic handlers have a strict input signature (`&ArgMatches`, `&CommandContext`)
//!   and return serializable data. They focus purely on business logic.
//!
//! - Render handlers are pluggable callbacks provided by the consuming framework.
//!   They receive (view name, data) and return a formatted string. All rendering
//!   decisions (format, theme, template engine) live in the render handler.
//!
//! This separation allows:
//! - Using dispatch without any rendering (just return data)
//! - Using dispatch with custom renderers (not just standout-render)
//! - Keeping format/theme/template logic out of the dispatch layer
//!
//! ## Render Handler Pattern
//!
//! The render handler is a closure that captures rendering context:
//!
//! ```rust,ignore
//! // Framework (e.g., standout) creates the render handler at runtime
//! // after parsing CLI args to determine format
//! let format = extract_output_mode(&matches);  // --output=json
//! let theme = &config.theme;
//!
//! let render_handler = move |view: &str, data: &Value| {
//!     // All format/theme knowledge lives here, not in dispatch
//!     my_renderer::render(view, data, theme, format)
//! };
//!
//! dispatcher.run_with_renderer(matches, render_handler);
//! ```
//!
//! This pattern means dispatch calls `render_handler(view, data)` without knowing
//! what format, theme, or template engine is being used.
//!
//! # State Management
//!
//! [`CommandContext`] provides two mechanisms for dependency injection:
//!
//! - **`app_state`**: Immutable, app-lifetime state (database, config, API clients).
//!   Configured at app build time, shared across all dispatches via `Arc<Extensions>`.
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
//! - Handler traits: Thread-safe ([`Handler`]) and local ([`LocalHandler`]) variants
//! - Hook system: Pre/post dispatch and post-output hooks for cross-cutting concerns
//! - State injection: App-level state via `app_state`, per-request state via `extensions`
//! - Render abstraction: Pluggable render handlers via [`RenderFn`] / [`LocalRenderFn`]
//!
//! # Usage
//!
//! ## Standalone (no rendering framework)
//!
//! ```rust,ignore
//! use standout_dispatch::{Handler, Output, from_fn};
//!
//! // Simple render handler that just serializes to JSON
//! let render = from_fn(|data, _| Ok(serde_json::to_string_pretty(data)?));
//!
//! Dispatcher::builder()
//!     .command("list", list_handler, render)
//!     .build()?
//!     .run(cmd, args);
//! ```
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
//! In this case, `standout` creates the render handler internally, injecting
//! the template registry, theme, and output format from CLI args.

// Core modules
mod dispatch;
mod handler;
mod hooks;
mod render;

// Re-export command routing utilities
pub use dispatch::{
    extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
    path_to_string, string_to_path,
};

// Re-export handler types
pub use handler::{
    CommandContext, Extensions, FnHandler, Handler, HandlerResult, IntoHandlerResult,
    LocalFnHandler, LocalHandler, LocalSimpleFnHandler, Output, RunResult, SimpleFnHandler,
};

// Re-export hook types
pub use hooks::{
    HookError, HookPhase, Hooks, PostDispatchFn, PostOutputFn, PreDispatchFn, RenderedOutput,
};

// Re-export render abstraction
pub use render::{from_fn, from_fn_mut, LocalRenderFn, RenderError, RenderFn};
