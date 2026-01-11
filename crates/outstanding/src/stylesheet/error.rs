//! Error types for stylesheet parsing.

use std::path::PathBuf;

/// Error type for stylesheet parsing failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StylesheetError {
    /// YAML parse error.
    Parse {
        /// Optional source file path.
        path: Option<PathBuf>,
        /// Error message from the YAML parser.
        message: String,
    },

    /// Invalid color format.
    InvalidColor {
        /// Style name where the error occurred.
        style: String,
        /// The invalid color value.
        value: String,
        /// Optional source file path.
        path: Option<PathBuf>,
    },

    /// Unknown attribute in style definition.
    UnknownAttribute {
        /// Style name where the error occurred.
        style: String,
        /// The unknown attribute name.
        attribute: String,
        /// Optional source file path.
        path: Option<PathBuf>,
    },

    /// Invalid shorthand syntax.
    InvalidShorthand {
        /// Style name where the error occurred.
        style: String,
        /// The invalid shorthand value.
        value: String,
        /// Optional source file path.
        path: Option<PathBuf>,
    },

    /// Alias validation error (dangling reference or cycle).
    AliasError {
        /// The underlying validation error.
        source: crate::style::StyleValidationError,
    },

    /// Invalid style definition structure.
    InvalidDefinition {
        /// Style name where the error occurred.
        style: String,
        /// Description of what was wrong.
        message: String,
        /// Optional source file path.
        path: Option<PathBuf>,
    },
}

impl std::fmt::Display for StylesheetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StylesheetError::Parse { path, message } => {
                if let Some(p) = path {
                    write!(f, "Failed to parse stylesheet {}: {}", p.display(), message)
                } else {
                    write!(f, "Failed to parse stylesheet: {}", message)
                }
            }
            StylesheetError::InvalidColor { style, value, path } => {
                let location = path
                    .as_ref()
                    .map(|p| format!(" in {}", p.display()))
                    .unwrap_or_default();
                write!(
                    f,
                    "Invalid color '{}' for style '{}'{}",
                    value, style, location
                )
            }
            StylesheetError::UnknownAttribute {
                style,
                attribute,
                path,
            } => {
                let location = path
                    .as_ref()
                    .map(|p| format!(" in {}", p.display()))
                    .unwrap_or_default();
                write!(
                    f,
                    "Unknown attribute '{}' in style '{}'{}",
                    attribute, style, location
                )
            }
            StylesheetError::InvalidShorthand { style, value, path } => {
                let location = path
                    .as_ref()
                    .map(|p| format!(" in {}", p.display()))
                    .unwrap_or_default();
                write!(
                    f,
                    "Invalid shorthand '{}' for style '{}'{}",
                    value, style, location
                )
            }
            StylesheetError::AliasError { source } => {
                write!(f, "Style alias error: {}", source)
            }
            StylesheetError::InvalidDefinition {
                style,
                message,
                path,
            } => {
                let location = path
                    .as_ref()
                    .map(|p| format!(" in {}", p.display()))
                    .unwrap_or_default();
                write!(
                    f,
                    "Invalid definition for style '{}'{}: {}",
                    style, location, message
                )
            }
        }
    }
}

impl std::error::Error for StylesheetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StylesheetError::AliasError { source } => Some(source),
            _ => None,
        }
    }
}
