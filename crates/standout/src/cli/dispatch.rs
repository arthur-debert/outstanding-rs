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
use crate::cli::handler::{CommandContext, ExternalFailure, RunError, RunErrorKind};
use crate::cli::hooks::{HookError, Hooks};
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

#[derive(Debug)]
struct HandlerErrorSource(Box<dyn std::error::Error + Send + Sync + 'static>);

impl std::fmt::Display for HandlerErrorSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for HandlerErrorSource {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
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
    result: crate::cli::HandlerResult<T>,
    matches: &ArgMatches,
    ctx: &CommandContext,
    hooks: Option<&Hooks>,
    template: &str,
    theme: &Theme,
    context_registry: &ContextRegistry,
    template_engine: &dyn standout_render::template::TemplateEngine,
    output_mode: crate::OutputMode,
    structured_output_projection: Option<&StructuredOutputProjection>,
    ambiguous_width: crate::AmbiguousWidth,
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
                    json_data =
                        hooks
                            .run_post_dispatch(matches, ctx, json_data)
                            .map_err(|error| {
                                hook_run_error(error, crate::cli::HookPhase::PostDispatch)
                            })?;
                }

                let ambiguous_width =
                    standout_render::detect_ambiguous_width_override().unwrap_or(ambiguous_width);
                let render_ctx = RenderContext::with_ambiguous_width(
                    output_mode,
                    standout_render::detect_terminal_width(),
                    ambiguous_width,
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
        Err(error) => Err(handler_run_error(error)),
    }
}

/// Converts the one application-declared escape hatch without changing the
/// status policy for ordinary handler errors.
pub(crate) fn handler_run_error(error: anyhow::Error) -> RunError {
    let error = match error.downcast::<ExternalFailure>() {
        Ok(external) => return RunError::from(external),
        Err(error) => error,
    };

    RunError::new(format!("Error: {}", error), RunErrorKind::Handler)
        .with_source(HandlerErrorSource(error.into_boxed_dyn_error()))
}

/// Converts a hook failure using the phase that actually executed.
///
/// Only the pre-dispatch seam recognizes `ExternalFailure`; a post-dispatch or
/// post-output hook cannot opt into external status handling by self-labeling
/// its `HookError` as pre-dispatch.
pub(crate) fn hook_run_error(mut error: HookError, phase: crate::cli::HookPhase) -> RunError {
    if phase == crate::cli::HookPhase::PreDispatch {
        if let Some(source) = error.source.take() {
            match source.downcast::<ExternalFailure>() {
                Ok(external) => return RunError::from(*external),
                Err(source) => error.source = Some(source),
            }
        }
    }

    error.phase = phase;
    RunError::new(format!("Hook error: {}", error), RunErrorKind::Hook(phase)).with_source(error)
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
            crate::AmbiguousWidth,
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
    ambiguous_width: crate::AmbiguousWidth,
) -> Result<DispatchOutput, RunError> {
    (dispatch_fn.borrow_mut())(matches, ctx, hooks, output_mode, theme, ambiguous_width)
}

// Note: extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
// path_to_string, and string_to_path are now re-exported from standout-dispatch at the top
// of this file. Tests for these functions are in the standout-dispatch crate.
