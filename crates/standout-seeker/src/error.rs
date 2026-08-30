use thiserror::Error;
#[derive(Debug, Error)]
pub enum SeekerError {
    #[error("invalid regex pattern: {0}")]
    InvalidRegex(#[from] regex::Error),
    #[error("operator '{op}' is not valid for {value_type} values")]
    InvalidOperatorForType {
        op: &'static str,
        value_type: &'static str,
    },
    #[error("type mismatch: clause expects {expected}, got {actual}")]
    TypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
}
pub type Result<T> = std::result::Result<T, SeekerError>;
