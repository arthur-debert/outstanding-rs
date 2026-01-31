//! CreateView result type and builder.
//!
//! CreateView provides a standardized structure for displaying the result
//! of a create operation:
//! - The newly created item
//! - Dry-run mode indication
//! - Validation errors (if any)
//! - Status messages
//!
//! # Example
//!
//! ```rust
//! use standout::views::create_view;
//!
//! #[derive(serde::Serialize)]
//! struct Task {
//!     id: String,
//!     title: String,
//! }
//!
//! let task = Task {
//!     id: "task-1".to_string(),
//!     title: "New feature".to_string(),
//! };
//!
//! let result = create_view(task)
//!     .success("Task created successfully")
//!     .build();
//! ```

use serde::Serialize;

use super::{Message, MessageLevel};

/// A validation error for a specific field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationError {
    /// The field that failed validation (e.g., "title", "email").
    pub field: String,
    /// The error message describing the validation failure.
    pub message: String,
}

impl ValidationError {
    /// Create a new validation error.
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Result type for create view handlers.
///
/// This struct is serialized and passed to the create view template.
/// The framework-supplied `standout/create-view` template handles
/// rendering, or you can provide your own.
#[derive(Debug, Clone, Serialize)]
pub struct CreateViewResult<T> {
    /// The newly created item.
    pub item: T,

    /// Whether this was a dry-run (no actual creation).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,

    /// Validation errors that occurred.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub validation_errors: Vec<ValidationError>,

    /// Status messages (info, warning, error).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
}

impl<T> CreateViewResult<T> {
    /// Create a new create view result with just the item.
    pub fn new(item: T) -> Self {
        Self {
            item,
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

    /// Returns true if the creation was successful (no validation errors).
    pub fn is_valid(&self) -> bool {
        self.validation_errors.is_empty()
    }
}

/// Builder for constructing `CreateViewResult` instances.
///
/// Use [`create_view()`] to start building:
///
/// ```rust
/// use standout::views::create_view;
///
/// let item = serde_json::json!({"id": 1, "name": "Test"});
/// let result = create_view(item)
///     .success("Created successfully")
///     .build();
/// ```
#[derive(Debug)]
pub struct CreateViewBuilder<T> {
    item: T,
    dry_run: bool,
    validation_errors: Vec<ValidationError>,
    messages: Vec<Message>,
}

impl<T> CreateViewBuilder<T> {
    /// Create a new builder with the given item.
    pub fn new(item: T) -> Self {
        Self {
            item,
            dry_run: false,
            validation_errors: Vec::new(),
            messages: Vec::new(),
        }
    }

    /// Mark this as a dry-run (no actual creation performed).
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

    /// Build the `CreateViewResult`.
    pub fn build(self) -> CreateViewResult<T> {
        CreateViewResult {
            item: self.item,
            dry_run: self.dry_run,
            validation_errors: self.validation_errors,
            messages: self.messages,
        }
    }
}

/// Create a new create view builder with the given item.
///
/// This is the primary entry point for constructing `CreateViewResult` instances.
///
/// # Examples
///
/// Basic usage:
///
/// ```rust
/// use standout::views::create_view;
///
/// let task = serde_json::json!({"id": "t1", "title": "Test"});
/// let result = create_view(task)
///     .success("Task created")
///     .build();
/// assert!(result.is_valid());
/// ```
///
/// With validation errors:
///
/// ```rust
/// use standout::views::create_view;
///
/// let partial = serde_json::json!({"title": ""});
/// let result = create_view(partial)
///     .validation_error("title", "Title cannot be empty")
///     .build();
/// assert!(!result.is_valid());
/// assert!(result.has_validation_errors());
/// ```
///
/// Dry-run mode:
///
/// ```rust
/// use standout::views::create_view;
///
/// let task = serde_json::json!({"id": "t1", "title": "Test"});
/// let result = create_view(task)
///     .dry_run()
///     .info("Would create task with these values")
///     .build();
/// assert!(result.is_dry_run());
/// ```
pub fn create_view<T>(item: T) -> CreateViewBuilder<T> {
    CreateViewBuilder::new(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_view_builder_basic() {
        let result = create_view("item").build();
        assert_eq!(result.item, "item");
        assert!(!result.dry_run);
        assert!(result.validation_errors.is_empty());
        assert!(result.messages.is_empty());
        assert!(result.is_valid());
    }

    #[test]
    fn test_create_view_builder_dry_run() {
        let result = create_view("item").dry_run().build();
        assert!(result.is_dry_run());
    }

    #[test]
    fn test_create_view_builder_with_validation_errors() {
        let result = create_view("item")
            .validation_error("title", "Title is required")
            .validation_error("email", "Invalid email format")
            .build();

        assert!(result.has_validation_errors());
        assert!(!result.is_valid());
        assert_eq!(result.validation_errors.len(), 2);
        assert_eq!(result.validation_errors[0].field, "title");
        assert_eq!(result.validation_errors[1].field, "email");
    }

    #[test]
    fn test_create_view_builder_with_multiple_validation_errors() {
        let errors = vec![
            ValidationError::new("a", "Error A"),
            ValidationError::new("b", "Error B"),
        ];
        let result = create_view("item").validation_errors(errors).build();

        assert_eq!(result.validation_errors.len(), 2);
    }

    #[test]
    fn test_create_view_builder_with_messages() {
        let result = create_view("item")
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
    fn test_create_view_serialization() {
        let result = create_view(serde_json::json!({"id": 1}))
            .success("Created")
            .build();

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"item\":{\"id\":1}"));
        assert!(json.contains("\"messages\""));
    }

    #[test]
    fn test_create_view_serialization_skips_empty() {
        let result = create_view("item").build();
        let json = serde_json::to_string(&result).unwrap();

        // Should not contain optional fields when empty/false
        assert!(!json.contains("\"dry_run\""));
        assert!(!json.contains("\"validation_errors\""));
        assert!(!json.contains("\"messages\""));
    }

    #[test]
    fn test_create_view_dry_run_serialization() {
        let result = create_view("item").dry_run().build();
        let json = serde_json::to_string(&result).unwrap();

        // dry_run should be present when true
        assert!(json.contains("\"dry_run\":true"));
    }

    #[test]
    fn test_validation_error() {
        let error = ValidationError::new("email", "Invalid format");
        assert_eq!(error.field, "email");
        assert_eq!(error.message, "Invalid format");
    }
}
