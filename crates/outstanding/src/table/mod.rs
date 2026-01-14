//! Unicode-aware column formatting for terminal tables.
//!
//! This module provides utilities for aligned, column-based terminal output
//! that correctly handles Unicode (CJK characters count as 2 columns) and
//! preserves ANSI escape codes without counting them toward width.
//!
//! ## TableFormatter vs Template Filters
//!
//! Two approaches, choose based on your needs:
//!
//! | Approach | Use When |
//! |----------|----------|
//! | **Template filters** (`col`, `pad_left`) | Simple tables, column widths known at template time |
//! | **TableFormatter** | Dynamic widths, CSV export, complex specs with data extraction |
//!
//! Template filters are simpler for most cases. Use TableFormatter when you
//! need width resolution from actual data or structured CSV export.
//!
//! ## Template Filters (Declarative)
//!
//! Filters are available in all Outstanding templates:
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
//! ## TableFormatter (Imperative)
//!
//! For programmatic control and CSV export:
//!
//! ```rust
//! use outstanding::table::{FlatDataSpec, Column, Width, Align, TableFormatter};
//!
//! let spec = FlatDataSpec::builder()
//!     .column(Column::new(Width::Fixed(8)))
//!     .column(Column::new(Width::Fill))
//!     .column(Column::new(Width::Fixed(10)).align(Align::Right))
//!     .separator("  ")
//!     .build();
//!
//! let formatter = TableFormatter::new(&spec, 80);
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
    Align, Column, ColumnBuilder, Decorations, FlatDataSpec, FlatDataSpecBuilder, TableSpec,
    TableSpecBuilder, TruncateAt, Width,
};

// Re-export utility functions
pub use util::{
    display_width, pad_center, pad_left, pad_right, truncate_end, truncate_middle, truncate_start,
};
