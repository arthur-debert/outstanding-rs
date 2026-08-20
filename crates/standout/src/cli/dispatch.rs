//! Command dispatch logic.
//!
//! Internal types and functions for dispatching commands to handlers.
//!
//! This module provides the dispatch function type for single-threaded CLI apps:
//!
//! - [`DispatchFn`]: Dispatch using `Rc<RefCell<dyn FnMut>>` (single-threaded)

use clap::ArgMatches;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::cli::builder::{SharedTemplateEngine, TemplateAbsence, TemplateRef};
use crate::cli::handler::Output as HandlerOutput;
use crate::cli::handler::{CommandContext, ExternalFailure, RunError, RunErrorKind};
use crate::cli::hooks::{ArtifactOutput, HookError, Hooks};
use crate::context::ContextRegistry;
use crate::Theme;
use crate::{ColorPolicy, RenderRequest, StructuredOutputProjection, TargetProperties};
use serde::Serialize;
use standout_render::render_request_split;

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
    /// A compound artifact whose report is deliberately *not* rendered yet.
    ///
    /// The report can only name the destination once the framework has
    /// selected one and the write has succeeded, so the artifact travels with
    /// the [`RenderRequest`] needed to render it later, in `App::dispatch`,
    /// after the write.
    Artifact {
        /// Bytes, suggestion, stdout opt-in, and the serialized report.
        output: ArtifactOutput,
        /// How to render that report once the receipt exists. Boxed: the
        /// request dwarfs the other variants' payloads.
        request: Box<RenderRequest>,
    },
    /// No output (silent)
    Silent,
}

/// Maps glue [`TemplateRef`] onto the render-time type.
///
/// Silent and binary absence cannot serialize: they keep the actionable
/// [`absent_template_render_error`]. Only structured-only maps to
/// render-time [`standout_render::TemplateRef::Absent`]. Named templates
/// still require a registry; the leaf loads the include tree from it.
fn render_time_template(
    command_path: &str,
    template: &TemplateRef,
    template_registry: Option<&Rc<crate::TemplateRegistry>>,
    output_mode: crate::OutputMode,
) -> Result<standout_render::TemplateRef, RunError> {
    match template {
        TemplateRef::Named(name) | TemplateRef::Convention(name) => {
            if template_registry.is_none() {
                return Err(RunError::new(
                    format!(
                        "command `{command_path}` references template `{name}`, but no template registry is configured; add .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\") before .build()"
                    ),
                    RunErrorKind::Render,
                ));
            }
            Ok(standout_render::TemplateRef::Named(name.clone()))
        }
        TemplateRef::Inline(source) => Ok(standout_render::TemplateRef::Inline(source.clone())),
        TemplateRef::Absent(reason) => {
            match reason {
                TemplateAbsence::Silent | TemplateAbsence::Binary => Err(
                    absent_template_render_error(command_path, *reason, output_mode),
                ),
                TemplateAbsence::StructuredOnly => {
                    if matches!(
                        output_mode,
                        crate::OutputMode::Term
                            | crate::OutputMode::Text
                            | crate::OutputMode::TermDebug
                    ) {
                        return Err(absent_template_render_error(
                            command_path,
                            *reason,
                            output_mode,
                        ));
                    }
                    Ok(standout_render::TemplateRef::Absent)
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_render_request(
    command_path: &str,
    json_data: serde_json::Value,
    template: &TemplateRef,
    theme: &Theme,
    context_registry: &ContextRegistry,
    template_engine: &SharedTemplateEngine,
    template_registry: Option<&Rc<crate::TemplateRegistry>>,
    output_mode: crate::OutputMode,
    structured_output_projection: Option<&StructuredOutputProjection>,
    target: TargetProperties,
    warnings: Option<standout_render::warnings::WarningBuffer>,
) -> Result<RenderRequest, RunError> {
    let template = render_time_template(command_path, template, template_registry, output_mode)?;
    Ok(RenderRequest {
        data: json_data,
        template,
        theme: theme.clone(),
        format: output_mode,
        color_policy: ColorPolicy::Auto,
        target,
        engine: template_engine.clone(),
        registry: template_registry.cloned(),
        context_registry: Some(context_registry.clone()),
        csv_projection: structured_output_projection
            .map(|projection| projection.csv_projection().clone()),
        extras: HashMap::new(),
        warnings,
    })
}

fn render_via_request(request: &RenderRequest) -> Result<(String, String), RunError> {
    let rendered = render_request_split(request).map_err(render_error)?;
    Ok((rendered.formatted, rendered.raw))
}

fn render_error(error: standout_render::RenderError) -> RunError {
    RunError::new(error.to_string(), RunErrorKind::Render)
}

fn absent_template_render_error(
    command_path: &str,
    reason: TemplateAbsence,
    output_mode: crate::OutputMode,
) -> RunError {
    let reason = match reason {
        TemplateAbsence::Silent => "silent",
        TemplateAbsence::StructuredOnly => "structured-only",
        TemplateAbsence::Binary => "binary",
    };
    RunError::new(
        format!(
            "command `{command_path}` is declared {reason} and cannot render data in --output {output_mode:?}; configure a template with .template(...) or .template_name(...) or return the matching Output variant"
        ),
        RunErrorKind::Render,
    )
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
    template: &TemplateRef,
    theme: &Theme,
    context_registry: &ContextRegistry,
    template_engine: &SharedTemplateEngine,
    template_registry: Option<&Rc<crate::TemplateRegistry>>,
    output_mode: crate::OutputMode,
    structured_output_projection: Option<&StructuredOutputProjection>,
    ambiguous_width: crate::AmbiguousWidth,
    target: Option<TargetProperties>,
) -> Result<DispatchOutput, RunError> {
    let output = match result {
        Ok(output) => output,
        Err(error) => return Err(handler_run_error(error)),
    };

    let command_path = ctx.command_path.join(".");
    let target = match target {
        Some(target) => target,
        None => {
            let mut detected = TargetProperties::detect();
            detected.ambiguous_width = ambiguous_width;
            detected
        }
    };
    let warnings = ctx
        .extensions
        .get::<standout_render::warnings::WarningBuffer>()
        .cloned();
    let request_for = |json_data: serde_json::Value| {
        build_render_request(
            &command_path,
            json_data,
            template,
            theme,
            context_registry,
            template_engine,
            template_registry,
            output_mode,
            structured_output_projection,
            target,
            warnings.clone(),
        )
    };

    match output {
        HandlerOutput::Render(data) => {
            let json_data = serialize_handler_data(&data)?;
            let json_data = run_post_dispatch_hooks(json_data, matches, ctx, hooks)?;
            let request = request_for(json_data)?;
            let (formatted, raw) = render_via_request(&request)?;
            Ok(DispatchOutput::Text { formatted, raw })
        }
        HandlerOutput::Silent => Ok(DispatchOutput::Silent),
        HandlerOutput::Binary { data, filename } => Ok(DispatchOutput::Binary(data, filename)),
        HandlerOutput::Artifact(artifact) => {
            let (bytes, suggested_destination, stdout_allowed, report) = artifact.into_parts();

            // The report is handler data like any other, so it passes through
            // post-dispatch hooks on the same seam `Output::Render` uses. It
            // is *not* rendered here: rendering waits for the receipt.
            let report = match report {
                Some(report) => {
                    let json = serialize_handler_data(&report)?;
                    Some(run_post_dispatch_hooks(json, matches, ctx, hooks)?)
                }
                None => None,
            };

            let request = request_for(serde_json::Value::Null)?;
            Ok(DispatchOutput::Artifact {
                request: Box::new(request),
                output: ArtifactOutput {
                    bytes,
                    suggested_destination,
                    stdout_allowed,
                    report,
                },
            })
        }
        // `Output` is `#[non_exhaustive]`, so this arm is required from
        // outside standout-dispatch. It is unreachable for any variant this
        // version knows; a future one lands here loudly rather than silently.
        _ => Err(RunError::new(
            "Unsupported handler output variant: this standout version cannot present it",
            RunErrorKind::Render,
        )),
    }
}

/// Serializes handler data, mapping failure onto the render origin.
fn serialize_handler_data<T: Serialize>(data: &T) -> Result<serde_json::Value, RunError> {
    serde_json::to_value(data).map_err(|e| {
        RunError::new(
            format!("Failed to serialize handler result: {}", e),
            RunErrorKind::Render,
        )
    })
}

/// Runs post-dispatch hooks over serialized handler data, if any are registered.
fn run_post_dispatch_hooks(
    json_data: serde_json::Value,
    matches: &ArgMatches,
    ctx: &CommandContext,
    hooks: Option<&Hooks>,
) -> Result<serde_json::Value, RunError> {
    match hooks {
        Some(hooks) => hooks
            .run_post_dispatch(matches, ctx, json_data)
            .map_err(|error| hook_run_error(error, crate::cli::HookPhase::PostDispatch)),
        None => Ok(json_data),
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
/// Takes ArgMatches, CommandContext, optional Hooks, OutputMode, Theme,
/// ambiguous-width policy, and optional destination facts from `run_with`.
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
            Option<TargetProperties>,
        ) -> Result<DispatchOutput, RunError>,
    >,
>;

/// Dispatches the command with the given context.
#[allow(clippy::too_many_arguments)]
pub fn dispatch(
    dispatch_fn: &DispatchFn,
    matches: &ArgMatches,
    ctx: &CommandContext,
    hooks: Option<&Hooks>,
    output_mode: crate::OutputMode,
    theme: &crate::Theme,
    ambiguous_width: crate::AmbiguousWidth,
    target: Option<TargetProperties>,
) -> Result<DispatchOutput, RunError> {
    (dispatch_fn.borrow_mut())(
        matches,
        ctx,
        hooks,
        output_mode,
        theme,
        ambiguous_width,
        target,
    )
}

// Note: extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
// path_to_string, and string_to_path are now re-exported from standout-dispatch at the top
// of this file. Tests for these functions are in the standout-dispatch crate.
