//! Core types for tabular output configuration.
//!
//! This module defines the data structures used to specify table layout:
//! column widths, alignment, truncation strategies, and decorations.

use serde::{Deserialize, Serialize};

/// Text alignment within a column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    /// Left-align text (pad on the right).
    #[default]
    Left,
    /// Right-align text (pad on the left).
    Right,
    /// Center text (pad on both sides).
    Center,
}

/// Position where truncation occurs when content exceeds max width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TruncateAt {
    /// Truncate at the end, keeping the start visible.
    /// Example: "Hello World" → "Hello W…"
    #[default]
    End,
    /// Truncate at the start, keeping the end visible.
    /// Example: "Hello World" → "…o World"
    Start,
    /// Truncate in the middle, keeping both start and end visible.
    /// Example: "Hello World" → "Hel…orld"
    Middle,
}

/// Specifies how a column determines its width.
/// Specifies how a column determines its width.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WidthRaw", into = "WidthRaw")]
pub enum Width {
    /// Fixed width in display columns.
    Fixed(usize),
    /// Width calculated from content, constrained by optional min/max bounds.
    Bounded {
        /// Minimum width (defaults to 0 if not specified).
        min: Option<usize>,
        /// Maximum width (unlimited if not specified).
        max: Option<usize>,
    },
    /// Expand to fill all remaining space.
    /// Only one column per table should use this.
    Fill,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum WidthRaw {
    Fixed(usize),
    Bounded {
        #[serde(default)]
        min: Option<usize>,
        #[serde(default)]
        max: Option<usize>,
    },
    FillStr(String),
}

impl From<Width> for WidthRaw {
    fn from(width: Width) -> Self {
        match width {
            Width::Fixed(w) => WidthRaw::Fixed(w),
            Width::Bounded { min, max } => WidthRaw::Bounded { min, max },
            Width::Fill => WidthRaw::FillStr("fill".to_string()),
        }
    }
}

impl TryFrom<WidthRaw> for Width {
    type Error = String;

    fn try_from(raw: WidthRaw) -> Result<Self, Self::Error> {
        match raw {
            WidthRaw::Fixed(w) => Ok(Width::Fixed(w)),
            WidthRaw::Bounded { min, max } => Ok(Width::Bounded { min, max }),
            WidthRaw::FillStr(s) if s == "fill" => Ok(Width::Fill),
            WidthRaw::FillStr(s) => Err(format!("Invalid width string: '{}'. Expected 'fill'.", s)),
        }
    }
}

impl Default for Width {
    fn default() -> Self {
        Width::Bounded {
            min: None,
            max: None,
        }
    }
}

impl Width {
    /// Create a fixed-width column.
    pub fn fixed(width: usize) -> Self {
        Width::Fixed(width)
    }

    /// Create a bounded-width column with both min and max.
    pub fn bounded(min: usize, max: usize) -> Self {
        Width::Bounded {
            min: Some(min),
            max: Some(max),
        }
    }

    /// Create a column with only a minimum width.
    pub fn min(min: usize) -> Self {
        Width::Bounded {
            min: Some(min),
            max: None,
        }
    }

    /// Create a column with only a maximum width.
    pub fn max(max: usize) -> Self {
        Width::Bounded {
            min: None,
            max: Some(max),
        }
    }

    /// Create a fill column that expands to remaining space.
    pub fn fill() -> Self {
        Width::Fill
    }
}

/// Configuration for a single column in a table.
#[derive(Clone, Debug)]
pub struct Column {
    /// How the column determines its width.
    pub width: Width,
    /// Text alignment within the column.
    pub align: Align,
    /// Where to truncate when content exceeds width.
    pub truncate: TruncateAt,
    /// String to show when truncation occurs.
    pub ellipsis: String,
    /// Representation for null/empty values.
    pub null_repr: String,
    /// Optional style name (resolved via theme).
    pub style: Option<String>,
}

impl Default for Column {
    fn default() -> Self {
        Column {
            width: Width::default(),
            align: Align::default(),
            truncate: TruncateAt::default(),
            ellipsis: "…".to_string(),
            null_repr: "-".to_string(),
            style: None,
        }
    }
}

impl Column {
    /// Create a new column with the specified width.
    pub fn new(width: Width) -> Self {
        Column {
            width,
            ..Default::default()
        }
    }

    /// Create a column builder for fluent construction.
    pub fn builder() -> ColumnBuilder {
        ColumnBuilder::default()
    }

    /// Set the text alignment.
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Set the truncation position.
    pub fn truncate(mut self, truncate: TruncateAt) -> Self {
        self.truncate = truncate;
        self
    }

    /// Set the ellipsis string.
    pub fn ellipsis(mut self, ellipsis: impl Into<String>) -> Self {
        self.ellipsis = ellipsis.into();
        self
    }

    /// Set the null/empty value representation.
    pub fn null_repr(mut self, null_repr: impl Into<String>) -> Self {
        self.null_repr = null_repr.into();
        self
    }

    /// Set the style name for this column.
    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }
}

/// Builder for constructing `Column` instances.
#[derive(Clone, Debug, Default)]
pub struct ColumnBuilder {
    width: Option<Width>,
    align: Option<Align>,
    truncate: Option<TruncateAt>,
    ellipsis: Option<String>,
    null_repr: Option<String>,
    style: Option<String>,
}

impl ColumnBuilder {
    /// Set the width strategy.
    pub fn width(mut self, width: Width) -> Self {
        self.width = Some(width);
        self
    }

    /// Set a fixed width.
    pub fn fixed(mut self, width: usize) -> Self {
        self.width = Some(Width::Fixed(width));
        self
    }

    /// Set the column to fill remaining space.
    pub fn fill(mut self) -> Self {
        self.width = Some(Width::Fill);
        self
    }

    /// Set bounded width with min and max.
    pub fn bounded(mut self, min: usize, max: usize) -> Self {
        self.width = Some(Width::bounded(min, max));
        self
    }

    /// Set the text alignment.
    pub fn align(mut self, align: Align) -> Self {
        self.align = Some(align);
        self
    }

    /// Set the truncation position.
    pub fn truncate(mut self, truncate: TruncateAt) -> Self {
        self.truncate = Some(truncate);
        self
    }

    /// Set the ellipsis string.
    pub fn ellipsis(mut self, ellipsis: impl Into<String>) -> Self {
        self.ellipsis = Some(ellipsis.into());
        self
    }

    /// Set the null/empty value representation.
    pub fn null_repr(mut self, null_repr: impl Into<String>) -> Self {
        self.null_repr = Some(null_repr.into());
        self
    }

    /// Set the style name.
    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Build the `Column` instance.
    pub fn build(self) -> Column {
        let default = Column::default();
        Column {
            width: self.width.unwrap_or(default.width),
            align: self.align.unwrap_or(default.align),
            truncate: self.truncate.unwrap_or(default.truncate),
            ellipsis: self.ellipsis.unwrap_or(default.ellipsis),
            null_repr: self.null_repr.unwrap_or(default.null_repr),
            style: self.style,
        }
    }
}

/// Decorations for table rows (separators, prefixes, suffixes).
#[derive(Clone, Debug, Default)]
pub struct Decorations {
    /// Separator between columns (e.g., "  " or " │ ").
    pub column_sep: String,
    /// Prefix at the start of each row.
    pub row_prefix: String,
    /// Suffix at the end of each row.
    pub row_suffix: String,
}

impl Decorations {
    /// Create decorations with just a column separator.
    pub fn with_separator(sep: impl Into<String>) -> Self {
        Decorations {
            column_sep: sep.into(),
            row_prefix: String::new(),
            row_suffix: String::new(),
        }
    }

    /// Set the column separator.
    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.column_sep = sep.into();
        self
    }

    /// Set the row prefix.
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.row_prefix = prefix.into();
        self
    }

    /// Set the row suffix.
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.row_suffix = suffix.into();
        self
    }

    /// Calculate the total overhead (prefix + suffix + separators between n columns).
    pub fn overhead(&self, num_columns: usize) -> usize {
        use crate::table::display_width;
        let prefix_width = display_width(&self.row_prefix);
        let suffix_width = display_width(&self.row_suffix);
        let sep_width = display_width(&self.column_sep);
        let sep_count = num_columns.saturating_sub(1);
        prefix_width + suffix_width + (sep_width * sep_count)
    }
}

/// Complete specification for a table's layout.
#[derive(Clone, Debug)]
pub struct TableSpec {
    /// Column specifications.
    pub columns: Vec<Column>,
    /// Row decorations (separators, prefix, suffix).
    pub decorations: Decorations,
}

impl TableSpec {
    /// Create a new table spec with the given columns and default decorations.
    pub fn new(columns: Vec<Column>) -> Self {
        TableSpec {
            columns,
            decorations: Decorations::default(),
        }
    }

    /// Create a table spec builder.
    pub fn builder() -> TableSpecBuilder {
        TableSpecBuilder::default()
    }

    /// Get the number of columns.
    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }

    /// Check if any column uses Fill width.
    pub fn has_fill_column(&self) -> bool {
        self.columns.iter().any(|c| matches!(c.width, Width::Fill))
    }
}

/// Builder for constructing `TableSpec` instances.
#[derive(Clone, Debug, Default)]
pub struct TableSpecBuilder {
    columns: Vec<Column>,
    decorations: Decorations,
}

impl TableSpecBuilder {
    /// Add a column to the table.
    pub fn column(mut self, column: Column) -> Self {
        self.columns.push(column);
        self
    }

    /// Add multiple columns from an iterator.
    pub fn columns(mut self, columns: impl IntoIterator<Item = Column>) -> Self {
        self.columns.extend(columns);
        self
    }

    /// Set the column separator.
    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.decorations.column_sep = sep.into();
        self
    }

    /// Set the row prefix.
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.decorations.row_prefix = prefix.into();
        self
    }

    /// Set the row suffix.
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.decorations.row_suffix = suffix.into();
        self
    }

    /// Set all decorations at once.
    pub fn decorations(mut self, decorations: Decorations) -> Self {
        self.decorations = decorations;
        self
    }

    /// Build the `TableSpec` instance.
    pub fn build(self) -> TableSpec {
        TableSpec {
            columns: self.columns,
            decorations: self.decorations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Align tests ---

    #[test]
    fn align_default_is_left() {
        assert_eq!(Align::default(), Align::Left);
    }

    #[test]
    fn align_serde_roundtrip() {
        let values = [Align::Left, Align::Right, Align::Center];
        for align in values {
            let json = serde_json::to_string(&align).unwrap();
            let parsed: Align = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, align);
        }
    }

    // --- TruncateAt tests ---

    #[test]
    fn truncate_at_default_is_end() {
        assert_eq!(TruncateAt::default(), TruncateAt::End);
    }

    #[test]
    fn truncate_at_serde_roundtrip() {
        let values = [TruncateAt::End, TruncateAt::Start, TruncateAt::Middle];
        for truncate in values {
            let json = serde_json::to_string(&truncate).unwrap();
            let parsed: TruncateAt = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, truncate);
        }
    }

    // --- Width tests ---

    #[test]
    fn width_constructors() {
        assert_eq!(Width::fixed(10), Width::Fixed(10));
        assert_eq!(
            Width::bounded(5, 20),
            Width::Bounded {
                min: Some(5),
                max: Some(20)
            }
        );
        assert_eq!(
            Width::min(5),
            Width::Bounded {
                min: Some(5),
                max: None
            }
        );
        assert_eq!(
            Width::max(20),
            Width::Bounded {
                min: None,
                max: Some(20)
            }
        );
        assert_eq!(Width::fill(), Width::Fill);
    }

    #[test]
    fn width_serde_fixed() {
        let width = Width::Fixed(10);
        let json = serde_json::to_string(&width).unwrap();
        assert_eq!(json, "10");
        let parsed: Width = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, width);
    }



    #[test]
    fn width_serde_bounded() {
        let width = Width::Bounded {
            min: Some(5),
            max: Some(20),
        };
        let json = serde_json::to_string(&width).unwrap();
        let parsed: Width = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, width);
    }

    #[test]
    fn width_serde_fill() {
        let width = Width::Fill;
        let json = serde_json::to_string(&width).unwrap();
        // Now serializes to "fill"
        assert_eq!(json, "\"fill\"");

        let parsed: Width = serde_json::from_str("\"fill\"").unwrap();
        assert_eq!(parsed, width);
    }

    // --- Column tests ---

    #[test]
    fn column_defaults() {
        let col = Column::default();
        assert!(matches!(col.width, Width::Bounded { min: None, max: None }));
        assert_eq!(col.align, Align::Left);
        assert_eq!(col.truncate, TruncateAt::End);
        assert_eq!(col.ellipsis, "…");
        assert_eq!(col.null_repr, "-");
        assert!(col.style.is_none());
    }

    #[test]
    fn column_fluent_api() {
        let col = Column::new(Width::Fixed(10))
            .align(Align::Right)
            .truncate(TruncateAt::Middle)
            .ellipsis("...")
            .null_repr("N/A")
            .style("header");

        assert_eq!(col.width, Width::Fixed(10));
        assert_eq!(col.align, Align::Right);
        assert_eq!(col.truncate, TruncateAt::Middle);
        assert_eq!(col.ellipsis, "...");
        assert_eq!(col.null_repr, "N/A");
        assert_eq!(col.style, Some("header".to_string()));
    }

    #[test]
    fn column_builder() {
        let col = Column::builder()
            .fixed(15)
            .align(Align::Center)
            .truncate(TruncateAt::Start)
            .build();

        assert_eq!(col.width, Width::Fixed(15));
        assert_eq!(col.align, Align::Center);
        assert_eq!(col.truncate, TruncateAt::Start);
    }

    #[test]
    fn column_builder_fill() {
        let col = Column::builder().fill().build();
        assert_eq!(col.width, Width::Fill);
    }

    // --- Decorations tests ---

    #[test]
    fn decorations_default() {
        let dec = Decorations::default();
        assert_eq!(dec.column_sep, "");
        assert_eq!(dec.row_prefix, "");
        assert_eq!(dec.row_suffix, "");
    }

    #[test]
    fn decorations_with_separator() {
        let dec = Decorations::with_separator("  ");
        assert_eq!(dec.column_sep, "  ");
    }

    #[test]
    fn decorations_overhead() {
        let dec = Decorations::default()
            .separator("  ")
            .prefix("│ ")
            .suffix(" │");

        // 3 columns: prefix(2) + suffix(2) + 2 separators(4) = 8
        assert_eq!(dec.overhead(3), 8);
        // 1 column: prefix(2) + suffix(2) + 0 separators = 4
        assert_eq!(dec.overhead(1), 4);
        // 0 columns: just prefix + suffix
        assert_eq!(dec.overhead(0), 4);
    }

    // --- TableSpec tests ---

    #[test]
    fn table_spec_builder() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(8)))
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fixed(10)))
            .separator("  ")
            .build();

        assert_eq!(spec.num_columns(), 3);
        assert!(spec.has_fill_column());
        assert_eq!(spec.decorations.column_sep, "  ");
    }

    #[test]
    fn table_spec_no_fill() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(8)))
            .column(Column::new(Width::Fixed(10)))
            .build();

        assert!(!spec.has_fill_column());
    }

    #[test]
    fn table_spec_columns_iterator() {
        let spec = TableSpec::builder()
            .columns(vec![
                Column::new(Width::Fixed(5)),
                Column::new(Width::Fixed(10)),
            ])
            .build();

        assert_eq!(spec.num_columns(), 2);
    }
}
