//! Command dispatch logic.
//!
//! Internal types and functions for dispatching commands to handlers.
//!
//! This module provides dispatch function types for both handler modes:
//!
//! - [`DispatchFn`]: Thread-safe dispatch using `Arc<dyn Fn + Send + Sync>`
//! - [`LocalDispatchFn`]: Local dispatch using `Rc<RefCell<dyn FnMut>>`

use clap::ArgMatches;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::cli::handler::CommandContext;
use crate::cli::handler::Output as HandlerOutput;
use crate::cli::hooks::Hooks;
use crate::context::{ContextRegistry, RenderContext};
use crate::{render_auto_with_context, TemplateRegistry, Theme};
use serde::Serialize;

// Re-export pure dispatch utilities from standout-dispatch
pub use standout_dispatch::{
    extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
};

/// Trait for dispatching commands.
///
/// This trait abstracts over the execution of dispatch functions, allowing
/// unified handling of both thread-safe (`Arc<Fn>`) and local (`Rc<RefCell<FnMut>>`)
/// handlers.
pub trait Dispatchable {
    /// Dispatches the command with the given context.
    ///
    /// `output_mode` is passed separately because CommandContext is render-agnostic
    /// (from standout-dispatch), while output_mode is a rendering concern.
    fn dispatch(
        &self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        hooks: Option<&Hooks>,
        output_mode: crate::OutputMode,
    ) -> Result<DispatchOutput, String>;
}

/// Internal result type for dispatch functions.
pub enum DispatchOutput {
    /// Text output (rendered template or JSON)
    Text(String),
    /// Binary output (bytes, filename)
    Binary(Vec<u8>, String),
    /// No output (silent)
    Silent,
}

/// Helper to render output from a handler.
///
/// This shared logic ensures consistency between ThreadSafe and Local dispatchers,
/// including hook execution, context injection, and rendering.
///
/// Note: `output_mode` is passed separately from `ctx` because CommandContext is
/// render-agnostic (from standout-dispatch), while output_mode is a rendering concern
/// managed by standout.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_handler_output<T: Serialize>(
    result: Result<HandlerOutput<T>, String>,
    matches: &ArgMatches,
    ctx: &CommandContext,
    hooks: Option<&Hooks>,
    template: &str,
    theme: &Theme,
    context_registry: &ContextRegistry,
    template_registry: Option<&TemplateRegistry>,
    output_mode: crate::OutputMode,
) -> Result<DispatchOutput, String> {
    match result {
        Ok(output) => match output {
            HandlerOutput::Render(data) => {
                let mut json_data = serde_json::to_value(&data)
                    .map_err(|e| format!("Failed to serialize handler result: {}", e))?;

                if let Some(hooks) = hooks {
                    json_data = hooks
                        .run_post_dispatch(matches, ctx, json_data)
                        .map_err(|e| format!("Hook error: {}", e))?;
                }

                let render_ctx = RenderContext::new(
                    output_mode,
                    crate::cli::app::get_terminal_width(),
                    theme,
                    &json_data,
                );

                let output = render_auto_with_context(
                    template,
                    &json_data,
                    theme,
                    output_mode,
                    context_registry,
                    &render_ctx,
                    template_registry,
                )
                .map_err(|e| e.to_string())?;
                Ok(DispatchOutput::Text(output))
            }
            HandlerOutput::Silent => Ok(DispatchOutput::Silent),
            HandlerOutput::Binary { data, filename } => Ok(DispatchOutput::Binary(data, filename)),
        },
        Err(e) => Err(format!("Error: {}", e)),
    }
}

/// Type-erased dispatch function for thread-safe handlers.
///
/// Takes ArgMatches, CommandContext, optional Hooks, and OutputMode. The hooks
/// parameter allows post-dispatch hooks to run between handler execution and
/// rendering. OutputMode is passed separately because CommandContext is
/// render-agnostic (from standout-dispatch), while output_mode is a rendering
/// concern managed by standout.
///
/// Used with [`App`](super::App) and [`Handler`](super::handler::Handler).
pub type DispatchFn = Arc<
    dyn Fn(
            &ArgMatches,
            &CommandContext,
            Option<&Hooks>,
            crate::OutputMode,
        ) -> Result<DispatchOutput, String>
        + Send
        + Sync,
>;

impl Dispatchable for DispatchFn {
    fn dispatch(
        &self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        hooks: Option<&Hooks>,
        output_mode: crate::OutputMode,
    ) -> Result<DispatchOutput, String> {
        (self)(matches, ctx, hooks, output_mode)
    }
}

/// Type-erased dispatch function for local (single-threaded) handlers.
///
/// Unlike [`DispatchFn`], this:
/// - Uses `Rc<RefCell<_>>` instead of `Arc` (no thread-safety overhead)
/// - Uses `FnMut` instead of `Fn` (allows mutable state)
/// - Does NOT require `Send + Sync`
///
/// OutputMode is passed separately because CommandContext is render-agnostic
/// (from standout-dispatch), while output_mode is a rendering concern.
///
/// Used with [`LocalApp`](super::LocalApp) and [`LocalHandler`](super::handler::LocalHandler).
pub type LocalDispatchFn = Rc<
    RefCell<
        dyn FnMut(
            &ArgMatches,
            &CommandContext,
            Option<&Hooks>,
            crate::OutputMode,
        ) -> Result<DispatchOutput, String>,
    >,
>;

impl Dispatchable for LocalDispatchFn {
    fn dispatch(
        &self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        hooks: Option<&Hooks>,
        output_mode: crate::OutputMode,
    ) -> Result<DispatchOutput, String> {
        (self.borrow_mut())(matches, ctx, hooks, output_mode)
    }
}

// Note: extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
// path_to_string, and string_to_path are now re-exported from standout-dispatch at the top
// of this file. Tests for these functions are in the standout-dispatch crate.
