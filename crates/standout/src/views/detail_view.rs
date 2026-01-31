//! DetailView result type and builder.
//!
//! DetailView provides a standardized structure for displaying a single object's details:
//! - The main item data
//! - Title and subtitle for header display
//! - Related items (e.g., linked entities)
//! - Suggested actions
//! - Status messages
//!
//! # Example
//!
//! ```rust
//! use standout::views::detail_view;
//!
//! #[derive(serde::Serialize)]
//! struct Task {
//!     id: String,
//!     title: String,
//!     status: String,
//! }
//!
//! let task = Task {
//!     id: "task-1".to_string(),
//!     title: "Implement feature".to_string(),
//!     status: "in_progress".to_string(),
//! };
//!
//! let result = detail_view(task)
//!     .title("Task Details")
//!     .subtitle("task-1")
//!     .action("Edit", "task update task-1")
//!     .action("Delete", "task delete task-1")
//!     .build();
//! ```

use serde::Serialize;
use std::collections::HashMap;

use super::{Message, MessageLevel};

/// A suggested action that can be taken on the displayed item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionSuggestion {
    /// Display label for the action (e.g., "Edit", "Delete")
    pub label: String,
    /// Command to execute (e.g., "task update task-1")
    pub command: String,
}

impl ActionSuggestion {
    /// Create a new action suggestion.
    pub fn new(label: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            command: command.into(),
        }
    }
}

/// Result type for detail view handlers.
///
/// This struct is serialized and passed to the detail view template.
/// The framework-supplied `standout/detail-view` template handles
/// rendering, or you can provide your own.
#[derive(Debug, Clone, Serialize)]
pub struct DetailViewResult<T> {
    /// The item to display.
    pub item: T,

    /// Optional title for the view header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Optional subtitle (e.g., item ID or type).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,

    /// Related items keyed by relationship name.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub related: HashMap<String, serde_json::Value>,

    /// Suggested actions for this item.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionSuggestion>,

    /// Status messages (info, warning, error).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
}

impl<T> DetailViewResult<T> {
    /// Create a new detail view result with just the item.
    pub fn new(item: T) -> Self {
        Self {
            item,
            title: None,
            subtitle: None,
            related: HashMap::new(),
            actions: Vec::new(),
            messages: Vec::new(),
        }
    }

    /// Returns true if there are any suggested actions.
    pub fn has_actions(&self) -> bool {
        !self.actions.is_empty()
    }

    /// Returns true if there are any related items.
    pub fn has_related(&self) -> bool {
        !self.related.is_empty()
    }
}

/// Builder for constructing `DetailViewResult` instances.
///
/// Use [`detail_view()`] to start building:
///
/// ```rust
/// use standout::views::detail_view;
///
/// let item = serde_json::json!({"id": 1, "name": "Test"});
/// let result = detail_view(item)
///     .title("Item Details")
///     .build();
/// ```
#[derive(Debug)]
pub struct DetailViewBuilder<T> {
    item: T,
    title: Option<String>,
    subtitle: Option<String>,
    related: HashMap<String, serde_json::Value>,
    actions: Vec<ActionSuggestion>,
    messages: Vec<Message>,
}

impl<T> DetailViewBuilder<T> {
    /// Create a new builder with the given item.
    pub fn new(item: T) -> Self {
        Self {
            item,
            title: None,
            subtitle: None,
            related: HashMap::new(),
            actions: Vec::new(),
            messages: Vec::new(),
        }
    }

    /// Set the title shown in the view header.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the subtitle shown below the title.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Add a related item.
    ///
    /// Related items are displayed in a separate section keyed by their
    /// relationship name (e.g., "author", "assignees", "parent_task").
    pub fn related(mut self, name: impl Into<String>, value: impl Serialize) -> Self {
        let json_value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
        self.related.insert(name.into(), json_value);
        self
    }

    /// Add a suggested action.
    pub fn action(mut self, label: impl Into<String>, command: impl Into<String>) -> Self {
        self.actions.push(ActionSuggestion::new(label, command));
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

    /// Build the `DetailViewResult`.
    pub fn build(self) -> DetailViewResult<T> {
        DetailViewResult {
            item: self.item,
            title: self.title,
            subtitle: self.subtitle,
            related: self.related,
            actions: self.actions,
            messages: self.messages,
        }
    }
}

/// Create a new detail view builder with the given item.
///
/// This is the primary entry point for constructing `DetailViewResult` instances.
///
/// # Examples
///
/// Basic usage:
///
/// ```rust
/// use standout::views::detail_view;
///
/// let task = serde_json::json!({"id": "t1", "title": "Test"});
/// let result = detail_view(task).build();
/// ```
///
/// With all options:
///
/// ```rust
/// use standout::views::{detail_view, MessageLevel};
///
/// let task = serde_json::json!({"id": "t1", "title": "Test"});
/// let result = detail_view(task)
///     .title("Task Details")
///     .subtitle("t1")
///     .related("author", serde_json::json!({"name": "Alice"}))
///     .action("Edit", "task update t1")
///     .action("Delete", "task delete t1")
///     .warning("Task is overdue")
///     .build();
/// ```
pub fn detail_view<T>(item: T) -> DetailViewBuilder<T> {
    DetailViewBuilder::new(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detail_view_builder_basic() {
        let result = detail_view("test item").build();
        assert_eq!(result.item, "test item");
        assert!(result.title.is_none());
        assert!(result.subtitle.is_none());
        assert!(result.related.is_empty());
        assert!(result.actions.is_empty());
        assert!(result.messages.is_empty());
    }

    #[test]
    fn test_detail_view_builder_with_title_subtitle() {
        let result = detail_view("item")
            .title("Details")
            .subtitle("ID: 123")
            .build();

        assert_eq!(result.title, Some("Details".to_string()));
        assert_eq!(result.subtitle, Some("ID: 123".to_string()));
    }

    #[test]
    fn test_detail_view_builder_with_related() {
        let result = detail_view("item")
            .related("author", serde_json::json!({"name": "Alice"}))
            .related("tags", vec!["rust", "cli"])
            .build();

        assert!(result.has_related());
        assert_eq!(result.related.len(), 2);
        assert!(result.related.contains_key("author"));
        assert!(result.related.contains_key("tags"));
    }

    #[test]
    fn test_detail_view_builder_with_actions() {
        let result = detail_view("item")
            .action("Edit", "item edit 1")
            .action("Delete", "item delete 1")
            .build();

        assert!(result.has_actions());
        assert_eq!(result.actions.len(), 2);
        assert_eq!(result.actions[0].label, "Edit");
        assert_eq!(result.actions[0].command, "item edit 1");
    }

    #[test]
    fn test_detail_view_builder_with_messages() {
        let result = detail_view("item")
            .info("Info message")
            .success("Success message")
            .warning("Warning message")
            .error("Error message")
            .build();

        assert_eq!(result.messages.len(), 4);
        assert_eq!(result.messages[0].level, MessageLevel::Info);
        assert_eq!(result.messages[1].level, MessageLevel::Success);
        assert_eq!(result.messages[2].level, MessageLevel::Warning);
        assert_eq!(result.messages[3].level, MessageLevel::Error);
    }

    #[test]
    fn test_detail_view_serialization() {
        let result = detail_view(serde_json::json!({"id": 1}))
            .title("Test")
            .build();

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"item\":{\"id\":1}"));
        assert!(json.contains("\"title\":\"Test\""));
    }

    #[test]
    fn test_detail_view_serialization_skips_empty() {
        let result = detail_view("item").build();
        let json = serde_json::to_string(&result).unwrap();

        // Should not contain optional fields when empty/None
        assert!(!json.contains("\"title\""));
        assert!(!json.contains("\"subtitle\""));
        assert!(!json.contains("\"related\""));
        assert!(!json.contains("\"actions\""));
        assert!(!json.contains("\"messages\""));
    }

    #[test]
    fn test_action_suggestion() {
        let action = ActionSuggestion::new("Edit", "item edit 1");
        assert_eq!(action.label, "Edit");
        assert_eq!(action.command, "item edit 1");
    }
}
