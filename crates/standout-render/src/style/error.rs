//! Error types for style validation and stylesheet parsing.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleValidationError {
    UnresolvedAlias { from: String, to: String },
    CycleDetected { path: Vec<String> },
}

impl std::fmt::Display for StyleValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StyleValidationError::UnresolvedAlias { from, to } => {
                write!(f, "style '{}' aliases non-existent style '{}'", from, to)
            }
            StyleValidationError::CycleDetected { path } => {
                write!(f, "cycle detected in style aliases: {}", path.join(" -> "))
            }
        }
    }
}

impl std::error::Error for StyleValidationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StylesheetError {
    Parse {
        path: Option<PathBuf>,
        message: String,
    },

    InvalidColor {
        style: String,
        value: String,
        path: Option<PathBuf>,
    },

    UnknownAttribute {
        style: String,
        attribute: String,
        path: Option<PathBuf>,
    },

    InvalidShorthand {
        style: String,
        value: String,
        path: Option<PathBuf>,
    },

    AliasError {
        source: StyleValidationError,
    },

    InvalidDefinition {
        style: String,
        message: String,
        path: Option<PathBuf>,
    },

    Load {
        message: String,
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
            StylesheetError::Load { message } => {
                write!(f, "Failed to load stylesheet: {}", message)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unresolved_alias_error_display() {
        let err = StyleValidationError::UnresolvedAlias {
            from: "orphan".to_string(),
            to: "missing".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("orphan"));
        assert!(msg.contains("missing"));
    }

    #[test]
    fn test_cycle_detected_error_display() {
        let err = StyleValidationError::CycleDetected {
            path: vec!["a".to_string(), "b".to_string(), "a".to_string()],
        };
        let msg = err.to_string();
        assert!(msg.contains("cycle"));
        assert!(msg.contains("a -> b -> a"));
    }
}
