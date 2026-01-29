//! Error types for the seeker crate.

use thiserror::Error;

/// Errors that can occur when building or executing queries.
#[derive(Debug, Error)]
pub enum SeekerError {
    /// Invalid regular expression pattern.
    #[error("invalid regex pattern: {0}")]
    InvalidRegex(#[from] regex::Error),

    /// Operator is not valid for the given value type.
    #[error("operator '{op}' is not valid for {value_type} values")]
    InvalidOperatorForType {
        op: &'static str,
        value_type: &'static str,
    },

    /// Type mismatch between clause value and field value.
    #[error("type mismatch: clause expects {expected}, got {actual}")]
    TypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
}

/// Result type for seeker operations.
pub type Result<T> = std::result::Result<T, SeekerError>;
