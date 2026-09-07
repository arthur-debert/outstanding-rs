use crate::cli::builder::App;
use crate::cli::dispatch::DispatchOutput;
use crate::cli::dispatch::PendingRender;
use crate::cli::handler::ArtifactDestination;
use crate::cli::handler::ArtifactReceipt;
use crate::cli::handler::ArtifactRun;
use crate::cli::handler::CommandContext;
use crate::cli::handler::Delivery;
use crate::cli::handler::DispatchResult;
use crate::cli::handler::ExitStatus;
use crate::cli::handler::OutputKind;
use crate::cli::handler::RunError;
use crate::cli::handler::RunErrorKind;
use crate::cli::handler::RunOutput;
use crate::cli::handler::RunRecorder;
use crate::cli::handler::StreamSink;
use crate::cli::hooks::ArtifactOutput;
use crate::cli::hooks::Hooks;
use crate::cli::hooks::RenderedOutput;
use crate::cli::hooks::TextOutput;
use crate::open_output_file;
use crate::write_binary_output;
use crate::write_output;
use crate::ColorPolicy;
use crate::OutputDestination;
use crate::Representation;
use clap::ArgMatches;
use standout_render::warnings::WarningBuffer;
use std::path::PathBuf;
use std::sync::Arc;

impl App {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn command_context(
        &self,
        path: Vec<String>,
        output_mode: Representation,
        color_policy: ColorPolicy,
        override_path: Option<&std::path::Path>,
        sink: &StreamSink,
        recorder: &RunRecorder,
        warnings: &WarningBuffer,
    ) -> Result<CommandContext, RunError> {
        recorder.set_delivery(match override_path {
            Some(path) => Delivery::File(path.to_path_buf()),
            None => Delivery::Stdout,
        });
        if let Some(path) = override_path.filter(|_| writes_through_the_sink(output_mode)) {
            if output_mode.is_stream() {
                let file = open_output_file(path).map_err(|e| {
                    RunError::final_write(
                        format!("Error writing output: {}", e),
                        Arc::new(e),
                        OutputKind::Text,
                    )
                })?;
                sink.redirect(file);
            } else {
                let path = path.to_path_buf();
                sink.redirect_on_first_write(move || open_output_file(&path));
            }
        }
        let mut ctx = CommandContext::new(path, self.app_state.clone())
            .with_presentation(output_mode, color_policy);
        ctx.extensions.insert(warnings.clone());
        Ok(ctx)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn present_dispatch_output(
        &self,
        dispatch_output: DispatchOutput,
        hooks: Option<&Hooks>,
        sub_matches: &ArgMatches,
        ctx: &CommandContext,
        output_mode: Representation,
        emits_events: bool,
        override_path: Option<PathBuf>,
        sink: &StreamSink,
        warnings: &WarningBuffer,
    ) -> DispatchResult {
        let mut pending_records: Option<(Vec<standout_render::RenderData>, String)> = None;
        let (output, render, status) = match dispatch_output {
            DispatchOutput::Text {
                formatted,
                raw,
                status,
            } => (
                RenderedOutput::Text(TextOutput::new(formatted, raw)),
                None,
                status,
            ),
            DispatchOutput::Binary(b, f) => {
                (RenderedOutput::Binary(b, f), None, ExitStatus::SUCCESS)
            }
            DispatchOutput::Artifact { output, render } => (
                RenderedOutput::Artifact(output),
                Some(render),
                ExitStatus::SUCCESS,
            ),
            DispatchOutput::Silent { status } => (RenderedOutput::Silent, None, status),
            DispatchOutput::Records { records, status } => {
                let document =
                    match standout_render::serialize_record_array(records.clone(), output_mode) {
                        Ok(document) => document,
                        Err(error) => {
                            return DispatchResult::Error(RunError::render(
                                error.to_string(),
                                Arc::new(error),
                            ))
                        }
                    };
                pending_records = Some((records, document.clone()));
                (
                    RenderedOutput::Text(TextOutput::new(document.clone(), document)),
                    None,
                    status,
                )
            }
        };

        let mut final_output = if let Some(hooks) = hooks {
            match hooks.run_post_output(sub_matches, ctx, output) {
                Ok(o) => o,
                Err(e) => {
                    return DispatchResult::Error(crate::cli::dispatch::hook_run_error(
                        e,
                        crate::cli::HookPhase::PostOutput,
                    ))
                }
            }
        } else {
            output
        };

        let mut warnings_included = false;
        if let Some((mut records, unhooked)) = pending_records {
            if matches!(&final_output, RenderedOutput::Text(t) if t.formatted == unhooked && t.raw == unhooked)
            {
                let snapshot = warnings.snapshot();
                if !snapshot.is_empty() {
                    records.extend(
                        crate::cli::warning_records(&snapshot)
                            .into_iter()
                            .map(Into::into),
                    );
                    let document =
                        match standout_render::serialize_record_array(records, output_mode) {
                            Ok(document) => document,
                            Err(error) => {
                                return DispatchResult::Error(RunError::render(
                                    error.to_string(),
                                    Arc::new(error),
                                ))
                            }
                        };
                    final_output =
                        RenderedOutput::Text(TextOutput::new(document.clone(), document));
                }
                warnings_included = true;
            }
        }

        // A payload and an artifact own the named file themselves.
        if !matches!(final_output, RenderedOutput::Text(_)) {
            sink.cancel_pending_redirect();
        }

        if let Err(error) = crate::cli::dispatch::reject_payload_from_a_post_output_hook(
            emits_events,
            final_output.is_binary(),
            final_output.is_artifact(),
        ) {
            return DispatchResult::Error(error);
        }

        if let Err(error) = crate::cli::dispatch::reject_payload_under_stream(
            output_mode,
            final_output.is_binary(),
            final_output.is_artifact(),
        ) {
            return DispatchResult::Error(error);
        }

        if let RenderedOutput::Artifact(artifact) = final_output {
            if status != ExitStatus::SUCCESS {
                return DispatchResult::Error(crate::cli::dispatch::status_without_a_carrier(
                    status, "artifact",
                ));
            }
            return self.complete_artifact(artifact, render, override_path, warnings);
        }

        // Before committing to stdout or a file, so a strict failure leaves no output.
        if let Some(error) = self.strict_style_tags_error(warnings) {
            return DispatchResult::Error(error);
        }

        if let Some(path) = override_path {
            let dest = OutputDestination::File(path);

            match &final_output {
                RenderedOutput::Text(t) if writes_through_the_sink(output_mode) => {
                    let written = sink.with_writer(|file| {
                        if output_mode.is_stream() {
                            writeln!(file, "{}", t.formatted)
                        } else {
                            write!(file, "{}", t.formatted)
                        }
                        .and_then(|()| file.flush())
                    });
                    if let Err(e) = written {
                        return DispatchResult::Error(RunError::final_write(
                            format!("Error writing output: {}", e),
                            Arc::new(e),
                            OutputKind::Text,
                        ));
                    }
                    final_output = RenderedOutput::Silent;
                }
                RenderedOutput::Text(t) => {
                    if let Err(e) = write_output(&t.formatted, &dest) {
                        return DispatchResult::Error(RunError::final_write(
                            format!("Error writing output: {}", e),
                            Arc::new(e),
                            OutputKind::Text,
                        ));
                    }
                    final_output = RenderedOutput::Silent;
                }
                RenderedOutput::Binary(b, _) => {
                    if let Err(e) = write_binary_output(b, &dest) {
                        return DispatchResult::Error(RunError::final_write(
                            format!("Error writing output: {}", e),
                            Arc::new(e),
                            OutputKind::Binary,
                        ));
                    }
                    final_output = RenderedOutput::Silent;
                }
                RenderedOutput::Artifact(_) => unreachable!("artifacts returned above"),
                RenderedOutput::Silent => {}
            }
        }

        let handled = |text: String| {
            DispatchResult::Handled(
                RunOutput::command(text)
                    .with_exit_status(status)
                    .with_warnings_included(warnings_included),
            )
        };
        match final_output {
            RenderedOutput::Text(t) => handled(t.formatted),
            RenderedOutput::Binary(_, _) if status != ExitStatus::SUCCESS => DispatchResult::Error(
                crate::cli::dispatch::status_without_a_carrier(status, "binary"),
            ),
            RenderedOutput::Binary(b, f) => DispatchResult::Binary(b, f),
            RenderedOutput::Artifact(_) => unreachable!("artifacts returned above"),
            RenderedOutput::Silent => handled(String::new()),
        }
    }

    /// The report renders first (its tags feed the strict check); bytes are written last.
    fn complete_artifact(
        &self,
        artifact: ArtifactOutput,
        render: Option<Box<PendingRender>>,
        override_path: Option<PathBuf>,
        warnings: &WarningBuffer,
    ) -> DispatchResult {
        let destination = match resolve_artifact_destination(&artifact, override_path) {
            Ok(destination) => destination,
            Err(error) => return DispatchResult::Error(error),
        };

        let receipt = ArtifactReceipt::new(destination.clone(), artifact.bytes.len());

        let report = match artifact.report {
            None => None,
            Some(report) => {
                let Some(render) = render else {
                    return DispatchResult::Error(RunError::new(
                        "Cannot render artifact report: the artifact carries a report but was \
                         not produced by a handler, so no template configuration is available",
                        RunErrorKind::Render,
                    ));
                };
                let envelope = match report_envelope(Some(report), &receipt) {
                    Ok(envelope) => envelope,
                    Err(error) => return DispatchResult::Error(error),
                };
                let request = match render.resolved(envelope) {
                    Ok(request) => request,
                    Err(error) => return DispatchResult::Error(error),
                };
                match standout_render::render_request_split(&request) {
                    Ok(rendered) => Some(rendered.formatted),
                    Err(error) => {
                        return DispatchResult::Error(RunError::render(
                            error.to_string(),
                            Arc::new(error),
                        ))
                    }
                }
            }
        };

        if let Some(error) = self.strict_style_tags_error(warnings) {
            return DispatchResult::Error(error);
        }

        if let ArtifactDestination::File(path) = &destination {
            let dest = OutputDestination::File(path.clone());
            if let Err(e) = write_binary_output(&artifact.bytes, &dest) {
                return DispatchResult::Error(RunError::final_write(
                    format!("Error writing artifact: {}", e),
                    Arc::new(e),
                    OutputKind::Artifact,
                ));
            }
        }

        DispatchResult::Artifact(ArtifactRun::new(
            artifact.bytes,
            artifact.suggested_destination,
            receipt,
            report,
        ))
    }
}

fn writes_through_the_sink(output_mode: Representation) -> bool {
    output_mode.is_stream() || output_mode.is_human()
}

fn resolve_artifact_destination(
    artifact: &ArtifactOutput,
    override_path: Option<PathBuf>,
) -> Result<ArtifactDestination, RunError> {
    if let Some(path) = override_path {
        return Ok(ArtifactDestination::File(path));
    }
    if let Some(path) = &artifact.suggested_destination {
        return Ok(ArtifactDestination::File(path.clone()));
    }
    if artifact.stdout_allowed {
        return Ok(ArtifactDestination::Stdout);
    }
    Err(RunError::new(
        "Error writing artifact: no destination selected (the artifact suggested none, \
         stdout was not allowed, and no output file was given)",
        RunErrorKind::FinalWrite(OutputKind::Artifact),
    ))
}

fn report_envelope(
    report: Option<standout_render::RenderData>,
    receipt: &ArtifactReceipt,
) -> Result<standout_render::RenderData, RunError> {
    let receipt = standout_render::RenderData::from_serialize(receipt).map_err(|e| {
        RunError::render(
            format!("Failed to serialize artifact receipt: {}", e),
            Arc::new(e),
        )
    })?;
    Ok(standout_render::RenderData::Object(
        [
            (
                "report".into(),
                report.unwrap_or(standout_render::RenderData::Null),
            ),
            ("receipt".into(), receipt),
        ]
        .into_iter()
        .collect(),
    ))
}

#[cfg(test)]
mod tests;
