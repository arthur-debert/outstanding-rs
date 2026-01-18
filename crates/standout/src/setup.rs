//! Error types for setup operations.

use crate::rendering::template::registry::RegistryError;
use minijinja::Error as JinjaError;

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
    /// I/O error during setup (e.g., loading templates/styles).
    Io(std::io::Error),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupError::Template(msg) => write!(f, "template error: {}", msg),
            SetupError::Stylesheet(msg) => write!(f, "stylesheet error: {}", msg),
            SetupError::ThemeNotFound(name) => write!(f, "theme not found: {}", name),
            SetupError::Config(msg) => write!(f, "configuration error: {}", msg),
            SetupError::DuplicateCommand(cmd) => write!(f, "duplicate command: {}", cmd),
            SetupError::Io(err) => write!(f, "setup I/O error: {}", err),
        }
    }
}

impl From<std::io::Error> for SetupError {
    fn from(e: std::io::Error) -> Self {
        SetupError::Io(e)
    }
}

impl std::error::Error for SetupError {}

impl From<JinjaError> for SetupError {
    fn from(e: JinjaError) -> Self {
        SetupError::Template(e.to_string())
    }
}

impl From<RegistryError> for SetupError {
    fn from(e: RegistryError) -> Self {
        SetupError::Template(e.to_string())
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
