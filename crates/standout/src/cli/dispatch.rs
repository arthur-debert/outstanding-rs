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
        /// What the artifact's report renders through, if the run ends with
        /// one. The post-output hooks can still add a report or take one away,
        /// so nothing here is resolved until they have returned.
        render: Box<PendingRender>,
    },
    Silent {
        status: ExitStatus,
    },
    /// An incremental command under `json` or `yaml`: its retained event
    /// records, then the summary's `result` record. The framework appends the
    /// run's warning records only if the post-output hooks return the document
    /// unchanged. CSV, whose rows are the events alone, arrives as `Text`.
    Records {
        records: Vec<serde_json::Value>,
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

/// A post-output hook hands back a `RenderedOutput`, which it may build as
/// `Binary` or `Artifact`, and it builds it while the run is under way. On a
/// command whose event type is not `NoEvents` the events already carried the
/// run's results, so a payload from the hook would be a second document sharing
/// one file or one stdout. The handler's own return cannot reach this shape:
/// `Handler::Outcome` is a `Summary` for every event type but `NoEvents`.
pub(crate) fn reject_payload_from_a_post_output_hook(
    emits_events: bool,
    is_binary: bool,
    is_artifact: bool,
) -> Result<(), RunError> {
    if !emits_events {
        return Ok(());
    }
    let payload = if is_binary {
        "binary"
    } else if is_artifact {
        "artifact"
    } else {
        return Ok(());
    };
    Err(RunError::new(
        format!(
            "{payload} output was produced by the post_output hook of a command that emits \
             events; the events carried the run's results, so the hook returns text or silence"
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
                // representation is the JSON a bare invocation serializes; only
                // the style-tag diagnostic view has nothing to show.
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

/// Everything a render needs but its data and its resolved template: a
/// post-output hook can add a report to an artifact that returned without one,
/// so only a run that ends with a report resolves a template.
pub(crate) struct PendingRender {
    command_path: String,
    template: TemplateRef,
    theme: Theme,
    context_registry: ContextRegistry,
    template_engine: SharedTemplateEngine,
    template_registry: Option<Rc<crate::TemplateRegistry>>,
    output_mode: crate::Representation,
    color_policy: ColorPolicy,
    csv_projection: Option<crate::CsvProjection>,
    target: TargetProperties,
    warnings: Option<standout_render::warnings::WarningBuffer>,
}

impl PendingRender {
    fn request(
        &self,
        data: serde_json::Value,
        template: standout_render::TemplateRef,
    ) -> RenderRequest {
        RenderRequest {
            data,
            template,
            theme: self.theme.clone(),
            format: self.output_mode,
            color_policy: self.color_policy,
            target: self.target,
            engine: self.template_engine.clone(),
            registry: self.template_registry.clone(),
            context_registry: Some(self.context_registry.clone()),
            csv_projection: self.csv_projection.clone(),
            extras: HashMap::new(),
            warnings: self.warnings.clone(),
        }
    }

    pub(crate) fn resolved(&self, data: serde_json::Value) -> Result<RenderRequest, RunError> {
        let template = render_time_template(
            &self.command_path,
            &self.template,
            self.template_registry.as_ref(),
            self.output_mode,
        )?;
        Ok(self.request(data, template))
    }

    fn untemplated(&self, data: serde_json::Value) -> RenderRequest {
        self.request(data, standout_render::TemplateRef::Absent)
    }
}

fn render_via_request(request: &RenderRequest) -> Result<(String, String), RunError> {
    let rendered = render_request_split(request).map_err(render_error)?;
    Ok((rendered.formatted, rendered.raw))
}

fn render_error(error: standout_render::RenderError) -> RunError {
    RunError::render(error.to_string(), error)
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
    document_records: Option<Vec<serde_json::Value>>,
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
    let render = PendingRender {
        command_path,
        template: template.clone(),
        theme: theme.clone(),
        context_registry: context_registry.clone(),
        template_engine: template_engine.clone(),
        template_registry: template_registry.cloned(),
        output_mode,
        color_policy,
        csv_projection: structured_output_projection
            .map(|projection| projection.csv_projection().clone()),
        target,
        warnings,
    };

    let event_document = |events: Vec<serde_json::Value>,
                          summary: Option<serde_json::Value>|
     -> Result<DispatchOutput, RunError> {
        if output_mode == crate::Representation::Csv {
            let request = render.untemplated(serde_json::Value::Array(events));
            let (formatted, raw) = render_via_request(&request)?;
            return Ok(DispatchOutput::Text {
                formatted,
                raw,
                status,
            });
        }
        let mut records = events;
        records.extend(summary.map(standout_render::result_record));
        Ok(DispatchOutput::Records { records, status })
    };

    match output {
        HandlerOutput::Render(data) => {
            let json_data = serialize_handler_data(&data)?;
            let json_data = run_post_dispatch_hooks(json_data, matches, ctx, hooks)?;
            recorder.record(json_data.clone());
            if let Some(events) = document_records {
                return event_document(events, Some(json_data));
            }
            let request = render.resolved(json_data)?;
            let (formatted, raw) = render_via_request(&request)?;
            Ok(DispatchOutput::Text {
                formatted,
                raw,
                status,
            })
        }
        HandlerOutput::Silent => match document_records {
            Some(events) => event_document(events, None),
            None => Ok(DispatchOutput::Silent { status }),
        },
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

            Ok(DispatchOutput::Artifact {
                render: Box::new(render),
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
    serde_json::to_value(data)
        .map_err(|e| RunError::render(format!("Failed to serialize handler result: {}", e), e))
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
/// members: a handler that could reach them could record or write values the
/// typed `Results` channel never saw.
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
