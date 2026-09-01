use crate::{
    write_binary_output, write_output, InputSources, OutputDestination, OutputMode, RenderRequest,
    TargetProperties,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use standout_render::warnings::WarningBuffer;
use std::io::Write;
use std::path::PathBuf;

use super::{
    output_mode_flag_spelling, App, AppBuilder, HookRegistrationSource, PendingCommand,
    TemplateRef, OUTPUT_MODE_FLAG_VALUES,
};
use crate::cli::default_command::ParseFailure;
use crate::cli::dispatch::{dispatch, extract_command_path, get_deepest_matches, DispatchOutput};
use crate::cli::group::{ErasedConfigRecipe, GroupBuilder, GroupEntry};
use crate::cli::handler::{
    ArtifactDestination, ArtifactReceipt, ArtifactRun, CommandContext, DispatchResult, ExitStatus,
    OutputKind, RunError, RunErrorKind, RunOutput, SuccessKind,
};
use crate::cli::hooks::{ArtifactOutput, RenderedOutput, TextOutput};
use crate::cli::questionnaire::{
    augment_questionnaire_command, render_questions_result, validate_questionnaire_surface,
    QUESTIONNAIRE_ANSWERS_ARG, QUESTIONNAIRE_YES_ARG, QUESTIONS_SUBCOMMAND,
};
use crate::topics::display_with_pager;
use crate::SetupError;

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
        output_mode: OutputMode,
    ) -> crate::cli::CompletedRun {
        self.collect_run_warnings(|warnings| {
            (
                self.dispatch_with_target(
                    matches,
                    output_mode,
                    self.process_edge_target(),
                    InputSources::from_process(),
                    warnings,
                ),
                output_mode,
            )
        })
    }

    fn process_edge_target(&self) -> TargetProperties {
        let mut target = TargetProperties::detect();
        target.ambiguous_width = self.ambiguous_width;
        target
    }

    fn dispatch_with_target(
        &self,
        matches: ArgMatches,
        output_mode: OutputMode,
        target: TargetProperties,
        sources: InputSources,
        warnings: WarningBuffer,
    ) -> DispatchResult {
        self.ensure_commands_finalized();

        let path = extract_command_path(&matches);
        let path_str = path.join(".");

        let commands = self.get_commands();
        if let Some(dispatch_fn) = commands.get(&path_str) {
            let mut ctx = CommandContext::new(path, self.app_state.clone());
            ctx.extensions.insert(sources);
            ctx.extensions.insert(warnings);

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
                hooks,
                output_mode,
                &self.theme,
                target,
            ) {
                Ok(output) => output,
                Err(e) => return DispatchResult::Error(e),
            };

            let (output, request) = match dispatch_output {
                DispatchOutput::Text { formatted, raw } => {
                    (RenderedOutput::Text(TextOutput::new(formatted, raw)), None)
                }
                DispatchOutput::Binary(b, f) => (RenderedOutput::Binary(b, f), None),
                DispatchOutput::Artifact { output, request } => {
                    (RenderedOutput::Artifact(output), Some(request))
                }
                DispatchOutput::Silent => (RenderedOutput::Silent, None),
            };

            let mut final_output = if let Some(hooks) = hooks {
                match hooks.run_post_output(sub_matches, &ctx, output) {
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

            let override_path = self.output_file_flag.as_ref().and_then(|_| {
                matches
                    .try_get_one::<String>("_output_file_path")
                    .unwrap_or(None)
                    .map(PathBuf::from)
            });

            if let RenderedOutput::Artifact(artifact) = final_output {
                return complete_artifact(artifact, request, override_path);
            }

            if let Some(path) = override_path {
                let dest = OutputDestination::File(path);

                match &final_output {
                    RenderedOutput::Text(t) => {
                        if let Err(e) = write_output(&t.raw, &dest) {
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

            match final_output {
                RenderedOutput::Text(t) => DispatchResult::Handled(RunOutput::command(t.formatted)),
                RenderedOutput::Binary(b, f) => DispatchResult::Binary(b, f),
                RenderedOutput::Artifact(_) => unreachable!("artifacts returned above"),
                RenderedOutput::Silent => {
                    DispatchResult::Handled(RunOutput::command(String::new()))
                }
            }
        } else {
            DispatchResult::NoMatch(matches)
        }
    }

    fn dispatch_from_with_target<I, T>(
        &self,
        cmd: Command,
        args: I,
        target: TargetProperties,
        sources: InputSources,
        warnings: WarningBuffer,
    ) -> (DispatchResult, OutputMode)
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();

        if let Err(error) = self
            .malformed_registrations()
            .and_then(|()| self.validate_questionnaire_surfaces(&cmd))
            .and_then(|()| self.unreachable_registrations(&cmd))
        {
            return (
                DispatchResult::Error(RunError::new(error.to_string(), RunErrorKind::ClapUsage)),
                self.extract_output_mode_from_unparsed(&args),
            );
        }

        let mut augmented_cmd = self.augment_command_with_help(cmd);

        if let Some(error) = self.help_word_collision(&augmented_cmd) {
            return (
                DispatchResult::Error(RunError::new(error.to_string(), RunErrorKind::ClapUsage)),
                self.extract_output_mode_from_unparsed(&args),
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
                )
            }
            Err(ParseFailure::Clap(e)) => {
                let output_mode = self.extract_output_mode_from_unparsed(&args);
                if let Some(display) = self.intercept_display_help(
                    &mut augmented_cmd,
                    &args,
                    &e,
                    Some(target),
                    Some(warnings.clone()),
                ) {
                    return (display.into(), output_mode);
                }
                if e.use_stderr() {
                    return (
                        DispatchResult::Error(RunError::new(
                            e.to_string(),
                            RunErrorKind::ClapUsage,
                        )),
                        output_mode,
                    );
                }
                let output = match e.kind() {
                    clap::error::ErrorKind::DisplayVersion => {
                        RunOutput::clap_version(e.to_string())
                    }
                    _ => RunOutput::clap_help(e.to_string()),
                };
                return (DispatchResult::Handled(output), output_mode);
            }
        };

        let output_mode = self.extract_output_mode(&matches);

        if let Some(display) = self.intercept_help_word(
            &mut augmented_cmd,
            &matches,
            Some(target),
            Some(warnings.clone()),
        ) {
            return (display.into(), output_mode);
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
                    );
                }
            }
            return (
                render_questions_result(questionnaire, &matches),
                output_mode,
            );
        }

        (
            self.dispatch_with_target(matches, output_mode, target, sources, warnings),
            output_mode,
        )
    }

    pub fn run<I, T>(&self, cmd: Command, args: I) -> bool
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let capture_window = standout_render::diagnostics::begin_capture();

        let mut target = TargetProperties::detect();
        target.ambiguous_width = self.ambiguous_width;
        let sources = InputSources::from_process();
        let result = self.run_with(cmd, args, target, sources);
        let primary_status = result.exit_status();
        let warnings = result.warnings().to_vec();

        let paged = match result.outcome() {
            crate::cli::DispatchResult::Handled(output)
                if output.kind() == SuccessKind::PagedHelp =>
            {
                display_with_pager(output.as_str()).is_ok()
            }
            _ => false,
        };

        let stdout = std::io::stdout();
        let stderr = std::io::stderr();
        let mut stdout = stdout.lock();
        let mut stderr = stderr.lock();
        let (handled, final_write_failure) = if paged {
            (true, None)
        } else {
            emit_run_result(result.outcome(), &mut stdout, &mut stderr)
        };
        drop(stdout);
        drop(stderr);

        standout_render::warnings::flush_to_stderr(
            &self.theme,
            result.output_mode(),
            target,
            &warnings,
        );

        drop(capture_window);

        let status = final_write_failure
            .as_ref()
            .map(RunError::exit_status)
            .or(primary_status);
        if let Some(status) = status.filter(|status| status.code() != 0) {
            std::process::exit(i32::from(status.code()));
        }

        handled
    }

    pub(crate) fn seed_startup_warnings(&self, warnings: &WarningBuffer) {
        for message in &self.startup_warnings {
            warnings.push(message.clone());
        }
    }

    fn collect_run_warnings(
        &self,
        inner: impl FnOnce(WarningBuffer) -> (DispatchResult, OutputMode),
    ) -> crate::cli::CompletedRun {
        let warnings = WarningBuffer::new();
        self.seed_startup_warnings(&warnings);
        let (outcome, output_mode) = inner(warnings.clone());
        let mut collected = warnings.take();
        let outcome = self.enforce_strict_style_tags(outcome, &mut collected);
        crate::cli::CompletedRun::from_dispatch(outcome, collected, output_mode)
    }

    /// Apply the `strict_style_tags` gate to a completed run. When strict mode
    /// is on and the render left any style tag unresolved, a successful outcome
    /// becomes a [`RunErrorKind::Render`] error naming those tags (a non-zero
    /// exit); the now-superseded "degraded to unstyled text" warning is dropped
    /// so the failure is reported once. Off by default and a no-op for a clean
    /// render, an already-failed outcome, or a no-match handoff, so the graceful
    /// path is untouched.
    ///
    /// Reads the render diagnostics captured for this run — available only
    /// inside a capture window, which the process `run` and the test harness
    /// both open around dispatch — so lower-level entry points that render
    /// without a window simply skip the gate.
    fn enforce_strict_style_tags(
        &self,
        outcome: DispatchResult,
        warnings: &mut Vec<String>,
    ) -> DispatchResult {
        if !self.strict_style_tags || outcome.exit_status() != Some(ExitStatus::SUCCESS) {
            return outcome;
        }
        let unresolved = standout_render::diagnostics::unresolved_in_current_window();
        if unresolved.is_empty() {
            return outcome;
        }
        warnings.retain(|warning| {
            !warning.starts_with(standout_render::diagnostics::UNRESOLVED_DEGRADATION_PREFIX)
        });
        let (noun, pronoun, object) = if unresolved.len() == 1 {
            ("style tag", "It is", "it")
        } else {
            ("style tags", "They are", "them")
        };
        DispatchResult::Error(RunError::new(
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
        self.collect_run_warnings(|warnings| {
            self.dispatch_from_with_target(cmd, args, target, sources, warnings)
        })
    }

    pub(crate) fn augment_framework_surface(&self, mut cmd: Command) -> Command {
        self.augment_questionnaire_commands(&mut cmd, &[]);

        if let Some(version) = self.version {
            cmd = cmd.version(version);
        }

        if let Some(ref flag_name) = self.output_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_mode")
                    .long(flag)
                    .value_name("MODE")
                    .global(true)
                    .value_parser(OUTPUT_MODE_FLAG_VALUES)
                    .default_value(output_mode_flag_spelling(self.output_mode_fallback))
                    .help("Output format"),
            );
        }

        if let Some(ref flag_name) = self.output_file_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_file_path")
                    .long(flag)
                    .value_name("PATH")
                    .global(true)
                    .action(ArgAction::Set)
                    .help("Write output to file instead of stdout"),
            );
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

    /// A registration path is `.`-separated command names, and the one path
    /// with no names is the empty string: the root command of a flat app. A
    /// leading, trailing or doubled `.` leaves a blank name, which dispatch
    /// can never produce — it joins the names clap reports back — so the
    /// registration would sit unreachable behind a path that reads as valid.
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

    /// A registration no invocation can reach: the app registered a handler
    /// under a path its clap `Command` declares no subcommand for, so `run`
    /// would report "no handler" for a command the app believes it owns. The
    /// reverse direction — a clap subcommand with no registration — is partial
    /// adoption and stays a `NoMatch` handoff to the fallback that owns it.
    ///
    /// A clap alias is not a name a registration can use: clap reports the
    /// canonical command for an alias, so `ls` registered against
    /// `Command::new("list").alias("ls")` is unreachable from either spelling.
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

/// The empty registration path is the root command of a flat app, which every
/// clap `Command` has, so it walks to no segments rather than to one blank one.
/// Every other path splits literally: `malformed_registrations` has already
/// rejected the ones with a blank segment.
fn path_segments(path: &str) -> Vec<&str> {
    if path.is_empty() {
        return Vec::new();
    }
    path.split('.').collect()
}

/// What an unreachable registration names, when the CLI does declare the
/// command under some other spelling: the same path modulo `-` versus `_` (the
/// mismatch a kebab-case derive produces against a snake_case registration),
/// or an alias of it. Both carry the declared path the app should register.
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

fn complete_artifact(
    artifact: ArtifactOutput,
    request: Option<Box<RenderRequest>>,
    override_path: Option<PathBuf>,
) -> DispatchResult {
    let destination = match resolve_artifact_destination(&artifact, override_path) {
        Ok(destination) => destination,
        Err(error) => return DispatchResult::Error(error),
    };

    if let ArtifactDestination::File(path) = &destination {
        let dest = OutputDestination::File(path.clone());
        if let Err(e) = write_binary_output(&artifact.bytes, &dest) {
            return DispatchResult::Error(RunError::new(
                format!("Error writing artifact: {}", e),
                RunErrorKind::FinalWrite(OutputKind::Artifact),
            ));
        }
    }

    let receipt = ArtifactReceipt::new(destination, artifact.bytes.len());

    let report = match artifact.report {
        None => None,
        Some(report) => {
            let Some(mut request) = request else {
                return DispatchResult::Error(RunError::new(
                    "Cannot render artifact report: the artifact carries a report but was not \
                     produced by a handler, so no template configuration is available",
                    RunErrorKind::Render,
                ));
            };
            let envelope = match report_envelope(Some(report), &receipt) {
                Ok(envelope) => envelope,
                Err(error) => return DispatchResult::Error(error),
            };
            request.data = envelope;
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

    DispatchResult::Artifact(ArtifactRun::new(
        artifact.bytes,
        artifact.suggested_destination,
        receipt,
        report,
    ))
}

fn emit_artifact<W: Write, E: Write>(
    run: &ArtifactRun,
    stdout: &mut W,
    stderr: &mut E,
) -> Option<RunError> {
    let to_stdout = run.destination().is_stdout();

    if to_stdout {
        if let Err(error) = stdout.write_all(run.bytes()).and_then(|()| stdout.flush()) {
            return Some(RunError::new(
                format!("Error writing artifact stdout: {}", error),
                RunErrorKind::FinalWrite(OutputKind::Artifact),
            ));
        }
    }

    let report = run.report().filter(|report| !report.is_empty())?;

    let written = if to_stdout {
        writeln!(stderr, "{}", report).and_then(|()| stderr.flush())
    } else {
        writeln!(stdout, "{}", report).and_then(|()| stdout.flush())
    };

    written.err().map(|error| {
        RunError::new(
            format!("Error writing artifact report: {}", error),
            RunErrorKind::FinalWrite(OutputKind::Artifact),
        )
    })
}

fn emit_run_result<W: Write, E: Write>(
    result: &DispatchResult,
    stdout: &mut W,
    stderr: &mut E,
) -> (bool, Option<RunError>) {
    let failure = match result {
        DispatchResult::Handled(output) if output.is_empty() => None,
        DispatchResult::Handled(output) => writeln!(stdout, "{}", output)
            .and_then(|()| stdout.flush())
            .err()
            .and_then(|error| final_write_error_unless_broken_pipe(error, OutputKind::Text)),
        DispatchResult::Binary(bytes, _) => stdout
            .write_all(bytes)
            .and_then(|()| stdout.flush())
            .err()
            .map(|error| {
                RunError::new(
                    format!("Error writing binary stdout: {}", error),
                    RunErrorKind::FinalWrite(OutputKind::Binary),
                )
            }),
        DispatchResult::Artifact(run) => emit_artifact(run, stdout, stderr),
        DispatchResult::Silent => None,
        DispatchResult::Error(error) => (if error.writes_diagnostic_verbatim() {
            stderr.write_all(error.as_str().as_bytes())
        } else {
            writeln!(stderr, "{}", error)
        })
        .and_then(|()| stderr.flush())
        .err()
        .map(|write_error| {
            RunError::new(
                format!("Error writing stderr: {}", write_error),
                RunErrorKind::FinalWrite(OutputKind::Text),
            )
        }),
        DispatchResult::NoMatch(_) => return (false, None),
        _ => return (false, None),
    };

    if let Some(error) = &failure {
        let _ = writeln!(stderr, "{}", error).and_then(|()| stderr.flush());
    }
    (true, failure)
}

fn final_write_error_unless_broken_pipe(
    error: std::io::Error,
    kind: OutputKind,
) -> Option<RunError> {
    if kind == OutputKind::Text && error.kind() == std::io::ErrorKind::BrokenPipe {
        None
    } else {
        Some(RunError::new(
            format!("Error writing stdout: {}", error),
            RunErrorKind::FinalWrite(kind),
        ))
    }
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
    ];

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Json);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Json);
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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Json);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
            |_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 })),
            crate::TemplateRef::Inline(("{{ value }}").to_string()),
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

        let result = standout.run_command::<_, ()>(
            "test",
            sub_matches,
            |_m, _ctx| {
                panic!("Handler should not be called");
            },
            crate::TemplateRef::Absent,
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
            |_m, _ctx| {
                Ok(HandlerOutput::Render(Data {
                    msg: "hello".into(),
                }))
            },
            crate::TemplateRef::Inline(("{{ msg }}").to_string()),
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

        let result = standout.run_command::<_, ()>(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Silent),
            crate::TemplateRef::Absent,
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

        let result = standout.run_command::<_, ()>(
            "export",
            sub_matches,
            |_m, _ctx| {
                Ok(HandlerOutput::Binary {
                    data: vec![0xDE, 0xAD],
                    filename: "data.bin".into(),
                })
            },
            crate::TemplateRef::Absent,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_binary());
        let (bytes, filename) = output.as_binary().unwrap();
        assert_eq!(bytes, &[0xDE, 0xAD]);
        assert_eq!(filename, "data.bin");
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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

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
            |_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 })),
            crate::TemplateRef::Inline(
                ("value={{ value }}, added={{ added_by_hook }}").to_string(),
            ),
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
            |_m, _ctx| Ok(HandlerOutput::Render(Data { valid: false })),
            crate::TemplateRef::Inline(("{{ valid }}").to_string()),
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
    fn test_dispatch_with_output_file_text_mode() {
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
            [
                "app",
                "--output",
                "text",
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
            ["app", "--output=term", "list"],
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
            ["app", "--output=term", "list"],
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
            ["app", "--output=term", "list"],
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
            ["app", "--output=term", "test"],
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
            ["app", "--output=term", "test"],
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
            ["app", "--output=term", "test"],
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
            ["app", "--output=term", "test"],
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
            ["app", "--output=term", "test"],
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
                ["app", "--output=term", "test"],
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

    struct FailingWriter;

    impl std::io::Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "closed",
            ))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "closed",
            ))
        }
    }

    #[derive(Default)]
    struct FlushFailingWriter {
        bytes: Vec<u8>,
    }

    impl std::io::Write for FlushFailingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "flush closed",
            ))
        }
    }

    #[test]
    fn final_emission_routes_success_and_diagnostics_to_distinct_streams() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let (handled, failure) = emit_run_result(
            &DispatchResult::Handled(RunOutput::command("hello")),
            &mut stdout,
            &mut stderr,
        );
        assert!(handled);
        assert!(failure.is_none());
        assert_eq!(stdout, b"hello\n");
        assert!(stderr.is_empty());

        stdout.clear();
        let (handled, failure) = emit_run_result(
            &DispatchResult::Error(RunError::new("bad argv", RunErrorKind::ClapUsage)),
            &mut stdout,
            &mut stderr,
        );
        assert!(handled);
        assert!(failure.is_none());
        assert!(stdout.is_empty());
        assert_eq!(stderr, b"bad argv\n");
    }

    #[test]
    fn final_text_broken_pipe_is_successful_early_termination() {
        let mut stderr = Vec::new();
        let (_, text_failure) = emit_run_result(
            &DispatchResult::Handled(RunOutput::command("hello")),
            &mut FailingWriter,
            &mut stderr,
        );
        assert!(text_failure.is_none());
        assert!(stderr.is_empty());
    }

    #[test]
    fn final_binary_write_failures_keep_payload_kind() {
        let mut stderr = Vec::new();
        let (_, binary_failure) = emit_run_result(
            &DispatchResult::Binary(vec![0, 1], "data.bin".into()),
            &mut FailingWriter,
            &mut stderr,
        );
        let binary_failure = binary_failure.unwrap();
        assert_eq!(
            binary_failure.kind(),
            RunErrorKind::FinalWrite(OutputKind::Binary)
        );
        assert_eq!(
            binary_failure.exit_status(),
            crate::cli::ExitStatus::FAILURE
        );
    }

    #[test]
    fn final_text_broken_pipe_flush_is_successful_early_termination() {
        let mut text_stdout = FlushFailingWriter::default();
        let (_, text_failure) = emit_run_result(
            &DispatchResult::Handled(RunOutput::command("hello")),
            &mut text_stdout,
            &mut Vec::new(),
        );
        assert_eq!(text_stdout.bytes, b"hello\n");
        assert!(text_failure.is_none());
    }

    #[test]
    fn final_binary_flush_failures_keep_payload_kind() {
        let mut binary_stdout = FlushFailingWriter::default();
        let (_, binary_failure) = emit_run_result(
            &DispatchResult::Binary(vec![0, 1], "data.bin".into()),
            &mut binary_stdout,
            &mut Vec::new(),
        );
        assert_eq!(binary_stdout.bytes, [0, 1]);
        assert_eq!(
            binary_failure.unwrap().kind(),
            RunErrorKind::FinalWrite(OutputKind::Binary)
        );
    }

    #[test]
    fn artifact_report_write_failures_keep_artifact_kind_on_both_channels() {
        let file_run = ArtifactRun::new(
            vec![0, 1],
            None,
            ArtifactReceipt::new(ArtifactDestination::File("out.bin".into()), 2),
            Some("wrote out.bin".into()),
        );
        let (_, file_report_failure) = emit_run_result(
            &DispatchResult::Artifact(file_run),
            &mut FailingWriter,
            &mut Vec::new(),
        );
        assert_eq!(
            file_report_failure.unwrap().kind(),
            RunErrorKind::FinalWrite(OutputKind::Artifact)
        );

        let stdout_run = ArtifactRun::new(
            vec![0, 1],
            None,
            ArtifactReceipt::new(ArtifactDestination::Stdout, 2),
            Some("wrote stdout".into()),
        );
        let mut stdout = Vec::new();
        let (_, stdout_report_failure) = emit_run_result(
            &DispatchResult::Artifact(stdout_run),
            &mut stdout,
            &mut FailingWriter,
        );
        assert_eq!(stdout, [0, 1]);
        assert_eq!(
            stdout_report_failure.unwrap().kind(),
            RunErrorKind::FinalWrite(OutputKind::Artifact)
        );
    }
}
