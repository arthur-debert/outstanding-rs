//! Dispatch and execution methods for AppBuilder.
//!
//! This module contains methods for dispatching and running commands:
//! - `commands()` - dispatch macro integration
//! - `dispatch()` - match and execute handler
//! - `dispatch_from()` - parse args and dispatch
//! - `run()` - dispatch and print
//! - `run_to_string()` - dispatch and return

use crate::{write_binary_output, write_output, OutputDestination, OutputMode};
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::io::Write;
use std::path::PathBuf;

use super::{AppBuilder, PendingCommand};
use crate::cli::dispatch::{
    dispatch, extract_command_path, get_deepest_matches, insert_default_command, DispatchOutput,
    Presentation,
};
use crate::cli::group::{ErasedConfigRecipe, GroupBuilder, GroupEntry};
use crate::cli::handler::{
    ArtifactDestination, ArtifactReceipt, ArtifactRun, CommandContext, OutputKind, RunError,
    RunErrorKind, RunOutput, RunResult,
};
use crate::cli::hooks::{ArtifactOutput, RenderedOutput, TextOutput};
use crate::cli::questionnaire::{
    augment_questionnaire_command, render_questions_result, validate_questionnaire_surface,
    ANSWERS_ARG_ID, QUESTIONS_SUBCOMMAND, YES_ARG_ID,
};
use crate::SetupError;

impl AppBuilder {
    /// Registers commands from a dispatch closure (used by the `dispatch!` macro).
    ///
    /// This method accepts a closure that configures a [`GroupBuilder`] with commands
    /// and nested groups. It's typically used with the [`dispatch!`] macro:
    ///
    /// ```rust,ignore
    /// use standout::cli::{dispatch, App};
    ///
    /// App::builder()
    ///     .template_dir("templates")
    ///     .commands(dispatch! {
    ///         db: {
    ///             migrate => db::migrate,
    ///             backup => db::backup,
    ///         },
    ///         version => version,
    ///     })
    ///     .build()
    /// ```
    ///
    /// The closure receives an empty [`GroupBuilder`] and should return it with
    /// commands added. Each top-level entry becomes a command or group.
    pub fn commands<F>(mut self, configure: F) -> Result<Self, SetupError>
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());

        // Extract default command if set in the builder
        if let Some(ref default_cmd) = builder.default_command {
            self.default_command = Some(default_cmd.clone());
        }

        // Register all entries from the group builder with deferred closure creation
        for (name, entry) in builder.entries {
            match entry {
                GroupEntry::Command { mut handler } => {
                    let template = handler
                        .template()
                        .map(String::from)
                        .unwrap_or_else(|| self.resolve_template(&name));

                    if let Some(hooks) = handler.take_hooks() {
                        self.command_hooks.insert(name.clone(), hooks);
                    }
                    if let Some(questionnaire) = handler.take_questionnaire() {
                        self.questionnaire_commands
                            .insert(name.clone(), questionnaire);
                    }

                    // Create a recipe for deferred closure creation
                    let recipe = ErasedConfigRecipe::from_handler(handler);

                    // Check for duplicates
                    if self.pending_commands.borrow().contains_key(&name) {
                        return Err(SetupError::DuplicateCommand(name));
                    }

                    // Store pending command
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

    /// Dispatches to a registered handler if one matches the command path.
    ///
    /// Returns:
    /// - `RunResult::Handled(output)` if a handler was found and executed successfully,
    /// - `RunResult::Binary(bytes, filename)` for binary output,
    /// - `RunResult::Handled(RunOutput::command(String::new()))` if the handler
    ///   completed silently (preserving the capture compatibility accessor),
    /// - `RunResult::Error(msg)` if a handler, hook, or output step failed,
    /// - `RunResult::NoMatch(matches)` if no handler matched.
    ///
    /// If hooks are registered for the command, they are executed:
    /// - Pre-dispatch hooks run before the handler
    /// - Post-dispatch hooks run after the handler but before rendering
    /// - Post-output hooks run after rendering
    ///
    /// Handler errors and hook errors both abort execution and return
    /// `RunResult::Error(msg)`. Callers using `dispatch()` directly are
    /// responsible for writing the error to stderr and choosing an exit code.
    pub fn dispatch(&self, matches: ArgMatches, output_mode: OutputMode) -> RunResult {
        // Ensure commands are finalized (creates dispatch closures with current theme)
        self.ensure_commands_finalized();

        // Build command path from matches
        let path = extract_command_path(&matches);
        let path_str = path.join(".");

        // Look up handler
        let commands = self.get_commands();
        if let Some(dispatch_fn) = commands.get(&path_str) {
            let mut ctx = CommandContext::new(path, self.app_state.clone());

            // Get hooks for this command (used for pre-dispatch, post-dispatch, and post-output)
            let hooks = self.command_hooks.get(&path_str);

            // Run pre-dispatch hooks if registered (hooks can inject state via ctx.extensions)
            if let Some(hooks) = hooks {
                if let Err(e) = hooks.run_pre_dispatch(&matches, &mut ctx) {
                    return RunResult::Error(super::super::dispatch::hook_run_error(
                        e,
                        crate::cli::HookPhase::PreDispatch,
                    ));
                }
            }

            // Get the subcommand matches for the deepest command
            let sub_matches = get_deepest_matches(&matches);

            // Run the handler (post-dispatch hooks are run inside dispatch function)
            // output_mode is passed separately because CommandContext is render-agnostic
            // Late binding: theme is resolved here at dispatch time, not when commands were registered
            let default_theme = crate::Theme::default();
            let theme = self.theme.as_ref().unwrap_or(&default_theme);
            let dispatch_output = match dispatch(
                dispatch_fn,
                sub_matches,
                &ctx,
                hooks,
                output_mode,
                theme,
                self.ambiguous_width,
            ) {
                Ok(output) => output,
                Err(e) => return RunResult::Error(e),
            };

            // Convert to Output enum for post-output hooks. An artifact's
            // presentation travels beside the hook payload: hooks may still
            // change the bytes or the report, so the report is only rendered
            // after the write, further down.
            let (output, presentation) = match dispatch_output {
                DispatchOutput::Text { formatted, raw } => {
                    (RenderedOutput::Text(TextOutput::new(formatted, raw)), None)
                }
                DispatchOutput::Binary(b, f) => (RenderedOutput::Binary(b, f), None),
                DispatchOutput::Artifact {
                    output,
                    presentation,
                } => (RenderedOutput::Artifact(output), Some(presentation)),
                DispatchOutput::Silent => (RenderedOutput::Silent, None),
            };

            // Run post-output hooks if registered
            let mut final_output = if let Some(hooks) = hooks {
                match hooks.run_post_output(&matches, &ctx, output) {
                    Ok(o) => o,
                    Err(e) => {
                        return RunResult::Error(super::super::dispatch::hook_run_error(
                            e,
                            crate::cli::HookPhase::PostOutput,
                        ))
                    }
                }
            } else {
                output
            };

            // The explicit output-file override, when the app enabled the flag.
            let override_path = self.output_file_flag.as_ref().and_then(|_| {
                matches
                    .try_get_one::<String>("_output_file_path")
                    .unwrap_or(None)
                    .map(PathBuf::from)
            });

            // The artifact path owns its own destination policy and write, so
            // it is resolved before the legacy override handling below.
            if let RenderedOutput::Artifact(artifact) = final_output {
                return complete_artifact(artifact, presentation, override_path);
            }

            // Handle file output if configured
            if let Some(path) = override_path {
                let dest = OutputDestination::File(path);

                match &final_output {
                    RenderedOutput::Text(t) => {
                        // Write raw output (without ANSI codes) to file
                        if let Err(e) = write_output(&t.raw, &dest) {
                            return RunResult::Error(RunError::new(
                                format!("Error writing output: {}", e),
                                RunErrorKind::FinalWrite(OutputKind::Text),
                            ));
                        }
                        // Suppress further output
                        final_output = RenderedOutput::Silent;
                    }
                    RenderedOutput::Binary(b, _) => {
                        if let Err(e) = write_binary_output(b, &dest) {
                            return RunResult::Error(RunError::new(
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

            // Convert back to RunResult (using formatted for terminal display)
            match final_output {
                RenderedOutput::Text(t) => RunResult::Handled(RunOutput::command(t.formatted)),
                RenderedOutput::Binary(b, f) => RunResult::Binary(b, f),
                RenderedOutput::Artifact(_) => unreachable!("artifacts returned above"),
                // Preserve the 7.x capture contract: silent success is an
                // empty handled string. `run()` still emits nothing.
                RenderedOutput::Silent => RunResult::Handled(RunOutput::command(String::new())),
            }
        } else {
            RunResult::NoMatch(matches)
        }
    }

    /// Parses arguments and dispatches to registered handlers.
    ///
    /// This is the recommended entry point when using the command handler system.
    /// It augments the command with `--output` flag, parses arguments, and
    /// dispatches to registered handlers.
    ///
    /// # Returns
    ///
    /// - `RunResult::Handled(output)` if a registered handler processed the command
    /// - `RunResult::NoMatch(matches)` if no handler matched (for manual handling)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::{App, HandlerResult, Output, RunResult};
    ///
    /// let result = App::builder()
    ///     .command("list", |_m, _ctx| Ok(HandlerOutput::Render(vec!["a", "b"]), "{{ . }}")
    ///     .dispatch_from(cmd, std::env::args());
    ///
    /// match result {
    ///     RunResult::Handled(output) => println!("{}", output),
    ///     RunResult::NoMatch(matches) => {
    ///         // Handle manually
    ///     }
    /// }
    /// ```
    pub fn dispatch_from<I, T>(&self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        // Collect args to Vec<String> so we can potentially reparse with default command
        let args: Vec<String> = args
            .into_iter()
            .map(|a| a.into().to_string_lossy().into_owned())
            .collect();

        if let Err(error) = self.validate_questionnaire_surfaces(&cmd) {
            return RunResult::Error(RunError::new(error.to_string(), RunErrorKind::ClapUsage));
        }

        // Augment command with framework-owned flags and questionnaire command surface.
        let augmented_cmd = self.augment_command_for_dispatch(cmd.clone());

        // Parse arguments. Clap's "errors" include `--help` and `--version`,
        // which are successful display paths (stdout, exit 0). Real parse
        // errors (unknown flag, missing required arg, etc.) get `use_stderr()
        // == true` and should surface as `RunResult::Error` so they exit
        // non-zero on stderr.
        let matches = match augmented_cmd.try_get_matches_from(&args) {
            Ok(m) => m,
            Err(e) => {
                if e.use_stderr() {
                    return RunResult::Error(RunError::new(e.to_string(), RunErrorKind::ClapUsage));
                }
                let output = match e.kind() {
                    clap::error::ErrorKind::DisplayVersion => {
                        RunOutput::clap_version(e.to_string())
                    }
                    _ => RunOutput::clap_help(e.to_string()),
                };
                return RunResult::Handled(output);
            }
        };

        // A naked invocation may resolve to a default command — statically, or
        // per-invocation via `default_command_with`. Both funnel through
        // `resolve_default_command` so this path and `get_matches_from` agree.
        let matches = match self.resolve_default_command(&cmd, &matches) {
            Err(e) => {
                return RunResult::Error(RunError::new(e.to_string(), RunErrorKind::DefaultCommand))
            }
            Ok(None) => matches,
            Ok(Some(default_cmd)) => {
                let new_args = insert_default_command(args, &default_cmd);

                // Reparse with default command inserted
                let augmented_cmd = self.augment_command_for_dispatch(cmd);
                match augmented_cmd.try_get_matches_from(&new_args) {
                    Ok(m) => m,
                    Err(e) => {
                        if e.use_stderr() {
                            return RunResult::Error(RunError::new(
                                e.to_string(),
                                RunErrorKind::ClapUsage,
                            ));
                        }
                        let output = match e.kind() {
                            clap::error::ErrorKind::DisplayVersion => {
                                RunOutput::clap_version(e.to_string())
                            }
                            _ => RunOutput::clap_help(e.to_string()),
                        };
                        return RunResult::Handled(output);
                    }
                }
            }
        };

        if let Some((path, questionnaire)) = self.questionnaire_questions_invocation(&matches) {
            if let Some(parent_matches) =
                command_matches_for_path(&matches, &path.split('.').collect::<Vec<_>>())
            {
                let has_answers = parent_matches
                    .try_get_one::<String>(ANSWERS_ARG_ID)
                    .unwrap_or(None)
                    .is_some();
                let has_yes = parent_matches
                    .try_get_one::<bool>(YES_ARG_ID)
                    .unwrap_or(None)
                    == Some(&true);
                if has_answers || has_yes {
                    return RunResult::Error(RunError::new(
                        "`questions` renders the blank answer sheet and cannot be combined with --answers or --yes",
                        RunErrorKind::ClapUsage,
                    ));
                }
            }
            return render_questions_result(questionnaire, &matches);
        }

        // Extract output mode
        let output_mode = if self.output_flag.is_some() {
            match matches
                .get_one::<String>("_output_mode")
                .map(|s| s.as_str())
            {
                Some("term") => OutputMode::Term,
                Some("text") => OutputMode::Text,
                Some("term-debug") => OutputMode::TermDebug,
                Some("json") => OutputMode::Json,
                Some("yaml") => OutputMode::Yaml,
                Some("xml") => OutputMode::Xml,
                Some("csv") => OutputMode::Csv,
                _ => OutputMode::Auto,
            }
        } else {
            OutputMode::Auto
        };

        // Dispatch to handler
        self.dispatch(matches, output_mode)
    }

    /// Runs the CLI: parses arguments, dispatches to handlers, and prints output.
    ///
    /// This is the main entry point for command execution. It handles everything:
    /// parsing, dispatch, rendering, and output.
    ///
    /// # Returns
    ///
    /// - `true` if a handler processed and printed output
    /// - `false` if no handler matched (caller should handle manually)
    ///
    /// # Errors and exit codes
    ///
    /// On `RunResult::Error`, this function writes the diagnostic to stderr
    /// and exits with its typed status: Clap usage errors use 2, runtime
    /// failures use 1, and an application-declared `ExternalFailure` preserves
    /// its exact nonzero status and verbatim diagnostic. Final text and binary
    /// writes are framework-owned; a write failure is diagnosed on stderr and exits 1.
    /// Callers needing fine-grained control over exit codes should use
    /// [`Self::run_to_string`] or [`Self::dispatch_from`] and match on
    /// `RunResult` themselves.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::{App, HandlerResult, Output};
    ///
    /// let handled = App::builder()
    ///     .command("list", |_m, _ctx| Ok(HandlerOutput::Render(vec!["a", "b"])), "{{ . }}")?
    ///     .build()?
    ///     .run(cmd, std::env::args());
    ///
    /// if !handled {
    ///     // Handle unregistered commands manually
    /// }
    /// ```
    pub fn run<I, T>(&self, cmd: Command, args: I) -> bool
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let result = self.dispatch_from(cmd, args);
        let primary_status = result.exit_status();
        let stdout = std::io::stdout();
        let stderr = std::io::stderr();
        let mut stdout = stdout.lock();
        let mut stderr = stderr.lock();
        let (handled, final_write_failure) = emit_run_result(&result, &mut stdout, &mut stderr);
        drop(stdout);
        drop(stderr);

        // After the primary output has been flushed to stdout, render any
        // framework warnings collected during setup/dispatch to stderr so
        // they appear last on the user's terminal. `OutputMode::Auto` is a
        // safe default here: the renderer's final decision on styling is
        // driven by whether stderr itself is a color-capable TTY.
        let default_theme = crate::Theme::default();
        let theme = self.theme.as_ref().unwrap_or(&default_theme);
        standout_render::warnings::flush_to_stderr(theme, OutputMode::Auto);

        let status = final_write_failure
            .as_ref()
            .map(RunError::exit_status)
            .or(primary_status);
        if let Some(status) = status.filter(|status| status.code() != 0) {
            std::process::exit(i32::from(status.code()));
        }

        handled
    }

    /// Runs the CLI and returns the rendered output as a string.
    ///
    /// Similar to `run()`, but returns the output instead of printing it.
    /// Useful for testing or when you need to capture and process the output.
    ///
    /// # Returns
    ///
    /// - `RunResult::Handled(output)` - Handler executed successfully, or Clap produced help/version.
    ///   Silent completion remains an empty handled string for capture compatibility.
    /// - `RunResult::Binary(bytes, filename)` - Handler produced binary output
    /// - `RunResult::Error(error)` - A typed handler, hook, render, write, Clap usage, or external failure
    /// - `RunResult::NoMatch(matches)` - No handler matched
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::{App, HandlerResult, Output, RunResult};
    ///
    /// let result = App::builder()
    ///     .command("list", |_m, _ctx| Ok(HandlerOutput::Render(vec!["a", "b"])), "{{ . }}")?
    ///     .build()?
    ///     .run_to_string(cmd, std::env::args());
    ///
    /// match result {
    ///     RunResult::Handled(output) => println!("{}", output),
    ///     RunResult::Binary(bytes, filename) => std::fs::write(filename, bytes)?,
    ///     RunResult::Error(error) => {
    ///         eprintln!("{}", error);
    ///         std::process::exit(error.exit_status().code().into());
    ///     },
    ///     RunResult::NoMatch(matches) => { /* handle manually */ },
    ///     // RunResult is #[non_exhaustive]; cover Silent and any future variants.
    ///     _ => {},
    /// }
    /// ```
    pub fn run_to_string<I, T>(&self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        self.dispatch_from(cmd, args)
    }

    /// Augments a command for dispatch (adds --output flag without help subcommand).
    pub(crate) fn augment_command_for_dispatch(&self, mut cmd: Command) -> Command {
        self.augment_questionnaire_commands(&mut cmd, &[]);

        if let Some(ref flag_name) = self.output_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_mode")
                    .long(flag)
                    .value_name("MODE")
                    .global(true)
                    .value_parser([
                        "auto",
                        "term",
                        "text",
                        "term-debug",
                        "json",
                        "yaml",
                        "xml",
                        "csv",
                    ])
                    .default_value("auto")
                    .help("Output format"),
            );
        }

        // Add output file flag if enabled
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

    pub(crate) fn validate_questionnaire_surfaces(&self, cmd: &Command) -> Result<(), SetupError> {
        for path in self.questionnaire_commands.keys() {
            let parts = path.split('.').collect::<Vec<_>>();
            let Some(command) = crate::cli::app::find_subcommand_recursive(cmd, &parts) else {
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

fn command_matches_for_path<'a>(matches: &'a ArgMatches, path: &[&str]) -> Option<&'a ArgMatches> {
    let mut current = matches;
    for segment in path {
        current = current.subcommand_matches(segment)?;
    }
    Some(current)
}

/// Selects the destination for an artifact, deterministically.
///
/// The policy is, in order:
///
/// 1. the explicit output-file override;
/// 2. the application's suggested destination (opt-in);
/// 3. stdout, if the application opted in.
///
/// An artifact that matches none of these is a typed final-write failure: the
/// framework refuses to invent a file, and refuses to drop the bytes silently.
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

/// Builds the value the artifact report is rendered from.
///
/// The shape is always `{"report": <report or null>, "receipt": {…}}`. Fixing
/// the envelope keeps the framework's receipt from colliding with an
/// application key and lets templates rely on `{{ receipt.destination }}`
/// whatever the report's own type is.
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

/// Completes an artifact: select a destination, write, and only then report.
///
/// The ordering is the whole point of the shape. A report rendered before the
/// write could promise a file that never appeared; a write failure therefore
/// returns a typed error and no report at all.
///
/// The stdout destination is the one deferral: its bytes are handed to the
/// framework's stdout writer (see `emit_run_result`) so that capture APIs stay
/// side-effect-free, exactly as `RunResult::Binary` already behaves.
fn complete_artifact(
    artifact: ArtifactOutput,
    presentation: Option<Box<Presentation>>,
    override_path: Option<PathBuf>,
) -> RunResult {
    let destination = match resolve_artifact_destination(&artifact, override_path) {
        Ok(destination) => destination,
        Err(error) => return RunResult::Error(error),
    };

    if let ArtifactDestination::File(path) = &destination {
        let dest = OutputDestination::File(path.clone());
        if let Err(e) = write_binary_output(&artifact.bytes, &dest) {
            return RunResult::Error(RunError::new(
                format!("Error writing artifact: {}", e),
                RunErrorKind::FinalWrite(OutputKind::Artifact),
            ));
        }
    }

    let receipt = ArtifactReceipt::new(destination, artifact.bytes.len());

    let report = match artifact.report {
        None => None,
        Some(report) => {
            // A post-output hook can turn text into an artifact, but it has no
            // presentation configuration to render a report with. Say so
            // rather than dropping the report on the floor.
            let Some(presentation) = presentation else {
                return RunResult::Error(RunError::new(
                    "Cannot render artifact report: the artifact carries a report but was not \
                     produced by a handler, so no template configuration is available",
                    RunErrorKind::Render,
                ));
            };
            let envelope = match report_envelope(Some(report), &receipt) {
                Ok(envelope) => envelope,
                Err(error) => return RunResult::Error(error),
            };
            match presentation.render(&envelope) {
                Ok((formatted, _raw)) => Some(formatted),
                Err(error) => return RunResult::Error(error),
            }
        }
    };

    RunResult::Artifact(ArtifactRun::new(
        artifact.bytes,
        artifact.suggested_destination,
        receipt,
        report,
    ))
}

/// Emits a completed artifact run through the framework-owned writers.
///
/// The report channel follows the destination: an artifact written to a file
/// leaves stdout free for its report, while an artifact written to stdout owns
/// stdout entirely, so its report goes to stderr where it cannot corrupt the
/// bytes. Either way the bytes go first — a stdout write that fails must not be
/// preceded by a success report.
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

/// Emits one captured result through framework-owned writers.
///
/// Keeping this as a writer seam makes final text/binary failures typed and
/// unit-testable while `run()` retains its public `bool` contract.
fn emit_run_result<W: Write, E: Write>(
    result: &RunResult,
    stdout: &mut W,
    stderr: &mut E,
) -> (bool, Option<RunError>) {
    let failure = match result {
        RunResult::Handled(output) if output.is_empty() => None,
        RunResult::Handled(output) => writeln!(stdout, "{}", output)
            .and_then(|()| stdout.flush())
            .err()
            .map(|error| {
                RunError::new(
                    format!("Error writing stdout: {}", error),
                    RunErrorKind::FinalWrite(OutputKind::Text),
                )
            }),
        RunResult::Binary(bytes, _) => stdout
            .write_all(bytes)
            .and_then(|()| stdout.flush())
            .err()
            .map(|error| {
                RunError::new(
                    format!("Error writing binary stdout: {}", error),
                    RunErrorKind::FinalWrite(OutputKind::Binary),
                )
            }),
        RunResult::Artifact(run) => emit_artifact(run, stdout, stderr),
        RunResult::Silent => None,
        RunResult::Error(error) => (if error.kind() == RunErrorKind::External {
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
        RunResult::NoMatch(_) => return (false, None),
        _ => return (false, None),
    };

    if let Some(error) = &failure {
        // Best effort: if stderr itself is broken there is nowhere else to
        // report the final-write diagnostic, but the typed failure still
        // determines status 1.
        let _ = writeln!(stderr, "{}", error).and_then(|()| stderr.flush());
    }
    (true, failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::handler::HandlerResult;
    use crate::cli::handler::Output as HandlerOutput;
    use crate::cli::hooks::{HookError, Hooks, RenderedOutput};

    // ============================================================================
    // Dispatch Macro Integration Tests
    // ============================================================================

    #[test]
    fn test_dispatch_macro_simple() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .commands(dispatch! {
                list => |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]})))
            })
            .unwrap();

        assert!(builder.has_command("list"));

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("items"));
    }

    #[test]
    fn test_dispatch_macro_with_groups() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .commands(dispatch! {
                db: {
                    migrate => |_m, _ctx| Ok(HandlerOutput::Render(json!({"migrated": true}))),
                    backup => |_m, _ctx| Ok(HandlerOutput::Render(json!({"backed_up": true}))),
                },
                version => |_m, _ctx| Ok(HandlerOutput::Render(json!({"v": "1.0"}))),
            })
            .unwrap();

        assert!(builder.has_command("db.migrate"));
        assert!(builder.has_command("db.backup"));
        assert!(builder.has_command("version"));

        // Test dispatch to nested command
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
        let result = builder.dispatch(matches, OutputMode::Json);
        assert!(result.is_handled());
        assert!(result.output().unwrap().contains("migrated"));
    }

    #[test]
    fn test_dispatch_macro_with_template() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .commands(dispatch! {
                list => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                    template: "Count: {{ count }}",
                }
            })
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

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
            .commands(dispatch! {
                list => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                    template: "{{ ok }}",
                    pre_dispatch: move |_, _| {
                        hook_called_clone.store(true, Ordering::SeqCst);
                        Ok(())
                    },
                }
            })
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_dispatch_macro_deeply_nested() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
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

    // ============================================================================
    // Core Dispatch Tests
    // ============================================================================

    #[test]
    fn test_dispatch_to_handler() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                "Count: {{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 42"));
    }

    #[test]
    fn test_dispatch_unhandled_fallthrough() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))), "")
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let matches = cmd.try_get_matches_from(["app", "other"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(!result.is_handled());
        assert!(result.matches().is_some());
    }

    #[test]
    fn test_dispatch_json_output() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test", "value": 123}))),
                "{{ name }}: {{ value }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"value\": 123"));
    }

    #[test]
    fn test_dispatch_nested_command() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "config.get",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"key": "value"}))),
                "{{ key }}",
            )
            .unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("value"));
    }

    #[test]
    fn test_dispatch_silent_result() {
        let builder = AppBuilder::new()
            .command("quiet", |_m, _ctx| Ok(HandlerOutput::<()>::Silent), "")
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("quiet"));

        let matches = cmd.try_get_matches_from(["app", "quiet"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));
    }

    #[test]
    fn test_dispatch_error_result() {
        let builder = AppBuilder::new()
            .command(
                "fail",
                |_m, _ctx| Err::<HandlerOutput<()>, _>(anyhow::anyhow!("something went wrong")),
                "",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("fail"));

        let matches = cmd.try_get_matches_from(["app", "fail"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_error(), "expected Error, got {:?}", result);
        let msg = result.error().unwrap();
        assert!(msg.contains("Error:"));
        assert!(msg.contains("something went wrong"));
    }

    #[test]
    fn test_dispatch_from_basic() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
                "Items: {{ items }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "list"]);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
    }

    #[test]
    fn test_dispatch_from_with_json_flag() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
                "Count: {{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--output=json", "list"]);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"count\": 5"));
    }

    #[test]
    fn test_dispatch_from_unhandled() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))), "")
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let result = builder.dispatch_from(cmd, ["app", "other"]);

        assert!(!result.is_handled());
    }

    // ============================================================================
    // Hook Execution Tests
    // ============================================================================

    #[test]
    fn test_dispatch_with_pre_dispatch_hook() {
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 1}))),
                "{{ count }}",
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
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
        assert_eq!(result.output(), Some("1"));
    }

    #[test]
    fn test_dispatch_pre_dispatch_hook_abort() {
        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| -> HandlerResult<()> {
                    panic!("Handler should not be called");
                },
                "",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .pre_dispatch(|_, _ctx| Err(HookError::pre_dispatch("blocked by hook"))),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_error(), "expected Error, got {:?}", result);
        let msg = result.error().unwrap();
        assert!(msg.contains("Hook error"));
        assert!(msg.contains("blocked by hook"));
    }

    #[test]
    fn test_dispatch_with_post_output_hook() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"}))),
                "{{ msg }}",
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
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("HELLO"));
    }

    #[test]
    fn test_dispatch_post_output_hook_chain() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "test"}))),
                "{{ msg }}",
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
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("[TEST]"));
    }

    #[test]
    fn test_dispatch_post_output_hook_abort() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"}))),
                "{{ msg }}",
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
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_error(), "expected Error, got {:?}", result);
        let msg = result.error().unwrap();
        assert!(msg.contains("Hook error"));
        assert!(msg.contains("post-processing failed"));
    }

    #[test]
    fn test_dispatch_hooks_for_nested_command() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "config.get",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"value": "secret"}))),
                "{{ value }}",
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
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("***"));
    }

    #[test]
    fn test_dispatch_no_hooks_for_command() {
        use serde_json::json;

        // Register hooks for "list" but dispatch "other"
        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "list"}))),
                "{{ msg }}",
            )
            .unwrap()
            .command(
                "other",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "other"}))),
                "{{ msg }}",
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
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("other"));
    }

    #[test]
    fn test_dispatch_binary_output_with_hook() {
        let builder = AppBuilder::new()
            .command(
                "export",
                |_m, _ctx| -> HandlerResult<()> {
                    Ok(HandlerOutput::Binary {
                        data: vec![1, 2, 3],
                        filename: "out.bin".into(),
                    })
                },
                "",
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
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_binary());
        let (bytes, filename) = result.binary().unwrap();
        assert_eq!(bytes, &[1, 2, 3, 4]);
        assert_eq!(filename, "out.bin");
    }

    #[test]
    fn test_hooks_passed_to_built_standout() {
        let standout = AppBuilder::new()
            .hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())))
            .build()
            .unwrap();

        assert!(standout.get_hooks("list").is_some());
        assert!(standout.get_hooks("other").is_none());
    }

    #[test]
    fn test_run_command_with_hooks() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            value: i32,
        }

        let standout = AppBuilder::new()
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
            "{{ value }}",
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.as_text(), Some("wrapped: 42"));
    }

    #[test]
    fn test_run_command_pre_dispatch_abort() {
        let standout = AppBuilder::new()
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
            "",
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

        let standout = AppBuilder::new().build().unwrap();

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
            "{{ msg }}",
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_text(), Some("hello"));
    }

    #[test]
    fn test_run_command_silent() {
        let standout = AppBuilder::new().build().unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command::<_, ()>(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Silent),
            "",
        );

        assert!(result.is_ok());
        assert!(result.unwrap().is_silent());
    }

    #[test]
    fn test_run_command_binary() {
        let standout = AppBuilder::new()
            .hooks(
                "export",
                Hooks::new().post_output(|_, _ctx, output| {
                    // Verify we receive binary output
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
            "",
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_binary());
        let (bytes, filename) = output.as_binary().unwrap();
        assert_eq!(bytes, &[0xDE, 0xAD]);
        assert_eq!(filename, "data.bin");
    }

    // ============================================================================
    // Post-dispatch Hook Tests
    // ============================================================================

    #[test]
    fn test_dispatch_with_post_dispatch_hook() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
                "Count: {{ count }}, Modified: {{ modified }}",
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
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Count: 5"));
        assert!(output.contains("Modified: true"));
    }

    #[test]
    fn test_dispatch_post_dispatch_hook_abort() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": []}))),
                "{{ items }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_dispatch(|_, _ctx, data| {
                    // Abort if no items
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
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_error(), "expected Error, got {:?}", result);
        let msg = result.error().unwrap();
        assert!(msg.contains("Hook error"));
        assert!(msg.contains("no items to display"));
    }

    #[test]
    fn test_dispatch_post_dispatch_chain() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"value": 1}))),
                "{{ value }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .post_dispatch(|_, _ctx, mut data| {
                        // First hook: multiply by 2
                        if let Some(v) = data.get_mut("value") {
                            *v = json!(v.as_i64().unwrap_or(0) * 2);
                        }
                        Ok(data)
                    })
                    .post_dispatch(|_, _ctx, mut data| {
                        // Second hook: add 10
                        if let Some(v) = data.get_mut("value") {
                            *v = json!(v.as_i64().unwrap_or(0) + 10);
                        }
                        Ok(data)
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        // 1 * 2 = 2, 2 + 10 = 12
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
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"}))),
                "{{ msg }}",
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
        let result = builder.dispatch(matches, OutputMode::Text);

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
            "value={{ value }}, added={{ added_by_hook }}",
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
            "{{ valid }}",
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.message, "invalid data");
        assert_eq!(err.phase, HookPhase::PostDispatch);
    }

    // ============================================================================
    // Default Command Tests
    // ============================================================================

    #[test]
    fn test_default_command_builder() {
        let builder = AppBuilder::new().default_command("list");

        assert_eq!(builder.default_command, Some("list".to_string()));
    }

    #[test]
    fn test_default_command_naked_invocation() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .default_command("list")
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
                "Items: {{ items }}",
            )
            .unwrap()
            .command(
                "add",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"added": true}))),
                "Added: {{ added }}",
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("add"));

        // Naked invocation should dispatch to default command
        let result = builder.dispatch_from(cmd, ["app"]);
        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
    }

    #[test]
    fn test_default_command_with_options() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .default_command("list")
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                "Count: {{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        // Naked invocation with --output flag should work
        let result = builder.dispatch_from(cmd, ["app", "--output=json"]);
        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"count\": 42"));
    }

    #[test]
    fn test_default_command_explicit_command_overrides() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .default_command("list")
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"cmd": "list"}))),
                "{{ cmd }}",
            )
            .unwrap()
            .command(
                "add",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"cmd": "add"}))),
                "{{ cmd }}",
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("add"));

        // Explicit command should override default
        let result = builder.dispatch_from(cmd, ["app", "add"]);
        assert!(result.is_handled());
        assert_eq!(result.output(), Some("add"));
    }

    #[test]
    fn test_default_command_no_default_set() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": []}))),
                "Items: {{ items }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        // Without default command, naked invocation should return NoMatch
        let result = builder.dispatch_from(cmd, ["app"]);
        assert!(!result.is_handled());
    }

    // ============================================================================
    // Output File Flag Tests
    // ============================================================================

    #[test]
    fn test_dispatch_with_output_file_flag() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        let path_str = file_path.to_str().unwrap();

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                "Count: {{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--output-file-path", path_str, "list"]);

        assert!(result.is_handled());
        // Verify output is suppressed (silent)
        assert_eq!(result.output(), Some(""));

        // Verify file content
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
            .output_file_flag(Some("save-to"))
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 99}))),
                "{{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--save-to", path_str, "list"]);

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
            .command(
                "show",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test", "count": 42}))),
                "unused",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("show"));

        let result = builder.dispatch_from(
            cmd,
            [
                "app",
                "--output",
                "json",
                "--output-file-path",
                path_str,
                "show",
            ],
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
            .command(
                "show",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "Alice"}))),
                "Hello {{ name }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("show"));

        let result = builder.dispatch_from(
            cmd,
            [
                "app",
                "--output",
                "text",
                "--output-file-path",
                path_str,
                "show",
            ],
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
            .no_output_file_flag()
            .command(
                "show",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                "Count: {{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("show"));

        // Without the flag, output goes to stdout normally
        let result = builder.dispatch_from(cmd, ["app", "show"]);

        assert!(result.is_handled());
        assert!(result.output().unwrap().contains("Count: 42"));
    }

    // ============================================================================
    // Theme Ordering Tests (issue #31 fix)
    // ============================================================================

    #[test]
    fn test_theme_ordering_command_before_theme() {
        use crate::Theme;
        use console::Style;
        use serde_json::json;

        // Create a theme with a custom "late" style
        let theme = Theme::new().add("late", Style::new().bold());

        // BUG TEST: Register command BEFORE setting theme
        // This tests if the closure captures the theme at registration time
        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test"}))),
                "[late]{{ name }}[/late]",
            )
            .unwrap()
            .theme(theme); // Theme set AFTER command registration

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.dispatch_from(cmd, ["app", "--output=term", "list"]);

        assert!(result.is_handled());
        let output = result.output().unwrap();

        // This will FAIL if closures capture theme at registration time
        // (because theme was None when .command() was called)
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

        // Create a theme with a "test_style" tag
        let theme = Theme::new().add("test_style", Style::new().bold());

        // Build with theme set BEFORE command registration
        let builder = AppBuilder::new()
            .theme(theme)
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test"}))),
                "[test_style]{{ name }}[/test_style]",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.dispatch_from(cmd, ["app", "--output=term", "list"]);

        assert!(result.is_handled());
        let output = result.output().unwrap();

        // If theme was passed correctly, there should be NO "[test_style?]" in output
        // (unknown style indicators appear when style is not found)
        assert!(
            !output.contains("[test_style?]"),
            "Theme was not passed to dispatch - output: {}",
            output
        );
    }

    #[test]
    fn test_styles_and_default_theme_with_command() {
        // This is the exact bug reported by a user: using .styles() + .default_theme()
        // instead of .theme() directly caused styles to not be found.
        // The fix ensures theme is resolved from stylesheet registry BEFORE
        // commands are finalized.
        use serde_json::json;
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // Create a theme file with a custom style
        fs::write(
            temp_dir.path().join("dark.yaml"),
            r#"
header:
  fg: blue
  bold: true
"#,
        )
        .unwrap();

        // This is the pattern that was broken:
        // .styles_dir() + .default_theme() + .command()
        let app = AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .default_theme("dark")
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"title": "Results"}))),
                "[header]{{ title }}[/header]",
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = app.dispatch_from(cmd, ["app", "--output=term", "list"]);

        assert!(result.is_handled());
        let output = result.output().unwrap();

        // The bug caused [header?] to appear instead of styled text
        assert!(
            !output.contains("[header?]"),
            "ORDERING BUG: .styles() + .default_theme() not applied - output: {}",
            output
        );
    }

    // ============================================================================
    // Builder Ordering Permutation Tests (issue #89)
    // These tests verify that builder methods work in any order.
    // ============================================================================

    #[test]
    fn test_builder_ordering_theme_before_command() {
        use crate::Theme;
        use console::Style;
        use serde_json::json;

        let theme = Theme::new().add("mystyle", Style::new().bold());

        // Order: theme -> command
        let app = AppBuilder::new()
            .theme(theme)
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"}))),
                "[mystyle]{{ x }}[/mystyle]",
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.dispatch_from(cmd, ["app", "--output=term", "test"]);

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

        // Order: command -> theme
        let app = AppBuilder::new()
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"}))),
                "[mystyle]{{ x }}[/mystyle]",
            )
            .unwrap()
            .theme(theme)
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.dispatch_from(cmd, ["app", "--output=term", "test"]);

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

        // Order: styles -> default_theme -> command
        let app = AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .default_theme("mytheme")
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"}))),
                "[mystyle]{{ x }}[/mystyle]",
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.dispatch_from(cmd, ["app", "--output=term", "test"]);

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

        // Order: command -> styles -> default_theme (command FIRST)
        let app = AppBuilder::new()
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"}))),
                "[mystyle]{{ x }}[/mystyle]",
            )
            .unwrap()
            .styles_dir(temp_dir.path())
            .unwrap()
            .default_theme("mytheme")
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.dispatch_from(cmd, ["app", "--output=term", "test"]);

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

        // Order: default_theme -> styles -> command (default_theme BEFORE styles)
        let app = AppBuilder::new()
            .default_theme("mytheme")
            .styles_dir(temp_dir.path())
            .unwrap()
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"}))),
                "[mystyle]{{ x }}[/mystyle]",
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.dispatch_from(cmd, ["app", "--output=term", "test"]);

        assert!(
            !result.output().unwrap().contains("[mystyle?]"),
            "default_theme -> styles -> command ordering failed"
        );
    }

    #[test]
    fn test_builder_ordering_all_permutations_with_explicit_theme() {
        // Test multiple orderings with explicit .theme() to ensure order independence
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

        let template = "[perm]{{ val }}[/perm]";

        // Permutation 1: theme -> command -> context
        let app1 = AppBuilder::new()
            .theme(make_theme())
            .command("test", make_handler(), template)
            .unwrap()
            .context("extra", minijinja::Value::from("x"))
            .build()
            .unwrap();

        // Permutation 2: command -> theme -> context
        let app2 = AppBuilder::new()
            .command("test", make_handler(), template)
            .unwrap()
            .theme(make_theme())
            .context("extra", minijinja::Value::from("x"))
            .build()
            .unwrap();

        // Permutation 3: context -> command -> theme
        let app3 = AppBuilder::new()
            .context("extra", minijinja::Value::from("x"))
            .command("test", make_handler(), template)
            .unwrap()
            .theme(make_theme())
            .build()
            .unwrap();

        // Permutation 4: context -> theme -> command
        let app4 = AppBuilder::new()
            .context("extra", minijinja::Value::from("x"))
            .theme(make_theme())
            .command("test", make_handler(), template)
            .unwrap()
            .build()
            .unwrap();

        // Permutation 5: command -> context -> theme
        let app5 = AppBuilder::new()
            .command("test", make_handler(), template)
            .unwrap()
            .context("extra", minijinja::Value::from("x"))
            .theme(make_theme())
            .build()
            .unwrap();

        // Permutation 6: theme -> context -> command
        let app6 = AppBuilder::new()
            .theme(make_theme())
            .context("extra", minijinja::Value::from("x"))
            .command("test", make_handler(), template)
            .unwrap()
            .build()
            .unwrap();

        for (i, app) in [app1, app2, app3, app4, app5, app6].into_iter().enumerate() {
            let cmd = Command::new("app").subcommand(Command::new("test"));
            let result = app.dispatch_from(cmd, ["app", "--output=term", "test"]);

            assert!(
                !result.output().unwrap().contains("[perm?]"),
                "Permutation {} failed: style not found",
                i + 1
            );
        }
    }

    // ============================================================================
    // App State Tests
    // ============================================================================

    #[test]
    fn test_dispatch_with_app_state() {
        use serde_json::json;

        struct Database {
            url: String,
        }

        let builder = AppBuilder::new()
            .app_state(Database {
                url: "postgres://localhost".into(),
            })
            .command(
                "list",
                |_m, ctx| {
                    // Handler should have access to app_state
                    let db = ctx.app_state.get::<Database>().unwrap();
                    Ok(HandlerOutput::Render(json!({"db_url": db.url.clone()})))
                },
                "{{ db_url }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.dispatch_from(cmd, ["app", "list"]);

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
            .app_state(Config { debug: true })
            .command(
                "list",
                |_m, ctx| {
                    // Use get_required for better error handling
                    let config = ctx.app_state.get_required::<Config>()?;
                    Ok(HandlerOutput::Render(json!({"debug": config.debug})))
                },
                "debug={{ debug }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.dispatch_from(cmd, ["app", "list"]);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("debug=true"));
    }

    #[test]
    fn test_dispatch_app_state_missing_type_error() {
        use serde_json::json;

        struct NotProvided;

        // Note: No app_state registered
        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, ctx| {
                    // This should fail because NotProvided wasn't registered
                    let _missing = ctx.app_state.get_required::<NotProvided>()?;
                    Ok(HandlerOutput::Render(json!({})))
                },
                "",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.dispatch_from(cmd, ["app", "list"]);

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
            .app_state(Database {
                name: "mydb".into(),
            })
            .app_state(Config { version: 42 })
            .command(
                "info",
                |_m, ctx| {
                    let db = ctx.app_state.get_required::<Database>()?;
                    let config = ctx.app_state.get_required::<Config>()?;
                    Ok(HandlerOutput::Render(json!({
                        "db": db.name,
                        "version": config.version
                    })))
                },
                "db={{ db }}, version={{ version }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let result = builder.dispatch_from(cmd, ["app", "info"]);

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
            .app_state(Database {
                name: "maindb".into(),
            })
            .command(
                "list",
                |_m, ctx| {
                    // app_state: immutable, shared
                    let db = ctx.app_state.get_required::<Database>()?;

                    // extensions: mutable, per-request (set by pre-dispatch hook)
                    let scope = ctx.extensions.get_required::<UserScope>()?;

                    Ok(HandlerOutput::Render(json!({
                        "db": db.name,
                        "user": scope.user_id
                    })))
                },
                "db={{ db }}, user={{ user }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().pre_dispatch(|_, ctx| {
                    // Extensions are per-request, injected by hooks
                    ctx.extensions.insert(UserScope {
                        user_id: "user123".into(),
                    });
                    Ok(())
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.dispatch_from(cmd, ["app", "list"]);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("db=maindb, user=user123"));
    }

    #[test]
    fn test_built_app_dispatch_with_app_state() {
        use serde_json::json;

        struct ApiConfig {
            base_url: String,
        }

        // Test that app_state works after .build() is called
        let app = AppBuilder::new()
            .app_state(ApiConfig {
                base_url: "https://api.example.com".into(),
            })
            .command(
                "fetch",
                |_m, ctx| {
                    let config = ctx.app_state.get_required::<ApiConfig>()?;
                    Ok(HandlerOutput::Render(json!({"url": config.base_url})))
                },
                "{{ url }}",
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("fetch"));
        let result = app.dispatch_from(cmd, ["app", "fetch"]);

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
            &RunResult::Handled(RunOutput::command("hello")),
            &mut stdout,
            &mut stderr,
        );
        assert!(handled);
        assert!(failure.is_none());
        assert_eq!(stdout, b"hello\n");
        assert!(stderr.is_empty());

        stdout.clear();
        let (handled, failure) = emit_run_result(
            &RunResult::Error(RunError::new("bad argv", RunErrorKind::ClapUsage)),
            &mut stdout,
            &mut stderr,
        );
        assert!(handled);
        assert!(failure.is_none());
        assert!(stdout.is_empty());
        assert_eq!(stderr, b"bad argv\n");
    }

    #[test]
    fn final_text_and_binary_write_failures_keep_payload_kind() {
        let mut stderr = Vec::new();
        let (_, text_failure) = emit_run_result(
            &RunResult::Handled(RunOutput::command("hello")),
            &mut FailingWriter,
            &mut stderr,
        );
        let text_failure = text_failure.unwrap();
        assert_eq!(
            text_failure.kind(),
            RunErrorKind::FinalWrite(OutputKind::Text)
        );
        assert_eq!(text_failure.exit_status(), crate::cli::ExitStatus::FAILURE);

        stderr.clear();
        let (_, binary_failure) = emit_run_result(
            &RunResult::Binary(vec![0, 1], "data.bin".into()),
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
    fn final_text_and_binary_flush_failures_keep_payload_kind() {
        let mut text_stdout = FlushFailingWriter::default();
        let (_, text_failure) = emit_run_result(
            &RunResult::Handled(RunOutput::command("hello")),
            &mut text_stdout,
            &mut Vec::new(),
        );
        assert_eq!(text_stdout.bytes, b"hello\n");
        assert_eq!(
            text_failure.unwrap().kind(),
            RunErrorKind::FinalWrite(OutputKind::Text)
        );

        let mut binary_stdout = FlushFailingWriter::default();
        let (_, binary_failure) = emit_run_result(
            &RunResult::Binary(vec![0, 1], "data.bin".into()),
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
            &RunResult::Artifact(file_run),
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
            &RunResult::Artifact(stdout_run),
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
