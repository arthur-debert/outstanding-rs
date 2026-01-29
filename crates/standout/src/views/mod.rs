//! View abstractions for standardized CLI output patterns.
//!
//! This module provides high-level view types that encode common CLI output patterns:
//! list views, detail views, and eventually full CRUD operations.
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

mod list_view;
mod message;

pub use list_view::{list_view, ListViewBuilder, ListViewResult};
pub use message::{Message, MessageLevel};
