//! Traits for derive macro integration.
//!
//! These traits are implemented by the `#[derive(Tabular)]` and `#[derive(TabularRow)]`
//! macros to enable type-safe tabular formatting.

use super::TabularSpec;

/// Trait for types that can generate a `TabularSpec`.
///
/// This trait is automatically implemented by `#[derive(Tabular)]`.
///
/// # Example
///
/// ```ignore
/// use standout::tabular::{Tabular, TabularSpec};
/// use serde::Serialize;
///
/// #[derive(Serialize, Tabular)]
/// struct Task {
///     #[col(width = 8, style = "muted")]
///     id: String,
///
///     #[col(width = "fill")]
///     title: String,
///
///     #[col(width = 12, align = "right")]
///     status: String,
/// }
///
/// // Use the generated spec
/// let spec = Task::tabular_spec();
/// ```
pub trait Tabular {
    /// Returns the `TabularSpec` for this type.
    fn tabular_spec() -> TabularSpec;
}

/// Trait for types that can be converted to a row of strings.
///
/// This trait is automatically implemented by `#[derive(TabularRow)]`.
/// It provides optimized row extraction without JSON serialization.
///
/// # Example
///
/// ```ignore
/// use standout::tabular::TabularRow;
///
/// #[derive(TabularRow)]
/// struct Task {
///     id: String,
///     title: String,
///     status: String,
/// }
///
/// let task = Task {
///     id: "TSK-001".to_string(),
///     title: "Implement feature".to_string(),
///     status: "pending".to_string(),
/// };
///
/// let row: Vec<String> = task.to_row();
/// assert_eq!(row, vec!["TSK-001", "Implement feature", "pending"]);
/// ```
pub trait TabularRow {
    /// Converts this instance to a row of string values.
    fn to_row(&self) -> Vec<String>;
}

/// Trait for types that implement Display.
pub trait TabularFieldDisplay {
    fn to_tabular_cell(&self) -> String;
}

impl<T: std::fmt::Display> TabularFieldDisplay for T {
    fn to_tabular_cell(&self) -> String {
        self.to_string()
    }
}

/// Trait for Option types.
pub trait TabularFieldOption {
    fn to_tabular_cell(&self) -> String;
}

impl<T: std::fmt::Display> TabularFieldOption for Option<T> {
    fn to_tabular_cell(&self) -> String {
        match self {
            Some(v) => v.to_string(),
            None => String::new(),
        }
    }
}
