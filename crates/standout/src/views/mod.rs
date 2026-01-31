//! View abstractions for standardized CLI output patterns.
//!
//! This module provides high-level view types that encode common CLI output patterns:
//! list views, detail views, and full Resource operations.
//!
//! # ListView
//!
//! The most common pattern - displaying a collection of items:
//!
//! ```rust
//! use standout::views::{list_view, Message, MessageLevel};
//!
//! # fn load_tasks() -> Vec<String> { vec![] }
//! let tasks = load_tasks();
//! let result = list_view(tasks)
//!     .intro("Your tasks:")
//!     .ending("Use 'task add' to create more")
//!     .message(MessageLevel::Warning, "2 tasks are overdue")
//!     .build();
//! ```
//!
//! When combined with the `#[derive(Tabular)]` macro on your item type,
//! the framework renders items as a formatted table with zero template code.
//!
//! # Resource Views
//!
//! For object-centric CLI patterns, use the Resource view types:
//!
//! - [`DetailViewResult`] - Display a single item with related data and actions
//! - [`CreateViewResult`] - Display the result of a create operation
//! - [`UpdateViewResult`] - Display before/after state of an update operation
//! - [`DeleteViewResult`] - Display delete confirmation with undo support

mod create_view;
mod delete_view;
mod detail_view;
mod list_view;
mod message;
mod update_view;

pub use create_view::{create_view, CreateViewBuilder, CreateViewResult, ValidationError};
pub use delete_view::{delete_view, DeleteViewBuilder, DeleteViewResult};
pub use detail_view::{detail_view, ActionSuggestion, DetailViewBuilder, DetailViewResult};
pub use list_view::{list_view, ListViewBuilder, ListViewResult};
pub use message::{Message, MessageLevel};
pub use update_view::{update_view, UpdateViewBuilder, UpdateViewResult};
