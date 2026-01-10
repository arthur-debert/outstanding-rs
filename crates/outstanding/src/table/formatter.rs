//! Table row formatter.
//!
//! This module provides the `TableFormatter` type that formats data rows
//! according to a table specification, producing aligned output.

use super::resolve::ResolvedWidths;
use super::types::{Align, Column, TableSpec, TruncateAt};
use super::util::{
    display_width, pad_center, pad_left, pad_right, truncate_end, truncate_middle, truncate_start,
};

/// Formats table rows according to a specification.
///
/// The formatter holds resolved column widths and produces formatted rows.
/// It supports row-by-row formatting for interleaved output patterns.
///
/// # Example
///
/// ```rust
/// use outstanding::table::{TableSpec, Column, Width, TableFormatter};
///
/// let spec = TableSpec::builder()
///     .column(Column::new(Width::Fixed(8)))
///     .column(Column::new(Width::Fill))
///     .column(Column::new(Width::Fixed(10)))
///     .separator("  ")
///     .build();
///
/// let formatter = TableFormatter::new(&spec, 80);
///
/// // Format rows one at a time (enables interleaved output)
/// let row1 = formatter.format_row(&["abc123", "path/to/file.rs", "pending"]);
/// println!("{}", row1);
/// println!("  └─ Note: needs review");  // Interleaved content
/// let row2 = formatter.format_row(&["def456", "src/lib.rs", "done"]);
/// println!("{}", row2);
/// ```
#[derive(Clone, Debug)]
pub struct TableFormatter {
    /// Column specifications.
    columns: Vec<Column>,
    /// Resolved widths for each column.
    widths: Vec<usize>,
    /// Column separator string.
    separator: String,
    /// Row prefix string.
    prefix: String,
    /// Row suffix string.
    suffix: String,
}

impl TableFormatter {
    /// Create a new formatter by resolving widths from the spec.
    ///
    /// # Arguments
    ///
    /// * `spec` - Table specification
    /// * `total_width` - Total available width including decorations
    pub fn new(spec: &TableSpec, total_width: usize) -> Self {
        let resolved = spec.resolve_widths(total_width);
        Self::from_resolved(spec, resolved)
    }

    /// Create a formatter with pre-resolved widths.
    ///
    /// Use this when you've already calculated widths (e.g., from data).
    pub fn from_resolved(spec: &TableSpec, resolved: ResolvedWidths) -> Self {
        TableFormatter {
            columns: spec.columns.clone(),
            widths: resolved.widths,
            separator: spec.decorations.column_sep.clone(),
            prefix: spec.decorations.row_prefix.clone(),
            suffix: spec.decorations.row_suffix.clone(),
        }
    }

    /// Create a formatter from explicit widths and columns.
    ///
    /// This is useful for direct construction without a full TableSpec.
    pub fn with_widths(columns: Vec<Column>, widths: Vec<usize>) -> Self {
        TableFormatter {
            columns,
            widths,
            separator: String::new(),
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    /// Set the column separator.
    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.separator = sep.into();
        self
    }

    /// Set the row prefix.
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Set the row suffix.
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Format a single row of values.
    ///
    /// Values are truncated/padded according to the column specifications.
    /// Missing values use the column's null representation.
    ///
    /// # Arguments
    ///
    /// * `values` - Slice of cell values (strings)
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::table::{TableSpec, Column, Width, TableFormatter};
    ///
    /// let spec = TableSpec::builder()
    ///     .column(Column::new(Width::Fixed(10)))
    ///     .column(Column::new(Width::Fixed(8)))
    ///     .separator(" | ")
    ///     .build();
    ///
    /// let formatter = TableFormatter::new(&spec, 80);
    /// let output = formatter.format_row(&["Hello", "World"]);
    /// assert_eq!(output, "Hello      | World   ");
    /// ```
    pub fn format_row<S: AsRef<str>>(&self, values: &[S]) -> String {
        let mut result = String::new();
        result.push_str(&self.prefix);

        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                result.push_str(&self.separator);
            }

            let width = self.widths.get(i).copied().unwrap_or(0);
            let value = values.get(i).map(|s| s.as_ref()).unwrap_or(&col.null_repr);

            let formatted = format_cell(value, width, col);
            result.push_str(&formatted);
        }

        result.push_str(&self.suffix);
        result
    }

    /// Format multiple rows.
    ///
    /// Returns a vector of formatted row strings.
    pub fn format_rows<S: AsRef<str>>(&self, rows: &[Vec<S>]) -> Vec<String> {
        rows.iter().map(|row| self.format_row(row)).collect()
    }

    /// Get the resolved width for a column by index.
    pub fn column_width(&self, index: usize) -> Option<usize> {
        self.widths.get(index).copied()
    }

    /// Get all resolved column widths.
    pub fn widths(&self) -> &[usize] {
        &self.widths
    }

    /// Get the number of columns.
    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }
}

/// Format a single cell value according to column spec.
fn format_cell(value: &str, width: usize, col: &Column) -> String {
    if width == 0 {
        return String::new();
    }

    let current_width = display_width(value);

    // Truncate if needed
    let truncated = if current_width > width {
        match col.truncate {
            TruncateAt::End => truncate_end(value, width, &col.ellipsis),
            TruncateAt::Start => truncate_start(value, width, &col.ellipsis),
            TruncateAt::Middle => truncate_middle(value, width, &col.ellipsis),
        }
    } else {
        value.to_string()
    };

    // Pad to width
    match col.align {
        Align::Left => pad_right(&truncated, width),
        Align::Right => pad_left(&truncated, width),
        Align::Center => pad_center(&truncated, width),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::Width;

    fn simple_spec() -> TableSpec {
        TableSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator(" | ")
            .build()
    }

    #[test]
    fn format_basic_row() {
        let formatter = TableFormatter::new(&simple_spec(), 80);
        let output = formatter.format_row(&["Hello", "World"]);
        assert_eq!(output, "Hello      | World   ");
    }

    #[test]
    fn format_row_with_truncation() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(8)))
            .build();
        let formatter = TableFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello World"]);
        assert_eq!(output, "Hello W…");
    }

    #[test]
    fn format_row_right_align() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)).align(Align::Right))
            .build();
        let formatter = TableFormatter::new(&spec, 80);

        let output = formatter.format_row(&["42"]);
        assert_eq!(output, "        42");
    }

    #[test]
    fn format_row_center_align() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)).align(Align::Center))
            .build();
        let formatter = TableFormatter::new(&spec, 80);

        let output = formatter.format_row(&["hi"]);
        assert_eq!(output, "    hi    ");
    }

    #[test]
    fn format_row_truncate_start() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)).truncate(TruncateAt::Start))
            .build();
        let formatter = TableFormatter::new(&spec, 80);

        let output = formatter.format_row(&["/path/to/file.rs"]);
        assert_eq!(display_width(&output), 10);
        assert!(output.starts_with("…"));
    }

    #[test]
    fn format_row_truncate_middle() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)).truncate(TruncateAt::Middle))
            .build();
        let formatter = TableFormatter::new(&spec, 80);

        let output = formatter.format_row(&["abcdefghijklmno"]);
        assert_eq!(display_width(&output), 10);
        assert!(output.contains("…"));
    }

    #[test]
    fn format_row_with_null() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)).null_repr("N/A"))
            .separator("  ")
            .build();
        let formatter = TableFormatter::new(&spec, 80);

        // Only provide first value - second uses null_repr
        let output = formatter.format_row(&["value"]);
        assert!(output.contains("N/A"));
    }

    #[test]
    fn format_row_with_decorations() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator(" │ ")
            .prefix("│ ")
            .suffix(" │")
            .build();
        let formatter = TableFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello", "World"]);
        assert!(output.starts_with("│ "));
        assert!(output.ends_with(" │"));
        assert!(output.contains(" │ "));
    }

    #[test]
    fn format_multiple_rows() {
        let formatter = TableFormatter::new(&simple_spec(), 80);
        let rows = vec![vec!["a", "1"], vec!["b", "2"], vec!["c", "3"]];

        let output = formatter.format_rows(&rows);
        assert_eq!(output.len(), 3);
    }

    #[test]
    fn format_row_fill_column() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(5)))
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fixed(5)))
            .separator("  ")
            .build();

        // Total: 30, overhead: 4 (2 separators), fixed: 10, fill: 16
        let formatter = TableFormatter::new(&spec, 30);
        let _output = formatter.format_row(&["abc", "middle", "xyz"]);

        // Check that widths are as expected
        assert_eq!(formatter.widths(), &[5, 16, 5]);
    }

    #[test]
    fn formatter_accessors() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .build();
        let formatter = TableFormatter::new(&spec, 80);

        assert_eq!(formatter.num_columns(), 2);
        assert_eq!(formatter.column_width(0), Some(10));
        assert_eq!(formatter.column_width(1), Some(8));
        assert_eq!(formatter.column_width(2), None);
    }

    #[test]
    fn format_empty_spec() {
        let spec = TableSpec::builder().build();
        let formatter = TableFormatter::new(&spec, 80);

        let output = formatter.format_row::<&str>(&[]);
        assert_eq!(output, "");
    }

    #[test]
    fn format_with_ansi() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .build();
        let formatter = TableFormatter::new(&spec, 80);

        let styled = "\x1b[31mred\x1b[0m";
        let output = formatter.format_row(&[styled]);

        // ANSI codes should be preserved, display width should be 10
        assert!(output.contains("\x1b[31m"));
        assert_eq!(display_width(&output), 10);
    }

    #[test]
    fn format_with_explicit_widths() {
        let columns = vec![Column::new(Width::Fixed(5)), Column::new(Width::Fixed(10))];
        let formatter = TableFormatter::with_widths(columns, vec![5, 10]).separator(" - ");

        let output = formatter.format_row(&["hi", "there"]);
        assert_eq!(output, "hi    - there     ");
    }
}
