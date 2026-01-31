//! UpdateView result type and builder.
//!
//! UpdateView provides a standardized structure for displaying the result
//! of an update operation:
//! - The item state before and after the update
//! - List of changed fields
//! - Dry-run mode indication
//! - Validation errors (if any)
//! - Status messages
//!
//! # Example
//!
//! ```rust
//! use standout::views::update_view;
//!
//! #[derive(Clone, serde::Serialize)]
//! struct Task {
//!     id: String,
//!     title: String,
//!     status: String,
//! }
//!
//! let before = Task {
//!     id: "task-1".to_string(),
//!     title: "Old title".to_string(),
//!     status: "pending".to_string(),
//! };
//!
//! let after = Task {
//!     id: "task-1".to_string(),
//!     title: "New title".to_string(),
//!     status: "pending".to_string(),
//! };
//!
//! let result = update_view(after)
//!     .before(before)
//!     .changed_field("title")
//!     .success("Task updated successfully")
//!     .build();
//! ```

use serde::Serialize;

use super::create_view::ValidationError;
use super::{Message, MessageLevel};

/// Result type for update view handlers.
///
/// This struct is serialized and passed to the update view template.
/// The framework-supplied `standout/update-view` template handles
/// rendering, or you can provide your own.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateViewResult<T> {
    /// The item state before the update (for diff display).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<T>,

    /// The item state after the update.
    pub after: T,

    /// List of field names that were changed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changed_fields: Vec<String>,

    /// Whether this was a dry-run (no actual update).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,

    /// Validation errors that occurred.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub validation_errors: Vec<ValidationError>,

    /// Status messages (info, warning, error).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
}

impl<T> UpdateViewResult<T> {
    /// Create a new update view result with just the after state.
    pub fn new(after: T) -> Self {
        Self {
            before: None,
            after,
            changed_fields: Vec::new(),
            dry_run: false,
            validation_errors: Vec::new(),
            messages: Vec::new(),
        }
    }

    /// Returns true if this was a dry-run.
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Returns true if there are validation errors.
    pub fn has_validation_errors(&self) -> bool {
        !self.validation_errors.is_empty()
    }

    /// Returns true if the update was valid (no validation errors).
    pub fn is_valid(&self) -> bool {
        self.validation_errors.is_empty()
    }

    /// Returns true if any fields were changed.
    pub fn has_changes(&self) -> bool {
        !self.changed_fields.is_empty()
    }

    /// Returns true if the before state is available.
    pub fn has_before(&self) -> bool {
        self.before.is_some()
    }
}

/// Builder for constructing `UpdateViewResult` instances.
///
/// Use [`update_view()`] to start building:
///
/// ```rust
/// use standout::views::update_view;
///
/// let after = serde_json::json!({"id": 1, "name": "Updated"});
/// let result = update_view(after)
///     .changed_field("name")
///     .success("Updated successfully")
///     .build();
/// ```
#[derive(Debug)]
pub struct UpdateViewBuilder<T> {
    before: Option<T>,
    after: T,
    changed_fields: Vec<String>,
    dry_run: bool,
    validation_errors: Vec<ValidationError>,
    messages: Vec<Message>,
}

impl<T> UpdateViewBuilder<T> {
    /// Create a new builder with the after state.
    pub fn new(after: T) -> Self {
        Self {
            before: None,
            after,
            changed_fields: Vec::new(),
            dry_run: false,
            validation_errors: Vec::new(),
            messages: Vec::new(),
        }
    }

    /// Set the before state for diff display.
    pub fn before(mut self, before: T) -> Self {
        self.before = Some(before);
        self
    }

    /// Add a changed field name.
    pub fn changed_field(mut self, field: impl Into<String>) -> Self {
        self.changed_fields.push(field.into());
        self
    }

    /// Add multiple changed field names.
    pub fn changed_fields(mut self, fields: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.changed_fields
            .extend(fields.into_iter().map(Into::into));
        self
    }

    /// Mark this as a dry-run (no actual update performed).
    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// Add a validation error.
    pub fn validation_error(
        mut self,
        field: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        self.validation_errors
            .push(ValidationError::new(field, message));
        self
    }

    /// Add multiple validation errors.
    pub fn validation_errors(mut self, errors: Vec<ValidationError>) -> Self {
        self.validation_errors.extend(errors);
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

    /// Build the `UpdateViewResult`.
    pub fn build(self) -> UpdateViewResult<T> {
        UpdateViewResult {
            before: self.before,
            after: self.after,
            changed_fields: self.changed_fields,
            dry_run: self.dry_run,
            validation_errors: self.validation_errors,
            messages: self.messages,
        }
    }
}

/// Create a new update view builder with the after state.
///
/// This is the primary entry point for constructing `UpdateViewResult` instances.
///
/// # Examples
///
/// Basic usage:
///
/// ```rust
/// use standout::views::update_view;
///
/// let after = serde_json::json!({"id": "t1", "title": "Updated"});
/// let result = update_view(after)
///     .changed_field("title")
///     .success("Task updated")
///     .build();
/// assert!(result.has_changes());
/// ```
///
/// With before/after comparison:
///
/// ```rust
/// use standout::views::update_view;
///
/// let before = serde_json::json!({"id": "t1", "title": "Old"});
/// let after = serde_json::json!({"id": "t1", "title": "New"});
/// let result = update_view(after)
///     .before(before)
///     .changed_field("title")
///     .build();
/// assert!(result.has_before());
/// ```
///
/// Dry-run mode:
///
/// ```rust
/// use standout::views::update_view;
///
/// let after = serde_json::json!({"id": "t1", "title": "Would be updated"});
/// let result = update_view(after)
///     .dry_run()
///     .changed_field("title")
///     .info("Would update these fields")
///     .build();
/// assert!(result.is_dry_run());
/// ```
pub fn update_view<T>(after: T) -> UpdateViewBuilder<T> {
    UpdateViewBuilder::new(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_view_builder_basic() {
        let result = update_view("after").build();
        assert_eq!(result.after, "after");
        assert!(result.before.is_none());
        assert!(result.changed_fields.is_empty());
        assert!(!result.dry_run);
        assert!(result.validation_errors.is_empty());
        assert!(result.messages.is_empty());
        assert!(result.is_valid());
        assert!(!result.has_changes());
    }

    #[test]
    fn test_update_view_builder_with_before() {
        let result = update_view("after").before("before").build();
        assert!(result.has_before());
        assert_eq!(result.before, Some("before"));
    }

    #[test]
    fn test_update_view_builder_with_changed_fields() {
        let result = update_view("after")
            .changed_field("title")
            .changed_field("status")
            .build();

        assert!(result.has_changes());
        assert_eq!(result.changed_fields, vec!["title", "status"]);
    }

    #[test]
    fn test_update_view_builder_with_multiple_changed_fields() {
        let result = update_view("after").changed_fields(["a", "b", "c"]).build();

        assert_eq!(result.changed_fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_update_view_builder_dry_run() {
        let result = update_view("after").dry_run().build();
        assert!(result.is_dry_run());
    }

    #[test]
    fn test_update_view_builder_with_validation_errors() {
        let result = update_view("after")
            .validation_error("title", "Title is required")
            .validation_error("status", "Invalid status")
            .build();

        assert!(result.has_validation_errors());
        assert!(!result.is_valid());
        assert_eq!(result.validation_errors.len(), 2);
    }

    #[test]
    fn test_update_view_builder_with_messages() {
        let result = update_view("after")
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
    fn test_update_view_serialization() {
        let result = update_view(serde_json::json!({"id": 1}))
            .changed_field("name")
            .success("Updated")
            .build();

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"after\":{\"id\":1}"));
        assert!(json.contains("\"changed_fields\":[\"name\"]"));
    }

    #[test]
    fn test_update_view_serialization_skips_empty() {
        let result = update_view("after").build();
        let json = serde_json::to_string(&result).unwrap();

        // Should not contain optional fields when empty/false
        assert!(!json.contains("\"before\""));
        assert!(!json.contains("\"changed_fields\""));
        assert!(!json.contains("\"dry_run\""));
        assert!(!json.contains("\"validation_errors\""));
        assert!(!json.contains("\"messages\""));
    }

    #[test]
    fn test_update_view_with_before_serialization() {
        let result = update_view("after").before("before").build();
        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains("\"before\":\"before\""));
        assert!(json.contains("\"after\":\"after\""));
    }
}
