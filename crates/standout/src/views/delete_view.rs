//! DeleteView result type and builder.
//!
//! DeleteView provides a standardized structure for displaying the result
//! of a delete operation:
//! - The deleted item (for confirmation display)
//! - Confirmation status
//! - Soft-delete indication
//! - Undo command (if available)
//! - Status messages
//!
//! # Example
//!
//! ```rust
//! use standout::views::delete_view;
//!
//! #[derive(serde::Serialize)]
//! struct Task {
//!     id: String,
//!     title: String,
//! }
//!
//! let task = Task {
//!     id: "task-1".to_string(),
//!     title: "Old task".to_string(),
//! };
//!
//! let result = delete_view(task)
//!     .confirmed()
//!     .undo_command("task restore task-1")
//!     .success("Task deleted")
//!     .build();
//! ```

use serde::Serialize;

use super::{Message, MessageLevel};

/// Result type for delete view handlers.
///
/// This struct is serialized and passed to the delete view template.
/// The framework-supplied `standout/delete-view` template handles
/// rendering, or you can provide your own.
#[derive(Debug, Clone, Serialize)]
pub struct DeleteViewResult<T> {
    /// The deleted item (for display/confirmation).
    pub item: T,

    /// Whether the deletion was confirmed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub confirmed: bool,

    /// Whether this was a soft-delete (item still recoverable).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub soft_deleted: bool,

    /// Command to undo the deletion (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo_command: Option<String>,

    /// Status messages (info, warning, error).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
}

impl<T> DeleteViewResult<T> {
    /// Create a new delete view result with just the item.
    pub fn new(item: T) -> Self {
        Self {
            item,
            confirmed: false,
            soft_deleted: false,
            undo_command: None,
            messages: Vec::new(),
        }
    }

    /// Returns true if the deletion was confirmed.
    pub fn is_confirmed(&self) -> bool {
        self.confirmed
    }

    /// Returns true if this was a soft-delete.
    pub fn is_soft_deleted(&self) -> bool {
        self.soft_deleted
    }

    /// Returns true if an undo command is available.
    pub fn has_undo(&self) -> bool {
        self.undo_command.is_some()
    }
}

/// Builder for constructing `DeleteViewResult` instances.
///
/// Use [`delete_view()`] to start building:
///
/// ```rust
/// use standout::views::delete_view;
///
/// let item = serde_json::json!({"id": 1, "name": "Test"});
/// let result = delete_view(item)
///     .confirmed()
///     .success("Deleted successfully")
///     .build();
/// ```
#[derive(Debug)]
pub struct DeleteViewBuilder<T> {
    item: T,
    confirmed: bool,
    soft_deleted: bool,
    undo_command: Option<String>,
    messages: Vec<Message>,
}

impl<T> DeleteViewBuilder<T> {
    /// Create a new builder with the given item.
    pub fn new(item: T) -> Self {
        Self {
            item,
            confirmed: false,
            soft_deleted: false,
            undo_command: None,
            messages: Vec::new(),
        }
    }

    /// Mark the deletion as confirmed.
    pub fn confirmed(mut self) -> Self {
        self.confirmed = true;
        self
    }

    /// Set the confirmation status explicitly.
    pub fn with_confirmed(mut self, confirmed: bool) -> Self {
        self.confirmed = confirmed;
        self
    }

    /// Mark this as a soft-delete (item still recoverable).
    pub fn soft_deleted(mut self) -> Self {
        self.soft_deleted = true;
        self
    }

    /// Set the undo command.
    pub fn undo_command(mut self, command: impl Into<String>) -> Self {
        self.undo_command = Some(command.into());
        self
    }

    /// Add a status message.
    pub fn message(mut self, level: MessageLevel, text: impl Into<String>) -> Self {
        self.messages.push(Message::new(level, text));
        self
    }

    /// Add an info message.
    pub fn info(self, text: impl Into<String>) -> Self {
        self.message(MessageLevel::Info, text)
    }

    /// Add a success message.
    pub fn success(self, text: impl Into<String>) -> Self {
        self.message(MessageLevel::Success, text)
    }

    /// Add a warning message.
    pub fn warning(self, text: impl Into<String>) -> Self {
        self.message(MessageLevel::Warning, text)
    }

    /// Add an error message.
    pub fn error(self, text: impl Into<String>) -> Self {
        self.message(MessageLevel::Error, text)
    }

    /// Build the `DeleteViewResult`.
    pub fn build(self) -> DeleteViewResult<T> {
        DeleteViewResult {
            item: self.item,
            confirmed: self.confirmed,
            soft_deleted: self.soft_deleted,
            undo_command: self.undo_command,
            messages: self.messages,
        }
    }
}

/// Create a new delete view builder with the given item.
///
/// This is the primary entry point for constructing `DeleteViewResult` instances.
///
/// # Examples
///
/// Basic usage:
///
/// ```rust
/// use standout::views::delete_view;
///
/// let task = serde_json::json!({"id": "t1", "title": "Test"});
/// let result = delete_view(task)
///     .confirmed()
///     .success("Task deleted")
///     .build();
/// assert!(result.is_confirmed());
/// ```
///
/// Soft-delete with undo:
///
/// ```rust
/// use standout::views::delete_view;
///
/// let task = serde_json::json!({"id": "t1", "title": "Test"});
/// let result = delete_view(task)
///     .confirmed()
///     .soft_deleted()
///     .undo_command("task restore t1")
///     .info("Task moved to trash")
///     .build();
/// assert!(result.is_soft_deleted());
/// assert!(result.has_undo());
/// ```
///
/// Pending confirmation:
///
/// ```rust
/// use standout::views::delete_view;
///
/// let task = serde_json::json!({"id": "t1", "title": "Test"});
/// let result = delete_view(task)
///     .warning("Are you sure? Use --confirm to proceed")
///     .build();
/// assert!(!result.is_confirmed());
/// ```
pub fn delete_view<T>(item: T) -> DeleteViewBuilder<T> {
    DeleteViewBuilder::new(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delete_view_builder_basic() {
        let result = delete_view("item").build();
        assert_eq!(result.item, "item");
        assert!(!result.confirmed);
        assert!(!result.soft_deleted);
        assert!(result.undo_command.is_none());
        assert!(result.messages.is_empty());
    }

    #[test]
    fn test_delete_view_builder_confirmed() {
        let result = delete_view("item").confirmed().build();
        assert!(result.is_confirmed());
    }

    #[test]
    fn test_delete_view_builder_with_confirmed() {
        let result = delete_view("item").with_confirmed(true).build();
        assert!(result.is_confirmed());

        let result = delete_view("item").with_confirmed(false).build();
        assert!(!result.is_confirmed());
    }

    #[test]
    fn test_delete_view_builder_soft_deleted() {
        let result = delete_view("item").soft_deleted().build();
        assert!(result.is_soft_deleted());
    }

    #[test]
    fn test_delete_view_builder_undo_command() {
        let result = delete_view("item").undo_command("task restore t1").build();

        assert!(result.has_undo());
        assert_eq!(result.undo_command, Some("task restore t1".to_string()));
    }

    #[test]
    fn test_delete_view_builder_with_messages() {
        let result = delete_view("item")
            .info("Info")
            .success("Success")
            .warning("Warning")
            .error("Error")
            .build();

        assert_eq!(result.messages.len(), 4);
        assert_eq!(result.messages[0].level, MessageLevel::Info);
        assert_eq!(result.messages[1].level, MessageLevel::Success);
        assert_eq!(result.messages[2].level, MessageLevel::Warning);
        assert_eq!(result.messages[3].level, MessageLevel::Error);
    }

    #[test]
    fn test_delete_view_full_example() {
        let result = delete_view("task")
            .confirmed()
            .soft_deleted()
            .undo_command("restore task-1")
            .success("Task deleted")
            .build();

        assert!(result.is_confirmed());
        assert!(result.is_soft_deleted());
        assert!(result.has_undo());
        assert_eq!(result.messages.len(), 1);
    }

    #[test]
    fn test_delete_view_serialization() {
        let result = delete_view(serde_json::json!({"id": 1}))
            .confirmed()
            .success("Deleted")
            .build();

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"item\":{\"id\":1}"));
        assert!(json.contains("\"confirmed\":true"));
    }

    #[test]
    fn test_delete_view_serialization_skips_empty() {
        let result = delete_view("item").build();
        let json = serde_json::to_string(&result).unwrap();

        // Should not contain optional fields when false/None
        assert!(!json.contains("\"confirmed\""));
        assert!(!json.contains("\"soft_deleted\""));
        assert!(!json.contains("\"undo_command\""));
        assert!(!json.contains("\"messages\""));
    }

    #[test]
    fn test_delete_view_serialization_with_soft_delete() {
        let result = delete_view("item")
            .soft_deleted()
            .undo_command("restore")
            .build();

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"soft_deleted\":true"));
        assert!(json.contains("\"undo_command\":\"restore\""));
    }
}
