//! The run's presentation and destination: the framework flags the builder
//! installs, the entry points that dispatch a command, and the write of what
//! the run produced.
//!
//! The payload and delivery decisions read the post-output hooks' final result,
//! because a hook can still turn a document into a payload or add a report to
//! an artifact whose handler returned none.

mod command_tree;
mod config;
mod dispatch;
mod output;
mod paging;

use paging::file_destination;

use crate::{
    write_output, ColorPolicy, InputSources, OutputDestination, Representation, TargetProperties,
};
use clap::Command;
use standout_render::warnings::WarningBuffer;
use std::sync::Arc;

use super::App;
use crate::cli::handler::{
    Delivery, DispatchResult, ExitStatus, OutputKind, RunError, RunErrorKind, RunRecorder,
    StreamCapture, StreamSink, SuccessKind,
};
use crate::cli::pager::Pager;
use crate::cli::ProcessOutcome;
use std::io::Write;

const CONFIG_OVERRIDE_ARG: &str = "_config_override";

struct RunOutcome {
    outcome: DispatchResult,
    output_mode: Representation,
    color_policy: ColorPolicy,
    pager: Option<Pager>,
}

impl RunOutcome {
    fn to_stdout(
        outcome: DispatchResult,
        output_mode: Representation,
        color_policy: ColorPolicy,
    ) -> Self {
        Self {
            outcome,
            output_mode,
            color_policy,
            pager: None,
        }
    }
}

/// The strict-mode failure for the style tags the render window has left
/// unresolved, or `None` when there are none. Callers have already decided
/// that strict mode is on; `warnings` loses the superseded degrade warning.
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

impl App {
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
        let args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
        let output_file = self.output_file_from_unparsed(&args);
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

        let help_to_file = output_file
            .zip(help_page(result.outcome()))
            .map(|(path, page)| {
                write_output(page, &OutputDestination::File(path))
                    .err()
                    .map(|error| {
                        RunError::final_write(
                            format!("Error writing output: {}", error),
                            Arc::new(error),
                            OutputKind::Text,
                        )
                    })
            });
        let paged = help_to_file.is_none() && self.page_delivery(&result);

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

        if !crate::cli::emit::warnings_delivered_on_stdout(result.outcome(), output_mode) {
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

        ProcessOutcome {
            handled,
            status,
            final_write_failure,
        }
    }

    pub(crate) fn seed_startup_warnings(&self, warnings: &WarningBuffer) {
        for message in &self.startup_warnings {
            warnings.push(message.clone());
        }
    }

    /// The capture window every entry point shares. Help and answer-sheet
    /// outcomes return before the pre-commit strict check, so they are checked
    /// here instead, and the run's surviving delivery is recorded here too.
    /// Every clap rejection passes through here, so this is also where an
    /// application's `usage_exit_status` replaces the framework's `2`.
    fn collect_run_warnings(
        &self,
        recorder: &RunRecorder,
        inner: impl FnOnce(WarningBuffer) -> RunOutcome,
    ) -> crate::cli::CompletedRun {
        let warnings = WarningBuffer::new();
        self.seed_startup_warnings(&warnings);
        let _capture = standout_render::diagnostics::begin_capture();
        let RunOutcome {
            mut outcome,
            output_mode,
            color_policy,
            pager,
        } = inner(warnings.clone());
        if outcome.success_kind().is_some() {
            if let Some(error) = self.strict_style_tags_error(&warnings) {
                outcome = DispatchResult::Error(error);
            }
        }
        if let (Some(status), DispatchResult::Error(error)) = (self.usage_exit_status, &outcome) {
            if error.kind() == RunErrorKind::ClapUsage {
                outcome = DispatchResult::Error(error.clone().with_usage_exit_status(status));
            }
        }
        if let (Some(pager), DispatchResult::Handled(output)) = (&pager, &outcome) {
            if !output.as_str().is_empty() {
                recorder.set_delivery(Delivery::Pager(pager.command().to_string()));
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

    /// `run_with` with the run's color policy and the sink the whole run writes
    /// through: the handler's events, then whatever the caller writes after.
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

    /// `run_with_sink` with the run's recorder named.
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
}

/// The help page a run ended in, from clap's own `--help` or from the grouped
/// page standout renders for `--help` and the `help` word.
fn help_page(outcome: &crate::cli::DispatchResult) -> Option<&str> {
    let crate::cli::DispatchResult::Handled(output) = outcome else {
        return None;
    };
    matches!(output.kind(), SuccessKind::ClapHelp).then(|| output.as_str())
}

#[cfg(test)]
mod tests;
