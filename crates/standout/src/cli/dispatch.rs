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

use crate::cli::builder::{
    refresh_engine_templates, SharedTemplateEngine, TemplateAbsence, TemplateRef,
};
use crate::cli::handler::Output as HandlerOutput;
use crate::cli::handler::{CommandContext, ExternalFailure, RunError, RunErrorKind};
use crate::cli::hooks::{ArtifactOutput, HookError, Hooks};
use crate::context::{ContextRegistry, RenderContext};
use crate::StructuredOutputProjection;
use crate::Theme;
use serde::Serialize;
use standout_render::template::ResolvedTemplate;

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
    /// the presentation configuration needed to render it later, in
    /// `AppBuilder::dispatch`, after the write.
    Artifact {
        /// Bytes, suggestion, stdout opt-in, and the serialized report.
        output: ArtifactOutput,
        /// How to render that report once the receipt exists. Boxed: the
        /// presentation config dwarfs the other variants' payloads.
        presentation: Box<Presentation>,
    },
    /// No output (silent)
    Silent,
}

/// Everything needed to turn one JSON value into presented text.
///
/// The render pipeline normally runs inside [`render_handler_output`], but the
/// artifact path has to defer it until after the final write. Capturing the
/// configuration in an owned value lets the same rendering rules apply at
/// either point, instead of the artifact path growing a second, drifting copy.
pub struct Presentation {
    command_path: String,
    template: TemplateRef,
    theme: Theme,
    context_registry: ContextRegistry,
    template_engine: SharedTemplateEngine,
    template_registry: Option<Rc<crate::TemplateRegistry>>,
    output_mode: crate::OutputMode,
    structured_output_projection: Option<StructuredOutputProjection>,
    ambiguous_width: crate::AmbiguousWidth,
}

impl Presentation {
    /// Renders `json_data`, returning `(formatted, raw)`.
    ///
    /// Structured modes serialize the value directly; templated modes render
    /// the command's template. A CSV projection, when configured, replaces the
    /// template for `OutputMode::Csv`.
    pub(crate) fn render(
        &self,
        json_data: &serde_json::Value,
    ) -> Result<(String, String), RunError> {
        let ambiguous_width =
            standout_render::detect_ambiguous_width_override().unwrap_or(self.ambiguous_width);
        let render_ctx = RenderContext::with_ambiguous_width(
            self.output_mode,
            standout_render::detect_terminal_width(),
            ambiguous_width,
            &self.theme,
            json_data,
        );

        // Projection happens at the presentation boundary: after
        // post-dispatch hooks and before post-output hooks.
        let render_result = match (self.output_mode, self.structured_output_projection.as_ref()) {
            (crate::OutputMode::Csv, Some(projection)) => {
                standout_render::template::RenderResult::plain(
                    projection
                        .csv_projection()
                        .render(json_data)
                        .map_err(|e| RunError::new(e.to_string(), RunErrorKind::Render))?,
                )
            }
            _ => self.render_template_ref(json_data, &render_ctx)?,
        };

        Ok((render_result.formatted, render_result.raw))
    }

    fn render_template_ref(
        &self,
        json_data: &serde_json::Value,
        render_ctx: &RenderContext,
    ) -> Result<standout_render::template::RenderResult, RunError> {
        if let TemplateRef::Absent(reason) = &self.template {
            return match reason {
                TemplateAbsence::StructuredOnly => {
                    let mode = match self.output_mode {
                        crate::OutputMode::Auto => crate::OutputMode::Json,
                        crate::OutputMode::Json
                        | crate::OutputMode::Yaml
                        | crate::OutputMode::Xml
                        | crate::OutputMode::Csv => self.output_mode,
                        crate::OutputMode::Term
                        | crate::OutputMode::Text
                        | crate::OutputMode::TermDebug => {
                            return Err(absent_template_render_error(
                                &self.command_path,
                                *reason,
                                self.output_mode,
                            ));
                        }
                    };
                    standout_render::template::render_auto_with_engine_split(
                        &**self.template_engine.borrow(),
                        "",
                        json_data,
                        &self.theme,
                        mode,
                        &self.context_registry,
                        render_ctx,
                    )
                    .map_err(render_error)
                }
                TemplateAbsence::Silent | TemplateAbsence::Binary => Err(
                    absent_template_render_error(&self.command_path, *reason, self.output_mode),
                ),
            };
        }

        if self.output_mode.is_structured() {
            return standout_render::template::render_auto_with_engine_split(
                &**self.template_engine.borrow(),
                "",
                json_data,
                &self.theme,
                self.output_mode,
                &self.context_registry,
                render_ctx,
            )
            .map_err(render_error);
        }

        match &self.template {
            TemplateRef::Inline(source) => {
                standout_render::template::render_auto_with_engine_split(
                    &**self.template_engine.borrow(),
                    source,
                    json_data,
                    &self.theme,
                    self.output_mode,
                    &self.context_registry,
                    render_ctx,
                )
                .map_err(render_error)
            }
            TemplateRef::Named(name) => {
                let registry = self.template_registry.as_ref().ok_or_else(|| {
                    RunError::new(
                        format!(
                            "command `{}` references template `{}`, but no template registry is configured",
                            self.command_path, name
                        ),
                        RunErrorKind::Render,
                    )
                })?;
                registry.get_content(name).map_err(|error| {
                    RunError::new(
                        format!(
                            "template `{}` for command `{}` could not be loaded: {}",
                            name, self.command_path, error
                        ),
                        RunErrorKind::Render,
                    )
                })?;
                {
                    let mut engine = self.template_engine.borrow_mut();
                    refresh_engine_templates(&mut **engine, registry).map_err(|error| {
                        RunError::new(
                            format!(
                                "template `{}`{} for command `{}` could not be compiled: {}",
                                name,
                                template_location(registry, name),
                                self.command_path,
                                error
                            ),
                            RunErrorKind::Render,
                        )
                    })?;
                }
                standout_render::template::render_auto_with_engine_split(
                    &**self.template_engine.borrow(),
                    name,
                    json_data,
                    &self.theme,
                    self.output_mode,
                    &self.context_registry,
                    render_ctx,
                )
                .map_err(render_error)
            }
            TemplateRef::Absent(_) => unreachable!("absence handled before template rendering"),
        }
    }
}

fn template_location(registry: &crate::TemplateRegistry, name: &str) -> String {
    match registry.get(name) {
        Ok(ResolvedTemplate::File(path)) => format!(" at `{}`", path.display()),
        Ok(ResolvedTemplate::Inline(_)) | Err(_) => String::new(),
    }
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
            "command `{command_path}` is declared {reason} and cannot render data in --output {output_mode:?}; configure a template with .template(...) or return the matching Output variant"
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
) -> Result<DispatchOutput, RunError> {
    let output = match result {
        Ok(output) => output,
        Err(error) => return Err(handler_run_error(error)),
    };

    let presentation = Presentation {
        command_path: ctx.command_path.join("."),
        template: template.clone(),
        theme: theme.clone(),
        context_registry: context_registry.clone(),
        template_engine: template_engine.clone(),
        template_registry: template_registry.cloned(),
        output_mode,
        structured_output_projection: structured_output_projection.cloned(),
        ambiguous_width,
    };

    match output {
        HandlerOutput::Render(data) => {
            let json_data = serialize_handler_data(&data)?;
            let json_data = run_post_dispatch_hooks(json_data, matches, ctx, hooks)?;
            let (formatted, raw) = presentation.render(&json_data)?;
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

            Ok(DispatchOutput::Artifact {
                presentation: Box::new(presentation),
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
