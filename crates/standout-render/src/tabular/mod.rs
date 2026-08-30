//! Unicode-aware column formatting for terminal tables.
//!
//! Handles Unicode display width (CJK characters count as 2 columns) and
//! ANSI escapes (excluded from width) so text aligns and truncates without
//! visual drift.
//!
//! Two APIs, pick based on need: template filters (`col`, `pad_left`, …) for
//! simple tables with widths known at template time, or [`TabularFormatter`]
//! for dynamic widths, CSV export, or specs that extract data from structs.
//!
//! Column widths: [`Width::Fixed`] (exact), [`Width::Bounded`] (auto-sized
//! within bounds from content), [`Width::Fill`] (one per table, takes the
//! remaining space). Truncation: [`TruncateAt::End`], [`TruncateAt::Start`],
//! [`TruncateAt::Middle`] (keeps both ends, useful for paths).
//!
//! Semantic style tags do not consume display width — truncation and
//! wrapping preserve styles on retained text and emit balanced tags, so a
//! styled cell can be measured and fitted without first converting it to
//! plain text.
//!
//! Columns can nest sub-columns for per-row layout within a parent column:
//! exactly one sub-column is [`Width::Fill`], the rest are Fixed or Bounded,
//! and widths are resolved per-row from actual content.

mod decorator;
pub mod filters;
mod formatter;
mod resolve;
mod traits;
mod types;
mod util;

pub use decorator::{BorderStyle, Table};
pub use formatter::{CellOutput, CellValue, TabularFormatter};
pub use resolve::ResolvedWidths;
pub use traits::{Tabular, TabularFieldDisplay, TabularFieldOption, TabularRow};

pub use types::{
    Align, Anchor, Col, Column, ColumnBuilder, Decorations, FlatDataSpec, FlatDataSpecBuilder,
    Overflow, SubCol, SubColumn, SubColumns, TabularSpec, TabularSpecBuilder, TruncateAt, Width,
};

pub use util::{
    display_width, display_width_with_policy, pad_center, pad_center_with_policy, pad_left,
    pad_left_with_policy, pad_right, pad_right_with_policy, truncate_end, truncate_end_with_policy,
    truncate_middle, truncate_middle_with_policy, truncate_start, truncate_start_with_policy,
    visible_width, visible_width_with_policy, wrap, wrap_indent, wrap_indent_with_policy,
    wrap_with_policy,
};
