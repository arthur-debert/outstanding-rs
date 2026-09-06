//! Unicode-aware column formatting for terminal tables.
//!
//! Handles Unicode display width (CJK characters count as 2 columns) and ANSI
//! escapes (excluded from width) so text aligns and truncates without visual
//! drift. Semantic style tags likewise do not consume display width —
//! truncation and wrapping preserve styles on retained text and emit balanced
//! tags, so a styled cell can be measured and fitted without first converting
//! it to plain text.
//!
//! Two APIs: template filters (`col`, `pad_left`, …) for widths known at
//! template time, or [`TabularFormatter`] for dynamic widths, CSV export, or
//! specs that extract data from structs. The `tabular()`/`table()` template
//! functions take the row arrays about to be rendered, because sizing a
//! `{min, max}` column to the widest cell is a whole-table measurement a
//! formatter that sees one row at a time cannot do on its own.
//!
//! [`Width::Fill`] and `Fraction` columns split whatever space is left after
//! fixed and bounded columns are sized; with no flex column, leftover space
//! goes to the rightmost [`Width::Bounded`] column instead, ignoring its `max`,
//! since that is explicit layout expansion rather than the bound itself.

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
