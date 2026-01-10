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
//! - MiniJinja template filters for declarative formatting
//!
//! # Quick Start
//!
//! ## Using TableFormatter (Imperative)
//!
//! ```rust
//! use outstanding::table::{TableSpec, Column, Width, Align, TableFormatter};
//!
//! let spec = TableSpec::builder()
//!     .column(Column::new(Width::Fixed(8)))
//!     .column(Column::new(Width::Fill))
//!     .column(Column::new(Width::Fixed(10)).align(Align::Right))
//!     .separator("  ")
//!     .build();
//!
//! let formatter = TableFormatter::new(&spec, 80);
//!
//! // Format rows one at a time (enables interleaved output)
//! let row1 = formatter.format_row(&["abc123", "path/to/file.rs", "pending"]);
//! let row2 = formatter.format_row(&["def456", "src/lib.rs", "done"]);
//! ```
//!
//! ## Using Template Filters (Declarative)
//!
//! Template filters are automatically available when using outstanding's
//! render functions:
//!
//! ```jinja
//! {% for entry in entries %}
//! {{ entry.hash | col(7) }}  {{ entry.author | col(12) }}  {{ entry.message | col(50) }}
//! {% endfor %}
//! ```
//!
//! ## Utility Functions
//!
//! ```rust
//! use outstanding::table::{display_width, truncate_end, pad_right};
//!
//! let text = "Hello World";
//! let truncated = truncate_end(text, 8, "…");  // "Hello W…"
//! let padded = pad_right(&truncated, 10);      // "Hello W…  "
//! assert_eq!(display_width(&padded), 10);
//! ```
//!
//! # Column Width Strategies
//!
//! - [`Width::Fixed(n)`] - Exactly n display columns
//! - [`Width::Bounded { min, max }`] - Auto-calculate from content within bounds
//! - [`Width::Fill`] - Expand to fill remaining space (one per table)
//!
//! # Truncation Modes
//!
//! - [`TruncateAt::End`] - Keep start, truncate end: "Hello W…"
//! - [`TruncateAt::Start`] - Keep end, truncate start: "…o World"
//! - [`TruncateAt::Middle`] - Keep both ends: "Hel…orld"
//!
//! # Template Filters
//!
//! | Filter | Usage |
//! |--------|-------|
//! | `col` | `{{ value \| col(10) }}` or `{{ value \| col(10, align='right', truncate='middle') }}` |
//! | `pad_left` | `{{ value \| pad_left(10) }}` |
//! | `pad_right` | `{{ value \| pad_right(10) }}` |
//! | `pad_center` | `{{ value \| pad_center(10) }}` |
//! | `truncate_at` | `{{ value \| truncate_at(10, 'middle', '...') }}` |
//! | `display_width` | `{{ value \| display_width }}` |

pub mod filters;
mod formatter;
mod resolve;
mod types;
mod util;

// Re-export types
pub use formatter::TableFormatter;
pub use resolve::ResolvedWidths;
pub use types::{
    Align, Column, ColumnBuilder, Decorations, TableSpec, TableSpecBuilder, TruncateAt, Width,
};

// Re-export utility functions
pub use util::{
    display_width, pad_center, pad_left, pad_right, truncate_end, truncate_middle, truncate_start,
};
