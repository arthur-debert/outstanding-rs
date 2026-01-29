//! Message types for view status and feedback.

use serde::{Deserialize, Serialize};

/// Severity level for status messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageLevel {
    /// Informational message (neutral)
    Info,
    /// Success message (positive outcome)
    Success,
    /// Warning message (attention needed)
    Warning,
    /// Error message (something went wrong)
    Error,
}

impl MessageLevel {
    /// Returns the style name used for this level in templates.
    ///
    /// Maps to framework styles: `standout-info`, `standout-success`, etc.
    pub fn style_name(&self) -> &'static str {
        match self {
            MessageLevel::Info => "standout-info",
            MessageLevel::Success => "standout-success",
            MessageLevel::Warning => "standout-warning",
            MessageLevel::Error => "standout-error",
        }
    }
}

impl std::fmt::Display for MessageLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageLevel::Info => write!(f, "info"),
            MessageLevel::Success => write!(f, "success"),
            MessageLevel::Warning => write!(f, "warning"),
            MessageLevel::Error => write!(f, "error"),
        }
    }
}

/// A status message with severity level.
///
/// Messages appear at the end of view output and are styled
/// according to their level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// The severity level
    pub level: MessageLevel,
    /// The message text
    pub text: String,
}

impl Message {
    /// Create a new message.
    pub fn new(level: MessageLevel, text: impl Into<String>) -> Self {
        Self {
            level,
            text: text.into(),
        }
    }

    /// Create an info message.
    pub fn info(text: impl Into<String>) -> Self {
        Self::new(MessageLevel::Info, text)
    }

    /// Create a success message.
    pub fn success(text: impl Into<String>) -> Self {
        Self::new(MessageLevel::Success, text)
    }

    /// Create a warning message.
    pub fn warning(text: impl Into<String>) -> Self {
        Self::new(MessageLevel::Warning, text)
    }

    /// Create an error message.
    pub fn error(text: impl Into<String>) -> Self {
        Self::new(MessageLevel::Error, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::warning("Something happened");
        assert_eq!(msg.level, MessageLevel::Warning);
        assert_eq!(msg.text, "Something happened");
    }

    #[test]
    fn test_message_shortcuts() {
        assert_eq!(Message::info("test").level, MessageLevel::Info);
        assert_eq!(Message::success("test").level, MessageLevel::Success);
        assert_eq!(Message::warning("test").level, MessageLevel::Warning);
        assert_eq!(Message::error("test").level, MessageLevel::Error);
    }

    #[test]
    fn test_style_names() {
        assert_eq!(MessageLevel::Info.style_name(), "standout-info");
        assert_eq!(MessageLevel::Success.style_name(), "standout-success");
        assert_eq!(MessageLevel::Warning.style_name(), "standout-warning");
        assert_eq!(MessageLevel::Error.style_name(), "standout-error");
    }

    #[test]
    fn test_serialization() {
        let msg = Message::warning("Test warning");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"level\":\"warning\""));
        assert!(json.contains("\"text\":\"Test warning\""));
    }
}
