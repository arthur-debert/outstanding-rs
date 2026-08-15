//! Help interception result types.
//!
//! Help is decided once, in [`HelpDisplay`], and projected onto whichever
//! result type the calling parse path speaks: [`HelpResult`] for configured
//! parsing (`get_matches_from` / `parse_from`), [`RunResult`] for dispatch
//! (`dispatch_from` / `run` / `run_to_string`). Two projections of one decision
//! are what keep `myapp help` from meaning one thing through one entry point
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
    /// Clap answered the help arm itself: a bad flag on the word, a topic that
    /// names nothing, or a display Clap wants to make. The user's line is what
    /// is at fault (or nothing is), and Clap's own `use_stderr` split says
    /// which.
    Clap(clap::Error),
    /// Help could not be rendered — a broken template or theme. Never the
    /// user's mistake, whatever the line said.
    ///
    /// It carries a Clap error because that is the currency of the configured
    /// path: `get_matches_from` hands its caller a `clap::Error` for every
    /// failure. Classifying at the point of failure rather than re-deriving it
    /// downstream is the point of the variant — a render failure and a rejected
    /// flag are indistinguishable by the time they are both `clap::Error`.
    RenderFailed(clap::Error),
}

impl From<HelpDisplay> for HelpResult {
    fn from(display: HelpDisplay) -> Self {
        match display {
            HelpDisplay::Rendered { text, paged: true } => HelpResult::PagedHelp(text),
            HelpDisplay::Rendered { text, paged: false } => HelpResult::Help(text),
            HelpDisplay::Clap(e) | HelpDisplay::RenderFailed(e) => HelpResult::Error(e),
        }
    }
}

impl From<HelpDisplay> for RunResult {
    /// Projects a help display onto the dispatch path's result type.
    ///
    /// A rendered help is a typed success — the pager request rides along in
    /// [`SuccessKind::PagedHelp`](crate::cli::SuccessKind::PagedHelp), since
    /// only `run()` can act on it and the capture APIs must stay
    /// side-effect-free.
    ///
    /// Failures keep their origin. Clap's answer to the help arm keeps Clap's
    /// own split — a rejected line goes to stderr as
    /// [`RunErrorKind::ClapUsage`], a display it wants to make stays a success
    /// — while a render failure is [`RunErrorKind::Render`]: a broken template
    /// or theme is the application's bug and must not be reported to the user
    /// as a usage error, nor exit with the usage status.
    fn from(display: HelpDisplay) -> Self {
        match display {
            HelpDisplay::Rendered { text, paged } => RunResult::Handled(if paged {
                RunOutput::paged_help(text)
            } else {
                RunOutput::clap_help(text)
            }),
            HelpDisplay::Clap(e) if e.use_stderr() => {
                RunResult::Error(RunError::new(e.to_string(), RunErrorKind::ClapUsage))
            }
            HelpDisplay::Clap(e) => RunResult::Handled(RunOutput::clap_help(e.to_string())),
            HelpDisplay::RenderFailed(e) => {
                RunResult::Error(RunError::new(e.to_string(), RunErrorKind::Render))
            }
        }
    }
}
