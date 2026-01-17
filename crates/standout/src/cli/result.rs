//! Help interception result type.

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
