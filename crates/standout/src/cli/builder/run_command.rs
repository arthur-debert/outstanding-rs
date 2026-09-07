use super::App;
use crate::cli::handler::emits_events;
use crate::cli::handler::CommandContext;
use crate::cli::handler::Handler;
use crate::cli::handler::HandlerOutcome;
use crate::cli::handler::Output as HandlerOutput;
use crate::cli::handler::Results;
use crate::cli::handler::StreamSink;
use crate::cli::hooks::ArtifactOutput;
use crate::cli::hooks::HookError;
use crate::cli::hooks::RenderedOutput;
use crate::cli::hooks::TextOutput;
use crate::render_request_split;
use crate::ColorPolicy;
use crate::InputSources;
use crate::RenderRequest;
use clap::ArgMatches;
use standout_render::warnings::WarningBuffer;
use std::collections::HashMap;

impl App {
    /// One handler, hooks and render included. A typed `--color` outranks
    /// `color_policy`, which decides the run unless it is `Auto`; an `Auto`
    /// policy falls to `[term] color` (`NO_COLOR` turning a configured `always`
    /// down) and last to the destination. `sink` takes the handler's events as
    /// it emits them.
    #[allow(clippy::too_many_arguments)]
    pub fn run_command<H>(
        &self,
        path: &str,
        matches: &ArgMatches,
        mut handler: H,
        template: crate::TemplateRef,
        color_policy: ColorPolicy,
        sink: StreamSink,
    ) -> Result<RenderedOutput, HookError>
    where
        H: Handler,
    {
        let config = if self.config_exempt_commands.contains(path) {
            None
        } else {
            self.resolve_config(matches)
                .map_err(|error| HookError::pre_dispatch("Config error").with_source(error))?
        };
        let resolved = self.resolve_run(
            matches,
            config.as_ref().and_then(|config| config.term.as_ref()),
            None,
            color_policy,
            self.output_mode_fallback,
            self.process_edge_target(),
        );
        let (output_mode, color_policy, target) = (
            resolved.representation,
            resolved.color_policy,
            resolved.target,
        );
        let mut ctx = CommandContext::new(
            path.split('.').map(String::from).collect(),
            self.app_state.clone(),
        )
        .with_presentation(output_mode, color_policy);
        let warnings = WarningBuffer::new();
        self.seed_startup_warnings(&warnings);
        ctx.extensions.insert(InputSources::from_process());
        ctx.extensions.insert(warnings.clone());
        if let Some(config) = config {
            config.install(&mut ctx.extensions);
        }

        let hooks = self.command_hooks.get(path);

        if let Some(chains) = self.command_input_chains.get(path) {
            chains.run_pre_dispatch(matches, &mut ctx)?;
        }

        if let Some(resolution) = self.command_questionnaire_resolution.get(path) {
            resolution.run_pre_dispatch(matches, &mut ctx)?;
        }

        if let Some(hooks) = hooks {
            hooks.run_pre_dispatch(matches, &mut ctx)?;
        }

        let destination = std::rc::Rc::new(crate::cli::events::EventDestination::new(
            sink,
            crate::cli::events::EventContext {
                command_path: path.to_string(),
                template: crate::cli::events::rendered_event_template(&template),
                theme: self.theme.clone(),
                context_registry: self.context_registry.clone(),
                template_engine: self.template_engine.clone(),
                template_registry: self.template_registry.clone(),
                representation: output_mode,
                color_policy,
                target,
                warnings: Some(warnings.clone()),
                strict_style_tags: self.strict_style_tags,
            },
        ));
        let mut results = Results::<H::Event>::for_run(None, destination.clone());
        let handled = handler
            .handle(matches, &ctx, &mut results)
            .map(HandlerOutcome::into_output);
        drop(results);
        if let Some(failure) = destination.take_failure() {
            return Err(HookError::post_output("Render error").with_source(failure));
        }
        let document_records = emits_events::<H::Event>()
            .then(|| destination.take_document_records())
            .flatten();
        let (output, status) = match handled {
            Ok(output) => output.split_exit_status(),
            Err(e) => return Err(HookError::post_output("Handler error").with_source(e)),
        };
        let reject_status_without_a_carrier = |is_binary: bool, is_artifact: bool| {
            crate::cli::dispatch::reject_status_without_a_carrier(status, is_binary, is_artifact)
                .map_err(|e| HookError::post_output("Render error").with_source(e))
        };
        reject_status_without_a_carrier(output.is_binary(), output.is_artifact())?;

        let render_value =
            |data: standout_render::RenderData| -> Result<RenderedOutput, HookError> {
                let request = RenderRequest {
                    data,
                    template: template.clone(),
                    theme: self.theme.clone(),
                    format: output_mode,
                    color_policy,
                    target,
                    engine: self.template_engine.clone(),
                    registry: self.template_registry.clone(),
                    context_registry: Some(self.context_registry.clone()),
                    csv_projection: self.csv_projection_for(path),
                    extras: HashMap::new(),
                    warnings: Some(warnings.clone()),
                };
                render_request_split(&request)
                    .map(|rendered| {
                        RenderedOutput::Text(TextOutput::new(rendered.formatted, rendered.raw))
                    })
                    .map_err(|e| HookError::post_output("Render error").with_source(e))
            };
        let event_rows = output_mode == crate::Representation::Csv;

        let output = match output {
            HandlerOutput::Render(data) => {
                let mut json_data = standout_render::RenderData::from_serialize(&data)
                    .map_err(|e| HookError::post_dispatch("Serialization error").with_source(e))?;

                if let Some(hooks) = hooks {
                    json_data = hooks.run_post_dispatch(matches, &ctx, json_data)?;
                }

                match document_records {
                    Some(mut records) if !event_rows => {
                        records.push(standout_render::result_record(json_data));
                        run_document(records, output_mode)?
                    }
                    Some(records) => render_value(standout_render::RenderData::Array(records))?,
                    None => render_value(json_data)?,
                }
            }
            HandlerOutput::Silent => match document_records {
                Some(records) if !event_rows => run_document(records, output_mode)?,
                Some(records) => render_value(standout_render::RenderData::Array(records))?,
                None => RenderedOutput::Silent,
            },
            HandlerOutput::Binary { data, filename } => RenderedOutput::Binary(data, filename),
            HandlerOutput::Artifact(artifact) => {
                let (bytes, suggested_destination, stdout_allowed, report) = artifact.into_parts();
                let report = match report {
                    Some(report) => {
                        let mut json = standout_render::RenderData::from_serialize(&report)
                            .map_err(|e| {
                                HookError::post_dispatch("Serialization error").with_source(e)
                            })?;
                        if let Some(hooks) = hooks {
                            json = hooks.run_post_dispatch(matches, &ctx, json)?;
                        }
                        Some(json)
                    }
                    None => None,
                };
                RenderedOutput::Artifact(ArtifactOutput {
                    bytes,
                    suggested_destination,
                    stdout_allowed,
                    report,
                })
            }
            _ => {
                return Err(HookError::post_output(
                    "Unsupported handler output variant: this standout version cannot present it",
                ));
            }
        };

        let output = match hooks {
            Some(hooks) => hooks.run_post_output(matches, &ctx, output)?,
            None => output,
        };
        reject_status_without_a_carrier(output.is_binary(), output.is_artifact())?;
        crate::cli::dispatch::reject_payload_from_a_post_output_hook(
            emits_events::<H::Event>(),
            output.is_binary(),
            output.is_artifact(),
        )
        .map_err(|e| HookError::post_output("Render error").with_source(e))?;
        crate::cli::dispatch::reject_payload_under_stream(
            output_mode,
            output.is_binary(),
            output.is_artifact(),
        )
        .map_err(|e| HookError::post_output("Render error").with_source(e))?;
        Ok(output)
    }
}

/// The one document an incremental command ends in under `json` or `yaml`. No
/// warning record joins the array: `run_command` owns no stdout of its own.
fn run_document(
    records: Vec<standout_render::RenderData>,
    output_mode: crate::Representation,
) -> Result<RenderedOutput, HookError> {
    let document = standout_render::serialize_record_array(records, output_mode)
        .map_err(|e| HookError::post_output("Render error").with_source(e))?;
    Ok(RenderedOutput::Text(TextOutput::new(
        document.clone(),
        document,
    )))
}

#[cfg(test)]
mod tests;
