//! Help interception result types.
//!
//! Help is decided once, in [`HelpDisplay`], and projected onto whichever
//! result type the calling parse path speaks: [`HelpResult`] for configured
//! parsing (`get_matches_from` / `parse_from`), [`RunResult`] for dispatch
//! (`dispatch_from` / `run` / `run_to_string`). Two projections of one decision
//! is what keeps `myapp help` from meaning one thing through one entry point
//! and something else through the other.

use crate::cli::handler::{RunError, RunErrorKind, RunOutput, RunResult};

/// Result of the help interception.
///
/// After processing a command, the CLI returns this enum to indicate
/// what action should be taken.
#[derive(Debug)]
pub enum HelpResult {
    /// Normal matches found (no help requested).
    Matches(clap::ArgMatches),
    /// Help was rendered. Caller should print or display as needed.
    Help(String),
    /// Help was rendered and should be displayed through a pager.
    PagedHelp(String),
    /// Error: Subcommand or topic not found.
    Error(clap::Error),
}

/// A help request answered instead of the root's parse.
///
/// Deliberately narrower than [`HelpResult`]: this is only ever produced when
/// help *was* the request, so it carries no "and here are your matches" case
/// for either projection to invent a meaning for.
#[derive(Debug)]
pub(crate) enum HelpDisplay {
    /// Help rendered. `paged` carries the `--page` request, which only the
    /// printing entry points can honour.
    Rendered {
        /// The rendered help text.
        text: String,
        /// Whether `--page` asked for a pager.
        paged: bool,
    },
    /// The request named neither a command nor a topic, or rendering failed.
    Error(clap::Error),
}

impl From<HelpDisplay> for HelpResult {
    fn from(display: HelpDisplay) -> Self {
        match display {
            HelpDisplay::Rendered { text, paged: true } => HelpResult::PagedHelp(text),
            HelpDisplay::Rendered { text, paged: false } => HelpResult::Help(text),
            HelpDisplay::Error(e) => HelpResult::Error(e),
        }
    }
}

impl From<HelpDisplay> for RunResult {
    /// Projects a help display onto the dispatch path's result type.
    ///
    /// A rendered help is a typed success — the pager request rides along in
    /// [`SuccessKind::PagedHelp`](crate::cli::SuccessKind::PagedHelp), since
    /// only `run()` can act on it and the capture APIs must stay
    /// side-effect-free. An error keeps Clap's own split: a usage failure goes
    /// to stderr with a nonzero status, while a display error stays a success.
    fn from(display: HelpDisplay) -> Self {
        match display {
            HelpDisplay::Rendered { text, paged } => RunResult::Handled(if paged {
                RunOutput::paged_help(text)
            } else {
                RunOutput::clap_help(text)
            }),
            HelpDisplay::Error(e) if e.use_stderr() => {
                RunResult::Error(RunError::new(e.to_string(), RunErrorKind::ClapUsage))
            }
            HelpDisplay::Error(e) => RunResult::Handled(RunOutput::clap_help(e.to_string())),
        }
    }
}
