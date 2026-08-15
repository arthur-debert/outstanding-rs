//! Error types for setup operations.

use standout_dispatch::verify::HandlerMismatchError;
use standout_render::{RegistryError, RenderError};

/// Error type for setup operations.
#[derive(Debug)]
pub enum SetupError {
    /// Template loading or rendering error.
    Template(String),
    /// Stylesheet loading or parsing error.
    Stylesheet(String),
    /// Theme not found.
    ThemeNotFound(String),
    /// Configuration error.
    Config(String),
    /// Duplicate command registered.
    DuplicateCommand(String),
    /// The application claims `help`, the name standout installs for itself.
    DuplicateHelpWord(HelpWordSource),
    /// I/O error during setup (e.g., loading templates/styles).
    Io(std::io::Error),
    /// Verification failed (handler vs command mismatch).
    VerificationFailed(HandlerMismatchError),
}

/// Where the application's own `help` command was found.
///
/// The remedy is the same either way; the two differ only in where the author
/// has to go to apply it, and in when standout can see the claim — a
/// registration is visible to `build()`, a clap declaration only once the
/// `Command` reaches a parse entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpWordSource {
    /// Registered with standout: `.command("help", …)` or a sibling.
    Registered,
    /// Declared on the application's clap `Command`, by name or alias.
    Declared,
}

impl HelpWordSource {
    /// The clause naming where the application's `help` was found, written to
    /// sit inside [`SetupError::DuplicateHelpWord`]'s one-sentence report.
    fn declaration(self) -> &'static str {
        match self {
            HelpWordSource::Registered => "this application registers a `help` command",
            HelpWordSource::Declared => {
                "this application's clap `Command` declares `help` (as a subcommand name or alias)"
            }
        }
    }
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupError::Template(msg) => write!(f, "template error: {}", msg),
            SetupError::Stylesheet(msg) => write!(f, "stylesheet error: {}", msg),
            SetupError::ThemeNotFound(name) => write!(f, "theme not found: {}", name),
            SetupError::Config(msg) => write!(f, "configuration error: {}", msg),
            SetupError::DuplicateCommand(cmd) => write!(f, "duplicate command: {}", cmd),
            SetupError::DuplicateHelpWord(source) => write!(
                f,
                "duplicate command: help — {}, and standout installs a `help` word of \
                 its own under .help_handling(true). Rename the application's command, \
                 or drop .help_handling(true) to keep the name (help is then clap's \
                 own, and command_groups and topics become unavailable)",
                source.declaration()
            ),
            SetupError::Io(err) => write!(f, "setup I/O error: {}", err),
            SetupError::VerificationFailed(err) => write!(f, "verification failed:\n{}", err),
        }
    }
}

impl From<std::io::Error> for SetupError {
    fn from(e: std::io::Error) -> Self {
        SetupError::Io(e)
    }
}

impl std::error::Error for SetupError {}

impl From<RenderError> for SetupError {
    fn from(e: RenderError) -> Self {
        SetupError::Template(e.to_string())
    }
}

impl From<RegistryError> for SetupError {
    fn from(e: RegistryError) -> Self {
        SetupError::Template(e.to_string())
    }
}

impl From<HandlerMismatchError> for SetupError {
    fn from(e: HandlerMismatchError) -> Self {
        SetupError::VerificationFailed(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_error_display() {
        let err = SetupError::Template("test error".into());
        assert_eq!(err.to_string(), "template error: test error");

        let err = SetupError::ThemeNotFound("dark".into());
        assert_eq!(err.to_string(), "theme not found: dark");
    }
}
