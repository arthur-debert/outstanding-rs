//! Unicode-aware column formatting for terminal tables.
//!
//! This module provides utilities for aligned, column-based terminal output
//! that correctly handles Unicode (CJK characters count as 2 columns) and
//! preserves ANSI escape codes without counting them toward width.
//!
//! ## TabularFormatter vs Template Filters
//!
//! Two approaches, choose based on your needs:
//!
//! | Approach | Use When |
//! |----------|----------|
//! | Template filters (`col`, `pad_left`) | Simple tables, column widths known at template time |
//! | TabularFormatter | Dynamic widths, CSV export, complex specs with data extraction |
//!
//! Template filters are simpler for most cases. Use TabularFormatter when you
//! need width resolution from actual data or structured CSV export.
//!
//! ## Template Filters (Declarative)
//!
//! Filters are available in all Standout templates:
//!
//! ```jinja
//! {% for entry in entries %}
//! {{ entry.hash | col(7) }}  {{ entry.author | col(12) }}  {{ entry.message | col(50) }}
//! {% endfor %}
//! ```
//!
//! | Filter | Usage |
//! |--------|-------|
//! | `col` | `{{ value \| col(10) }}` or `{{ value \| col(10, align='right', truncate='middle') }}` |
//! | `pad_left` | `{{ value \| pad_left(10) }}` |
//! | `pad_right` | `{{ value \| pad_right(10) }}` |
//! | `pad_center` | `{{ value \| pad_center(10) }}` |
//! | `truncate_at` | `{{ value \| truncate_at(10, 'middle', '...') }}` |
//! | `display_width` | `{{ value \| display_width }}` |
//!
//! ## TabularFormatter (Imperative)
//!
//! For programmatic control and CSV export:
//!
//! ```rust
//! use standout_render::tabular::{FlatDataSpec, Column, Width, Align, TabularFormatter};
//!
//! let spec = FlatDataSpec::builder()
//!     .column(Column::new(Width::Fixed(8)))
//!     .column(Column::new(Width::Fill))
//!     .column(Column::new(Width::Fixed(10)).align(Align::Right))
//!     .separator("  ")
//!     .build();
//!
//! let formatter = TabularFormatter::new(&spec, 80);
//! let row = formatter.format_row(&["abc123", "path/to/file.rs", "pending"]);
//! ```
//!
//! ## Width Strategies
//!
//! - [`Width::Fixed(n)`] - Exactly n display columns
//! - [`Width::Bounded { min, max }`] - Auto-size within bounds based on content
//! - [`Width::Fill`] - Expand to fill remaining space
//!
//! ## Truncation Modes
//!
//! - [`TruncateAt::End`] - Keep start: "Hello W…"
//! - [`TruncateAt::Start`] - Keep end: "…o World"
//! - [`TruncateAt::Middle`] - Keep both: "Hel…orld" (useful for paths)
//!
//! ## Sub-Columns
//!
//! Columns can contain inner sub-columns for per-row layout within a parent
//! column. Exactly one sub-column is [`Width::Fill`] (the grower); the rest
//! are Fixed or Bounded. Widths are resolved per-row from actual content.
//!
//! ```rust
//! use standout_render::tabular::{
//!     TabularSpec, Col, SubCol, SubColumns, TabularFormatter, CellValue,
//! };
//!
//! let spec = TabularSpec::builder()
//!     .column(Col::fixed(4))
//!     .column(Col::fill().sub_columns(
//!         SubColumns::new(
//!             vec![SubCol::fill(), SubCol::bounded(0, 30).right()],
//!             " ",
//!         ).unwrap(),
//!     ))
//!     .separator("  ")
//!     .build();
//!
//! let formatter = TabularFormatter::new(&spec, 60);
//! let row = formatter.format_row_cells(&[
//!     CellValue::Single("1."),
//!     CellValue::Sub(vec!["Title", "[tag]"]),
//! ]);
//! ```
//!
//! ## Utility Functions
//!
//! ```rust
//! use standout_render::tabular::{display_width, truncate_end, pad_right, wrap};
//!
//! let text = "Hello World";
//! let truncated = truncate_end(text, 8, "…");  // "Hello W…"
//! let padded = pad_right(&truncated, 10);      // "Hello W…  "
//! assert_eq!(display_width(&padded), 10);
//!
//! // Word-wrap long text
//! let lines = wrap("hello world foo bar", 11);
//! assert_eq!(lines, vec!["hello world", "foo bar"]);
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

mod decorator;
pub mod filters;
mod formatter;
mod resolve;
mod traits;
mod types;
mod util;

// Re-export types
pub use decorator::{BorderStyle, Table};
pub use formatter::{CellOutput, CellValue, TabularFormatter};
pub use resolve::ResolvedWidths;
pub use traits::{Tabular, TabularFieldDisplay, TabularFieldOption, TabularRow};

// Note: Tabular and TabularRow derive macros are re-exported from the main `standout` crate
// when the "macros" feature is enabled.
pub use types::{
    Align, Anchor, Col, Column, ColumnBuilder, Decorations, FlatDataSpec, FlatDataSpecBuilder,
    Overflow, SubCol, SubColumn, SubColumns, TabularSpec, TabularSpecBuilder, TruncateAt, Width,
};

// Re-export utility functions
pub use util::{
    display_width, display_width_with_policy, pad_center, pad_center_with_policy, pad_left,
    pad_left_with_policy, pad_right, pad_right_with_policy, truncate_end, truncate_end_with_policy,
    truncate_middle, truncate_middle_with_policy, truncate_start, truncate_start_with_policy,
    visible_width, visible_width_with_policy, wrap, wrap_indent, wrap_indent_with_policy,
    wrap_with_policy,
};
