//! Error types for template rendering.
//!
//! This module provides [`RenderError`], the primary error type for all rendering
//! operations. It abstracts over the underlying template engine's errors, providing
//! a stable public API.

use std::fmt;

/// Error type for template rendering operations.
///
/// This error type provides a stable API that doesn't expose implementation details
/// of the underlying template engine. All public rendering functions return this type.
#[derive(Debug)]
pub enum RenderError {
    /// Template syntax error or compilation failure.
    TemplateError(String),

    /// Template not found in the registry.
    TemplateNotFound(String),

    /// Data serialization error.
    SerializationError(String),

    /// Style validation error (invalid alias, cycle, etc.).
    StyleError(String),

    /// I/O error (e.g., reading template from disk).
    IoError(std::io::Error),

    /// Other operational error.
    OperationError(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::TemplateError(msg) => write!(f, "template error: {}", msg),
            RenderError::TemplateNotFound(name) => write!(f, "template not found: {}", name),
            RenderError::SerializationError(msg) => write!(f, "serialization error: {}", msg),
            RenderError::StyleError(msg) => write!(f, "style error: {}", msg),
            RenderError::IoError(err) => write!(f, "I/O error: {}", err),
            RenderError::OperationError(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RenderError::IoError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for RenderError {
    fn from(err: std::io::Error) -> Self {
        RenderError::IoError(err)
    }
}

impl From<serde_json::Error> for RenderError {
    fn from(err: serde_json::Error) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<serde_yaml::Error> for RenderError {
    fn from(err: serde_yaml::Error) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<quick_xml::DeError> for RenderError {
    fn from(err: quick_xml::DeError) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<csv::Error> for RenderError {
    fn from(err: csv::Error) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<csv::IntoInnerError<csv::Writer<Vec<u8>>>> for RenderError {
    fn from(err: csv::IntoInnerError<csv::Writer<Vec<u8>>>) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for RenderError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

// Conversion from minijinja::Error - this keeps internal compatibility
impl From<minijinja::Error> for RenderError {
    fn from(err: minijinja::Error) -> Self {
        use minijinja::ErrorKind;

        match err.kind() {
            ErrorKind::TemplateNotFound => RenderError::TemplateNotFound(err.to_string()),
            ErrorKind::SyntaxError
            | ErrorKind::BadEscape
            | ErrorKind::UndefinedError
            | ErrorKind::UnknownTest
            | ErrorKind::UnknownFunction
            | ErrorKind::UnknownFilter
            | ErrorKind::UnknownMethod => RenderError::TemplateError(err.to_string()),
            ErrorKind::BadSerialization => RenderError::SerializationError(err.to_string()),
            _ => RenderError::OperationError(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = RenderError::TemplateNotFound("foo".to_string());
        assert!(err.to_string().contains("template not found"));
        assert!(err.to_string().contains("foo"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let render_err: RenderError = io_err.into();
        assert!(matches!(render_err, RenderError::IoError(_)));
    }

    #[test]
    fn test_from_minijinja_template_not_found() {
        let mj_err = minijinja::Error::new(
            minijinja::ErrorKind::TemplateNotFound,
            "template 'foo' not found",
        );
        let render_err: RenderError = mj_err.into();
        assert!(matches!(render_err, RenderError::TemplateNotFound(_)));
    }
}
