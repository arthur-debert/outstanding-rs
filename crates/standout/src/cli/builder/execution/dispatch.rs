use super::RunOutcome;
use crate::cli::builder::App;
use crate::cli::config::ResolvedConfig;
use crate::cli::default_command::ParseFailure;
use crate::cli::dispatch::dispatch;
use crate::cli::dispatch::extract_command_path;
use crate::cli::dispatch::get_deepest_matches;
use crate::cli::handler::DispatchResult;
use crate::cli::handler::RunError;
use crate::cli::handler::RunErrorKind;
use crate::cli::handler::RunOutput;
use crate::cli::handler::RunRecorder;
use crate::cli::handler::StreamCapture;
use crate::cli::handler::StreamSink;
use crate::cli::questionnaire::render_questions_result;
use crate::cli::questionnaire::QUESTIONNAIRE_ANSWERS_ARG;
use crate::cli::questionnaire::QUESTIONNAIRE_YES_ARG;
use crate::cli::questionnaire::QUESTIONS_SUBCOMMAND;
use crate::ColorPolicy;
use crate::InputSources;
use crate::Representation;
use crate::TargetProperties;
use clap::ArgMatches;
use clap::Command;
use standout_render::warnings::WarningBuffer;

impl App {
    pub fn dispatch(
        &self,
        matches: ArgMatches,
        output_mode: Representation,
    ) -> crate::cli::CompletedRun {
        let capture = StreamCapture::default();
        let recorder = RunRecorder::new();
        let run = self.collect_run_warnings(&recorder, |warnings| {
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
                Err(error) => {
                    return RunOutcome {
                        outcome: DispatchResult::Error(error),
                        output_mode,
                        color_policy,
                        pager: None,
                    }
                }
            };
            RunOutcome {
                outcome: self.dispatch_with_target(
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
                pager: resolved.pager,
            }
        });
        run.with_entries(String::from_utf8_lossy(&capture.take()).into_owned())
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
            color_policy,
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
        let emits_events = self.emits_events_for(&path_str);

        if let Some(chains) = self.command_input_chains.get(&path_str) {
            if let Err(e) = chains.run_pre_dispatch(sub_matches, &mut ctx) {
                return DispatchResult::Error(crate::cli::dispatch::hook_run_error(
                    e,
                    crate::cli::HookPhase::PreDispatch,
                ));
            }
        }

        if let Some(resolution) = self.command_questionnaire_resolution.get(&path_str) {
            if let Err(e) = resolution.run_pre_dispatch(sub_matches, &mut ctx) {
                return DispatchResult::Error(crate::cli::dispatch::hook_run_error(
                    e,
                    crate::cli::HookPhase::PreDispatch,
                ));
            }
        }

        if let Some(hooks) = hooks {
            if let Err(e) = hooks.run_pre_dispatch(sub_matches, &mut ctx) {
                return DispatchResult::Error(crate::cli::dispatch::hook_run_error(
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
            emits_events,
            override_path,
            &sink,
            &warnings,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dispatch_from_with_target<I, T>(
        &self,
        cmd: Command,
        args: I,
        target: TargetProperties,
        color_policy: ColorPolicy,
        sources: InputSources,
        sink: StreamSink,
        recorder: RunRecorder,
        warnings: WarningBuffer,
    ) -> RunOutcome
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
        let named_color = color_policy;
        let typed_color = self.typed_color_from_unparsed(&args);
        let color_policy = self.resolve_color_policy(typed_color, named_color, None);

        if let Err(error) = self.validated_command_tree(&cmd) {
            return RunOutcome::to_stdout(
                DispatchResult::Error(RunError::new(error.to_string(), RunErrorKind::ClapUsage)),
                self.extract_output_mode_from_unparsed(&args),
                color_policy,
            );
        }

        let mut augmented_cmd = self.augment_command_with_help(cmd);

        if let Some(error) = self.help_word_collision(&augmented_cmd) {
            return RunOutcome::to_stdout(
                DispatchResult::Error(RunError::new(error.to_string(), RunErrorKind::ClapUsage)),
                self.extract_output_mode_from_unparsed(&args),
                color_policy,
            );
        }

        let matches = match self.parse_with_default_command(&augmented_cmd, &args, sources.stdin())
        {
            Ok(matches) => matches,
            Err(ParseFailure::UnknownDefault(e)) => {
                return RunOutcome::to_stdout(
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
                    let pager = self.pager_for_rendered_help(&display, &args, target, output_mode);
                    return RunOutcome {
                        outcome: display.into(),
                        output_mode,
                        color_policy,
                        pager,
                    };
                }
                if e.use_stderr() {
                    return RunOutcome::to_stdout(
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
                return RunOutcome::to_stdout(
                    DispatchResult::Handled(output),
                    output_mode,
                    color_policy,
                );
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
            let pager = self.pager_for_rendered_help(&display, &args, target, output_mode);
            return RunOutcome {
                outcome: display.into(),
                output_mode,
                color_policy,
                pager,
            };
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
                    return RunOutcome::to_stdout(
                        DispatchResult::Error(RunError::new(
                            "`questions` renders the blank answer sheet and cannot be combined with --answers or --yes",
                            RunErrorKind::ClapUsage,
                        )),
                        output_mode,
                        color_policy,
                    );
                }
            }
            return RunOutcome::to_stdout(
                render_questions_result(questionnaire, &matches),
                output_mode,
                color_policy,
            );
        }

        let config = match self.resolve_config_for(&matches) {
            Ok(config) => config,
            Err(error) => {
                return RunOutcome::to_stdout(
                    DispatchResult::Error(error),
                    output_mode,
                    color_policy,
                )
            }
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

        RunOutcome {
            outcome: self.dispatch_with_target(
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
            pager: resolved.pager,
        }
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

struct ContextInputs {
    sources: InputSources,
    config: Option<ResolvedConfig>,
}

fn command_matches_for_path<'a>(matches: &'a ArgMatches, path: &[&str]) -> Option<&'a ArgMatches> {
    let mut current = matches;
    for segment in path {
        current = current.subcommand_matches(segment)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests;
