//! Command dispatch logic.
//!
//! Internal types and functions for dispatching commands to handlers.
//!
//! This module provides the dispatch function type for single-threaded CLI apps:
//!
//! - [`DispatchFn`]: Dispatch using `Rc<RefCell<dyn FnMut>>` (single-threaded)

use clap::ArgMatches;
use std::cell::RefCell;
use std::rc::Rc;

use crate::cli::handler::Output as HandlerOutput;
use crate::cli::handler::{CommandContext, RunError, RunErrorKind};
use crate::cli::hooks::Hooks;
use crate::context::{ContextRegistry, RenderContext};
use crate::StructuredOutputProjection;
use crate::Theme;
use serde::Serialize;

// Re-export pure dispatch utilities from standout-dispatch
pub use standout_dispatch::{
    extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
};

/// Internal result type for dispatch functions.
pub enum DispatchOutput {
    /// Text output with both formatted (ANSI) and raw versions.
    Text {
        /// The formatted output with ANSI codes (for terminal display)
        formatted: String,
        /// The raw output without ANSI codes (for piping)
        raw: String,
    },
    /// Binary output (bytes, filename)
    Binary(Vec<u8>, String),
    /// No output (silent)
    Silent,
}

/// Helper to render output from a handler.
///
/// This shared logic ensures consistent hook execution, context injection, and rendering.
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
    template_engine: &dyn standout_render::template::TemplateEngine,
    output_mode: crate::OutputMode,
    structured_output_projection: Option<&StructuredOutputProjection>,
) -> Result<DispatchOutput, RunError> {
    match result {
        Ok(output) => match output {
            HandlerOutput::Render(data) => {
                let mut json_data = serde_json::to_value(&data).map_err(|e| {
                    RunError::new(
                        format!("Failed to serialize handler result: {}", e),
                        RunErrorKind::Render,
                    )
                })?;

                if let Some(hooks) = hooks {
                    json_data = hooks
                        .run_post_dispatch(matches, ctx, json_data)
                        .map_err(|e| {
                            RunError::new(format!("Hook error: {}", e), RunErrorKind::Hook(e.phase))
                        })?;
                }

                let render_ctx = RenderContext::new(
                    output_mode,
                    standout_render::detect_terminal_width(),
                    theme,
                    &json_data,
                );

                // Projection happens at the presentation boundary: after
                // post-dispatch hooks and before post-output hooks.
                let render_result = match (output_mode, structured_output_projection) {
                    (crate::OutputMode::Csv, Some(projection)) => {
                        standout_render::template::RenderResult::plain(
                            projection
                                .csv_projection()
                                .render(&json_data)
                                .map_err(|e| RunError::new(e.to_string(), RunErrorKind::Render))?,
                        )
                    }
                    _ => standout_render::template::render_auto_with_engine_split(
                        template_engine,
                        template,
                        &json_data,
                        theme,
                        output_mode,
                        context_registry,
                        &render_ctx,
                    )
                    .map_err(|e| RunError::new(e.to_string(), RunErrorKind::Render))?,
                };

                Ok(DispatchOutput::Text {
                    formatted: render_result.formatted,
                    raw: render_result.raw,
                })
            }
            HandlerOutput::Silent => Ok(DispatchOutput::Silent),
            HandlerOutput::Binary { data, filename } => Ok(DispatchOutput::Binary(data, filename)),
        },
        Err(e) => Err(RunError::new(
            format!("Error: {}", e),
            RunErrorKind::Handler,
        )),
    }
}

/// Type-erased dispatch function for single-threaded handlers.
///
/// Takes ArgMatches, CommandContext, optional Hooks, OutputMode, and Theme.
/// The hooks parameter allows post-dispatch hooks to run between handler
/// execution and rendering. OutputMode is passed separately because CommandContext
/// is render-agnostic, while output_mode is a rendering concern.
/// Theme is passed at runtime (late binding) to ensure the correct theme is used.
///
/// Uses `Rc<RefCell<_>>` and `FnMut` for single-threaded CLI apps.
pub type DispatchFn = Rc<
    RefCell<
        dyn FnMut(
            &ArgMatches,
            &CommandContext,
            Option<&Hooks>,
            crate::OutputMode,
            &crate::Theme,
        ) -> Result<DispatchOutput, RunError>,
    >,
>;

/// Dispatches the command with the given context.
pub fn dispatch(
    dispatch_fn: &DispatchFn,
    matches: &ArgMatches,
    ctx: &CommandContext,
    hooks: Option<&Hooks>,
    output_mode: crate::OutputMode,
    theme: &crate::Theme,
) -> Result<DispatchOutput, RunError> {
    (dispatch_fn.borrow_mut())(matches, ctx, hooks, output_mode, theme)
}

// Note: extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
// path_to_string, and string_to_path are now re-exported from standout-dispatch at the top
// of this file. Tests for these functions are in the standout-dispatch crate.
