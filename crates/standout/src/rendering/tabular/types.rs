//! Core types for tabular output configuration.
//!
//! This module defines the data structures used to specify table layout:
//! column widths, alignment, truncation strategies, and decorations.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// How a column handles content that exceeds its width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// Truncate content with an ellipsis marker.
    Truncate {
        /// Where to truncate (start, middle, or end).
        at: TruncateAt,
        /// The marker to show when truncation occurs (default: "…").
        marker: String,
    },
    /// Wrap content to multiple lines at word boundaries.
    Wrap {
        /// Number of spaces to indent continuation lines (default: 0).
        indent: usize,
    },
    /// Hard cut without any marker.
    Clip,
    /// Allow content to overflow (ignore width limit).
    Expand,
}

impl Default for Overflow {
    fn default() -> Self {
        Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        }
    }
}

impl Overflow {
    /// Create a truncate overflow with default marker.
    pub fn truncate(at: TruncateAt) -> Self {
        Overflow::Truncate {
            at,
            marker: "…".to_string(),
        }
    }

    /// Create a truncate overflow with custom marker.
    pub fn truncate_with_marker(at: TruncateAt, marker: impl Into<String>) -> Self {
        Overflow::Truncate {
            at,
            marker: marker.into(),
        }
    }

    /// Create a wrap overflow with no indent.
    pub fn wrap() -> Self {
        Overflow::Wrap { indent: 0 }
    }

    /// Create a wrap overflow with continuation indent.
    pub fn wrap_with_indent(indent: usize) -> Self {
        Overflow::Wrap { indent }
    }
}

/// Column position anchor on the row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    /// Column flows left-to-right from the start (default).
    #[default]
    Left,
    /// Column is positioned at the right edge.
    Right,
}

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
    /// Multiple Fill columns share remaining space equally.
    Fill,
    /// Proportional: takes n parts of the remaining space.
    /// `Fraction(2)` gets twice the space of `Fraction(1)` or `Fill`.
    Fraction(usize),
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
    StringVariant(String),
}

impl From<Width> for WidthRaw {
    fn from(width: Width) -> Self {
        match width {
            Width::Fixed(w) => WidthRaw::Fixed(w),
            Width::Bounded { min, max } => WidthRaw::Bounded { min, max },
            Width::Fill => WidthRaw::StringVariant("fill".to_string()),
            Width::Fraction(n) => WidthRaw::StringVariant(format!("{}fr", n)),
        }
    }
}

impl TryFrom<WidthRaw> for Width {
    type Error = String;

    fn try_from(raw: WidthRaw) -> Result<Self, Self::Error> {
        match raw {
            WidthRaw::Fixed(w) => Ok(Width::Fixed(w)),
            WidthRaw::Bounded { min, max } => Ok(Width::Bounded { min, max }),
            WidthRaw::StringVariant(s) if s == "fill" => Ok(Width::Fill),
            WidthRaw::StringVariant(s) if s.ends_with("fr") => {
                let num_str = s.trim_end_matches("fr");
                num_str
                    .parse::<usize>()
                    .map(Width::Fraction)
                    .map_err(|_| format!("Invalid fraction: '{}'. Expected format like '2fr'.", s))
            }
            WidthRaw::StringVariant(s) => Err(format!(
                "Invalid width string: '{}'. Expected 'fill' or '<n>fr'.",
                s
            )),
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

    /// Create a fractional width column.
    /// `Fraction(2)` gets twice the space of `Fraction(1)` or `Fill`.
    pub fn fraction(n: usize) -> Self {
        Width::Fraction(n)
    }
}

/// Configuration for a single column in a table.
#[derive(Clone, Debug)]
pub struct Column {
    /// Optional column name/identifier.
    pub name: Option<String>,
    /// How the column determines its width.
    pub width: Width,
    /// Text alignment within the column.
    pub align: Align,
    /// Column position anchor (left or right edge).
    pub anchor: Anchor,
    /// How to handle content that exceeds width.
    pub overflow: Overflow,
    /// Representation for null/empty values.
    pub null_repr: String,
    /// Optional style name (resolved via theme).
    pub style: Option<String>,
    /// When true, use the cell value as the style name.
    pub style_from_value: bool,
    /// Optional key for data extraction (supports dot notation for nested fields).
    pub key: Option<String>,
    /// Optional header title (for table headers and CSV export).
    pub header: Option<String>,
}

impl Default for Column {
    fn default() -> Self {
        Column {
            name: None,
            width: Width::default(),
            align: Align::default(),
            anchor: Anchor::default(),
            overflow: Overflow::default(),
            null_repr: "-".to_string(),
            style: None,
            style_from_value: false,
            key: None,
            header: None,
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

    /// Set the column name/identifier.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the text alignment.
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Set alignment to right (shorthand for `.align(Align::Right)`).
    pub fn right(self) -> Self {
        self.align(Align::Right)
    }

    /// Set alignment to center (shorthand for `.align(Align::Center)`).
    pub fn center(self) -> Self {
        self.align(Align::Center)
    }

    /// Set the column anchor position.
    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Anchor column to the right edge (shorthand for `.anchor(Anchor::Right)`).
    pub fn anchor_right(self) -> Self {
        self.anchor(Anchor::Right)
    }

    /// Set the overflow behavior.
    pub fn overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// Set overflow to wrap (shorthand for `.overflow(Overflow::wrap())`).
    pub fn wrap(self) -> Self {
        self.overflow(Overflow::wrap())
    }

    /// Set overflow to wrap with indent.
    pub fn wrap_indent(self, indent: usize) -> Self {
        self.overflow(Overflow::wrap_with_indent(indent))
    }

    /// Set overflow to clip (shorthand for `.overflow(Overflow::Clip)`).
    pub fn clip(self) -> Self {
        self.overflow(Overflow::Clip)
    }

    /// Set truncation position (configures Overflow::Truncate).
    pub fn truncate(mut self, at: TruncateAt) -> Self {
        self.overflow = match self.overflow {
            Overflow::Truncate { marker, .. } => Overflow::Truncate { at, marker },
            _ => Overflow::truncate(at),
        };
        self
    }

    /// Set truncation to middle (shorthand for `.truncate(TruncateAt::Middle)`).
    pub fn truncate_middle(self) -> Self {
        self.truncate(TruncateAt::Middle)
    }

    /// Set truncation to start (shorthand for `.truncate(TruncateAt::Start)`).
    pub fn truncate_start(self) -> Self {
        self.truncate(TruncateAt::Start)
    }

    /// Set the ellipsis/marker for truncation.
    pub fn ellipsis(mut self, ellipsis: impl Into<String>) -> Self {
        self.overflow = match self.overflow {
            Overflow::Truncate { at, .. } => Overflow::Truncate {
                at,
                marker: ellipsis.into(),
            },
            _ => Overflow::truncate_with_marker(TruncateAt::End, ellipsis),
        };
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

    /// Use the cell value as the style name.
    ///
    /// When enabled, the cell content becomes the style tag.
    /// For example, cell value "error" renders as `[error]error[/error]`.
    pub fn style_from_value(mut self) -> Self {
        self.style_from_value = true;
        self
    }

    /// Set the data key for this column (e.g. "author.name").
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the header title for this column.
    pub fn header(mut self, header: impl Into<String>) -> Self {
        self.header = Some(header.into());
        self
    }
}

/// Builder for constructing `Column` instances.
#[derive(Clone, Debug, Default)]
pub struct ColumnBuilder {
    name: Option<String>,
    width: Option<Width>,
    align: Option<Align>,
    anchor: Option<Anchor>,
    overflow: Option<Overflow>,
    null_repr: Option<String>,
    style: Option<String>,
    style_from_value: bool,
    key: Option<String>,
    header: Option<String>,
}

impl ColumnBuilder {
    /// Set the column name/identifier.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

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

    /// Set fractional width.
    pub fn fraction(mut self, n: usize) -> Self {
        self.width = Some(Width::Fraction(n));
        self
    }

    /// Set the text alignment.
    pub fn align(mut self, align: Align) -> Self {
        self.align = Some(align);
        self
    }

    /// Set alignment to right.
    pub fn right(self) -> Self {
        self.align(Align::Right)
    }

    /// Set alignment to center.
    pub fn center(self) -> Self {
        self.align(Align::Center)
    }

    /// Set the column anchor position.
    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Anchor column to the right edge.
    pub fn anchor_right(self) -> Self {
        self.anchor(Anchor::Right)
    }

    /// Set the overflow behavior.
    pub fn overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = Some(overflow);
        self
    }

    /// Set overflow to wrap.
    pub fn wrap(self) -> Self {
        self.overflow(Overflow::wrap())
    }

    /// Set overflow to clip.
    pub fn clip(self) -> Self {
        self.overflow(Overflow::Clip)
    }

    /// Set the truncation position (configures Overflow::Truncate).
    pub fn truncate(mut self, at: TruncateAt) -> Self {
        self.overflow = Some(match self.overflow {
            Some(Overflow::Truncate { marker, .. }) => Overflow::Truncate { at, marker },
            _ => Overflow::truncate(at),
        });
        self
    }

    /// Set the ellipsis string for truncation.
    pub fn ellipsis(mut self, ellipsis: impl Into<String>) -> Self {
        self.overflow = Some(match self.overflow {
            Some(Overflow::Truncate { at, .. }) => Overflow::Truncate {
                at,
                marker: ellipsis.into(),
            },
            _ => Overflow::truncate_with_marker(TruncateAt::End, ellipsis),
        });
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

    /// Use cell value as the style name.
    pub fn style_from_value(mut self) -> Self {
        self.style_from_value = true;
        self
    }

    /// Set the data key.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the header title.
    pub fn header(mut self, header: impl Into<String>) -> Self {
        self.header = Some(header.into());
        self
    }

    /// Build the `Column` instance.
    pub fn build(self) -> Column {
        let default = Column::default();
        Column {
            name: self.name,
            width: self.width.unwrap_or(default.width),
            align: self.align.unwrap_or(default.align),
            anchor: self.anchor.unwrap_or(default.anchor),
            overflow: self.overflow.unwrap_or(default.overflow),
            null_repr: self.null_repr.unwrap_or(default.null_repr),
            style: self.style,
            style_from_value: self.style_from_value,
            key: self.key,
            header: self.header,
        }
    }
}

/// Shorthand constructors for creating columns.
///
/// Provides a concise API for common column configurations:
///
/// ```rust
/// use standout::tabular::Col;
///
/// let col = Col::fixed(10);           // Fixed width 10
/// let col = Col::min(5);              // At least 5, grows to fit
/// let col = Col::bounded(5, 20);      // Between 5 and 20
/// let col = Col::fill();              // Fill remaining space
/// let col = Col::fraction(2);         // 2 parts of remaining space
///
/// // Chain with fluent methods
/// let col = Col::fixed(10).right().style("header");
/// ```
pub struct Col;

impl Col {
    /// Create a fixed-width column.
    pub fn fixed(width: usize) -> Column {
        Column::new(Width::Fixed(width))
    }

    /// Create a column with minimum width that grows to fit content.
    pub fn min(min: usize) -> Column {
        Column::new(Width::min(min))
    }

    /// Create a column with maximum width that shrinks to fit content.
    pub fn max(max: usize) -> Column {
        Column::new(Width::max(max))
    }

    /// Create a bounded-width column (between min and max).
    pub fn bounded(min: usize, max: usize) -> Column {
        Column::new(Width::bounded(min, max))
    }

    /// Create a fill column that expands to remaining space.
    pub fn fill() -> Column {
        Column::new(Width::Fill)
    }

    /// Create a fractional width column.
    /// `Col::fraction(2)` gets twice the space of `Col::fraction(1)` or `Col::fill()`.
    pub fn fraction(n: usize) -> Column {
        Column::new(Width::Fraction(n))
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
        use crate::tabular::display_width;
        let prefix_width = display_width(&self.row_prefix);
        let suffix_width = display_width(&self.row_suffix);
        let sep_width = display_width(&self.column_sep);
        let sep_count = num_columns.saturating_sub(1);
        prefix_width + suffix_width + (sep_width * sep_count)
    }
}

/// Complete specification for a flat data layout (Table or CSV).
#[derive(Clone, Debug)]
pub struct FlatDataSpec {
    /// Column specifications.
    pub columns: Vec<Column>,
    /// Row decorations (separators, prefix, suffix).
    pub decorations: Decorations,
}

impl FlatDataSpec {
    /// Create a new spec with the given columns and default decorations.
    pub fn new(columns: Vec<Column>) -> Self {
        FlatDataSpec {
            columns,
            decorations: Decorations::default(),
        }
    }

    /// Create a spec builder.
    pub fn builder() -> FlatDataSpecBuilder {
        FlatDataSpecBuilder::default()
    }

    /// Get the number of columns.
    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }

    /// Check if any column uses Fill width.
    pub fn has_fill_column(&self) -> bool {
        self.columns.iter().any(|c| matches!(c.width, Width::Fill))
    }

    /// Extract a header row from the spec.
    ///
    /// Uses column `header` if present, otherwise `key`, otherwise empty string.
    pub fn extract_header(&self) -> Vec<String> {
        self.columns
            .iter()
            .map(|col| {
                col.header
                    .as_deref()
                    .or(col.key.as_deref())
                    .unwrap_or("")
                    .to_string()
            })
            .collect()
    }

    /// Extract a data row from a JSON value using the spec.
    ///
    /// For each column:
    /// - If `key` is set, traverses the JSON to find the value.
    /// - If `key` is unset/missing, uses `null_repr`.
    /// - Handles nested objects via dot notation (e.g. "author.name").
    pub fn extract_row(&self, data: &Value) -> Vec<String> {
        self.columns
            .iter()
            .map(|col| {
                if let Some(key) = &col.key {
                    extract_value(data, key).unwrap_or(col.null_repr.clone())
                } else {
                    col.null_repr.clone()
                }
            })
            .collect()
    }
}

/// Helper to extract a value from nested JSON using dot notation.
fn extract_value(data: &Value, path: &str) -> Option<String> {
    let mut current = data;
    for part in path.split('.') {
        match current {
            Value::Object(map) => {
                current = map.get(part)?;
            }
            _ => return None,
        }
    }

    match current {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        // For structured types, just jsonify them effectively
        v => Some(v.to_string()),
    }
}

/// Builder for constructing `FlatDataSpec` instances.
#[derive(Clone, Debug, Default)]
pub struct FlatDataSpecBuilder {
    columns: Vec<Column>,
    decorations: Decorations,
}

impl FlatDataSpecBuilder {
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

    /// Build the `FlatDataSpec` instance.
    pub fn build(self) -> FlatDataSpec {
        FlatDataSpec {
            columns: self.columns,
            decorations: self.decorations,
        }
    }
}

/// Type alias: TabularSpec is the preferred name for FlatDataSpec.
pub type TabularSpec = FlatDataSpec;
/// Type alias for the builder.
pub type TabularSpecBuilder = FlatDataSpecBuilder;

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

    #[test]
    fn width_serde_fraction() {
        let width = Width::Fraction(2);
        let json = serde_json::to_string(&width).unwrap();
        assert_eq!(json, "\"2fr\"");

        let parsed: Width = serde_json::from_str("\"2fr\"").unwrap();
        assert_eq!(parsed, width);

        // Also test 1fr
        let parsed_1: Width = serde_json::from_str("\"1fr\"").unwrap();
        assert_eq!(parsed_1, Width::Fraction(1));
    }

    #[test]
    fn width_fraction_constructor() {
        assert_eq!(Width::fraction(3), Width::Fraction(3));
    }

    // --- Overflow tests ---

    #[test]
    fn overflow_default() {
        let overflow = Overflow::default();
        assert!(matches!(
            overflow,
            Overflow::Truncate {
                at: TruncateAt::End,
                ..
            }
        ));
    }

    #[test]
    fn overflow_constructors() {
        let truncate = Overflow::truncate(TruncateAt::Middle);
        assert!(matches!(
            truncate,
            Overflow::Truncate {
                at: TruncateAt::Middle,
                ref marker
            } if marker == "…"
        ));

        let truncate_custom = Overflow::truncate_with_marker(TruncateAt::Start, "...");
        assert!(matches!(
            truncate_custom,
            Overflow::Truncate {
                at: TruncateAt::Start,
                ref marker
            } if marker == "..."
        ));

        let wrap = Overflow::wrap();
        assert!(matches!(wrap, Overflow::Wrap { indent: 0 }));

        let wrap_indent = Overflow::wrap_with_indent(4);
        assert!(matches!(wrap_indent, Overflow::Wrap { indent: 4 }));
    }

    // --- Anchor tests ---

    #[test]
    fn anchor_default() {
        assert_eq!(Anchor::default(), Anchor::Left);
    }

    #[test]
    fn anchor_serde_roundtrip() {
        let values = [Anchor::Left, Anchor::Right];
        for anchor in values {
            let json = serde_json::to_string(&anchor).unwrap();
            let parsed: Anchor = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, anchor);
        }
    }

    // --- Col shorthand tests ---

    #[test]
    fn col_shorthand_constructors() {
        let fixed = Col::fixed(10);
        assert_eq!(fixed.width, Width::Fixed(10));

        let min = Col::min(5);
        assert_eq!(
            min.width,
            Width::Bounded {
                min: Some(5),
                max: None
            }
        );

        let bounded = Col::bounded(5, 20);
        assert_eq!(
            bounded.width,
            Width::Bounded {
                min: Some(5),
                max: Some(20)
            }
        );

        let fill = Col::fill();
        assert_eq!(fill.width, Width::Fill);

        let fraction = Col::fraction(3);
        assert_eq!(fraction.width, Width::Fraction(3));
    }

    #[test]
    fn col_shorthand_chaining() {
        let col = Col::fixed(10).right().anchor_right().style("header");
        assert_eq!(col.width, Width::Fixed(10));
        assert_eq!(col.align, Align::Right);
        assert_eq!(col.anchor, Anchor::Right);
        assert_eq!(col.style, Some("header".to_string()));
    }

    #[test]
    fn column_wrap_shorthand() {
        let col = Col::fill().wrap();
        assert!(matches!(col.overflow, Overflow::Wrap { indent: 0 }));

        let col_indent = Col::fill().wrap_indent(2);
        assert!(matches!(col_indent.overflow, Overflow::Wrap { indent: 2 }));
    }

    #[test]
    fn column_clip_shorthand() {
        let col = Col::fixed(10).clip();
        assert!(matches!(col.overflow, Overflow::Clip));
    }

    #[test]
    fn column_named() {
        let col = Col::fixed(10).named("author");
        assert_eq!(col.name, Some("author".to_string()));
    }

    // --- Column tests ---

    #[test]
    fn column_defaults() {
        let col = Column::default();
        assert!(matches!(
            col.width,
            Width::Bounded {
                min: None,
                max: None
            }
        ));
        assert_eq!(col.align, Align::Left);
        assert_eq!(col.anchor, Anchor::Left);
        assert!(matches!(
            col.overflow,
            Overflow::Truncate {
                at: TruncateAt::End,
                ..
            }
        ));
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
        assert!(matches!(
            col.overflow,
            Overflow::Truncate {
                at: TruncateAt::Middle,
                ref marker
            } if marker == "..."
        ));
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
        assert!(matches!(
            col.overflow,
            Overflow::Truncate {
                at: TruncateAt::Start,
                ..
            }
        ));
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

    // --- FlatDataSpec tests ---

    #[test]
    fn flat_data_spec_builder() {
        let spec = FlatDataSpec::builder()
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
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(8)))
            .column(Column::new(Width::Fixed(10)))
            .build();

        assert!(!spec.has_fill_column());
    }

    #[test]
    fn extract_fields_from_json() {
        let json = serde_json::json!({
            "name": "Alice",
            "meta": {
                "age": 30,
                "role": "admin"
            }
        });

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).key("name"))
            .column(Column::new(Width::Fixed(5)).key("meta.age"))
            .column(Column::new(Width::Fixed(10)).key("meta.role"))
            .column(Column::new(Width::Fixed(10)).key("missing.field")) // Should use null_repr
            .build();

        let row = spec.extract_row(&json);
        assert_eq!(row[0], "Alice");
        assert_eq!(row[1], "30"); // Numbers coerced to string
        assert_eq!(row[2], "admin");
        assert_eq!(row[3], "-"); // Default null_repr
    }

    #[test]
    fn extract_header_row() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).header("Name").key("name"))
            .column(Column::new(Width::Fixed(5)).key("age")) // Fallback to key
            .column(Column::new(Width::Fixed(10))) // Empty
            .build();

        let header = spec.extract_header();
        assert_eq!(header[0], "Name");
        assert_eq!(header[1], "age");
        assert_eq!(header[2], "");
    }
}
