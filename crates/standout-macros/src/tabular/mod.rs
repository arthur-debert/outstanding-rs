//! Derive macros for tabular layout configuration.
//!
//! This module provides derive macros that generate tabular specifications
//! from struct field annotations, eliminating boilerplate and enabling
//! type-safe column definitions.
//!
//! # Available Macros
//!
//! - [`Tabular`] - Generate a `TabularSpec` from struct field annotations
//! - [`TabularRow`] - Generate optimized row extraction without JSON serialization
//!
//! # Example
//!
//! ```ignore
//! use standout::tabular::{Tabular, TabularRow, TabularSpec};
//! use serde::Serialize;
//!
//! #[derive(Serialize, Tabular, TabularRow)]
//! #[tabular(separator = " │ ")]
//! struct Task {
//!     #[col(width = 8, style = "muted")]
//!     id: String,
//!
//!     #[col(width = "fill", overflow = "wrap")]
//!     title: String,
//!
//!     #[col(width = 12, align = "right")]
//!     status: String,
//! }
//!
//! // Generated: Task::tabular_spec() -> TabularSpec
//! // Generated: impl TabularRow for Task { fn to_row(&self) -> Vec<String> }
//! ```

mod attrs;
mod derive_row;
mod derive_tabular;

pub use derive_row::tabular_row_derive_impl;
pub use derive_tabular::tabular_derive_impl;
