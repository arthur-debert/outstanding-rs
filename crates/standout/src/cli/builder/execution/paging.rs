use crate::cli::builder::App;
use crate::cli::builder::NO_PAGER_ARG;
use crate::cli::builder::OUTPUT_FILE_ARG;
use crate::cli::dispatch::extract_command_path;
use crate::cli::handler::Delivery;
use crate::cli::handler::DispatchResult;
use crate::cli::pager::Pager;
use crate::cli::pager::PagerOutcome;
use crate::ColorPolicy;
use crate::Representation;
use crate::TargetProperties;
use clap::ArgMatches;
use std::path::PathBuf;

impl App {
    pub(crate) fn process_edge_target(&self) -> TargetProperties {
        let mut target = TargetProperties::detect();
        target.ambiguous_width = self.ambiguous_width;
        target
    }

    /// Decides a run's presentation and destination for every entry point.
    /// Help is decided elsewhere: it short-circuits clap and leaves no
    /// `ArgMatches`.
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
        let representation = self.typed_output_mode(matches).unwrap_or_else(|| {
            term.and_then(|term| term.output)
                .map_or(representation_fallback, Representation::from)
        });
        let target = file_destination(target, self.output_file_override(matches).is_some());
        RunResolution {
            representation,
            color_policy: self.resolve_color_policy(
                self.typed_color_policy(matches).or(typed_color),
                named_color,
                term,
            ),
            target,
            pager: self
                .pager_for_run(
                    target,
                    representation,
                    self.paging_is_suppressed_in(matches),
                )
                .filter(|_| self.pages_its_output(&extract_command_path(matches).join("."))),
        }
    }

    pub(super) fn output_file_override(&self, matches: &ArgMatches) -> Option<PathBuf> {
        self.output_file_flag.as_ref().and_then(|_| {
            matches
                .try_get_one::<String>(OUTPUT_FILE_ARG)
                .unwrap_or(None)
                .map(PathBuf::from)
        })
    }

    /// The pager the run's human output goes to, or `None` when paging does
    /// not apply. Resolving names a pager without starting one.
    fn pager_for_run(
        &self,
        target: TargetProperties,
        output_mode: Representation,
        suppressed: bool,
    ) -> Option<Pager> {
        if !target.stdout_is_terminal || output_mode != Representation::Human || suppressed {
            return None;
        }
        Pager::resolve(self.name.as_deref())
    }

    /// `--help` short-circuits clap, so the paging rule reads the output file
    /// and `--no-pager` from argv instead of from `ArgMatches`.
    pub(super) fn pager_for_rendered_help(
        &self,
        display: &crate::cli::result::HelpDisplay,
        args: &[std::ffi::OsString],
        target: TargetProperties,
        output_mode: Representation,
    ) -> Option<Pager> {
        if !matches!(display, crate::cli::result::HelpDisplay::Rendered { .. }) {
            return None;
        }
        self.pager_for_run(
            file_destination(target, self.output_file_from_unparsed(args).is_some()),
            output_mode,
            self.paging_is_suppressed(args),
        )
    }

    fn paging_is_suppressed_in(&self, matches: &ArgMatches) -> bool {
        matches
            .try_get_one::<bool>(NO_PAGER_ARG)
            .unwrap_or(None)
            .copied()
            .unwrap_or(false)
    }

    fn pages_its_output(&self, path: &str) -> bool {
        self.pageable_for(path) && !self.emits_events_for(path)
    }

    /// `true` when the pager took the bytes stdout would have received,
    /// terminating newline included, or when its reader left. A pager that
    /// could not start returns `false` and leaves them for the caller to write.
    pub(super) fn page_delivery(&self, run: &crate::cli::CompletedRun) -> bool {
        let Delivery::Pager(command) = run.delivery() else {
            return false;
        };
        let DispatchResult::Handled(output) = run.outcome() else {
            return false;
        };
        match Pager::named(command.clone()).page(&format!("{}\n", output)) {
            PagerOutcome::Paged | PagerOutcome::ReaderLeft => true,
            PagerOutcome::CouldNotStart => false,
        }
    }
}

pub(crate) struct RunResolution {
    pub(crate) representation: Representation,
    pub(crate) color_policy: ColorPolicy,
    pub(crate) target: TargetProperties,
    pub(crate) pager: Option<Pager>,
}

/// A named output file is never a terminal, so `auto` resolves to plain text
/// in it; an explicit `--color always` still writes escapes there.
pub(super) fn file_destination(
    mut target: TargetProperties,
    writes_to_a_file: bool,
) -> TargetProperties {
    if writes_to_a_file {
        target.stdout_is_terminal = false;
        target.stdout_color_capability = false;
    }
    target
}

#[cfg(test)]
mod tests;
