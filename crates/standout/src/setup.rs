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
    /// Theme not found in the configured stylesheets.
    ThemeNotFound(String),
    /// Configuration error.
    Config(String),
    /// Duplicate command registered.
    ///
    /// The payload is the report, not just the name: `Display` renders it as
    /// `duplicate command: {payload}`, so a collision that needs more than a
    /// name to act on — the `help` word standout installs for itself — writes
    /// the guidance into it.
    DuplicateCommand(String),
    /// I/O error during setup (e.g., loading templates/styles).
    Io(std::io::Error),
    /// Verification failed (handler vs command mismatch).
    VerificationFailed(HandlerMismatchError),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupError::Template(msg) => write!(f, "template error: {}", msg),
            SetupError::Stylesheet(msg) => write!(f, "stylesheet error: {}", msg),
            SetupError::ThemeNotFound(name) => write!(
                f,
                "theme `{}` not found; add it with .styles(embed_styles!(\"src/styles\")) or .styles_dir(\"path/to/styles\"), select an available stylesheet with .default_theme(...), or pass a Theme directly with .theme(...)",
                name
            ),
            SetupError::Config(msg) => write!(f, "configuration error: {}", msg),
            SetupError::DuplicateCommand(cmd) => write!(f, "duplicate command: {}", cmd),
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
        assert!(err.to_string().contains("theme `dark` not found"));
        assert!(err.to_string().contains(".styles(embed_styles!"));
        assert!(err.to_string().contains(".theme(...)"));
    }
}
