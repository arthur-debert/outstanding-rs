use clap::ArgMatches;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::cli::builder::{SharedTemplateEngine, TemplateAbsence, TemplateRef};
use crate::cli::handler::Output as HandlerOutput;
use crate::cli::handler::{
    AppFailure, CommandContext, Diagnostic, ExitStatus, ExternalFailure, RunError, RunErrorKind,
    RunRecorder, StreamSink,
};
use crate::cli::hooks::{ArtifactOutput, HookError, Hooks};
use crate::context::ContextRegistry;
use crate::Theme;
use crate::{ColorPolicy, RenderRequest, StructuredOutputProjection, TargetProperties};
use serde::Serialize;
use standout_render::render_request_split;

pub use standout_dispatch::{extract_command_path, get_deepest_matches};

pub enum DispatchOutput {
    Text {
        formatted: String,
        raw: String,
        status: ExitStatus,
    },
    Binary(Vec<u8>, String),
    Artifact {
        output: ArtifactOutput,
        request: Box<RenderRequest>,
    },
    Silent {
        status: ExitStatus,
    },
}

/// A binary or artifact outcome has nowhere to carry a status, so it is a render error.
pub(crate) fn status_without_a_carrier(status: ExitStatus, output: &str) -> RunError {
    RunError::new(
        format!(
            "exit status {} was declared on {output} output; a declared status rides on \
             Output::Render and Output::Silent only",
            status.code()
        ),
        RunErrorKind::Render,
    )
}

pub(crate) fn reject_status_without_a_carrier(
    status: Option<ExitStatus>,
    is_binary: bool,
    is_artifact: bool,
) -> Result<(), RunError> {
    let Some(status) = status else {
        return Ok(());
    };
    let carrier = if is_binary {
        "binary"
    } else if is_artifact {
        "artifact"
    } else {
        return Ok(());
    };
    Err(status_without_a_carrier(status, carrier))
}

/// An `ndjson` stream has no room for a payload, so it is a render error.
pub(crate) fn payload_without_a_stream(output: &str) -> RunError {
    RunError::new(
        format!(
            "{output} output was produced under ndjson; a stream carries Output::Render and \
             Output::Silent only"
        ),
        RunErrorKind::Render,
    )
}

/// Rendered events are already on the destination, so a payload cannot follow
/// them: it would be a second document sharing one file or one stdout.
pub(crate) fn reject_payload_after_events(
    events: usize,
    output: &DispatchOutput,
) -> Result<(), RunError> {
    if events == 0 {
        return Ok(());
    }
    let payload = match output {
        DispatchOutput::Binary(_, _) => "binary",
        DispatchOutput::Artifact { .. } => "artifact",
        DispatchOutput::Text { .. } | DispatchOutput::Silent { .. } => return Ok(()),
    };
    Err(RunError::new(
        format!(
            "{payload} output was produced by a command that emitted events; a command \
             producing events carries Output::Render and Output::Silent only"
        ),
        RunErrorKind::Render,
    ))
}

pub(crate) fn reject_payload_under_stream(
    output_mode: crate::Representation,
    is_binary: bool,
    is_artifact: bool,
) -> Result<(), RunError> {
    if !output_mode.is_stream() {
        return Ok(());
    }
    let payload = if is_binary {
        "binary"
    } else if is_artifact {
        "artifact"
    } else {
        return Ok(());
    };
    Err(payload_without_a_stream(payload))
}

fn render_time_template(
    command_path: &str,
    template: &TemplateRef,
    template_registry: Option<&Rc<crate::TemplateRegistry>>,
    output_mode: crate::Representation,
) -> Result<standout_render::TemplateRef, RunError> {
    match template {
        TemplateRef::Named(name) => {
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
                // A structured-only command has no template, so its human
                // representation is the JSON its bare invocation serializes;
                // only the style-tag diagnostic view has nothing to show.
                TemplateAbsence::StructuredOnly => {
                    if output_mode.is_debug() {
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
    output_mode: crate::Representation,
    color_policy: ColorPolicy,
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
        color_policy,
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
    output_mode: crate::Representation,
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_handler_output<T: Serialize>(
    result: crate::cli::HandlerResult<T>,
    matches: &ArgMatches,
    ctx: &CommandContext,
    recorder: &RunRecorder,
    hooks: Option<&Hooks>,
    template: &TemplateRef,
    theme: &Theme,
    context_registry: &ContextRegistry,
    template_engine: &SharedTemplateEngine,
    template_registry: Option<&Rc<crate::TemplateRegistry>>,
    output_mode: crate::Representation,
    color_policy: ColorPolicy,
    structured_output_projection: Option<&StructuredOutputProjection>,
    target: TargetProperties,
) -> Result<DispatchOutput, RunError> {
    let (output, status) = match result {
        Ok(output) => output.split_exit_status(),
        Err(error) => return Err(handler_run_error(error)),
    };
    reject_status_without_a_carrier(status, output.is_binary(), output.is_artifact())?;
    let status = status.unwrap_or(ExitStatus::SUCCESS);

    let command_path = ctx.command_path.join(".");
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
            color_policy,
            structured_output_projection,
            target,
            warnings.clone(),
        )
    };

    match output {
        HandlerOutput::Render(data) => {
            let json_data = serialize_handler_data(&data)?;
            let json_data = run_post_dispatch_hooks(json_data, matches, ctx, hooks)?;
            recorder.record(json_data.clone());
            let request = request_for(json_data)?;
            let (formatted, raw) = render_via_request(&request)?;
            Ok(DispatchOutput::Text {
                formatted,
                raw,
                status,
            })
        }
        HandlerOutput::Silent => Ok(DispatchOutput::Silent { status }),
        HandlerOutput::Binary { data, filename } => Ok(DispatchOutput::Binary(data, filename)),
        HandlerOutput::Artifact(artifact) => {
            let (bytes, suggested_destination, stdout_allowed, report) = artifact.into_parts();

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
        _ => Err(RunError::new(
            "Unsupported handler output variant: this standout version cannot present it",
            RunErrorKind::Render,
        )),
    }
}

fn serialize_handler_data<T: Serialize>(data: &T) -> Result<serde_json::Value, RunError> {
    serde_json::to_value(data).map_err(|e| {
        RunError::new(
            format!("Failed to serialize handler result: {}", e),
            RunErrorKind::Render,
        )
    })
}

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

fn frame_diagnostic(error: &dyn std::fmt::Display) -> String {
    format!("Error: {}", error)
}

pub(crate) fn handler_run_error(error: anyhow::Error) -> RunError {
    let error = match error.downcast::<ExternalFailure>() {
        Ok(external) => return RunError::from(external),
        Err(error) => error,
    };
    let error = match error.downcast::<AppFailure>() {
        Ok(app) => return RunError::from(app),
        Err(error) => error,
    };

    let error = match error.downcast::<Diagnostic>() {
        Ok(diagnostic) => {
            return RunError::new(frame_diagnostic(&diagnostic), RunErrorKind::Handler)
                .with_diagnostic(diagnostic.clone())
                .with_source(diagnostic)
        }
        Err(error) => error,
    };

    RunError::new(frame_diagnostic(&error), RunErrorKind::Handler)
        .with_diagnostic(Diagnostic::error(error.to_string()))
        .with_source(HandlerErrorSource(error.into_boxed_dyn_error()))
}

pub(crate) fn hook_run_error(mut error: HookError, phase: crate::cli::HookPhase) -> RunError {
    if phase == crate::cli::HookPhase::PreDispatch {
        if let Some(source) = error.source.take() {
            let source = match source.downcast::<ExternalFailure>() {
                Ok(external) => return RunError::from(*external),
                Err(source) => source,
            };
            match source.downcast::<AppFailure>() {
                Ok(app) => return RunError::from(*app),
                Err(source) => error.source = Some(source),
            }
        }
    }

    error.phase = phase;
    let diagnostic = error
        .source
        .as_ref()
        .and_then(|source| source.downcast_ref::<Diagnostic>())
        .cloned()
        .unwrap_or_else(|| Diagnostic::error(error.message.clone()));
    RunError::new(frame_diagnostic(&error), RunErrorKind::Hook(phase))
        .with_diagnostic(diagnostic)
        .with_source(error)
}

/// The recorder and the sink are parameters rather than `CommandContext`
/// members: they are the framework's, and a handler that could reach them could
/// record or write values the typed `Results` channel never saw.
pub type DispatchFn = Rc<
    RefCell<
        dyn FnMut(
            &ArgMatches,
            &CommandContext,
            &RunRecorder,
            &StreamSink,
            Option<&Hooks>,
            crate::Representation,
            ColorPolicy,
            &crate::Theme,
            TargetProperties,
        ) -> Result<DispatchOutput, RunError>,
    >,
>;

#[allow(clippy::too_many_arguments)]
pub fn dispatch(
    dispatch_fn: &DispatchFn,
    matches: &ArgMatches,
    ctx: &CommandContext,
    recorder: &RunRecorder,
    sink: &StreamSink,
    hooks: Option<&Hooks>,
    output_mode: crate::Representation,
    color_policy: ColorPolicy,
    theme: &crate::Theme,
    target: TargetProperties,
) -> Result<DispatchOutput, RunError> {
    (dispatch_fn.borrow_mut())(
        matches,
        ctx,
        recorder,
        sink,
        hooks,
        output_mode,
        color_policy,
        theme,
        target,
    )
}
