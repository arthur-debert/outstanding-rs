//! Hook system for pre/post command execution.
//!
//! Hooks allow you to run custom code before and after command handlers execute.
//! They are registered per-command and support chaining with transformation.
//!
//! # Hook Points
//!
//! - Pre-dispatch: Runs before the command handler. Can abort execution.
//! - Post-dispatch: Runs after the handler but before rendering. Receives the raw
//!   handler data as `serde_json::Value`. Can inspect, modify, or replace the data.
//! - Post-output: Runs after output is generated. Can transform output or abort.
//!
//! # Example
//!
//! ```rust,ignore
//! use standout::cli::{App, Hooks, RenderedOutput};
//! use serde_json::json;
//!
//! App::builder()
//!     .command("list", handler, template)
//!     .hooks("list", Hooks::new()
//!         .pre_dispatch(|_m, ctx| {
//!             println!("Running: {}", ctx.command_path.join(" "));
//!             Ok(())
//!         })
//!         .post_dispatch(|_m, _ctx, mut data| {
//!             // Add metadata before rendering
//!             if let Some(obj) = data.as_object_mut() {
//!                 obj.insert("timestamp".into(), json!(chrono::Utc::now().to_rfc3339()));
//!             }
//!             Ok(data)
//!         })
//!         .post_output(|_m, _ctx, output| {
//!             // Copy to clipboard (pseudo-code)
//!             if let RenderedOutput::Text(ref text) = output {
//!                 // clipboard::copy(text)?;
//!             }
//!             Ok(output)
//!         }))
//!     .build()?
//!     .run(cmd, args);
//! ```
//!
//! This module re-exports hook types from `standout-dispatch`.
//! See [`standout_dispatch::hooks`] for the underlying implementation.

// Re-export all hook types from standout-dispatch.
// These types are render-agnostic and focus on hook execution.
pub use standout_dispatch::{
    HookError, HookPhase, Hooks, PostDispatchFn, PostOutputFn, PreDispatchFn, RenderedOutput,
};

// Tests for these types are in the standout-dispatch crate.
