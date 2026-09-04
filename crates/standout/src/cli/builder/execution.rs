use crate::{
    open_output_file, write_binary_output, write_output, ColorPolicy, InputSources,
    OutputDestination, Representation, TargetProperties,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use standout_render::warnings::WarningBuffer;
use std::path::PathBuf;

use super::{
    output_mode_flag_spelling, App, AppBuilder, HookRegistrationSource, PendingCommand,
    TemplateRef, COLOR_ARG, COLOR_FLAG_DEFAULT, COLOR_FLAG_VALUES, NO_PAGER_ARG, OUTPUT_FILE_ARG,
    OUTPUT_MODE_ARG, OUTPUT_MODE_FLAG_VALUES,
};
use crate::cli::config::{
    config_command, config_command_tree, config_result_output, config_run_error,
    parse_override_pair, ResolvedConfig, CONFIG_COMMAND,
};
use crate::cli::default_command::ParseFailure;
use crate::cli::dispatch::{
    dispatch, extract_command_path, get_deepest_matches, render_handler_output, DispatchOutput,
    PendingRender,
};
use crate::cli::group::{ErasedConfigRecipe, GroupBuilder, GroupEntry};
use crate::cli::handler::{
    ArtifactDestination, ArtifactReceipt, ArtifactRun, CommandContext, Delivery, DispatchResult,
    ExitStatus, OutputKind, RunError, RunErrorKind, RunOutput, RunRecorder, StreamCapture,
    StreamSink, SuccessKind,
};
use crate::cli::hooks::{ArtifactOutput, Hooks, RenderedOutput, TextOutput};
use crate::cli::pager::{Pager, PagerOutcome};
use crate::cli::questionnaire::{
    augment_questionnaire_command, render_questions_result, validate_questionnaire_surface,
    QUESTIONNAIRE_ANSWERS_ARG, QUESTIONNAIRE_YES_ARG, QUESTIONS_SUBCOMMAND,
};
use crate::cli::ProcessOutcome;
use crate::SetupError;
use std::io::Write;

const CONFIG_OVERRIDE_ARG: &str = "_config_override";

/// True for the representations whose bytes accrue while the command runs, so
/// the sink is the one destination the whole run writes through.
fn writes_through_the_sink(output_mode: Representation) -> bool {
    output_mode.is_stream() || output_mode.is_human()
}

pub(crate) struct RunResolution {
    pub(crate) representation: Representation,
    pub(crate) color_policy: ColorPolicy,
    pub(crate) target: TargetProperties,
}

/// The strict-mode failure for whatever the render window has left unresolved
/// so far, or `None` when it has left nothing. Callers apply it before writing
/// the bytes it describes; `warnings` drops the superseded degrade warning.
/// Every caller has already decided that strict mode is on.
pub(crate) fn unresolved_style_tags_error(warnings: Option<&WarningBuffer>) -> Option<RunError> {
    let unresolved = standout_render::diagnostics::unresolved_in_current_window();
    if unresolved.is_empty() {
        return None;
    }
    if let Some(warnings) = warnings {
        warnings.retain(|warning| {
            !warning.starts_with(standout_render::diagnostics::UNRESOLVED_DEGRADATION_PREFIX)
        });
    }
    let (noun, pronoun, object) = if unresolved.len() == 1 {
        ("style tag", "It is", "it")
    } else {
        ("style tags", "They are", "them")
    };
    Some(RunError::new(
        format!(
            "strict_style_tags is enabled and the render left {count} {noun} unresolved: \
             {tags}. {pronoun} not defined in the active theme (a typo, or a tag the theme \
             does not style). Define {object} in the theme, correct the tag name, or disable \
             strict_style_tags to degrade to unstyled text instead.",
            count = unresolved.len(),
            tags = unresolved.join(", "),
        ),
        RunErrorKind::Render,
    ))
}

struct ContextInputs {
    sources: InputSources,
    config: Option<ResolvedConfig>,
}

impl AppBuilder {
    pub fn commands<F>(mut self, configure: F) -> Result<Self, SetupError>
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());

        if let Some(ref default_cmd) = builder.default_command {
            self.default_command = Some(default_cmd.clone());
        }

        for (name, entry) in builder.entries {
            match entry {
                GroupEntry::Command { mut handler } => {
                    let template = if let Some(absence) = handler.template_absence() {
                        TemplateRef::Absent(absence)
                    } else if let Some(name) = handler.template_name() {
                        TemplateRef::Named(name.to_string())
                    } else {
                        TemplateRef::convention(&name)
                    };

                    if let Some(hooks) = handler.take_hooks() {
                        self.register_command_hooks(
                            &name,
                            hooks,
                            HookRegistrationSource::CommandConfig,
                        )?;
                    }
                    if let Some(questionnaire) = handler.take_questionnaire() {
                        self.questionnaire_commands
                            .insert(name.clone(), questionnaire);
                    }

                    let recipe = ErasedConfigRecipe::from_handler(handler);

                    if self.pending_commands.borrow().contains_key(&name) {
                        return Err(SetupError::DuplicateCommand(name));
                    }

                    self.pending_commands.borrow_mut().insert(
                        name,
                        PendingCommand {
                            recipe: Box::new(recipe),
                            template,
                        },
                    );
                }
                GroupEntry::Group { builder: nested } => {
                    self.register_group(&name, nested)?;
                }
            }
        }

        Ok(self)
    }
}

impl App {
    pub fn dispatch(
        &self,
        matches: ArgMatches,
        output_mode: Representation,
    ) -> crate::cli::CompletedRun {
        let capture = StreamCapture::default();
        let recorder = RunRecorder::new();
        let run = self.collect_run_warnings(&recorder, |warnings| {
            // Resolved before the config outcome is unwrapped, so a run that
            // fails to load one still reports the policy its flags asked for.
            let config = self.resolve_config_for(&matches);
            let resolved = self.resolve_run(
                &matches,
                config
                    .as_ref()
                    .ok()
                    .and_then(|config| config.as_ref())
                    .and_then(|config| config.term.as_ref()),
                None,
                ColorPolicy::Auto,
                output_mode,
                self.process_edge_target(),
            );
            let (output_mode, color_policy) = (resolved.representation, resolved.color_policy);
            let config = match config {
                Ok(config) => config,
                Err(error) => return (DispatchResult::Error(error), output_mode, color_policy),
            };
            (
                self.dispatch_with_target(
                    matches,
                    output_mode,
                    color_policy,
                    resolved.target,
                    ContextInputs {
                        sources: InputSources::from_process(),
                        config,
                    },
                    StreamSink::new(capture.clone()),
                    recorder.clone(),
                    warnings,
                ),
                output_mode,
                color_policy,
            )
        });
        run.with_entries(String::from_utf8_lossy(&capture.take()).into_owned())
    }

    pub(crate) fn process_edge_target(&self) -> TargetProperties {
        let mut target = TargetProperties::detect();
        target.ambiguous_width = self.ambiguous_width;
        target
    }

    /// The one place a run's presentation is decided, so a rule added here
    /// reaches every entry point: `dispatch`, the argv path, and the
    /// partial-adoption `run_command`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_run(
        &self,
        matches: &ArgMatches,
        term: Option<&crate::TermSettings>,
        typed_color: Option<ColorPolicy>,
        named_color: ColorPolicy,
        representation_fallback: Representation,
        target: TargetProperties,
    ) -> RunResolution {
        RunResolution {
            representation: self.typed_output_mode(matches).unwrap_or_else(|| {
                term.and_then(|term| term.output)
                    .map_or(representation_fallback, Representation::from)
            }),
            color_policy: self.resolve_color_policy(
                self.typed_color_policy(matches).or(typed_color),
                named_color,
                term,
            ),
            target: file_destination(target, self.output_file_override(matches).is_some()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_with_target(
        &self,
        matches: ArgMatches,
        output_mode: Representation,
        color_policy: ColorPolicy,
        target: TargetProperties,
        inputs: ContextInputs,
        sink: StreamSink,
        recorder: RunRecorder,
        warnings: WarningBuffer,
    ) -> DispatchResult {
        self.ensure_commands_finalized();

        let path = extract_command_path(&matches);
        let path_str = path.join(".");

        if let Some(action) = self.config_command_action(&matches) {
            return self.run_config_command(
                action,
                path,
                &matches,
                output_mode,
                color_policy,
                target,
                inputs.sources,
                &sink,
                &recorder,
                &warnings,
            );
        }

        let commands = self.get_commands();
        let Some(dispatch_fn) = commands.get(&path_str) else {
            return DispatchResult::NoMatch(matches);
        };
        let override_path = self.output_file_override(&matches);
        let mut ctx = match self.command_context(
            path,
            output_mode,
            override_path.as_deref(),
            &sink,
            &recorder,
            &warnings,
        ) {
            Ok(ctx) => ctx,
            Err(error) => return DispatchResult::Error(error),
        };
        ctx.extensions.insert(inputs.sources);
        if let Some(config) = inputs.config {
            config.install(&mut ctx.extensions);
        }

        let hooks = self.command_hooks.get(&path_str);
        let sub_matches = get_deepest_matches(&matches);

        if let Some(hooks) = hooks {
            if let Err(e) = hooks.run_pre_dispatch(sub_matches, &mut ctx) {
                return DispatchResult::Error(super::super::dispatch::hook_run_error(
                    e,
                    crate::cli::HookPhase::PreDispatch,
                ));
            }
        }

        let dispatch_output = match dispatch(
            dispatch_fn,
            sub_matches,
            &ctx,
            &recorder,
            &sink,
            hooks,
            output_mode,
            color_policy,
            &self.theme,
            target,
        ) {
            Ok(output) => output,
            Err(e) => return DispatchResult::Error(e),
        };

        self.present_dispatch_output(
            dispatch_output,
            hooks,
            sub_matches,
            &ctx,
            output_mode,
            self.emits_events_for(&path_str),
            override_path,
            &sink,
            &warnings,
        )
    }

    fn config_command_action(
        &self,
        matches: &ArgMatches,
    ) -> Option<Result<clapfig::ConfigAction, clapfig::ClapfigError>> {
        if !self.installs_config_command() {
            return None;
        }
        let (name, sub_matches) = matches.subcommand()?;
        (name == CONFIG_COMMAND).then(|| config_command().parse(sub_matches))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_config_command(
        &self,
        action: Result<clapfig::ConfigAction, clapfig::ClapfigError>,
        path: Vec<String>,
        matches: &ArgMatches,
        output_mode: Representation,
        color_policy: ColorPolicy,
        target: TargetProperties,
        sources: InputSources,
        sink: &StreamSink,
        recorder: &RunRecorder,
        warnings: &WarningBuffer,
    ) -> DispatchResult {
        let seam = self
            .config
            .as_ref()
            .expect("the config command is installed only beside a config seam");
        let overrides = match self.config_overrides(matches) {
            Ok(overrides) => overrides,
            Err(error) => return DispatchResult::Error(error),
        };
        let result = match action.and_then(|action| seam.handle(&action, &overrides)) {
            Ok(result) => result,
            Err(error) => return DispatchResult::Error(config_run_error(&error)),
        };
        let override_path = self.output_file_override(matches);
        let mut ctx = match self.command_context(
            path,
            output_mode,
            override_path.as_deref(),
            sink,
            recorder,
            warnings,
        ) {
            Ok(ctx) => ctx,
            Err(error) => return DispatchResult::Error(error),
        };
        ctx.extensions.insert(sources);
        let sub_matches = get_deepest_matches(matches);
        let (output, template) = config_result_output(result, output_mode);
        let dispatch_output = match render_handler_output(
            Ok(output),
            sub_matches,
            &ctx,
            recorder,
            None,
            &template,
            &self.theme,
            &self.context_registry,
            &self.template_engine,
            self.template_registry.as_ref(),
            output_mode,
            color_policy,
            None,
            target,
            None,
        ) {
            Ok(output) => output,
            Err(error) => return DispatchResult::Error(error),
        };
        self.present_dispatch_output(
            dispatch_output,
            None,
            sub_matches,
            &ctx,
            output_mode,
            false,
            override_path,
            sink,
            warnings,
        )
    }

    fn output_file_override(&self, matches: &ArgMatches) -> Option<PathBuf> {
        self.output_file_flag.as_ref().and_then(|_| {
            matches
                .try_get_one::<String>(OUTPUT_FILE_ARG)
                .unwrap_or(None)
                .map(PathBuf::from)
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn command_context(
        &self,
        path: Vec<String>,
        output_mode: Representation,
        override_path: Option<&std::path::Path>,
        sink: &StreamSink,
        recorder: &RunRecorder,
        warnings: &WarningBuffer,
    ) -> Result<CommandContext, RunError> {
        recorder.set_delivery(match override_path {
            Some(path) => Delivery::File(path.to_path_buf()),
            None => Delivery::Stdout,
        });
        // The file override takes the events too, so the sink is retargeted
        // before the handler runs and the summary follows them into it. Line
        // framing creates the file up front, the way it did before events
        // existed; the human representation waits for a first byte, so a run
        // that ends without one — an artifact, a binary payload — leaves the
        // file uncreated and reports its own failure.
        if let Some(path) = override_path.filter(|_| writes_through_the_sink(output_mode)) {
            if output_mode.is_stream() {
                let file = open_output_file(path).map_err(|e| {
                    RunError::new(
                        format!("Error writing output: {}", e),
                        RunErrorKind::FinalWrite(OutputKind::Text),
                    )
                })?;
                sink.redirect(file);
            } else {
                let path = path.to_path_buf();
                sink.redirect_on_first_write(move || open_output_file(&path));
            }
        }
        let mut ctx = CommandContext::new(path, self.app_state.clone());
        ctx.extensions.insert(warnings.clone());
        Ok(ctx)
    }

    #[allow(clippy::too_many_arguments)]
    fn present_dispatch_output(
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
        // The document a records outcome hands the post-output hooks carries no
        // warnings; its tail is assembled after they return, from the snapshot
        // standing then.
        let mut pending_records: Option<(Vec<serde_json::Value>, String)> = None;
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
                            return DispatchResult::Error(RunError::new(
                                error.to_string(),
                                RunErrorKind::Render,
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
                    return DispatchResult::Error(super::super::dispatch::hook_run_error(
                        e,
                        crate::cli::HookPhase::PostOutput,
                    ))
                }
            }
        } else {
            output
        };

        // Byte equality of both texts with the document the hooks were given,
        // not the return variant: only then does the framework still own the
        // document and append the warnings standing now. Anything else is the
        // hook's, and every warning goes to the stderr block instead.
        let mut warnings_included = false;
        if let Some((mut records, unhooked)) = pending_records {
            if matches!(&final_output, RenderedOutput::Text(t) if t.formatted == unhooked && t.raw == unhooked)
            {
                let snapshot = warnings.snapshot();
                if !snapshot.is_empty() {
                    records.extend(crate::cli::warning_records(&snapshot));
                    let document =
                        match standout_render::serialize_record_array(records, output_mode) {
                            Ok(document) => document,
                            Err(error) => {
                                return DispatchResult::Error(RunError::new(
                                    error.to_string(),
                                    RunErrorKind::Render,
                                ))
                            }
                        };
                    final_output =
                        RenderedOutput::Text(TextOutput::new(document.clone(), document));
                }
                warnings_included = true;
            }
        }

        // A binary payload and an artifact own the file the user named, and a
        // silent outcome writes nothing, so none of them takes the destination
        // the run's rendered text was pointed at.
        if !matches!(final_output, RenderedOutput::Text(_)) {
            sink.cancel_pending_redirect();
        }

        // On the hooks' final result: a hook can turn the document into a payload.
        if let Err(error) = super::super::dispatch::reject_payload_from_an_emitting_command(
            emits_events,
            final_output.is_binary(),
            final_output.is_artifact(),
        ) {
            return DispatchResult::Error(error);
        }

        if let Err(error) = super::super::dispatch::reject_payload_under_stream(
            output_mode,
            final_output.is_binary(),
            final_output.is_artifact(),
        ) {
            return DispatchResult::Error(error);
        }

        if let RenderedOutput::Artifact(artifact) = final_output {
            if status != ExitStatus::SUCCESS {
                return DispatchResult::Error(super::super::dispatch::status_without_a_carrier(
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
                // The events are already in the file, so the summary joins them
                // there: newline-terminated under line framing, and as the
                // unterminated document a batch run writes otherwise.
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
                        return DispatchResult::Error(RunError::new(
                            format!("Error writing output: {}", e),
                            RunErrorKind::FinalWrite(OutputKind::Text),
                        ));
                    }
                    final_output = RenderedOutput::Silent;
                }
                RenderedOutput::Text(t) => {
                    if let Err(e) = write_output(&t.formatted, &dest) {
                        return DispatchResult::Error(RunError::new(
                            format!("Error writing output: {}", e),
                            RunErrorKind::FinalWrite(OutputKind::Text),
                        ));
                    }
                    final_output = RenderedOutput::Silent;
                }
                RenderedOutput::Binary(b, _) => {
                    if let Err(e) = write_binary_output(b, &dest) {
                        return DispatchResult::Error(RunError::new(
                            format!("Error writing output: {}", e),
                            RunErrorKind::FinalWrite(OutputKind::Binary),
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
                super::super::dispatch::status_without_a_carrier(status, "binary"),
            ),
            RenderedOutput::Binary(b, f) => DispatchResult::Binary(b, f),
            RenderedOutput::Artifact(_) => unreachable!("artifacts returned above"),
            RenderedOutput::Silent => handled(String::new()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_from_with_target<I, T>(
        &self,
        cmd: Command,
        args: I,
        target: TargetProperties,
        color_policy: ColorPolicy,
        sources: InputSources,
        sink: StreamSink,
        recorder: RunRecorder,
        warnings: WarningBuffer,
    ) -> (DispatchResult, Representation, ColorPolicy)
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
        let named_color = color_policy;
        let typed_color = self.typed_color_from_unparsed(&args);
        let color_policy = self.resolve_color_policy(typed_color, named_color, None);

        if let Err(error) = self
            .malformed_registrations()
            .and_then(|()| self.validate_questionnaire_surfaces(&cmd))
            .and_then(|()| self.unreachable_registrations(&cmd))
            .and_then(|()| self.config_override_flag_collision(&cmd))
            .and_then(|()| self.framework_flag_collision(&cmd))
            .and_then(|()| self.config_command_collision(&cmd))
        {
            return (
                DispatchResult::Error(RunError::new(error.to_string(), RunErrorKind::ClapUsage)),
                self.extract_output_mode_from_unparsed(&args),
                color_policy,
            );
        }

        let mut augmented_cmd = self.augment_command_with_help(cmd);

        if let Some(error) = self.help_word_collision(&augmented_cmd) {
            return (
                DispatchResult::Error(RunError::new(error.to_string(), RunErrorKind::ClapUsage)),
                self.extract_output_mode_from_unparsed(&args),
                color_policy,
            );
        }

        let matches = match self.parse_with_default_command(&augmented_cmd, &args, sources.stdin())
        {
            Ok(matches) => matches,
            Err(ParseFailure::UnknownDefault(e)) => {
                return (
                    DispatchResult::Error(RunError::new(
                        e.to_string(),
                        RunErrorKind::DefaultCommand,
                    )),
                    self.extract_output_mode_from_unparsed(&args),
                    color_policy,
                )
            }
            Err(ParseFailure::Clap(e)) => {
                let output_mode = self.extract_output_mode_from_unparsed(&args);
                if let Some(display) = self.intercept_display_help(
                    &mut augmented_cmd,
                    &args,
                    &e,
                    Some(target),
                    color_policy,
                    Some(warnings.clone()),
                ) {
                    return (display.into(), output_mode, color_policy);
                }
                if e.use_stderr() {
                    return (
                        DispatchResult::Error(RunError::new(
                            e.to_string(),
                            RunErrorKind::ClapUsage,
                        )),
                        output_mode,
                        color_policy,
                    );
                }
                let output = match e.kind() {
                    clap::error::ErrorKind::DisplayVersion => {
                        RunOutput::clap_version(e.to_string())
                    }
                    _ => RunOutput::clap_help(e.to_string()),
                };
                return (DispatchResult::Handled(output), output_mode, color_policy);
            }
        };

        let output_mode = self.extract_output_mode(&matches);
        let typed_color = self.typed_color_policy(&matches).or(typed_color);
        let color_policy = self.resolve_color_policy(typed_color, named_color, None);

        if let Some(display) = self.intercept_help_word(
            &mut augmented_cmd,
            &matches,
            Some(target),
            color_policy,
            Some(warnings.clone()),
        ) {
            return (display.into(), output_mode, color_policy);
        }

        if let Some((path, questionnaire)) = self.questionnaire_questions_invocation(&matches) {
            if let Some(parent_matches) =
                command_matches_for_path(&matches, &path.split('.').collect::<Vec<_>>())
            {
                let has_answers = parent_matches
                    .try_get_one::<String>(QUESTIONNAIRE_ANSWERS_ARG)
                    .unwrap_or(None)
                    .is_some();
                let has_yes = parent_matches
                    .try_get_one::<bool>(QUESTIONNAIRE_YES_ARG)
                    .unwrap_or(None)
                    == Some(&true);
                if has_answers || has_yes {
                    return (
                        DispatchResult::Error(RunError::new(
                            "`questions` renders the blank answer sheet and cannot be combined with --answers or --yes",
                            RunErrorKind::ClapUsage,
                        )),
                        output_mode,
                        color_policy,
                    );
                }
            }
            return (
                render_questions_result(questionnaire, &matches),
                output_mode,
                color_policy,
            );
        }

        let config = match self.resolve_config_for(&matches) {
            Ok(config) => config,
            Err(error) => return (DispatchResult::Error(error), output_mode, color_policy),
        };
        let term = config.as_ref().and_then(|config| config.term.as_ref());
        let resolved = self.resolve_run(
            &matches,
            term,
            typed_color,
            named_color,
            self.output_mode_fallback,
            target,
        );
        let (output_mode, color_policy) = (resolved.representation, resolved.color_policy);

        (
            self.dispatch_with_target(
                matches,
                output_mode,
                color_policy,
                resolved.target,
                ContextInputs { sources, config },
                sink,
                recorder,
                warnings,
            ),
            output_mode,
            color_policy,
        )
    }

    fn resolve_config_for(&self, matches: &ArgMatches) -> Result<Option<ResolvedConfig>, RunError> {
        let path = extract_command_path(matches).join(".");
        if !self.get_commands().contains_key(&path) {
            return Ok(None);
        }
        self.resolve_config(matches)
    }

    pub(crate) fn resolve_config(
        &self,
        matches: &ArgMatches,
    ) -> Result<Option<ResolvedConfig>, RunError> {
        let Some(seam) = self.config.as_ref() else {
            return Ok(None);
        };
        let overrides = self.config_overrides(matches)?;
        let dir = std::env::current_dir()
            .map_err(|error| RunError::new(error.to_string(), RunErrorKind::Config))?;
        seam.resolve_at(&overrides, &dir)
            .map(Some)
            .map_err(|error| config_run_error(&error))
    }

    fn config_overrides(&self, matches: &ArgMatches) -> Result<Vec<(String, String)>, RunError> {
        match matches.try_get_many::<String>(CONFIG_OVERRIDE_ARG) {
            Ok(Some(pairs)) => pairs
                .map(|pair| parse_override_pair(pair))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|message| RunError::new(message, RunErrorKind::ClapUsage)),
            _ => Ok(Vec::new()),
        }
    }

    pub(crate) fn config_override_flag_collision(&self, cmd: &Command) -> Result<(), SetupError> {
        let Some(flag) = self.config_override_flag.as_deref() else {
            return Ok(());
        };
        // Generated `--help`/`--version` only exist per command once clap builds the tree.
        let built = matches!(flag, "help" | "version").then(|| {
            let mut built = cmd.clone();
            if let Some(version) = &self.version {
                built = built.version(version.clone());
            }
            built.build();
            built
        });
        if command_takes_flag(built.as_ref().unwrap_or(cmd), flag) {
            return Err(SetupError::Config(format!(
                "config_override_flag(\"{flag}\") is already taken by this application's clap Command"
            )));
        }
        Ok(())
    }

    /// The framework's own global flags against every command in the
    /// application's tree. clap rejects a duplicate long name with a debug
    /// assertion, which a release build never runs: the duplicate would reach
    /// users as a flag that answers to whichever definition clap reached
    /// first, so standout refuses the pair itself.
    pub(crate) fn framework_flag_collision(&self, cmd: &Command) -> Result<(), SetupError> {
        let installed = [
            (
                "output_flag",
                "no_output_flag()",
                self.output_flag.as_deref(),
            ),
            (
                "output_file_flag",
                "no_output_file_flag()",
                self.output_file_flag.as_deref(),
            ),
            ("color_flag", "no_color_flag()", self.color_flag.as_deref()),
            ("pager_flag", "no_pager_flag()", self.pager_flag.as_deref()),
        ];
        for (seam, removal, flag) in installed {
            let Some(flag) = flag else { continue };
            if let Some(owner) = command_declaring_long(cmd, flag, &[]) {
                return Err(SetupError::Config(format!(
                    "{seam} installs `--{flag}`, which this application already declares on \
                     `{owner}`. Rename standout's with {seam}(Some(\"...\")), drop it with \
                     {removal}, or rename the application's own flag"
                )));
            }
        }
        Ok(())
    }

    pub fn run<I, T>(&self, cmd: Command, args: I) -> bool
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let outcome = self.run_emitted(cmd, args);
        if outcome.status != ExitStatus::SUCCESS {
            std::process::exit(i32::from(outcome.status.code()));
        }
        outcome.handled
    }

    /// Help delivery: a rendered help page goes to the pager the environment
    /// names when stdout is a terminal, the encoding is human and the
    /// invocation did not suppress paging. `true` means the pager took the
    /// bytes and the caller owes the user nothing on stdout; a pager that
    /// could not start leaves the page unpaged with the run's own status. A
    /// named output file takes the page before this is reached.
    fn page_help(
        &self,
        app_name: &str,
        args: &[std::ffi::OsString],
        target: &TargetProperties,
        output_mode: Representation,
        outcome: &crate::cli::DispatchResult,
    ) -> bool {
        let Some(page) = help_page(outcome) else {
            return false;
        };
        if !target.stdout_is_terminal
            || output_mode != Representation::Human
            || self.paging_is_suppressed(args)
        {
            return false;
        }
        match Pager::resolve(app_name).map(|pager| pager.page(page)) {
            Some(PagerOutcome::Paged | PagerOutcome::ReaderLeft) => true,
            Some(PagerOutcome::CouldNotStart) | None => false,
        }
    }

    /// `run` without ending the process.
    pub fn run_emitted<I, T>(&self, cmd: Command, args: I) -> ProcessOutcome
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let mut target = TargetProperties::detect();
        target.ambiguous_width = self.ambiguous_width;
        let sources = InputSources::from_process();
        let sink = StreamSink::process_stdout();
        let app_name = cmd.get_name().to_string();
        let args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
        // Help short-circuits before an `ArgMatches` exists, so the file the
        // invocation names is read from argv here — early enough that the page
        // is rendered for a file rather than for the terminal.
        let output_file = self.output_file_from_unparsed(&args);
        // Nothing reads this run's values back, so its events are written and
        // dropped rather than retained for the length of the command.
        let result = self.run_recording(
            cmd,
            &args,
            file_destination(target, output_file.is_some()),
            ColorPolicy::Auto,
            sources,
            sink.clone(),
            RunRecorder::summary_only(),
        );
        let primary_status = result.exit_status();
        let warnings = result.warnings().to_vec();
        let output_mode = result.output_mode();

        // A named output file wins over the pager, and over stdout.
        let help_to_file = output_file
            .zip(help_page(result.outcome()))
            .map(|(path, page)| {
                write_output(page, &OutputDestination::File(path))
                    .err()
                    .map(|error| {
                        RunError::new(
                            format!("Error writing output: {}", error),
                            RunErrorKind::FinalWrite(OutputKind::Text),
                        )
                    })
            });
        let paged = help_to_file.is_none()
            && self.page_help(&app_name, &args, &target, output_mode, result.outcome());

        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        let (handled, mut final_write_failure) = if let Some(failure) = help_to_file {
            (true, failure)
        } else if paged {
            (true, None)
        } else if output_mode.is_stream() {
            let emitted = sink.with_writer(|stdout| {
                crate::cli::emit_run_result(result.outcome(), output_mode, stdout, &mut stderr)
            });
            match emitted {
                Ok(handled) => (handled, None),
                Err(failure) => (true, Some(failure)),
            }
        } else {
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            match crate::cli::emit_run_result(
                result.outcome(),
                output_mode,
                &mut stdout,
                &mut stderr,
            ) {
                Ok(handled) => (handled, None),
                Err(failure) => (true, Some(failure)),
            }
        };
        let warning_entries = sink.with_writer(|stdout| {
            crate::cli::emit_warning_entries(result.outcome(), &warnings, output_mode, stdout)
        });
        if let Err(failure) = warning_entries {
            let _ = writeln!(stderr, "{}", failure).and_then(|()| stderr.flush());
            final_write_failure.get_or_insert(failure);
        }
        drop(stderr);

        if !crate::cli::warnings_delivered_on_stdout(result.outcome(), output_mode) {
            standout_render::warnings::flush_to_stderr(
                &self.theme,
                result.color_policy(),
                target,
                &warnings,
            );
        }

        let status = final_write_failure
            .as_ref()
            .map(RunError::exit_status)
            .or(primary_status)
            .unwrap_or(ExitStatus::SUCCESS);

        ProcessOutcome { handled, status }
    }

    pub(crate) fn seed_startup_warnings(&self, warnings: &WarningBuffer) {
        for message in &self.startup_warnings {
            warnings.push(message.clone());
        }
    }

    /// The capture window every entry point shares; help and answer-sheet outcomes, which
    /// return before the pre-commit strict check, are checked here instead.
    fn collect_run_warnings(
        &self,
        recorder: &RunRecorder,
        inner: impl FnOnce(WarningBuffer) -> (DispatchResult, Representation, ColorPolicy),
    ) -> crate::cli::CompletedRun {
        let warnings = WarningBuffer::new();
        self.seed_startup_warnings(&warnings);
        let _capture = standout_render::diagnostics::begin_capture();
        let (mut outcome, output_mode, color_policy) = inner(warnings.clone());
        if outcome.success_kind().is_some() {
            if let Some(error) = self.strict_style_tags_error(&warnings) {
                outcome = DispatchResult::Error(error);
            }
        }
        crate::cli::CompletedRun::from_dispatch(
            outcome,
            warnings.take(),
            output_mode,
            color_policy,
            recorder,
        )
    }

    /// Call before any byte is written; `Some` replaces the output and drops the
    /// superseded degrade warning. Reads the window [`collect_run_warnings`] opens.
    fn strict_style_tags_error(&self, warnings: &WarningBuffer) -> Option<RunError> {
        if !self.strict_style_tags {
            return None;
        }
        unresolved_style_tags_error(Some(warnings))
    }

    /// Dispatch without writing either process stream; an output file override still writes.
    pub fn run_with<I, T>(
        &self,
        cmd: Command,
        args: I,
        target: TargetProperties,
        sources: InputSources,
    ) -> crate::cli::CompletedRun
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        self.run_with_color(cmd, args, target, ColorPolicy::Auto, sources)
    }

    /// `run_with` with the run's color policy named instead of resolved from the destination.
    pub fn run_with_color<I, T>(
        &self,
        cmd: Command,
        args: I,
        target: TargetProperties,
        color_policy: ColorPolicy,
        sources: InputSources,
    ) -> crate::cli::CompletedRun
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let capture = StreamCapture::default();
        let run = self.run_with_sink(
            cmd,
            args,
            target,
            color_policy,
            sources,
            StreamSink::new(capture.clone()),
        );
        run.with_entries(String::from_utf8_lossy(&capture.take()).into_owned())
    }

    /// `run_with` with the run's color policy and the destination the whole
    /// run writes through: a handler's events as it emits them, then whatever
    /// the caller writes after it.
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_sink<I, T>(
        &self,
        cmd: Command,
        args: I,
        target: TargetProperties,
        color_policy: ColorPolicy,
        sources: InputSources,
        sink: StreamSink,
    ) -> crate::cli::CompletedRun
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        self.run_recording(
            cmd,
            args,
            target,
            color_policy,
            sources,
            sink,
            RunRecorder::new(),
        )
    }

    /// `run_with_sink` with the run's recorder named, so an entry point that
    /// never reads the values back can install one that keeps no events.
    #[allow(clippy::too_many_arguments)]
    fn run_recording<I, T>(
        &self,
        cmd: Command,
        args: I,
        target: TargetProperties,
        color_policy: ColorPolicy,
        sources: InputSources,
        sink: StreamSink,
        recorder: RunRecorder,
    ) -> crate::cli::CompletedRun
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        self.collect_run_warnings(&recorder, |warnings| {
            self.dispatch_from_with_target(
                cmd,
                args,
                target,
                color_policy,
                sources,
                sink,
                recorder.clone(),
                warnings,
            )
        })
    }

    pub(crate) fn augment_framework_surface(&self, mut cmd: Command) -> Command {
        self.augment_questionnaire_commands(&mut cmd, &[]);

        if let Some(version) = &self.version {
            cmd = cmd.version(version.clone());
        }

        if let Some(ref flag_name) = self.output_flag {
            let mut arg = Arg::new(OUTPUT_MODE_ARG)
                .long(flag_name.clone())
                .value_name("MODE")
                .global(true)
                .value_parser(OUTPUT_MODE_FLAG_VALUES)
                .help("Structured output encoding");
            // The human representation has no spelling, so an application that
            // falls back to it leaves the flag with no default at all.
            if let Some(spelling) = output_mode_flag_spelling(self.output_mode_fallback) {
                arg = arg.default_value(spelling);
            }
            cmd = cmd.arg(arg);
        }

        if let Some(ref flag_name) = self.color_flag {
            cmd = cmd.arg(
                Arg::new(COLOR_ARG)
                    .long(flag_name.clone())
                    .value_name("WHEN")
                    .global(true)
                    .value_parser(COLOR_FLAG_VALUES)
                    .default_value(COLOR_FLAG_DEFAULT)
                    .help("When to color human output"),
            );
        }

        if let Some(ref flag_name) = self.pager_flag {
            cmd = cmd.arg(
                Arg::new(NO_PAGER_ARG)
                    .long(flag_name.clone())
                    .global(true)
                    .action(ArgAction::SetTrue)
                    .help("Write output straight to stdout, never through a pager"),
            );
        }

        if let Some(ref flag_name) = self.output_file_flag {
            cmd = cmd.arg(
                Arg::new(OUTPUT_FILE_ARG)
                    .long(flag_name.clone())
                    .value_name("PATH")
                    .global(true)
                    .action(ArgAction::Set)
                    .help("Write output to file instead of stdout"),
            );
        }

        if let Some(ref flag_name) = self.config_override_flag {
            cmd = cmd.arg(
                Arg::new(CONFIG_OVERRIDE_ARG)
                    .long(flag_name.clone())
                    .value_name("KEY=VALUE")
                    .global(true)
                    .action(ArgAction::Append)
                    .help("Override a configuration value"),
            );
        }

        if self.installs_config_command() {
            cmd = cmd.subcommand(config_command_tree());
        }

        cmd
    }

    fn augment_questionnaire_commands(&self, cmd: &mut Command, path: &[String]) {
        let path_str = path.join(".");
        if self.questionnaire_commands.contains_key(&path_str) {
            *cmd = augment_questionnaire_command(cmd.clone());
        }

        for subcommand in cmd.get_subcommands_mut() {
            let mut child_path = path.to_vec();
            child_path.push(subcommand.get_name().to_string());
            self.augment_questionnaire_commands(subcommand, &child_path);
        }
    }

    /// A blank name in a `.`-separated path can never match what dispatch joins from clap.
    pub(crate) fn malformed_registrations(&self) -> Result<(), SetupError> {
        let pending = self.pending_commands.borrow();
        let mut malformed: Vec<&str> = pending
            .keys()
            .chain(self.questionnaire_commands.keys())
            .map(String::as_str)
            .filter(|path| !path.is_empty() && path.split('.').any(str::is_empty))
            .collect();
        malformed.sort_unstable();
        malformed.dedup();

        let Some(path) = malformed.first() else {
            return Ok(());
        };

        Err(SetupError::Config(format!(
            "Registration path `{path}` has a blank command name: a path is \
             `.`-separated command names, and only the empty path names \
             something (the root command of a flat app). Drop the leading, \
             trailing or doubled `.`."
        )))
    }

    /// A registration with no clap subcommand behind it; canonical names only, never aliases.
    pub(crate) fn unreachable_registrations(&self, cmd: &Command) -> Result<(), SetupError> {
        let mut unreachable: Vec<String> = self
            .pending_commands
            .borrow()
            .keys()
            .filter(|path| {
                crate::cli::app::find_canonical_subcommand_recursive(cmd, &path_segments(path))
                    .is_none()
            })
            .cloned()
            .collect();
        unreachable.sort();

        let Some(path) = unreachable.first() else {
            return Ok(());
        };

        let hint = match declared_variant_in(cmd, path) {
            Some(DeclaredAs::SeparatorVariant(declared)) => format!(
                " The CLI declares `{}` — a registered name must match the CLI \
                 definition exactly (clap's derive names subcommands in kebab-case).",
                declared.replace('.', " "),
            ),
            Some(DeclaredAs::Alias(declared)) => format!(
                " The CLI declares `{}` and accepts `{}` as an alias for it — clap \
                 reports the declared name to dispatch, so register the handler under \
                 `{}`.",
                declared.replace('.', " "),
                path.replace('.', " "),
                declared.replace('.', " "),
            ),
            None => " Register the handler under a name the CLI declares, or drop the \
                     registration."
                .to_string(),
        };

        Err(SetupError::Config(format!(
            "No invocation reaches `{}`: this application registers a handler for it, \
             but its clap `Command` declares no such subcommand.{hint}",
            path.replace('.', " "),
        )))
    }

    pub(crate) fn validate_questionnaire_surfaces(&self, cmd: &Command) -> Result<(), SetupError> {
        for path in self.questionnaire_commands.keys() {
            let parts = path.split('.').collect::<Vec<_>>();
            let Some(command) = crate::cli::app::find_canonical_subcommand_recursive(cmd, &parts)
            else {
                continue;
            };
            validate_questionnaire_surface(command, path)?;
        }
        Ok(())
    }

    fn questionnaire_questions_invocation(
        &self,
        matches: &ArgMatches,
    ) -> Option<(&str, &crate::cli::questionnaire::QuestionnaireCommand)> {
        let path = extract_command_path(matches);
        let (last, parent) = path.split_last()?;
        if last.as_str() != QUESTIONS_SUBCOMMAND || parent.is_empty() {
            return None;
        }
        let parent_path = parent.join(".");
        self.questionnaire_commands
            .get_key_value(&parent_path)
            .map(|(path, command)| (path.as_str(), command))
    }
}

/// The empty path is the root command and yields no segments.
fn path_segments(path: &str) -> Vec<&str> {
    if path.is_empty() {
        return Vec::new();
    }
    path.split('.').collect()
}

/// The declared spelling an unreachable path matches modulo `-`/`_`, or by alias.
enum DeclaredAs {
    SeparatorVariant(String),
    Alias(String),
}

fn declared_variant_in(cmd: &Command, path: &str) -> Option<DeclaredAs> {
    let mut current = cmd;
    let mut declared = Vec::new();
    let mut through_alias = false;

    for segment in path_segments(path) {
        let wanted = segment.replace('-', "_");
        let sub = current
            .get_subcommands()
            .find(|sub| sub.get_name().replace('-', "_") == wanted)
            .or_else(|| {
                through_alias = true;
                current
                    .get_subcommands()
                    .find(|sub| sub.get_aliases().any(|alias| alias == segment))
            })?;
        declared.push(sub.get_name().to_string());
        current = sub;
    }

    let declared = declared.join(".");
    Some(if through_alias {
        DeclaredAs::Alias(declared)
    } else {
        DeclaredAs::SeparatorVariant(declared)
    })
}

fn command_matches_for_path<'a>(matches: &'a ArgMatches, path: &[&str]) -> Option<&'a ArgMatches> {
    let mut current = matches;
    for segment in path {
        current = current.subcommand_matches(segment)?;
    }
    Some(current)
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
    report: Option<serde_json::Value>,
    receipt: &ArtifactReceipt,
) -> Result<serde_json::Value, RunError> {
    let receipt = serde_json::to_value(receipt).map_err(|e| {
        RunError::new(
            format!("Failed to serialize artifact receipt: {}", e),
            RunErrorKind::Render,
        )
    })?;
    Ok(serde_json::json!({
        "report": report.unwrap_or(serde_json::Value::Null),
        "receipt": receipt,
    }))
}

impl App {
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

        // The template is resolved here and not before: a run that ends without
        // a report never needed one, and a post-output hook can add a report to
        // an artifact whose handler returned none.
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
                        return DispatchResult::Error(RunError::new(
                            error.to_string(),
                            RunErrorKind::Render,
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
                return DispatchResult::Error(RunError::new(
                    format!("Error writing artifact: {}", e),
                    RunErrorKind::FinalWrite(OutputKind::Artifact),
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

/// The rendered help page a run ended in, whichever path rendered it: clap's
/// own `--help`, or the grouped page standout renders for `--help` and the
/// `help` word.
fn help_page(outcome: &crate::cli::DispatchResult) -> Option<&str> {
    let crate::cli::DispatchResult::Handled(output) = outcome else {
        return None;
    };
    matches!(
        output.kind(),
        SuccessKind::ClapHelp | SuccessKind::PagedHelp
    )
    .then(|| output.as_str())
}

/// A named output file is never a terminal, so `auto` resolves to plain text
/// in it; an explicit `--color always` still writes escapes there.
fn file_destination(mut target: TargetProperties, writes_to_a_file: bool) -> TargetProperties {
    if writes_to_a_file {
        target.stdout_is_terminal = false;
        target.stdout_color_capability = false;
    }
    target
}

/// The framework's own arguments, so a command already carrying them does not
/// read as colliding with itself.
const FRAMEWORK_ARG_IDS: [&str; 5] = [
    OUTPUT_MODE_ARG,
    OUTPUT_FILE_ARG,
    COLOR_ARG,
    NO_PAGER_ARG,
    CONFIG_OVERRIDE_ARG,
];

/// The path of the first command in the tree declaring `flag` as a long name
/// or long alias, `path` naming the ancestors reached so far.
fn command_declaring_long(cmd: &Command, flag: &str, path: &[&str]) -> Option<String> {
    let here = || {
        let mut names: Vec<&str> = path.to_vec();
        names.push(cmd.get_name());
        names.join(" ")
    };
    let declared = cmd.get_arguments().any(|arg| {
        !FRAMEWORK_ARG_IDS.contains(&arg.get_id().as_str())
            && (arg.get_long() == Some(flag)
                || arg
                    .get_all_aliases()
                    .is_some_and(|aliases| aliases.contains(&flag)))
    });
    if declared {
        return Some(here());
    }
    let mut path = path.to_vec();
    path.push(cmd.get_name());
    cmd.get_subcommands().find_map(|sub| {
        if sub.get_long_flag() == Some(flag)
            || sub.get_all_long_flag_aliases().any(|alias| alias == flag)
        {
            return Some(path.join(" "));
        }
        command_declaring_long(sub, flag, &path)
    })
}

pub(crate) fn command_takes_flag(cmd: &Command, flag: &str) -> bool {
    cmd.get_arguments().any(|arg| {
        arg.get_id() == CONFIG_OVERRIDE_ARG
            || arg.get_long() == Some(flag)
            || arg
                .get_all_aliases()
                .is_some_and(|aliases| aliases.contains(&flag))
    }) || cmd.get_subcommands().any(|sub| {
        sub.get_long_flag() == Some(flag)
            || sub.get_all_long_flag_aliases().any(|alias| alias == flag)
            || command_takes_flag(sub, flag)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmbeddedTemplates;

    const TEMPLATES: &[(&str, &str)] = &[
        ("list", "Count: {{ count }}"),
        ("list-2", "{{ name }}: {{ value }}"),
        ("config/get", "{{ key }}"),
        ("list-3", "Items: {{ items }}"),
        ("list-4", "{{ count }}"),
        ("list-5", "{{ msg }}"),
        ("config/get-2", "{{ value }}"),
        ("other", "{{ msg }}"),
        ("list-6", "Count: {{ count }}, Modified: {{ modified }}"),
        ("list-7", "{{ items }}"),
        ("list-8", "{{ value }}"),
        ("add", "Added: {{ added }}"),
        ("list-9", "{{ cmd }}"),
        ("add-2", "{{ cmd }}"),
        ("show", "unused"),
        ("show-2", "Hello {{ name }}"),
        ("show-3", "Count: {{ count }}"),
        ("list-10", "[late]{{ name }}[/late]"),
        ("list-11", "[test_style]{{ name }}[/test_style]"),
        ("list-12", "[header]{{ title }}[/header]"),
        ("test", "[mystyle]{{ x }}[/mystyle]"),
        ("list-13", "{{ db_url }}"),
        ("list-14", "debug={{ debug }}"),
        ("info", "db={{ db }}, version={{ version }}"),
        ("list-15", "db={{ db }}, user={{ user }}"),
        ("fetch", "{{ url }}"),
        ("test-3", "[perm]{{ val }}[/perm]"),
        ("apply", "{{ done }} done"),
        ("apply.event", "starting {{ event.resource }}"),
    ];

    use crate::cli::handler::EventsFnHandler;
    use crate::cli::handler::FnHandler;
    use crate::cli::handler::HandlerResult;
    use crate::cli::handler::Output as HandlerOutput;
    use crate::cli::hooks::{HookError, Hooks, RenderedOutput};

    #[test]
    fn test_dispatch_macro_simple() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(dispatch! {
                list => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
                    structured_only: true,
                }
            })
            .unwrap();

        assert!(builder.has_command("list"));

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("items"));
    }

    #[test]
    fn test_dispatch_macro_with_groups() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(dispatch! {
                db: {
                    migrate => {
                        handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"migrated": true}))),
                        structured_only: true,
                    },
                    backup => {
                        handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"backed_up": true}))),
                        structured_only: true,
                    },
                },
                version => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"v": "1.0"}))),
                    structured_only: true,
                },
            })
            .unwrap();

        assert!(builder.has_command("db.migrate"));
        assert!(builder.has_command("db.backup"));
        assert!(builder.has_command("version"));

        let cmd = Command::new("app")
            .subcommand(
                Command::new("db")
                    .subcommand(Command::new("migrate"))
                    .subcommand(Command::new("backup")),
            )
            .subcommand(Command::new("version"));

        let matches = cmd
            .clone()
            .try_get_matches_from(["app", "db", "migrate"])
            .unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Json);
        assert!(result.is_handled());
        assert!(result.output().unwrap().contains("migrated"));
    }

    #[test]
    fn test_dispatch_macro_with_template() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(crate::EmbeddedTemplates::new(
                &[("list", "Count: {{ count }}")],
                "",
            ))
            .commands(dispatch! {
                list => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                    template_name: "list",
                }
            })
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 42"));
    }

    #[test]
    fn test_dispatch_macro_with_hooks() {
        use crate::dispatch;
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        let builder = AppBuilder::new()
            .templates(crate::EmbeddedTemplates::new(&[("list", "{{ ok }}")], ""))
            .commands(dispatch! {
                list => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                    template_name: "list",
                    pre_dispatch: move |_, _| {
                        hook_called_clone.store(true, Ordering::SeqCst);
                        Ok(())
                    },
                }
            })
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_dispatch_macro_deeply_nested() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(dispatch! {
                app: {
                    config: {
                        get => |_m, _ctx| Ok(HandlerOutput::Render(json!({"key": "value"}))),
                        set => |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                    },
                    start => |_m, _ctx| Ok(HandlerOutput::Render(json!({"started": true}))),
                },
            })
            .unwrap();

        assert!(builder.has_command("app.config.get"));
        assert!(builder.has_command("app.config.set"));
        assert!(builder.has_command("app.start"));
    }

    #[test]
    fn test_dispatch_to_handler() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42})))),
                |cfg| cfg,
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 42"));
    }

    #[test]
    fn test_dispatch_unhandled_fallthrough() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({})))),
                |config| config.structured_only(),
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let matches = cmd.try_get_matches_from(["app", "other"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(!result.is_handled());
        assert!(result.matches().is_some());
    }

    #[test]
    fn test_dispatch_json_output() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"name": "test", "value": 123})))
                }),
                |cfg| cfg.template_name("list-2"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"value\": 123"));
    }

    #[test]
    fn test_dispatch_nested_command() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "config.get",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"key": "value"})))),
                |cfg| cfg,
            )
            .unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("value"));
    }

    #[test]
    fn test_dispatch_silent_result() {
        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "quiet",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::<()>::Silent)),
                |config| config.silent(),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("quiet"));

        let matches = cmd.try_get_matches_from(["app", "quiet"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));
    }

    #[test]
    fn test_dispatch_error_result() {
        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "fail",
                FnHandler::new(|_m, _ctx| {
                    Err::<HandlerOutput<()>, _>(anyhow::anyhow!("something went wrong"))
                }),
                |config| config.silent(),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("fail"));

        let matches = cmd.try_get_matches_from(["app", "fail"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_error(), "expected Error, got {:?}", result);
        let msg = result.error().unwrap();
        assert!(msg.contains("Error:"));
        assert!(msg.contains("something went wrong"));
    }

    #[test]
    fn test_dispatch_from_basic() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]})))),
                |cfg| cfg.template_name("list-3"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
    }

    #[test]
    fn test_dispatch_from_with_json_flag() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5})))),
                |cfg| cfg,
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "--output=json", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"count\": 5"));
    }

    #[test]
    fn test_dispatch_from_unhandled() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({})))),
                |config| config.structured_only(),
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "other"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(!result.is_handled());
    }

    #[test]
    fn test_dispatch_with_pre_dispatch_hook() {
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 1})))),
                |cfg| cfg.template_name("list-4"),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().pre_dispatch(move |_, _ctx| {
                    hook_called_clone.store(true, Ordering::SeqCst);
                    Ok(())
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
        assert_eq!(result.output(), Some("1"));
    }

    #[test]
    fn test_dispatch_pre_dispatch_hook_abort() {
        let builder = AppBuilder::new()
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                    panic!("Handler should not be called");
                }),
                |config| config.silent(),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .pre_dispatch(|_, _ctx| Err(HookError::pre_dispatch("blocked by hook"))),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_error(), "expected Error, got {:?}", result);
        let msg = result.error().unwrap();
        assert_eq!(msg, "Error: hook error (pre-dispatch): blocked by hook");
    }

    #[test]
    fn test_dispatch_with_post_output_hook() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"})))),
                |cfg| cfg.template_name("list-5"),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(text_output) = output {
                        Ok(RenderedOutput::Text(TextOutput::new(
                            text_output.formatted.to_uppercase(),
                            text_output.raw.to_uppercase(),
                        )))
                    } else {
                        Ok(output)
                    }
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("HELLO"));
    }

    #[test]
    fn test_dispatch_post_output_hook_chain() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "test"})))),
                |cfg| cfg.template_name("list-5"),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .post_output(|_, _ctx, output| {
                        if let RenderedOutput::Text(text_output) = output {
                            Ok(RenderedOutput::Text(TextOutput::new(
                                format!("[{}]", text_output.formatted),
                                format!("[{}]", text_output.raw),
                            )))
                        } else {
                            Ok(output)
                        }
                    })
                    .post_output(|_, _ctx, output| {
                        if let RenderedOutput::Text(text_output) = output {
                            Ok(RenderedOutput::Text(TextOutput::new(
                                text_output.formatted.to_uppercase(),
                                text_output.raw.to_uppercase(),
                            )))
                        } else {
                            Ok(output)
                        }
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("[TEST]"));
    }

    #[test]
    fn test_dispatch_post_output_hook_abort() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"})))),
                |cfg| cfg.template_name("list-5"),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_output(|_, _ctx, _output| {
                    Err(HookError::post_output("post-processing failed"))
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_error(), "expected Error, got {:?}", result);
        let msg = result.error().unwrap();
        assert_eq!(
            msg,
            "Error: hook error (post-output): post-processing failed"
        );
    }

    #[test]
    fn test_dispatch_hooks_for_nested_command() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "config.get",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"value": "secret"})))),
                |cfg| cfg.template_name("config/get-2"),
            )
            .unwrap()
            .hooks(
                "config.get",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(_) = output {
                        Ok(RenderedOutput::Text(TextOutput::plain("***".into())))
                    } else {
                        Ok(output)
                    }
                }),
            );

        let cmd =
            Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("***"));
    }

    #[test]
    fn test_dispatch_no_hooks_for_command() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "list"})))),
                |cfg| cfg.template_name("list-5"),
            )
            .unwrap()
            .command_with(
                "other",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "other"})))),
                |cfg| cfg,
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_output(|_, _ctx, _| {
                    panic!("Should not be called for 'other' command");
                }),
            );

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let matches = cmd.try_get_matches_from(["app", "other"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("other"));
    }

    #[test]
    fn test_dispatch_binary_output_with_hook() {
        let builder = AppBuilder::new()
            .command_with(
                "export",
                FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                    Ok(HandlerOutput::Binary {
                        data: vec![1, 2, 3],
                        filename: "out.bin".into(),
                    })
                }),
                |config| config.binary(),
            )
            .unwrap()
            .hooks(
                "export",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Binary(mut bytes, filename) = output {
                        bytes.push(4);
                        Ok(RenderedOutput::Binary(bytes, filename))
                    } else {
                        Ok(output)
                    }
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("export"));

        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_binary());
        let (bytes, filename) = result.binary().unwrap();
        assert_eq!(bytes, &[1, 2, 3, 4]);
        assert_eq!(filename, "out.bin");
    }

    #[test]
    fn test_hooks_passed_to_built_standout() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())))
            .build()
            .unwrap();

        assert!(standout.command_hooks.contains_key("list"));
        assert!(!standout.command_hooks.contains_key("other"));
    }

    #[test]
    fn test_run_command_with_hooks() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            value: i32,
        }

        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks(
                "test",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(text_output) = output {
                        Ok(RenderedOutput::Text(TextOutput::new(
                            format!("wrapped: {}", text_output.formatted),
                            format!("wrapped: {}", text_output.raw),
                        )))
                    } else {
                        Ok(output)
                    }
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 }))),
            crate::TemplateRef::Inline(("{{ value }}").to_string()),
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.as_text(), Some("wrapped: 42"));
    }

    #[test]
    fn test_run_command_pre_dispatch_abort() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks(
                "test",
                Hooks::new().pre_dispatch(|_, _ctx| Err(HookError::pre_dispatch("access denied"))),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                panic!("Handler should not be called");
            }),
            crate::TemplateRef::Absent,
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("access denied"));
    }

    #[test]
    fn test_run_command_without_hooks() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            msg: String,
        }

        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            FnHandler::new(|_m, _ctx| {
                Ok(HandlerOutput::Render(Data {
                    msg: "hello".into(),
                }))
            }),
            crate::TemplateRef::Inline(("{{ msg }}").to_string()),
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_text(), Some("hello"));
    }

    #[test]
    fn test_run_command_silent() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            FnHandler::new(|_m, _ctx| -> HandlerResult<()> { Ok(HandlerOutput::Silent) }),
            crate::TemplateRef::Absent,
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        assert!(result.is_ok());
        assert!(result.unwrap().is_silent());
    }

    #[test]
    fn test_run_command_binary() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks(
                "export",
                Hooks::new().post_output(|_, _ctx, output| {
                    assert!(output.is_binary());
                    Ok(output)
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("export"));
        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let sub_matches = matches.subcommand_matches("export").unwrap();

        let result = standout.run_command(
            "export",
            sub_matches,
            FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                Ok(HandlerOutput::Binary {
                    data: vec![0xDE, 0xAD],
                    filename: "data.bin".into(),
                })
            }),
            crate::TemplateRef::Absent,
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_binary());
        let (bytes, filename) = output.as_binary().unwrap();
        assert_eq!(bytes, &[0xDE, 0xAD]);
        assert_eq!(filename, "data.bin");
    }

    fn status_without_a_carrier_message(error: HookError) -> String {
        let source = error.source.expect("the carrier error is the source");
        source.to_string()
    }

    #[test]
    fn run_command_rejects_a_declared_status_on_binary_output() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("export"));
        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let sub_matches = matches.subcommand_matches("export").unwrap();

        let result = standout.run_command(
            "export",
            sub_matches,
            FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                Ok(HandlerOutput::Binary {
                    data: vec![0xDE, 0xAD],
                    filename: "data.bin".into(),
                }
                .with_exit_status(ExitStatus::from(2)))
            }),
            crate::TemplateRef::Absent,
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        let message = status_without_a_carrier_message(result.unwrap_err());
        assert!(
            message.contains("exit status 2 was declared on binary output"),
            "{message}"
        );
    }

    #[test]
    fn run_command_rejects_a_declared_success_status_on_binary_output() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("export"));
        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let sub_matches = matches.subcommand_matches("export").unwrap();

        let result = standout.run_command(
            "export",
            sub_matches,
            FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                Ok(HandlerOutput::Binary {
                    data: vec![0xDE, 0xAD],
                    filename: "data.bin".into(),
                }
                .with_exit_status(ExitStatus::SUCCESS))
            }),
            crate::TemplateRef::Absent,
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        let message = status_without_a_carrier_message(result.unwrap_err());
        assert!(
            message.contains("exit status 0 was declared on binary output"),
            "{message}"
        );
    }

    #[test]
    fn run_command_rejects_a_declared_success_status_on_artifact_output() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("export"));
        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let sub_matches = matches.subcommand_matches("export").unwrap();

        let result = standout.run_command(
            "export",
            sub_matches,
            FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                Ok(HandlerOutput::Artifact(
                    crate::cli::Artifact::new(vec![1u8]).suggest_destination("out.bin"),
                )
                .with_exit_status(ExitStatus::SUCCESS))
            }),
            crate::TemplateRef::Absent,
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        let message = status_without_a_carrier_message(result.unwrap_err());
        assert!(
            message.contains("exit status 0 was declared on artifact output"),
            "{message}"
        );
    }

    #[test]
    fn run_command_rejects_a_declared_status_on_artifact_output() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("export"));
        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let sub_matches = matches.subcommand_matches("export").unwrap();

        let result = standout.run_command(
            "export",
            sub_matches,
            FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                Ok(HandlerOutput::Artifact(
                    crate::cli::Artifact::new(vec![1u8]).suggest_destination("out.bin"),
                )
                .with_exit_status(ExitStatus::from(2)))
            }),
            crate::TemplateRef::Absent,
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        let message = status_without_a_carrier_message(result.unwrap_err());
        assert!(
            message.contains("exit status 2 was declared on artifact output"),
            "{message}"
        );
    }

    #[test]
    fn run_command_rejects_a_declared_status_a_post_output_hook_turns_into_bytes() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks(
                "test",
                Hooks::new().post_output(|_, _ctx, output| match output {
                    RenderedOutput::Text(text) => Ok(RenderedOutput::Binary(
                        text.raw.into_bytes(),
                        "rendered.bin".into(),
                    )),
                    other => Ok(other),
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            FnHandler::new(|_m, _ctx| {
                Ok(HandlerOutput::Render(serde_json::json!({"value": 1}))
                    .with_exit_status(ExitStatus::from(2)))
            }),
            crate::TemplateRef::Inline("{{ value }}".to_string()),
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        let message = status_without_a_carrier_message(result.unwrap_err());
        assert!(
            message.contains("exit status 2 was declared on binary output"),
            "{message}"
        );
    }

    #[test]
    fn run_command_drops_a_declared_status_on_render_output() {
        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            FnHandler::new(|_m, _ctx| {
                Ok(HandlerOutput::Render(serde_json::json!({"value": 1}))
                    .with_exit_status(ExitStatus::from(2)))
            }),
            crate::TemplateRef::Inline("{{ value }}".to_string()),
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        assert_eq!(result.unwrap().as_text(), Some("1"));
    }

    #[test]
    fn test_dispatch_with_post_dispatch_hook() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5})))),
                |cfg| cfg.template_name("list-6"),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_dispatch(|_, _ctx, mut data| {
                    if let Some(obj) = data.as_object_mut() {
                        obj.insert("modified".into(), json!(true));
                    }
                    Ok(data)
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Count: 5"));
        assert!(output.contains("Modified: true"));
    }

    #[test]
    fn test_dispatch_post_dispatch_hook_abort() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": []})))),
                |cfg| cfg.template_name("list-7"),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_dispatch(|_, _ctx, data| {
                    if data
                        .get("items")
                        .and_then(|v| v.as_array())
                        .map(|a| a.is_empty())
                        == Some(true)
                    {
                        return Err(HookError::post_dispatch("no items to display"));
                    }
                    Ok(data)
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_error(), "expected Error, got {:?}", result);
        let msg = result.error().unwrap();
        assert_eq!(
            msg,
            "Error: hook error (post-dispatch): no items to display"
        );
    }

    #[test]
    fn test_dispatch_post_dispatch_chain() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"value": 1})))),
                |cfg| cfg.template_name("list-8"),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .post_dispatch(|_, _ctx, mut data| {
                        if let Some(v) = data.get_mut("value") {
                            *v = json!(v.as_i64().unwrap_or(0) * 2);
                        }
                        Ok(data)
                    })
                    .post_dispatch(|_, _ctx, mut data| {
                        if let Some(v) = data.get_mut("value") {
                            *v = json!(v.as_i64().unwrap_or(0) + 10);
                        }
                        Ok(data)
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("12"));
    }

    #[test]
    fn test_dispatch_all_three_hooks() {
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let call_order = Arc::new(AtomicUsize::new(0));
        let pre_order = call_order.clone();
        let post_dispatch_order = call_order.clone();
        let post_output_order = call_order.clone();

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"})))),
                |cfg| cfg.template_name("list-5"),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .pre_dispatch(move |_, _ctx| {
                        assert_eq!(pre_order.fetch_add(1, Ordering::SeqCst), 0);
                        Ok(())
                    })
                    .post_dispatch(move |_, _ctx, data| {
                        assert_eq!(post_dispatch_order.fetch_add(1, Ordering::SeqCst), 1);
                        Ok(data)
                    })
                    .post_output(move |_, _ctx, output| {
                        assert_eq!(post_output_order.fetch_add(1, Ordering::SeqCst), 2);
                        Ok(output)
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(call_order.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_run_command_with_post_dispatch_hook() {
        use serde::Serialize;
        use serde_json::json;

        #[derive(Serialize)]
        struct Data {
            value: i32,
        }

        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks(
                "test",
                Hooks::new().post_dispatch(|_, _ctx, mut data| {
                    if let Some(obj) = data.as_object_mut() {
                        obj.insert("added_by_hook".into(), json!("yes"));
                    }
                    Ok(data)
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 }))),
            crate::TemplateRef::Inline(
                ("value={{ value }}, added={{ added_by_hook }}").to_string(),
            ),
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.as_text(), Some("value=42, added=yes"));
    }

    #[test]
    fn test_run_command_post_dispatch_abort() {
        use crate::cli::hooks::HookPhase;
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            valid: bool,
        }

        let standout = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks(
                "test",
                Hooks::new().post_dispatch(|_, _ctx, data| {
                    if data.get("valid") == Some(&serde_json::json!(false)) {
                        return Err(HookError::post_dispatch("invalid data"));
                    }
                    Ok(data)
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(Data { valid: false }))),
            crate::TemplateRef::Inline(("{{ valid }}").to_string()),
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.message, "invalid data");
        assert_eq!(err.phase, HookPhase::PostDispatch);
    }

    #[test]
    fn test_default_command_builder() {
        let builder = AppBuilder::new().default_command("list");

        assert_eq!(builder.default_command, Some("list".to_string()));
    }

    #[test]
    fn test_default_command_naked_invocation() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .default_command("list")
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]})))),
                |cfg| cfg.template_name("list-3"),
            )
            .unwrap()
            .command_with(
                "add",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"added": true})))),
                |cfg| cfg,
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("add"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );
        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
    }

    #[test]
    fn test_default_command_with_options() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .default_command("list")
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42})))),
                |cfg| cfg,
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "--output=json"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );
        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"count\": 42"));
    }

    #[test]
    fn test_default_command_explicit_command_overrides() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .default_command("list")
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"cmd": "list"})))),
                |cfg| cfg.template_name("list-9"),
            )
            .unwrap()
            .command_with(
                "add",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"cmd": "add"})))),
                |cfg| cfg.template_name("add-2"),
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("add"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "add"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );
        assert!(result.is_handled());
        assert_eq!(result.output(), Some("add"));
    }

    #[test]
    fn test_default_command_no_default_set() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": []})))),
                |cfg| cfg.template_name("list-3"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );
        assert!(!result.is_handled());
    }

    /// clap's duplicate-long assertion only runs in debug builds, so standout
    /// names the seam itself rather than letting a release build take the
    /// duplicate.
    #[test]
    fn an_application_flag_colliding_with_a_framework_flag_is_a_setup_error() {
        use serde_json::json;

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 1})))),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(
            Command::new("list").arg(
                Arg::new("quiet")
                    .long("no-pager")
                    .action(ArgAction::SetTrue),
            ),
        );

        let error = app.verify_command(&cmd).unwrap_err().to_string();
        assert!(
            error.contains("pager_flag installs `--no-pager`")
                && error.contains("app list")
                && error.contains("no_pager_flag()"),
            "expected the seam named, got: {error}"
        );

        let result = app.run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );
        assert!(result.error().is_some_and(|error| error
            .to_string()
            .contains("pager_flag installs `--no-pager`")));
    }

    #[test]
    fn test_dispatch_with_output_file_flag() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        let path_str = file_path.to_str().unwrap();

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42})))),
                |cfg| cfg,
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "--output-file-path", path_str, "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));

        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "Count: 42");
    }

    #[test]
    fn test_dispatch_with_custom_output_file_flag() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("out.txt");
        let path_str = file_path.to_str().unwrap();

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .output_file_flag(Some("save-to"))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 99})))),
                |cfg| cfg.template_name("list-4"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "--save-to", path_str, "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));

        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "99");
    }

    #[test]
    fn test_dispatch_with_output_file_json_mode() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.json");
        let path_str = file_path.to_str().unwrap();

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"name": "test", "count": 42})))
                }),
                |cfg| cfg,
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("show"));

        let result = builder.build().unwrap().run_with(
            cmd,
            [
                "app",
                "--output",
                "json",
                "--output-file-path",
                path_str,
                "show",
            ],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));

        let content = std::fs::read_to_string(file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["name"], "test");
        assert_eq!(parsed["count"], 42);
    }

    #[test]
    fn test_dispatch_with_output_file_human_representation() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        let path_str = file_path.to_str().unwrap();

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "Alice"})))),
                |cfg| cfg.template_name("show-2"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("show"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "--output-file-path", path_str, "show"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));

        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "Hello Alice");
    }

    #[test]
    fn test_dispatch_without_output_file_flag() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .no_output_file_flag()
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42})))),
                |cfg| cfg.template_name("show-3"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("show"));

        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "show"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert!(result.output().unwrap().contains("Count: 42"));
    }

    #[test]
    fn test_theme_ordering_command_before_theme() {
        use crate::Theme;
        use console::Style;
        use serde_json::json;

        let theme = Theme::new().add("late", Style::new().bold());

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test"})))),
                |cfg| cfg.template_name("list-10"),
            )
            .unwrap()
            .theme(theme); // Theme set AFTER command registration

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        let output = result.output().unwrap();

        assert!(
            !output.contains("[late?]"),
            "ORDERING BUG: Theme set after .command() was not applied - output: {}",
            output
        );
    }

    #[test]
    fn test_theme_passed_to_dispatch_closure() {
        use crate::Theme;
        use console::Style;
        use serde_json::json;

        let theme = Theme::new().add("test_style", Style::new().bold());

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .theme(theme)
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test"})))),
                |cfg| cfg.template_name("list-11"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        let output = result.output().unwrap();

        assert!(
            !output.contains("[test_style?]"),
            "Theme was not passed to dispatch - output: {}",
            output
        );
    }

    #[test]
    fn test_styles_and_default_theme_with_command() {
        use serde_json::json;
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("dark.yaml"),
            r#"
header:
  fg: blue
  bold: true
"#,
        )
        .unwrap();

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .styles_dir(temp_dir.path())
            .unwrap()
            .default_theme("dark")
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"title": "Results"})))),
                |cfg| cfg.template_name("list-12"),
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = app.run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        let output = result.output().unwrap();

        assert!(
            !output.contains("[header?]"),
            "ORDERING BUG: .styles() + .default_theme() not applied - output: {}",
            output
        );
    }

    #[test]
    fn test_builder_ordering_theme_before_command() {
        use crate::Theme;
        use console::Style;
        use serde_json::json;

        let theme = Theme::new().add("mystyle", Style::new().bold());

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .theme(theme)
            .command_with(
                "test",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.run_with(
            cmd,
            ["app", "test"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(
            !result.output().unwrap().contains("[mystyle?]"),
            "theme -> command ordering failed"
        );
    }

    #[test]
    fn test_builder_ordering_command_before_theme() {
        use crate::Theme;
        use console::Style;
        use serde_json::json;

        let theme = Theme::new().add("mystyle", Style::new().bold());

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "test",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
                |cfg| cfg,
            )
            .unwrap()
            .theme(theme)
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.run_with(
            cmd,
            ["app", "test"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(
            !result.output().unwrap().contains("[mystyle?]"),
            "command -> theme ordering failed"
        );
    }

    #[test]
    fn test_builder_ordering_styles_default_theme_command() {
        use serde_json::json;
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("mytheme.yaml"),
            "mystyle: { bold: true }",
        )
        .unwrap();

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .styles_dir(temp_dir.path())
            .unwrap()
            .default_theme("mytheme")
            .command_with(
                "test",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.run_with(
            cmd,
            ["app", "test"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(
            !result.output().unwrap().contains("[mystyle?]"),
            "styles -> default_theme -> command ordering failed"
        );
    }

    #[test]
    fn test_builder_ordering_command_before_styles() {
        use serde_json::json;
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("mytheme.yaml"),
            "mystyle: { bold: true }",
        )
        .unwrap();

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "test",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
                |cfg| cfg,
            )
            .unwrap()
            .styles_dir(temp_dir.path())
            .unwrap()
            .default_theme("mytheme")
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.run_with(
            cmd,
            ["app", "test"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(
            !result.output().unwrap().contains("[mystyle?]"),
            "command -> styles -> default_theme ordering failed"
        );
    }

    #[test]
    fn test_builder_ordering_default_theme_before_styles() {
        use serde_json::json;
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("mytheme.yaml"),
            "mystyle: { bold: true }",
        )
        .unwrap();

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .default_theme("mytheme")
            .styles_dir(temp_dir.path())
            .unwrap()
            .command_with(
                "test",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.run_with(
            cmd,
            ["app", "test"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(
            !result.output().unwrap().contains("[mystyle?]"),
            "default_theme -> styles -> command ordering failed"
        );
    }

    #[test]
    fn test_builder_ordering_all_permutations_with_explicit_theme() {
        use crate::Theme;
        use console::Style;
        use serde_json::json;

        fn make_theme() -> Theme {
            Theme::new().add("perm", Style::new().italic())
        }

        fn make_handler() -> impl Fn(
            &clap::ArgMatches,
            &crate::cli::handler::CommandContext,
        ) -> HandlerResult<serde_json::Value> {
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"val": "test"})))
        }

        let app1 = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .theme(make_theme())
            .command_with("test", FnHandler::new(make_handler()), |cfg| {
                cfg.template_name("test-3")
            })
            .unwrap()
            .context("extra", minijinja::Value::from("x"))
            .build()
            .unwrap();

        let app2 = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with("test", FnHandler::new(make_handler()), |cfg| {
                cfg.template_name("test-3")
            })
            .unwrap()
            .theme(make_theme())
            .context("extra", minijinja::Value::from("x"))
            .build()
            .unwrap();

        let app3 = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context("extra", minijinja::Value::from("x"))
            .command_with("test", FnHandler::new(make_handler()), |cfg| {
                cfg.template_name("test-3")
            })
            .unwrap()
            .theme(make_theme())
            .build()
            .unwrap();

        let app4 = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context("extra", minijinja::Value::from("x"))
            .theme(make_theme())
            .command_with("test", FnHandler::new(make_handler()), |cfg| {
                cfg.template_name("test-3")
            })
            .unwrap()
            .build()
            .unwrap();

        let app5 = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with("test", FnHandler::new(make_handler()), |cfg| {
                cfg.template_name("test-3")
            })
            .unwrap()
            .context("extra", minijinja::Value::from("x"))
            .theme(make_theme())
            .build()
            .unwrap();

        let app6 = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .theme(make_theme())
            .context("extra", minijinja::Value::from("x"))
            .command_with("test", FnHandler::new(make_handler()), |cfg| {
                cfg.template_name("test-3")
            })
            .unwrap()
            .build()
            .unwrap();

        for (i, app) in [app1, app2, app3, app4, app5, app6].into_iter().enumerate() {
            let cmd = Command::new("app").subcommand(Command::new("test"));
            let result = app.run_with(
                cmd,
                ["app", "test"],
                crate::TargetProperties::detect(),
                crate::InputSources::from_process(),
            );

            assert!(
                !result.output().unwrap().contains("[perm?]"),
                "Permutation {} failed: style not found",
                i + 1
            );
        }
    }

    #[test]
    fn test_dispatch_with_app_state() {
        use serde_json::json;

        struct Database {
            url: String,
        }

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .app_state(Database {
                url: "postgres://localhost".into(),
            })
            .command_with(
                "list",
                FnHandler::new(|_m, ctx| {
                    let db = ctx.app_state.get::<Database>().unwrap();
                    Ok(HandlerOutput::Render(json!({"db_url": db.url.clone()})))
                }),
                |cfg| cfg.template_name("list-13"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("postgres://localhost"));
    }

    #[test]
    fn test_dispatch_app_state_get_required() {
        use serde_json::json;

        struct Config {
            debug: bool,
        }

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .app_state(Config { debug: true })
            .command_with(
                "list",
                FnHandler::new(|_m, ctx| {
                    let config = ctx.app_state.get_required::<Config>()?;
                    Ok(HandlerOutput::Render(json!({"debug": config.debug})))
                }),
                |cfg| cfg.template_name("list-14"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("debug=true"));
    }

    #[test]
    fn test_dispatch_app_state_missing_type_error() {
        use serde_json::json;

        struct NotProvided;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, ctx| {
                    let _missing = ctx.app_state.get_required::<NotProvided>()?;
                    Ok(HandlerOutput::Render(json!({})))
                }),
                |config| config.structured_only(),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_error(), "expected Error, got {:?}", result);
        let msg = result.error().unwrap();
        assert!(
            msg.contains("Extension missing"),
            "Expected 'Extension missing' in error, got: {}",
            msg
        );
    }

    #[test]
    fn test_dispatch_app_state_with_multiple_types() {
        use serde_json::json;

        struct Database {
            name: String,
        }
        struct Config {
            version: i32,
        }

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .app_state(Database {
                name: "mydb".into(),
            })
            .app_state(Config { version: 42 })
            .command_with(
                "info",
                FnHandler::new(|_m, ctx| {
                    let db = ctx.app_state.get_required::<Database>()?;
                    let config = ctx.app_state.get_required::<Config>()?;
                    Ok(HandlerOutput::Render(json!({
                        "db": db.name,
                        "version": config.version
                    })))
                }),
                |cfg| cfg,
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "info"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("db=mydb, version=42"));
    }

    #[test]
    fn test_dispatch_app_state_and_extensions_together() {
        use serde_json::json;

        struct Database {
            name: String,
        }
        struct UserScope {
            user_id: String,
        }

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .app_state(Database {
                name: "maindb".into(),
            })
            .command_with(
                "list",
                FnHandler::new(|_m, ctx| {
                    let db = ctx.app_state.get_required::<Database>()?;

                    let scope = ctx.extensions.get_required::<UserScope>()?;

                    Ok(HandlerOutput::Render(json!({
                        "db": db.name,
                        "user": scope.user_id
                    })))
                }),
                |cfg| cfg.template_name("list-15"),
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().pre_dispatch(|_, ctx| {
                    ctx.extensions.insert(UserScope {
                        user_id: "user123".into(),
                    });
                    Ok(())
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.build().unwrap().run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("db=maindb, user=user123"));
    }

    #[test]
    fn test_built_app_dispatch_with_app_state() {
        use serde_json::json;

        struct ApiConfig {
            base_url: String,
        }

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .app_state(ApiConfig {
                base_url: "https://api.example.com".into(),
            })
            .command_with(
                "fetch",
                FnHandler::new(|_m, ctx| {
                    let config = ctx.app_state.get_required::<ApiConfig>()?;
                    Ok(HandlerOutput::Render(json!({"url": config.base_url})))
                }),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("fetch"));
        let result = app.run_with(
            cmd,
            ["app", "fetch"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("https://api.example.com"));
    }

    /// The process edge installs the summary-only recorder, so a long run's
    /// events are written and dropped rather than held until it ends.
    #[test]
    fn a_summary_only_recorder_writes_every_event_and_returns_only_the_summary() {
        #[derive(serde::Serialize)]
        struct Started {
            resource: String,
        }

        let app = App::builder()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "apply",
                EventsFnHandler::new(
                    |_m, _ctx, results: &mut crate::cli::handler::Results<Started>| {
                        for n in 0..64 {
                            results.emit(Started {
                                resource: format!("r{n}"),
                            })?;
                        }
                        Ok::<_, anyhow::Error>(HandlerOutput::Render(
                            serde_json::json!({"done": 64}),
                        ))
                    },
                ),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap();

        let capture = StreamCapture::default();
        let run = app.run_recording(
            Command::new("app").subcommand(Command::new("apply")),
            ["app", "apply"],
            TargetProperties::detect(),
            ColorPolicy::Never,
            InputSources::from_process(),
            StreamSink::new(capture.clone()),
            RunRecorder::summary_only(),
        );

        let written = String::from_utf8(capture.take()).unwrap();
        assert_eq!(written.lines().count(), 64, "{written}");
        assert_eq!(run.results(), [serde_json::json!({"done": 64})]);
    }
}
