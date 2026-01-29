//! ListView result type and builder.
//!
//! ListView provides a standardized structure for displaying collections:
//! - Introduction text (optional header)
//! - Item list (the main content)
//! - Ending text (optional footer)
//! - Status messages (info, warnings, errors)
//!
//! # Rendering Modes
//!
//! The framework renders `ListViewResult` in three ways:
//!
//! 1. **Tabular mode** (default): When the item type implements `Tabular`,
//!    items are rendered as a formatted table. No template needed.
//!
//! 2. **Item template mode**: Provide an item template, and the framework
//!    iterates and renders each item.
//!
//! 3. **Full template override**: Provide a complete list template for
//!    total control.

use serde::Serialize;

use super::{Message, MessageLevel};

/// Result type for list view handlers.
///
/// This struct is serialized and passed to the list view template.
/// The framework-supplied `standout/list-view` template handles
/// rendering, or you can provide your own.
#[derive(Debug, Clone, Serialize)]
pub struct ListViewResult<T> {
    /// Items to display (post-filtering, post-ordering).
    pub items: Vec<T>,

    /// Text shown before the list (optional header).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intro: Option<String>,

    /// Text shown after the list (optional footer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ending: Option<String>,

    /// Status messages (info, warning, error).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,

    /// Total count before limit/offset (for "showing X of Y").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_count: Option<usize>,

    /// Applied filters summary (for "filtered by: ...").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_summary: Option<String>,
}

impl<T> ListViewResult<T> {
    /// Create a new list view result with just items.
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            intro: None,
            ending: None,
            messages: Vec::new(),
            total_count: None,
            filter_summary: None,
        }
    }

    /// Returns true if the item list is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns the number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

impl<T> Default for ListViewResult<T> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

/// Builder for constructing `ListViewResult` instances.
///
/// Use [`list_view()`] to start building:
///
/// ```rust
/// use standout::views::list_view;
///
/// let items = vec!["apple", "banana", "cherry"];
/// let result = list_view(items)
///     .intro("Available fruits:")
///     .build();
/// ```
#[derive(Debug)]
pub struct ListViewBuilder<T> {
    items: Vec<T>,
    intro: Option<String>,
    ending: Option<String>,
    messages: Vec<Message>,
    total_count: Option<usize>,
    filter_summary: Option<String>,
}

impl<T> ListViewBuilder<T> {
    /// Create a new builder with the given items.
    pub fn new(items: impl IntoIterator<Item = T>) -> Self {
        Self {
            items: items.into_iter().collect(),
            intro: None,
            ending: None,
            messages: Vec::new(),
            total_count: None,
            filter_summary: None,
        }
    }

    /// Set the introduction text shown before the list.
    pub fn intro(mut self, text: impl Into<String>) -> Self {
        self.intro = Some(text.into());
        self
    }

    /// Set the ending text shown after the list.
    pub fn ending(mut self, text: impl Into<String>) -> Self {
        self.ending = Some(text.into());
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

    /// Set the total count (before filtering/limiting).
    ///
    /// This enables "Showing X of Y" display when the list
    /// has been filtered or limited.
    pub fn total_count(mut self, count: usize) -> Self {
        self.total_count = Some(count);
        self
    }

    /// Set the filter summary text.
    ///
    /// This describes what filters were applied, e.g.,
    /// "status=pending, name contains 'auth'".
    pub fn filter_summary(mut self, summary: impl Into<String>) -> Self {
        self.filter_summary = Some(summary.into());
        self
    }

    /// Build the `ListViewResult`.
    pub fn build(self) -> ListViewResult<T> {
        ListViewResult {
            items: self.items,
            intro: self.intro,
            ending: self.ending,
            messages: self.messages,
            total_count: self.total_count,
            filter_summary: self.filter_summary,
        }
    }
}

/// Create a new list view builder with the given items.
///
/// This is the primary entry point for constructing `ListViewResult` instances.
///
/// # Examples
///
/// Basic usage:
///
/// ```rust
/// use standout::views::list_view;
///
/// let tasks = vec!["Task 1", "Task 2", "Task 3"];
/// let result = list_view(tasks).build();
/// assert_eq!(result.len(), 3);
/// ```
///
/// With all options:
///
/// ```rust
/// use standout::views::{list_view, MessageLevel};
///
/// let result = list_view(vec!["a", "b"])
///     .intro("Items:")
///     .ending("End of list")
///     .total_count(10)
///     .filter_summary("showing first 2")
///     .warning("Some items hidden")
///     .build();
///
/// assert_eq!(result.intro, Some("Items:".to_string()));
/// assert_eq!(result.total_count, Some(10));
/// ```
pub fn list_view<T>(items: impl IntoIterator<Item = T>) -> ListViewBuilder<T> {
    ListViewBuilder::new(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_view_builder_basic() {
        let result = list_view(vec![1, 2, 3]).build();
        assert_eq!(result.items, vec![1, 2, 3]);
        assert!(result.intro.is_none());
        assert!(result.ending.is_none());
        assert!(result.messages.is_empty());
    }

    #[test]
    fn test_list_view_builder_with_intro_ending() {
        let result = list_view(vec!["a", "b"])
            .intro("Header")
            .ending("Footer")
            .build();

        assert_eq!(result.intro, Some("Header".to_string()));
        assert_eq!(result.ending, Some("Footer".to_string()));
    }

    #[test]
    fn test_list_view_builder_with_messages() {
        let result = list_view(Vec::<i32>::new())
            .info("Info message")
            .warning("Warning message")
            .error("Error message")
            .build();

        assert_eq!(result.messages.len(), 3);
        assert_eq!(result.messages[0].level, MessageLevel::Info);
        assert_eq!(result.messages[1].level, MessageLevel::Warning);
        assert_eq!(result.messages[2].level, MessageLevel::Error);
    }

    #[test]
    fn test_list_view_builder_with_filter_info() {
        let result = list_view(vec![1, 2])
            .total_count(10)
            .filter_summary("status=active")
            .build();

        assert_eq!(result.total_count, Some(10));
        assert_eq!(result.filter_summary, Some("status=active".to_string()));
    }

    #[test]
    fn test_list_view_result_len_and_empty() {
        let empty: ListViewResult<i32> = list_view(vec![]).build();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        let non_empty = list_view(vec![1, 2, 3]).build();
        assert!(!non_empty.is_empty());
        assert_eq!(non_empty.len(), 3);
    }

    #[test]
    fn test_list_view_serialization() {
        let result = list_view(vec!["item1", "item2"])
            .intro("Header")
            .warning("Watch out")
            .build();

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"items\":[\"item1\",\"item2\"]"));
        assert!(json.contains("\"intro\":\"Header\""));
        assert!(json.contains("\"warning\""));
    }

    #[test]
    fn test_list_view_serialization_skips_empty() {
        let result = list_view(vec![1]).build();
        let json = serde_json::to_string(&result).unwrap();

        // Should not contain optional fields when empty/None
        assert!(!json.contains("\"intro\""));
        assert!(!json.contains("\"ending\""));
        assert!(!json.contains("\"messages\""));
        assert!(!json.contains("\"total_count\""));
        assert!(!json.contains("\"filter_summary\""));
    }

    #[test]
    fn test_list_view_default() {
        let result: ListViewResult<String> = ListViewResult::default();
        assert!(result.is_empty());
    }
}
