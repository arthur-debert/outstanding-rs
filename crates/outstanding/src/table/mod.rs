//! Tabular and columnar output support.
//!
//! This module provides utilities for creating aligned, column-based output
//! in terminal applications. It supports:
//!
//! - ANSI-aware text measurement and manipulation
//! - Multiple truncation strategies (start, middle, end)
//! - Flexible padding and alignment
//! - Configurable column widths (fixed, bounded, fill)
//! - Row-by-row formatting for interleaved output
//!
//! # Example
//!
//! ```rust
//! use outstanding::table::{
//!     display_width, truncate_end, pad_right,
//!     TableSpec, Column, Width, Align,
//! };
//!
//! // Basic text manipulation
//! let text = "Hello World";
//! let truncated = truncate_end(text, 8, "…");  // "Hello W…"
//! let padded = pad_right(&truncated, 10);      // "Hello W…  "
//! assert_eq!(display_width(&padded), 10);
//!
//! // Table specification
//! let spec = TableSpec::builder()
//!     .column(Column::new(Width::Fixed(8)))
//!     .column(Column::new(Width::Fill).align(Align::Right))
//!     .separator("  ")
//!     .build();
//! ```

mod types;
mod util;

// Re-export types
pub use types::{
    Align, Column, ColumnBuilder, Decorations, TableSpec, TableSpecBuilder, TruncateAt, Width,
};

// Re-export utility functions
pub use util::{
    display_width, pad_center, pad_left, pad_right, truncate_end, truncate_middle, truncate_start,
};
