//! Style validation errors.

/// Error returned when style validation fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleValidationError {
    /// An alias references a style that doesn't exist
    UnresolvedAlias { from: String, to: String },
    /// A cycle was detected in alias resolution
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
