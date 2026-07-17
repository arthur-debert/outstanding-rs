//! Table row formatter.
//!
//! This module provides the `TabularFormatter` type that formats data rows
//! according to a table specification, producing aligned output.
//!
//! # Template Integration
//!
//! `TabularFormatter` implements `minijinja::value::Object`, allowing it to be
//! used in templates when injected via context:
//!
//! ```jinja
//! {% for item in items %}
//! {{ table.row([item.name, item.value, item.status]) }}
//! {% endfor %}
//! ```
//!
//! Available methods in templates:
//! - `row(values)`: Format a row with the given values (array)
//! - `num_columns`: Get the number of columns
//!
//! # Example with Context Injection
//!
//! ```rust,ignore
//! use standout_render::tabular::{TabularSpec, Column, Width, TabularFormatter};
//! use standout_render::context::ContextRegistry;
//!
//! let spec = TabularSpec::builder()
//!     .column(Column::new(Width::Fixed(10)))
//!     .column(Column::new(Width::Fill))
//!     .separator("  ")
//!     .build();
//!
//! let mut registry = ContextRegistry::new();
//! registry.add_provider("table", |ctx| {
//!     let formatter = TabularFormatter::new(&spec, ctx.terminal_width.unwrap_or(80));
//!     minijinja::Value::from_object(formatter)
//! });
//! ```

use minijinja::value::{Enumerator, Object, Value};
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::sync::Arc;

use super::resolve::ResolvedWidths;
use super::traits::TabularRow;
use super::types::{
    Align, Anchor, Column, FlatDataSpec, Overflow, SubColumns, TabularSpec, TruncateAt, Width,
};
use super::util::{
    display_width_with_policy, pad_center_with_policy, pad_left_with_policy, pad_right_with_policy,
    truncate_end_with_policy, truncate_middle_with_policy, truncate_start_with_policy,
    visible_width_with_policy, wrap_indent_with_policy,
};
use crate::AmbiguousWidth;

/// Formats table rows according to a specification.
///
/// The formatter holds resolved column widths and produces formatted rows.
/// It supports row-by-row formatting for interleaved output patterns.
///
/// # Example
///
/// ```rust
/// use standout_render::tabular::{FlatDataSpec, Column, Width, TabularFormatter};
///
/// let spec = FlatDataSpec::builder()
///     .column(Column::new(Width::Fixed(8)))
///     .column(Column::new(Width::Fill))
///     .column(Column::new(Width::Fixed(10)))
///     .separator("  ")
///     .build();
///
/// let formatter = TabularFormatter::new(&spec, 80);
///
/// // Format rows one at a time (enables interleaved output)
/// let row1 = formatter.format_row(&["abc123", "path/to/file.rs", "pending"]);
/// println!("{}", row1);
/// println!("  └─ Note: needs review");  // Interleaved content
/// let row2 = formatter.format_row(&["def456", "src/lib.rs", "done"]);
/// println!("{}", row2);
/// ```
#[derive(Clone, Debug)]
pub struct TabularFormatter {
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
    /// Total target width for anchor calculations.
    total_width: usize,
    ambiguous_width: AmbiguousWidth,
}

impl TabularFormatter {
    /// Create a new formatter by resolving widths from the spec.
    ///
    /// # Arguments
    ///
    /// * `spec` - Table specification
    /// * `total_width` - Total available width including decorations
    pub fn new(spec: &FlatDataSpec, total_width: usize) -> Self {
        Self::with_ambiguous_width(spec, total_width, AmbiguousWidth::Narrow)
    }

    /// Creates a formatter with an explicit ambiguous-width policy.
    pub fn with_ambiguous_width(
        spec: &FlatDataSpec,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        let resolved = spec.resolve_widths_with_policy(total_width, policy);
        Self::from_resolved_with_width_and_policy(spec, resolved, total_width, policy)
    }

    /// Create a formatter with pre-resolved widths.
    ///
    /// Use this when you've already calculated widths (e.g., from data).
    pub fn from_resolved(spec: &FlatDataSpec, resolved: ResolvedWidths) -> Self {
        // Calculate total width from resolved widths + overhead
        let content_width: usize = resolved.widths.iter().sum();
        let overhead = spec.decorations.overhead(resolved.widths.len());
        let total_width = content_width + overhead;
        Self::from_resolved_with_width(spec, resolved, total_width)
    }

    /// Create a formatter with pre-resolved widths and explicit total width.
    pub fn from_resolved_with_width(
        spec: &FlatDataSpec,
        resolved: ResolvedWidths,
        total_width: usize,
    ) -> Self {
        Self::from_resolved_with_width_and_policy(
            spec,
            resolved,
            total_width,
            AmbiguousWidth::Narrow,
        )
    }

    /// Creates a formatter from resolved widths with an explicit policy.
    pub fn from_resolved_with_width_and_policy(
        spec: &FlatDataSpec,
        resolved: ResolvedWidths,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        TabularFormatter {
            columns: spec.columns.clone(),
            widths: resolved.widths,
            separator: spec.decorations.column_sep.clone(),
            prefix: spec.decorations.row_prefix.clone(),
            suffix: spec.decorations.row_suffix.clone(),
            total_width,
            ambiguous_width: policy,
        }
    }

    /// Create a formatter from explicit widths and columns.
    ///
    /// This is useful for direct construction without a full FlatDataSpec.
    pub fn with_widths(columns: Vec<Column>, widths: Vec<usize>) -> Self {
        Self::with_widths_and_ambiguous_width(columns, widths, AmbiguousWidth::Narrow)
    }

    /// Creates a formatter from explicit widths with an explicit policy.
    pub fn with_widths_and_ambiguous_width(
        columns: Vec<Column>,
        widths: Vec<usize>,
        policy: AmbiguousWidth,
    ) -> Self {
        let total_width = widths.iter().sum();
        TabularFormatter {
            columns,
            widths,
            separator: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            total_width,
            ambiguous_width: policy,
        }
    }

    /// Create a formatter from a type that implements `Tabular`.
    ///
    /// This constructor uses the `TabularSpec` generated by the `#[derive(Tabular)]`
    /// macro to configure the formatter.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout_render::tabular::{Tabular, TabularFormatter};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize, Tabular)]
    /// struct Task {
    ///     #[col(width = 8)]
    ///     id: String,
    ///     #[col(width = "fill")]
    ///     title: String,
    /// }
    ///
    /// let formatter = TabularFormatter::from_type::<Task>(80);
    /// ```
    pub fn from_type<T: super::traits::Tabular>(total_width: usize) -> Self {
        Self::from_type_with_ambiguous_width::<T>(total_width, AmbiguousWidth::Narrow)
    }

    /// Creates a formatter from a `Tabular` type with an explicit policy.
    pub fn from_type_with_ambiguous_width<T: super::traits::Tabular>(
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        let spec: TabularSpec = T::tabular_spec();
        Self::with_ambiguous_width(&spec, total_width, policy)
    }

    /// Set the total target width (for anchor gap calculations).
    pub fn total_width(mut self, width: usize) -> Self {
        self.total_width = width;
        self
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
    /// use standout_render::tabular::{FlatDataSpec, Column, Width, TabularFormatter};
    ///
    /// let spec = FlatDataSpec::builder()
    ///     .column(Column::new(Width::Fixed(10)))
    ///     .column(Column::new(Width::Fixed(8)))
    ///     .separator(" | ")
    ///     .build();
    ///
    /// let formatter = TabularFormatter::new(&spec, 80);
    /// let output = formatter.format_row(&["Hello", "World"]);
    /// assert_eq!(output, "Hello      | World   ");
    /// ```
    pub fn format_row<S: AsRef<str>>(&self, values: &[S]) -> String {
        // If any column has sub-columns, delegate to format_row_cells
        // wrapping plain strings as CellValue::Single
        if self.columns.iter().any(|c| c.sub_columns.is_some()) {
            let cell_values: Vec<CellValue<'_>> = values
                .iter()
                .map(|s| CellValue::Single(s.as_ref()))
                .collect();
            return self.format_row_cells(&cell_values);
        }

        let mut result = String::new();
        result.push_str(&self.prefix);

        // Find anchor transition point and calculate gap
        let (anchor_gap, anchor_transition) = self.calculate_anchor_gap();

        for (i, col) in self.columns.iter().enumerate() {
            // Insert separator (or anchor gap at transition point)
            if i > 0 {
                if anchor_gap > 0 && i == anchor_transition {
                    // Insert anchor gap instead of separator
                    result.push_str(&" ".repeat(anchor_gap));
                } else {
                    result.push_str(&self.separator);
                }
            }

            let width = self.widths.get(i).copied().unwrap_or(0);
            let value = values.get(i).map(|s| s.as_ref()).unwrap_or(&col.null_repr);

            let formatted = format_cell_with_policy(value, width, col, self.ambiguous_width);
            result.push_str(&formatted);
        }

        result.push_str(&self.suffix);
        result
    }

    /// Format a row with cell values that may include sub-column arrays.
    ///
    /// For columns with sub-columns, pass `CellValue::Sub(vec![...])`.
    /// For regular columns, pass `CellValue::Single("...")`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::tabular::{
    ///     FlatDataSpec, Column, Width, TabularFormatter, CellValue,
    ///     SubColumns, SubCol,
    /// };
    ///
    /// let sub_cols = SubColumns::new(
    ///     vec![SubCol::fill(), SubCol::bounded(0, 20).right()],
    ///     " ",
    /// ).unwrap();
    ///
    /// let spec = FlatDataSpec::builder()
    ///     .column(Column::new(Width::Fixed(4)))
    ///     .column(Column::new(Width::Fill).sub_columns(sub_cols))
    ///     .column(Column::new(Width::Fixed(6)).right())
    ///     .separator("  ")
    ///     .build();
    ///
    /// let formatter = TabularFormatter::new(&spec, 60);
    /// let row = formatter.format_row_cells(&[
    ///     CellValue::Single("1."),
    ///     CellValue::Sub(vec!["Gallery Navigation", "[feature]"]),
    ///     CellValue::Single("4d"),
    /// ]);
    /// ```
    pub fn format_row_cells(&self, values: &[CellValue<'_>]) -> String {
        let mut result = String::new();
        result.push_str(&self.prefix);

        let (anchor_gap, anchor_transition) = self.calculate_anchor_gap();

        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                if anchor_gap > 0 && i == anchor_transition {
                    result.push_str(&" ".repeat(anchor_gap));
                } else {
                    result.push_str(&self.separator);
                }
            }

            let width = self.widths.get(i).copied().unwrap_or(0);

            if let Some(sub_cols) = &col.sub_columns {
                // Sub-column formatting
                let sub_values: Vec<&str> = match values.get(i) {
                    Some(CellValue::Sub(v)) => v.clone(),
                    Some(CellValue::Single(s)) => vec![s],
                    None => vec![],
                };
                let formatted = format_sub_cells_with_policy(
                    sub_cols,
                    &sub_values,
                    width,
                    self.ambiguous_width,
                );
                result.push_str(&formatted);
            } else {
                // Normal cell formatting
                let value = match values.get(i) {
                    Some(CellValue::Single(s)) => *s,
                    Some(CellValue::Sub(v)) => v.first().copied().unwrap_or(&col.null_repr),
                    None => &col.null_repr,
                };
                let formatted = format_cell_with_policy(value, width, col, self.ambiguous_width);
                result.push_str(&formatted);
            }
        }

        result.push_str(&self.suffix);
        result
    }

    /// Calculate the anchor gap size and transition point.
    ///
    /// Returns (gap_size, transition_index) where:
    /// - gap_size is the number of spaces to insert between left and right groups
    /// - transition_index is the column index where right-anchored columns start
    fn calculate_anchor_gap(&self) -> (usize, usize) {
        // Find first right-anchored column
        let transition = self
            .columns
            .iter()
            .position(|c| c.anchor == Anchor::Right)
            .unwrap_or(self.columns.len());

        // If no right-anchored columns or all columns are right-anchored, no gap
        if transition == 0 || transition == self.columns.len() {
            return (0, transition);
        }

        // Calculate current content width
        let prefix_width = display_width_with_policy(&self.prefix, self.ambiguous_width);
        let suffix_width = display_width_with_policy(&self.suffix, self.ambiguous_width);
        let sep_width = display_width_with_policy(&self.separator, self.ambiguous_width);
        let content_width: usize = self.widths.iter().sum();
        let num_seps = self.columns.len().saturating_sub(1);
        let current_total = prefix_width + content_width + (num_seps * sep_width) + suffix_width;

        // Calculate gap - the extra space available to push right columns to the right
        if current_total >= self.total_width {
            // No room for a gap
            (0, transition)
        } else {
            // Gap = extra space, minus one separator (which we replace with the gap)
            let extra = self.total_width - current_total;
            // The gap replaces one separator, so add sep_width back to the gap
            (extra + sep_width, transition)
        }
    }

    /// Format multiple rows.
    ///
    /// Returns a vector of formatted row strings.
    pub fn format_rows<S: AsRef<str>>(&self, rows: &[Vec<S>]) -> Vec<String> {
        rows.iter().map(|row| self.format_row(row)).collect()
    }

    /// Format a row that may produce multiple output lines (due to wrapping).
    ///
    /// If any cell wraps to multiple lines, the output contains multiple lines
    /// with proper vertical alignment. Cells are top-aligned.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::tabular::{FlatDataSpec, Column, Width, Overflow, TabularFormatter};
    ///
    /// let spec = FlatDataSpec::builder()
    ///     .column(Column::new(Width::Fixed(10)).wrap())
    ///     .column(Column::new(Width::Fixed(8)))
    ///     .separator("  ")
    ///     .build();
    ///
    /// let formatter = TabularFormatter::new(&spec, 80);
    /// let lines = formatter.format_row_lines(&["This is a long text", "Short"]);
    /// // Returns multiple lines if the first column wraps
    /// ```
    pub fn format_row_lines<S: AsRef<str>>(&self, values: &[S]) -> Vec<String> {
        // Format each cell
        let cell_outputs: Vec<CellOutput> = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let width = self.widths.get(i).copied().unwrap_or(0);
                let value = values.get(i).map(|s| s.as_ref()).unwrap_or(&col.null_repr);
                format_cell_lines_with_policy(value, width, col, self.ambiguous_width)
            })
            .collect();

        // Find max lines needed
        let max_lines = cell_outputs
            .iter()
            .map(|c| c.line_count())
            .max()
            .unwrap_or(1);

        // If only single line, use simple path
        if max_lines == 1 {
            return vec![self.format_row(values)];
        }

        // Build output lines with anchor support
        let (anchor_gap, anchor_transition) = self.calculate_anchor_gap();
        let mut output = Vec::with_capacity(max_lines);

        for line_idx in 0..max_lines {
            let mut row = String::new();
            row.push_str(&self.prefix);

            for (i, (cell, col)) in cell_outputs.iter().zip(self.columns.iter()).enumerate() {
                if i > 0 {
                    if anchor_gap > 0 && i == anchor_transition {
                        row.push_str(&" ".repeat(anchor_gap));
                    } else {
                        row.push_str(&self.separator);
                    }
                }

                let width = self.widths.get(i).copied().unwrap_or(0);
                let line = cell.line_with_policy(line_idx, width, col.align, self.ambiguous_width);
                row.push_str(&line);
            }

            row.push_str(&self.suffix);
            output.push(row);
        }

        output
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

    /// Returns `true` if any column has sub-columns defined.
    pub fn has_sub_columns(&self) -> bool {
        self.columns.iter().any(|c| c.sub_columns.is_some())
    }

    /// Get the column specifications.
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// Returns the formatter's ambiguous-width policy.
    pub fn ambiguous_width(&self) -> AmbiguousWidth {
        self.ambiguous_width
    }

    pub(crate) fn rendered_width(&self) -> usize {
        self.widths.iter().sum::<usize>()
            + display_width_with_policy(&self.prefix, self.ambiguous_width)
            + display_width_with_policy(&self.suffix, self.ambiguous_width)
            + self.columns.len().saturating_sub(1)
                * display_width_with_policy(&self.separator, self.ambiguous_width)
    }

    pub(crate) fn limit_to_width(&mut self, maximum: usize) {
        let decoration_width = self
            .rendered_width()
            .saturating_sub(self.widths.iter().sum());
        let content_limit = maximum.saturating_sub(decoration_width);
        let mut excess = self
            .widths
            .iter()
            .sum::<usize>()
            .saturating_sub(content_limit);

        for width in self.widths.iter_mut().rev() {
            let reduction = (*width).min(excess);
            *width -= reduction;
            excess -= reduction;
            if excess == 0 {
                break;
            }
        }
        self.total_width = self.total_width.min(maximum);
    }

    /// Extract headers from column specifications.
    ///
    /// For each column, uses (in order of preference):
    /// 1. The `header` field if set
    /// 2. The `key` field if set
    /// 3. The `name` field if set
    /// 4. Empty string
    ///
    /// This is useful for `Table::header_from_columns()`.
    pub fn extract_headers(&self) -> Vec<String> {
        self.columns
            .iter()
            .map(|col| {
                col.header
                    .as_deref()
                    .or(col.key.as_deref())
                    .or(col.name.as_deref())
                    .unwrap_or("")
                    .to_string()
            })
            .collect()
    }

    /// Format a row by extracting values from a serializable struct.
    ///
    /// This method extracts field values from the struct based on each column's
    /// `key` or `name` field. Supports dot notation for nested field access
    /// (e.g., "user.email").
    ///
    /// # Arguments
    ///
    /// * `value` - Any serializable value to extract fields from
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::tabular::{FlatDataSpec, Column, Width, TabularFormatter};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct Record {
    ///     name: String,
    ///     status: String,
    ///     count: u32,
    /// }
    ///
    /// let spec = FlatDataSpec::builder()
    ///     .column(Column::new(Width::Fixed(20)).key("name"))
    ///     .column(Column::new(Width::Fixed(10)).key("status"))
    ///     .column(Column::new(Width::Fixed(5)).key("count"))
    ///     .separator("  ")
    ///     .build();
    ///
    /// let formatter = TabularFormatter::new(&spec, 80);
    /// let record = Record {
    ///     name: "example".to_string(),
    ///     status: "active".to_string(),
    ///     count: 42,
    /// };
    ///
    /// let row = formatter.row_from(&record);
    /// assert!(row.contains("example"));
    /// assert!(row.contains("active"));
    /// assert!(row.contains("42"));
    /// ```
    pub fn row_from<T: Serialize>(&self, value: &T) -> String {
        let values = self.extract_values(value);
        let string_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
        self.format_row(&string_refs)
    }

    /// Format a row with potential multi-line output from a serializable struct.
    ///
    /// Same as `row_from` but handles word-wrap columns that may produce
    /// multiple output lines.
    pub fn row_lines_from<T: Serialize>(&self, value: &T) -> Vec<String> {
        let values = self.extract_values(value);
        let string_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
        self.format_row_lines(&string_refs)
    }

    /// Format a row using the `TabularRow` trait.
    ///
    /// This method uses the optimized `to_row()` implementation generated by
    /// `#[derive(TabularRow)]`, avoiding JSON serialization overhead.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout_render::tabular::{TabularRow, TabularFormatter};
    ///
    /// #[derive(TabularRow)]
    /// struct Task {
    ///     id: String,
    ///     title: String,
    /// }
    ///
    /// let task = Task {
    ///     id: "TSK-001".to_string(),
    ///     title: "Implement feature".to_string(),
    /// };
    ///
    /// let formatter = TabularFormatter::from_type::<Task>(80);
    /// let row = formatter.row_from_trait(&task);
    /// ```
    pub fn row_from_trait<T: TabularRow>(&self, value: &T) -> String {
        let values = value.to_row();
        self.format_row(&values)
    }

    /// Format a row with potential multi-line output using the `TabularRow` trait.
    ///
    /// Same as `row_from_trait` but handles word-wrap columns that may produce
    /// multiple output lines.
    pub fn row_lines_from_trait<T: TabularRow>(&self, value: &T) -> Vec<String> {
        let values = value.to_row();
        self.format_row_lines(&values)
    }

    /// Extract values from a serializable struct based on column keys.
    fn extract_values<T: Serialize>(&self, value: &T) -> Vec<String> {
        // Convert to JSON for field access
        let json = match serde_json::to_value(value) {
            Ok(v) => v,
            Err(_) => return vec![String::new(); self.columns.len()],
        };

        self.columns
            .iter()
            .map(|col| {
                // Use key first, fall back to name
                let key = col.key.as_ref().or(col.name.as_ref());

                match key {
                    Some(k) => extract_field(&json, k),
                    None => col.null_repr.clone(),
                }
            })
            .collect()
    }
}

/// Extract a field value from JSON using dot notation.
///
/// Supports paths like "user.email" or "items.0.name".
fn extract_field(value: &JsonValue, path: &str) -> String {
    let mut current = value;

    for part in path.split('.') {
        match current {
            JsonValue::Object(map) => {
                current = match map.get(part) {
                    Some(v) => v,
                    None => return String::new(),
                };
            }
            JsonValue::Array(arr) => {
                // Try to parse as index
                if let Ok(idx) = part.parse::<usize>() {
                    current = match arr.get(idx) {
                        Some(v) => v,
                        None => return String::new(),
                    };
                } else {
                    return String::new();
                }
            }
            _ => return String::new(),
        }
    }

    // Convert final value to string
    match current {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => String::new(),
        // For arrays/objects, use JSON representation
        _ => current.to_string(),
    }
}

// ============================================================================
// MiniJinja Object Implementation
// ============================================================================

impl Object for TabularFormatter {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "num_columns" => Some(Value::from(self.num_columns())),
            "widths" => {
                let widths: Vec<Value> = self.widths.iter().map(|&w| Value::from(w)).collect();
                Some(Value::from(widths))
            }
            "separator" => Some(Value::from(self.separator.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["num_columns", "widths", "separator"])
    }

    fn call_method(
        self: &Arc<Self>,
        _state: &minijinja::State,
        name: &str,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        match name {
            "row" => {
                // row([value1, value2, ...]) - format a row
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "row() requires an array of values",
                    ));
                }

                let values_arg = &args[0];
                let has_sub_columns = self.columns.iter().any(|c| c.sub_columns.is_some());

                if has_sub_columns {
                    // Sub-column aware path: detect nested arrays
                    let outer_iter = match values_arg.try_iter() {
                        Ok(iter) => iter,
                        Err(_) => {
                            let values = vec![values_arg.to_string()];
                            let formatted = self.format_row(&values);
                            return Ok(Value::from(formatted));
                        }
                    };

                    // Collect all values, detecting nested arrays for sub-column cells
                    let mut owned_values: Vec<OwnedCellValue> = Vec::new();
                    for (i, v) in outer_iter.enumerate() {
                        let is_sub_col = self
                            .columns
                            .get(i)
                            .and_then(|c| c.sub_columns.as_ref())
                            .is_some();

                        if is_sub_col {
                            if let Ok(inner_iter) = v.try_iter() {
                                let sub_vals: Vec<String> =
                                    inner_iter.map(|iv| iv.to_string()).collect();
                                owned_values.push(OwnedCellValue::Sub(sub_vals));
                            } else {
                                owned_values.push(OwnedCellValue::Single(v.to_string()));
                            }
                        } else {
                            owned_values.push(OwnedCellValue::Single(v.to_string()));
                        }
                    }

                    // Build CellValue references from owned data
                    let cell_values: Vec<CellValue<'_>> = owned_values
                        .iter()
                        .map(|ov| match ov {
                            OwnedCellValue::Single(s) => CellValue::Single(s.as_str()),
                            OwnedCellValue::Sub(v) => {
                                CellValue::Sub(v.iter().map(|s| s.as_str()).collect())
                            }
                        })
                        .collect();

                    let formatted = self.format_row_cells(&cell_values);
                    Ok(Value::from(formatted))
                } else {
                    // Fast path: no sub-columns, flatten all values to strings
                    let values: Vec<String> = match values_arg.try_iter() {
                        Ok(iter) => iter.map(|v| v.to_string()).collect(),
                        Err(_) => vec![values_arg.to_string()],
                    };

                    let formatted = self.format_row(&values);
                    Ok(Value::from(formatted))
                }
            }
            "column_width" => {
                // column_width(index) - get width of a specific column
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "column_width() requires an index argument",
                    ));
                }

                let index = args[0].as_usize().ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "column_width() index must be a number",
                    )
                })?;

                match self.column_width(index) {
                    Some(w) => Ok(Value::from(w)),
                    None => Ok(Value::from(())),
                }
            }
            _ => Err(minijinja::Error::new(
                minijinja::ErrorKind::UnknownMethod,
                format!("TabularFormatter has no method '{}'", name),
            )),
        }
    }
}

/// Format a single cell value according to column spec.
#[cfg(test)]
fn format_cell(value: &str, width: usize, col: &Column) -> String {
    format_cell_with_policy(value, width, col, AmbiguousWidth::Narrow)
}

fn format_cell_with_policy(
    value: &str,
    width: usize,
    col: &Column,
    policy: AmbiguousWidth,
) -> String {
    // If style_from_value is set, use the value as the style
    let style_override = if col.style_from_value {
        Some(value)
    } else {
        None
    };
    let style = style_override.or(col.style.as_deref());
    format_value_with_policy(value, width, col.align, &col.overflow, style, policy)
}

/// Format a value with the given width, alignment, overflow, and optional style.
///
/// This is the core formatting function used by both regular cells and sub-cells.
/// It correctly handles BBCode tags in `value`: tags are stripped for width
/// measurement and truncation, but preserved in the output when the content fits.
#[cfg(test)]
fn format_value(
    value: &str,
    width: usize,
    align: Align,
    overflow: &Overflow,
    style: Option<&str>,
) -> String {
    format_value_with_policy(value, width, align, overflow, style, AmbiguousWidth::Narrow)
}

fn format_value_with_policy(
    value: &str,
    width: usize,
    align: Align,
    overflow: &Overflow,
    style: Option<&str>,
    policy: AmbiguousWidth,
) -> String {
    if width == 0 {
        return String::new();
    }

    let stripped = standout_bbparser::strip_tags(value);
    let current_width = display_width_with_policy(&stripped, policy);

    if current_width > width {
        // Content overflows — truncate the stripped text (tags are lost)
        let truncated = match overflow {
            Overflow::Truncate { at, marker } => match at {
                TruncateAt::End => truncate_end_with_policy(&stripped, width, marker, policy),
                TruncateAt::Start => truncate_start_with_policy(&stripped, width, marker, policy),
                TruncateAt::Middle => truncate_middle_with_policy(&stripped, width, marker, policy),
            },
            Overflow::Clip => truncate_end_with_policy(&stripped, width, "", policy),
            Overflow::Expand => {
                // Don't truncate — pad is also skipped below
                return apply_style(value, style);
            }
            Overflow::Wrap { .. } => {
                // Single-line fallback; multi-line wrapping handled by format_cell_lines
                truncate_end_with_policy(&stripped, width, "…", policy)
            }
        };

        let padded = match align {
            Align::Left => pad_right_with_policy(&truncated, width, policy),
            Align::Right => pad_left_with_policy(&truncated, width, policy),
            Align::Center => pad_center_with_policy(&truncated, width, policy),
        };
        apply_style(&padded, style)
    } else {
        // Content fits — pad the original value (preserving tags) using manual spacing
        let padding = width - current_width;
        let padded = match align {
            Align::Left => format!("{}{}", value, " ".repeat(padding)),
            Align::Right => format!("{}{}", " ".repeat(padding), value),
            Align::Center => {
                let left_pad = padding / 2;
                let right_pad = padding - left_pad;
                format!("{}{}{}", " ".repeat(left_pad), value, " ".repeat(right_pad))
            }
        };
        apply_style(&padded, style)
    }
}

// ============================================================================
// Sub-Column Support
// ============================================================================

/// A cell value that may be a single string or a list of sub-values.
///
/// Used with [`TabularFormatter::format_row_cells`] for columns that have
/// sub-columns. Regular columns use `Single`, columns with sub-columns use `Sub`.
#[derive(Clone, Debug)]
pub enum CellValue<'a> {
    /// Single string value for a regular column.
    Single(&'a str),
    /// Multiple sub-values for a column with sub-columns.
    Sub(Vec<&'a str>),
}

impl<'a> From<&'a str> for CellValue<'a> {
    fn from(s: &'a str) -> Self {
        CellValue::Single(s)
    }
}

/// Owned version of CellValue for use in MiniJinja Object methods
/// where we need to own the string data before borrowing into CellValue.
pub(crate) enum OwnedCellValue {
    Single(String),
    Sub(Vec<String>),
}

/// Resolve sub-column widths for a single row.
///
/// Non-grower sub-columns get their content width clamped to bounds.
/// The grower gets all remaining space. Separators between zero-width
/// sub-columns are skipped.
///
/// Returns a Vec of resolved widths such that:
/// `sum(widths) + separator_overhead == parent_width`
#[cfg(test)]
fn resolve_sub_widths(sub_cols: &SubColumns, values: &[&str], parent_width: usize) -> Vec<usize> {
    resolve_sub_widths_with_policy(sub_cols, values, parent_width, AmbiguousWidth::Narrow)
}

fn resolve_sub_widths_with_policy(
    sub_cols: &SubColumns,
    values: &[&str],
    parent_width: usize,
    policy: AmbiguousWidth,
) -> Vec<usize> {
    let sep_width = display_width_with_policy(&sub_cols.separator, policy);
    let n = sub_cols.columns.len();
    let mut widths = vec![0usize; n];
    let mut grower_index = 0;

    // Pass 1: resolve non-grower widths from content
    for (i, sub_col) in sub_cols.columns.iter().enumerate() {
        match &sub_col.width {
            Width::Fill => {
                grower_index = i;
            }
            Width::Fixed(w) => {
                widths[i] = *w;
            }
            Width::Bounded { min, max } => {
                let content_w = values
                    .get(i)
                    .map(|v| visible_width_with_policy(v, policy))
                    .unwrap_or(0);
                let min_w = min.unwrap_or(0);
                let max_w = max.unwrap_or(usize::MAX);
                widths[i] = content_w.max(min_w).min(max_w);
            }
            Width::Fraction(_) => {} // validated away at construction
        }
    }

    // Pass 2: compute separator overhead
    // The grower is always "visible" for separator purposes—even at zero width it
    // participates in the join so separators flanking it are emitted.  Only truly
    // zero-width *non-grower* columns are elided.
    let visible_non_growers = widths
        .iter()
        .enumerate()
        .filter(|&(i, &w)| i != grower_index && w > 0)
        .count();
    let visible_count = visible_non_growers + 1; // +1 for grower
    let sep_overhead = visible_count.saturating_sub(1) * sep_width;
    let available = parent_width.saturating_sub(sep_overhead);

    // Pass 3: clamp non-grower total if it exceeds available content budget
    let non_grower_total: usize = widths
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != grower_index)
        .map(|(_, &w)| w)
        .sum();

    if non_grower_total > available {
        let mut excess = non_grower_total - available;
        // Shrink from the last non-grower backwards
        for i in (0..n).rev() {
            if i == grower_index || widths[i] == 0 || excess == 0 {
                continue;
            }
            let reduction = excess.min(widths[i]);
            widths[i] -= reduction;
            excess -= reduction;
        }
    }

    // Pass 4: grower absorbs remaining space
    let clamped_total: usize = widths
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != grower_index)
        .map(|(_, &w)| w)
        .sum();
    widths[grower_index] = available.saturating_sub(clamped_total);

    widths
}

/// Format a cell that has sub-columns, producing a string of exactly
/// `parent_width` display columns.
///
/// Sub-column widths are resolved per-row from actual content. The grower
/// sub-column absorbs remaining space. Zero-width *non-grower* columns are
/// skipped entirely (no separator emitted). The grower is always present in
/// the output—even at zero width—so separator counts stay correct.
#[cfg(test)]
fn format_sub_cells(sub_cols: &SubColumns, values: &[&str], parent_width: usize) -> String {
    format_sub_cells_with_policy(sub_cols, values, parent_width, AmbiguousWidth::Narrow)
}

fn format_sub_cells_with_policy(
    sub_cols: &SubColumns,
    values: &[&str],
    parent_width: usize,
    policy: AmbiguousWidth,
) -> String {
    if parent_width == 0 {
        return String::new();
    }

    let widths = resolve_sub_widths_with_policy(sub_cols, values, parent_width, policy);
    let grower_index = sub_cols
        .columns
        .iter()
        .position(|c| matches!(c.width, Width::Fill))
        .unwrap_or(0);
    let sep = &sub_cols.separator;
    let mut parts: Vec<String> = Vec::new();

    for (i, (sub_col, &width)) in sub_cols.columns.iter().zip(widths.iter()).enumerate() {
        // Skip zero-width non-grower columns (they produce no output and no separator)
        if width == 0 && i != grower_index {
            continue;
        }
        if width == 0 {
            // Grower at zero width: emit empty string so separators flanking it are correct
            parts.push(String::new());
        } else {
            let value = values.get(i).copied().unwrap_or(&sub_col.null_repr);
            parts.push(format_value_with_policy(
                value,
                width,
                sub_col.align,
                &sub_col.overflow,
                sub_col.style.as_deref(),
                policy,
            ));
        }
    }

    parts.join(sep)
}

/// Result of formatting a cell, which may be single or multi-line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellOutput {
    /// Single line of formatted text.
    Single(String),
    /// Multiple lines (from word-wrap).
    Multi(Vec<String>),
}

impl CellOutput {
    /// Returns true if this is a single-line output.
    pub fn is_single(&self) -> bool {
        matches!(self, CellOutput::Single(_))
    }

    /// Returns the number of lines.
    pub fn line_count(&self) -> usize {
        match self {
            CellOutput::Single(_) => 1,
            CellOutput::Multi(lines) => lines.len().max(1),
        }
    }

    /// Get a specific line, padding to width if needed.
    ///
    /// Content may contain BBCode from `apply_style`, so we use `visible_width`
    /// for measurement and manual padding to avoid miscounting tags.
    pub fn line(&self, index: usize, width: usize, align: Align) -> String {
        self.line_with_policy(index, width, align, AmbiguousWidth::Narrow)
    }

    /// Gets a line using an explicit ambiguous-width policy.
    pub fn line_with_policy(
        &self,
        index: usize,
        width: usize,
        align: Align,
        policy: AmbiguousWidth,
    ) -> String {
        let content = match self {
            CellOutput::Single(s) if index == 0 => s.as_str(),
            CellOutput::Multi(lines) => lines.get(index).map(|s| s.as_str()).unwrap_or(""),
            _ => "",
        };

        let content_width = visible_width_with_policy(content, policy);
        if content_width >= width {
            return content.to_string();
        }
        let padding = width - content_width;
        match align {
            Align::Left => format!("{}{}", content, " ".repeat(padding)),
            Align::Right => format!("{}{}", " ".repeat(padding), content),
            Align::Center => {
                let left_pad = padding / 2;
                let right_pad = padding - left_pad;
                format!(
                    "{}{}{}",
                    " ".repeat(left_pad),
                    content,
                    " ".repeat(right_pad)
                )
            }
        }
    }

    /// Convert to a single string (first line for Multi).
    pub fn to_single(&self) -> String {
        match self {
            CellOutput::Single(s) => s.clone(),
            CellOutput::Multi(lines) => lines.first().cloned().unwrap_or_default(),
        }
    }
}

/// Wrap content with style tags if a style is specified.
fn apply_style(content: &str, style: Option<&str>) -> String {
    match style {
        Some(s) if !s.is_empty() => format!("[{}]{}[/{}]", s, content, s),
        _ => content.to_string(),
    }
}

/// Format a cell with potential multi-line output (for Wrap mode).
#[cfg(test)]
fn format_cell_lines(value: &str, width: usize, col: &Column) -> CellOutput {
    format_cell_lines_with_policy(value, width, col, AmbiguousWidth::Narrow)
}

fn format_cell_lines_with_policy(
    value: &str,
    width: usize,
    col: &Column,
    policy: AmbiguousWidth,
) -> CellOutput {
    if width == 0 {
        return CellOutput::Single(String::new());
    }

    let stripped = standout_bbparser::strip_tags(value);
    let current_width = display_width_with_policy(&stripped, policy);

    // Determine style: style_from_value takes precedence
    let style = if col.style_from_value {
        Some(value)
    } else {
        col.style.as_deref()
    };

    match &col.overflow {
        Overflow::Wrap { indent } => {
            if current_width <= width {
                // Fits on one line — pad original value (preserving tags) manually
                let padding = width - current_width;
                let padded = match col.align {
                    Align::Left => format!("{}{}", value, " ".repeat(padding)),
                    Align::Right => format!("{}{}", " ".repeat(padding), value),
                    Align::Center => {
                        let left_pad = padding / 2;
                        let right_pad = padding - left_pad;
                        format!("{}{}{}", " ".repeat(left_pad), value, " ".repeat(right_pad))
                    }
                };
                CellOutput::Single(apply_style(&padded, style))
            } else {
                // Wrap to multiple lines — tags are stripped (same as truncation)
                let wrapped = wrap_indent_with_policy(&stripped, width, *indent, policy);
                let padded: Vec<String> = wrapped
                    .into_iter()
                    .map(|line| {
                        let padded_line = match col.align {
                            Align::Left => pad_right_with_policy(&line, width, policy),
                            Align::Right => pad_left_with_policy(&line, width, policy),
                            Align::Center => pad_center_with_policy(&line, width, policy),
                        };
                        apply_style(&padded_line, style)
                    })
                    .collect();
                if padded.len() == 1 {
                    CellOutput::Single(padded.into_iter().next().unwrap())
                } else {
                    CellOutput::Multi(padded)
                }
            }
        }
        // All other modes are single-line
        _ => CellOutput::Single(format_cell_with_policy(value, width, col, policy)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::display_width;
    use crate::tabular::{TabularSpec, Width};

    fn simple_spec() -> FlatDataSpec {
        FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator(" | ")
            .build()
    }

    #[test]
    fn format_basic_row() {
        let formatter = TabularFormatter::new(&simple_spec(), 80);
        let output = formatter.format_row(&["Hello", "World"]);
        assert_eq!(output, "Hello      | World   ");
    }

    #[test]
    fn format_row_with_truncation() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(8)))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello World"]);
        assert_eq!(output, "Hello W…");
    }

    #[test]
    fn format_row_right_align() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).align(Align::Right))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["42"]);
        assert_eq!(output, "        42");
    }

    #[test]
    fn format_row_center_align() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).align(Align::Center))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["hi"]);
        assert_eq!(output, "    hi    ");
    }

    #[test]
    fn format_row_truncate_start() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).truncate(TruncateAt::Start))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["/path/to/file.rs"]);
        assert_eq!(display_width(&output), 10);
        assert!(output.starts_with("…"));
    }

    #[test]
    fn format_row_truncate_middle() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).truncate(TruncateAt::Middle))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["abcdefghijklmno"]);
        assert_eq!(display_width(&output), 10);
        assert!(output.contains("…"));
    }

    #[test]
    fn format_row_with_null() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)).null_repr("N/A"))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        // Only provide first value - second uses null_repr
        let output = formatter.format_row(&["value"]);
        assert!(output.contains("N/A"));
    }

    #[test]
    fn format_row_with_decorations() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator(" │ ")
            .prefix("│ ")
            .suffix(" │")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello", "World"]);
        assert!(output.starts_with("│ "));
        assert!(output.ends_with(" │"));
        assert!(output.contains(" │ "));
    }

    #[test]
    fn format_multiple_rows() {
        let formatter = TabularFormatter::new(&simple_spec(), 80);
        let rows = vec![vec!["a", "1"], vec!["b", "2"], vec!["c", "3"]];

        let output = formatter.format_rows(&rows);
        assert_eq!(output.len(), 3);
    }

    #[test]
    fn format_row_fill_column() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(5)))
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fixed(5)))
            .separator("  ")
            .build();

        // Total: 30, overhead: 4 (2 separators), fixed: 10, fill: 16
        let formatter = TabularFormatter::new(&spec, 30);
        let _output = formatter.format_row(&["abc", "middle", "xyz"]);

        // Check that widths are as expected
        assert_eq!(formatter.widths(), &[5, 16, 5]);
    }

    #[test]
    fn formatter_accessors() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        assert_eq!(formatter.num_columns(), 2);
        assert_eq!(formatter.column_width(0), Some(10));
        assert_eq!(formatter.column_width(1), Some(8));
        assert_eq!(formatter.column_width(2), None);
    }

    #[test]
    fn format_empty_spec() {
        let spec = FlatDataSpec::builder().build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row::<&str>(&[]);
        assert_eq!(output, "");
    }

    #[test]
    fn format_with_ansi() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let styled = "\x1b[31mred\x1b[0m";
        let output = formatter.format_row(&[styled]);

        // ANSI codes should be preserved, display width should be 10
        assert!(output.contains("\x1b[31m"));
        assert_eq!(display_width(&output), 10);
    }

    #[test]
    fn format_with_explicit_widths() {
        let columns = vec![Column::new(Width::Fixed(5)), Column::new(Width::Fixed(10))];
        let formatter = TabularFormatter::with_widths(columns, vec![5, 10]).separator(" - ");

        let output = formatter.format_row(&["hi", "there"]);
        assert_eq!(output, "hi    - there     ");
    }

    #[test]
    fn explicit_width_constructor_accepts_ambiguous_width_policy() {
        let formatter = TabularFormatter::with_widths_and_ambiguous_width(
            vec![Column::new(Width::Fixed(4))],
            vec![4],
            AmbiguousWidth::Wide,
        );

        assert_eq!(formatter.ambiguous_width(), AmbiguousWidth::Wide);
        assert_eq!(formatter.format_row(&["≈"]), "≈  ");
    }

    // ============================================================================
    // Object Trait Tests
    // ============================================================================

    #[test]
    fn object_get_num_columns() {
        let formatter = Arc::new(TabularFormatter::new(&simple_spec(), 80));
        let value = formatter.get_value(&Value::from("num_columns"));
        assert_eq!(value, Some(Value::from(2)));
    }

    #[test]
    fn object_get_widths() {
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .build();
        let formatter = Arc::new(TabularFormatter::new(&spec, 80));

        let value = formatter.get_value(&Value::from("widths"));
        assert!(value.is_some());
        let widths = value.unwrap();
        // Check we can iterate over the widths
        assert!(widths.try_iter().is_ok());
    }

    #[test]
    fn object_get_separator() {
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .separator(" | ")
            .build();
        let formatter = Arc::new(TabularFormatter::new(&spec, 80));

        let value = formatter.get_value(&Value::from("separator"));
        assert_eq!(value, Some(Value::from(" | ")));
    }

    #[test]
    fn object_get_unknown_returns_none() {
        let formatter = Arc::new(TabularFormatter::new(&simple_spec(), 80));
        let value = formatter.get_value(&Value::from("unknown"));
        assert_eq!(value, None);
    }

    #[test]
    fn object_row_method_via_template() {
        use minijinja::Environment;

        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator(" | ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let mut env = Environment::new();
        env.add_template("test", "{{ table.row(['Hello', 'World']) }}")
            .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! { table => Value::from_object(formatter) })
            .unwrap();

        assert_eq!(output, "Hello      | World   ");
    }

    #[test]
    fn object_row_method_in_loop() {
        use minijinja::Environment;

        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(8)))
            .column(Column::new(Width::Fixed(6)))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let mut env = Environment::new();
        env.add_template(
            "test",
            "{% for item in items %}{{ table.row([item.name, item.value]) }}\n{% endfor %}",
        )
        .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! {
                table => Value::from_object(formatter),
                items => vec![
                    minijinja::context! { name => "Alice", value => "100" },
                    minijinja::context! { name => "Bob", value => "200" },
                ]
            })
            .unwrap();

        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
    }

    #[test]
    fn object_column_width_method_via_template() {
        use minijinja::Environment;

        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let mut env = Environment::new();
        env.add_template(
            "test",
            "{{ table.column_width(0) }}-{{ table.column_width(1) }}",
        )
        .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! { table => Value::from_object(formatter) })
            .unwrap();

        assert_eq!(output, "10-8");
    }

    #[test]
    fn object_attribute_access_via_template() {
        use minijinja::Environment;

        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator(" | ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let mut env = Environment::new();
        env.add_template(
            "test",
            "cols={{ table.num_columns }}, sep='{{ table.separator }}'",
        )
        .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! { table => Value::from_object(formatter) })
            .unwrap();

        assert_eq!(output, "cols=2, sep=' | '");
    }

    // ============================================================================
    // Overflow Mode Tests (Phase 4)
    // ============================================================================

    #[test]
    fn format_cell_clip_no_marker() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(5)).clip())
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello World"]);
        // Clip truncates without marker
        assert_eq!(display_width(&output), 5);
        assert!(!output.contains("…"));
        assert!(output.starts_with("Hello"));
    }

    #[test]
    fn format_cell_expand_overflows() {
        // Expand mode lets content overflow
        let col = Column::new(Width::Fixed(5)).overflow(Overflow::Expand);
        let output = format_cell("Hello World", 5, &col);

        // Should NOT be truncated
        assert_eq!(output, "Hello World");
        assert_eq!(display_width(&output), 11); // Full width
    }

    #[test]
    fn format_cell_expand_pads_when_short() {
        let col = Column::new(Width::Fixed(10)).overflow(Overflow::Expand);
        let output = format_cell("Hi", 10, &col);

        // Should be padded to width
        assert_eq!(output, "Hi        ");
        assert_eq!(display_width(&output), 10);
    }

    #[test]
    fn format_cell_wrap_single_line() {
        // Content fits, no wrapping needed
        let col = Column::new(Width::Fixed(20)).wrap();
        let output = format_cell_lines("Short text", 20, &col);

        assert!(output.is_single());
        assert_eq!(output.line_count(), 1);
        assert_eq!(display_width(&output.to_single()), 20);
    }

    #[test]
    fn format_cell_wrap_multi_line() {
        let col = Column::new(Width::Fixed(10)).wrap();
        let output = format_cell_lines("This is a longer text that wraps", 10, &col);

        assert!(!output.is_single());
        assert!(output.line_count() > 1);

        // Each line should be padded to width
        if let CellOutput::Multi(lines) = &output {
            for line in lines {
                assert_eq!(display_width(line), 10);
            }
        }
    }

    #[test]
    fn format_cell_wrap_with_indent() {
        let col = Column::new(Width::Fixed(15)).overflow(Overflow::Wrap { indent: 2 });
        let output = format_cell_lines("First line then continuation", 15, &col);

        if let CellOutput::Multi(lines) = output {
            // First line should start normally
            assert!(lines[0].starts_with("First"));
            // Subsequent lines should be indented
            if lines.len() > 1 {
                // The line content should start with spaces due to indent
                let second_trimmed = lines[1].trim_start();
                assert!(lines[1].len() > second_trimmed.len()); // Has leading spaces
            }
        }
    }

    #[test]
    fn format_row_lines_single_line() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let lines = formatter.format_row_lines(&["Hello", "World"]);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], formatter.format_row(&["Hello", "World"]));
    }

    #[test]
    fn format_row_lines_multi_line() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(8)).wrap())
            .column(Column::new(Width::Fixed(6)))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let lines = formatter.format_row_lines(&["This is long", "Short"]);

        // Should have multiple lines due to wrapping
        assert!(!lines.is_empty());

        // Each line should have consistent width
        let expected_width = display_width(&lines[0]);
        for line in &lines {
            assert_eq!(display_width(line), expected_width);
        }
    }

    #[test]
    fn format_row_lines_mixed_columns() {
        // One column wraps, others don't
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(6))) // truncates
            .column(Column::new(Width::Fixed(10)).wrap()) // wraps
            .column(Column::new(Width::Fixed(4))) // truncates
            .separator(" ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let lines = formatter.format_row_lines(&["aaaaa", "this text wraps here", "bbbb"]);

        // Multiple lines due to middle column wrapping
        assert!(!lines.is_empty());
    }

    // ============================================================================
    // CellOutput Tests
    // ============================================================================

    #[test]
    fn cell_output_single_accessors() {
        let cell = CellOutput::Single("Hello".to_string());

        assert!(cell.is_single());
        assert_eq!(cell.line_count(), 1);
        assert_eq!(cell.to_single(), "Hello");
    }

    #[test]
    fn cell_output_multi_accessors() {
        let cell = CellOutput::Multi(vec!["Line 1".to_string(), "Line 2".to_string()]);

        assert!(!cell.is_single());
        assert_eq!(cell.line_count(), 2);
        assert_eq!(cell.to_single(), "Line 1");
    }

    #[test]
    fn cell_output_line_accessor() {
        let cell = CellOutput::Multi(vec!["First".to_string(), "Second".to_string()]);

        // Get first line, padded to 10
        let line0 = cell.line(0, 10, Align::Left);
        assert_eq!(line0, "First     ");
        assert_eq!(display_width(&line0), 10);

        // Get second line
        let line1 = cell.line(1, 10, Align::Right);
        assert_eq!(line1, "    Second");

        // Out of bounds returns empty padded
        let line2 = cell.line(2, 10, Align::Left);
        assert_eq!(line2, "          ");
    }

    // ============================================================================
    // Anchor Tests (Phase 5)
    // ============================================================================

    #[test]
    fn format_row_all_left_anchor_no_gap() {
        // All columns left-anchored - no gap inserted
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(5)))
            .column(Column::new(Width::Fixed(5)))
            .separator(" ")
            .build();
        let formatter = TabularFormatter::new(&spec, 50);

        let output = formatter.format_row(&["A", "B"]);
        // Total content: 5 + 1 + 5 = 11, no gap
        assert_eq!(output, "A     B    ");
        assert_eq!(display_width(&output), 11);
    }

    #[test]
    fn format_row_with_right_anchor() {
        // Left column + right column with gap
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(5))) // left-anchored
            .column(Column::new(Width::Fixed(5)).anchor_right()) // right-anchored
            .separator(" ")
            .build();

        // Total: 30, content: 5 + 5 = 10, sep: 1, overhead: 11
        // Gap: 30 - 11 + 1 = 20 (replaces separator)
        let formatter = TabularFormatter::new(&spec, 30);

        let output = formatter.format_row(&["L", "R"]);
        assert_eq!(display_width(&output), 30);
        // Left content at start, right content at end
        assert!(output.starts_with("L    "));
        assert!(output.ends_with("R    "));
    }

    #[test]
    fn format_row_with_right_anchor_exact_fit() {
        // When total_width equals content width, no gap
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(10)).anchor_right())
            .separator("  ")
            .build();

        // Total: 22 (10 + 2 + 10), no extra space
        let formatter = TabularFormatter::new(&spec, 22);

        let output = formatter.format_row(&["Left", "Right"]);
        assert_eq!(display_width(&output), 22);
        // Normal separator, no gap
        assert!(output.contains("  ")); // Original separator preserved
    }

    #[test]
    fn format_row_all_right_anchor_no_gap() {
        // All columns right-anchored - no gap needed
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(5)).anchor_right())
            .column(Column::new(Width::Fixed(5)).anchor_right())
            .separator(" ")
            .build();
        let formatter = TabularFormatter::new(&spec, 50);

        let output = formatter.format_row(&["A", "B"]);
        // No transition from left to right, so no gap
        assert_eq!(output, "A     B    ");
    }

    #[test]
    fn format_row_multiple_anchors() {
        // Two left, two right
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(4))) // L1
            .column(Column::new(Width::Fixed(4))) // L2
            .column(Column::new(Width::Fixed(4)).anchor_right()) // R1
            .column(Column::new(Width::Fixed(4)).anchor_right()) // R2
            .separator(" ")
            .build();

        // Content: 4*4 = 16, seps: 3, overhead: 19
        // Total: 40, gap: 40 - 19 + 1 = 22
        let formatter = TabularFormatter::new(&spec, 40);

        let output = formatter.format_row(&["A", "B", "C", "D"]);
        assert_eq!(display_width(&output), 40);
        // Left group at start, right group at end
        assert!(output.starts_with("A    B   "));
    }

    #[test]
    fn calculate_anchor_gap_no_transition() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(10)))
            .build();
        let formatter = TabularFormatter::new(&spec, 50);

        let (gap, transition) = formatter.calculate_anchor_gap();
        assert_eq!(transition, 2); // No right-anchored columns
        assert_eq!(gap, 0);
    }

    #[test]
    fn calculate_anchor_gap_with_transition() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(10)).anchor_right())
            .separator(" ")
            .build();
        let formatter = TabularFormatter::new(&spec, 50);

        let (gap, transition) = formatter.calculate_anchor_gap();
        assert_eq!(transition, 1); // Second column is right-anchored
        assert!(gap > 0);
    }

    #[test]
    fn format_row_lines_with_anchor() {
        // Multi-line output should also respect anchors
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(8)).wrap())
            .column(Column::new(Width::Fixed(6)).anchor_right())
            .separator(" ")
            .build();
        let formatter = TabularFormatter::new(&spec, 40);

        let lines = formatter.format_row_lines(&["This is text", "Right"]);

        // All lines should have consistent width due to anchor
        for line in &lines {
            assert_eq!(display_width(line), 40);
        }
    }

    // ============================================================================
    // Struct Extraction Tests (Phase 6)
    // ============================================================================

    #[test]
    fn row_from_simple_struct() {
        #[derive(Serialize)]
        struct Record {
            name: String,
            value: i32,
        }

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).key("name"))
            .column(Column::new(Width::Fixed(5)).key("value"))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let record = Record {
            name: "Test".to_string(),
            value: 42,
        };

        let row = formatter.row_from(&record);
        assert!(row.contains("Test"));
        assert!(row.contains("42"));
    }

    #[test]
    fn row_from_uses_name_as_fallback() {
        #[derive(Serialize)]
        struct Item {
            title: String,
        }

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(15)).named("title"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let item = Item {
            title: "Hello".to_string(),
        };

        let row = formatter.row_from(&item);
        assert!(row.contains("Hello"));
    }

    #[test]
    fn row_from_nested_field() {
        #[derive(Serialize)]
        struct User {
            email: String,
        }

        #[derive(Serialize)]
        struct Record {
            user: User,
            status: String,
        }

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(20)).key("user.email"))
            .column(Column::new(Width::Fixed(10)).key("status"))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let record = Record {
            user: User {
                email: "test@example.com".to_string(),
            },
            status: "active".to_string(),
        };

        let row = formatter.row_from(&record);
        assert!(row.contains("test@example.com"));
        assert!(row.contains("active"));
    }

    #[test]
    fn row_from_array_index() {
        #[derive(Serialize)]
        struct Record {
            items: Vec<String>,
        }

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).key("items.0"))
            .column(Column::new(Width::Fixed(10)).key("items.1"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let record = Record {
            items: vec!["First".to_string(), "Second".to_string()],
        };

        let row = formatter.row_from(&record);
        assert!(row.contains("First"));
        assert!(row.contains("Second"));
    }

    #[test]
    fn row_from_missing_field_uses_null_repr() {
        #[derive(Serialize)]
        struct Record {
            present: String,
        }

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).key("present"))
            .column(Column::new(Width::Fixed(10)).key("missing").null_repr("-"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let record = Record {
            present: "value".to_string(),
        };

        let row = formatter.row_from(&record);
        assert!(row.contains("value"));
        // Missing field should show empty (extract_field returns empty string)
    }

    #[test]
    fn row_from_no_key_uses_null_repr() {
        #[derive(Serialize)]
        struct Record {
            value: String,
        }

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).null_repr("N/A"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let record = Record {
            value: "test".to_string(),
        };

        let row = formatter.row_from(&record);
        assert!(row.contains("N/A"));
    }

    #[test]
    fn row_from_various_types() {
        #[derive(Serialize)]
        struct Record {
            string_val: String,
            int_val: i64,
            float_val: f64,
            bool_val: bool,
        }

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).key("string_val"))
            .column(Column::new(Width::Fixed(10)).key("int_val"))
            .column(Column::new(Width::Fixed(10)).key("float_val"))
            .column(Column::new(Width::Fixed(10)).key("bool_val"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let record = Record {
            string_val: "text".to_string(),
            int_val: 123,
            float_val: 9.87,
            bool_val: true,
        };

        let row = formatter.row_from(&record);
        assert!(row.contains("text"));
        assert!(row.contains("123"));
        assert!(row.contains("9.87"));
        assert!(row.contains("true"));
    }

    #[test]
    fn extract_field_simple() {
        let json = serde_json::json!({
            "name": "Alice",
            "age": 30
        });

        assert_eq!(extract_field(&json, "name"), "Alice");
        assert_eq!(extract_field(&json, "age"), "30");
        assert_eq!(extract_field(&json, "missing"), "");
    }

    #[test]
    fn extract_field_nested() {
        let json = serde_json::json!({
            "user": {
                "profile": {
                    "email": "test@example.com"
                }
            }
        });

        assert_eq!(
            extract_field(&json, "user.profile.email"),
            "test@example.com"
        );
        assert_eq!(extract_field(&json, "user.missing"), "");
    }

    #[test]
    fn extract_field_array() {
        let json = serde_json::json!({
            "items": ["a", "b", "c"]
        });

        assert_eq!(extract_field(&json, "items.0"), "a");
        assert_eq!(extract_field(&json, "items.1"), "b");
        assert_eq!(extract_field(&json, "items.10"), ""); // Out of bounds
    }

    #[test]
    fn row_lines_from_struct() {
        #[derive(Serialize)]
        struct Record {
            description: String,
            status: String,
        }

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).key("description").wrap())
            .column(Column::new(Width::Fixed(6)).key("status"))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let record = Record {
            description: "A longer description that wraps".to_string(),
            status: "OK".to_string(),
        };

        let lines = formatter.row_lines_from(&record);
        // Should have multiple lines due to wrapping
        assert!(!lines.is_empty());
    }

    // ============================================================================
    // Style Tests (Phase 7)
    // ============================================================================

    #[test]
    fn format_cell_with_style() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).style("header"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello"]);
        // Should wrap in style tags
        assert!(output.starts_with("[header]"));
        assert!(output.ends_with("[/header]"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn format_cell_style_from_value() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).style_from_value())
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["error"]);
        // Value "error" becomes the style
        assert!(output.contains("[error]"));
        assert!(output.contains("[/error]"));
    }

    #[test]
    fn format_cell_no_style() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello"]);
        // No style tags
        assert!(!output.contains("["));
        assert!(!output.contains("]"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn format_cell_style_overrides_style_from_value() {
        // When both style and style_from_value are set, style_from_value wins
        let mut col = Column::new(Width::Fixed(10));
        col.style = Some("default".to_string());
        col.style_from_value = true;

        let spec = FlatDataSpec::builder().column(col).build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["custom"]);
        // style_from_value takes precedence
        assert!(output.contains("[custom]"));
        assert!(output.contains("[/custom]"));
    }

    #[test]
    fn format_row_multiple_styled_columns() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(8)).style("name"))
            .column(Column::new(Width::Fixed(8)).style("status"))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Alice", "Active"]);
        assert!(output.contains("[name]"));
        assert!(output.contains("[status]"));
    }

    #[test]
    fn format_cell_lines_with_style() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).wrap().style("text"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let lines = formatter.format_row_lines(&["This is a long text that wraps"]);

        // Each line should have style tags
        for line in &lines {
            assert!(line.contains("[text]"));
            assert!(line.contains("[/text]"));
        }
    }

    // ============================================================================
    // Extract Headers Tests
    // ============================================================================

    #[test]
    fn extract_headers_from_header_field() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).header("Name"))
            .column(Column::new(Width::Fixed(8)).header("Status"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let headers = formatter.extract_headers();
        assert_eq!(headers, vec!["Name", "Status"]);
    }

    #[test]
    fn extract_headers_fallback_to_key() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).key("user_name"))
            .column(Column::new(Width::Fixed(8)).key("status"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let headers = formatter.extract_headers();
        assert_eq!(headers, vec!["user_name", "status"]);
    }

    #[test]
    fn extract_headers_fallback_to_name() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).named("col1"))
            .column(Column::new(Width::Fixed(8)).named("col2"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let headers = formatter.extract_headers();
        assert_eq!(headers, vec!["col1", "col2"]);
    }

    #[test]
    fn extract_headers_priority_order() {
        // header > key > name > ""
        let spec = FlatDataSpec::builder()
            .column(
                Column::new(Width::Fixed(10))
                    .header("Header")
                    .key("key")
                    .named("name"),
            )
            .column(
                Column::new(Width::Fixed(10))
                    .key("key_only")
                    .named("name_only"),
            )
            .column(Column::new(Width::Fixed(10)).named("name_only"))
            .column(Column::new(Width::Fixed(10))) // No header, key, or name
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let headers = formatter.extract_headers();
        assert_eq!(headers, vec!["Header", "key_only", "name_only", ""]);
    }

    #[test]
    fn extract_headers_empty_spec() {
        let spec = FlatDataSpec::builder().build();
        let formatter = TabularFormatter::new(&spec, 80);

        let headers = formatter.extract_headers();
        assert!(headers.is_empty());
    }

    // ============================================================================
    // Sub-Column Tests
    // ============================================================================

    use crate::tabular::{SubCol, SubColumns};

    fn padz_spec() -> (FlatDataSpec, SubColumns) {
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 20).right()], " ").unwrap();

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(4)))
            .column(Column::new(Width::Fill).sub_columns(sub_cols.clone()))
            .column(Column::new(Width::Fixed(6)).right())
            .separator("  ")
            .build();

        (spec, sub_cols)
    }

    #[test]
    fn sub_column_basic_title_and_tag() {
        let (spec, _) = padz_spec();
        let formatter = TabularFormatter::new(&spec, 60);

        let row = formatter.format_row_cells(&[
            CellValue::Single("1."),
            CellValue::Sub(vec!["Gallery Navigation", "[feature]"]),
            CellValue::Single("4d"),
        ]);

        assert!(row.contains("Gallery Navigation"));
        assert!(row.contains("[feature]"));
        assert!(row.contains("1."));
        assert!(row.contains("4d"));
        assert_eq!(display_width(&row), 60);
    }

    #[test]
    fn sub_column_tag_absent() {
        let (spec, _) = padz_spec();
        let formatter = TabularFormatter::new(&spec, 60);

        let row = formatter.format_row_cells(&[
            CellValue::Single("3."),
            CellValue::Sub(vec!["Fixing Layout of Image Nav", ""]),
            CellValue::Single("4d"),
        ]);

        assert!(row.contains("Fixing Layout of Image Nav"));
        // With empty tag, title should get all the space
        assert_eq!(display_width(&row), 60);
    }

    #[test]
    fn sub_column_grower_gets_remaining_space() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::fixed(10)], "  ").unwrap();

        let widths = resolve_sub_widths(&sub_cols, &["title", "fixed"], 50);
        // fixed=10, sep=2, grower=50-10-2=38
        assert_eq!(widths[0], 38);
        assert_eq!(widths[1], 10);
    }

    #[test]
    fn sub_column_non_grower_respects_fixed() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::fixed(15)], " ").unwrap();

        let widths = resolve_sub_widths(&sub_cols, &["x", "y"], 40);
        assert_eq!(widths[1], 15); // Always exact for Fixed
        assert_eq!(widths[0], 24); // 40 - 15 - 1
    }

    #[test]
    fn sub_column_non_grower_respects_bounded() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::bounded(5, 20)], " ").unwrap();

        // Content "short" is 5 chars, clamped to [5, 20] = 5
        let widths = resolve_sub_widths(&sub_cols, &["title", "short"], 40);
        assert_eq!(widths[1], 5);
        assert_eq!(widths[0], 34); // 40 - 5 - 1

        // Content "a very long tag value" is 21 chars, clamped to 20
        let widths2 = resolve_sub_widths(&sub_cols, &["title", "a very long tag value!"], 40);
        assert_eq!(widths2[1], 20);
        assert_eq!(widths2[0], 19); // 40 - 20 - 1

        // Content "" is 0 chars, clamped to min=5
        let widths3 = resolve_sub_widths(&sub_cols, &["title", ""], 40);
        assert_eq!(widths3[1], 5);
    }

    #[test]
    fn sub_column_bounded_min_zero() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 20)], " ").unwrap();

        // Empty content, min=0 means the sub-column gets 0 width
        let widths = resolve_sub_widths(&sub_cols, &["title", ""], 40);
        assert_eq!(widths[1], 0);
        // With zero-width non-grower, grower gets all space
        // visible count = 0 non-zero non-grower + 1 grower = 1, seps = 0
        assert_eq!(widths[0], 40);
    }

    #[test]
    fn sub_column_separator_skipped_for_zero_width() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 20)], "  ").unwrap();

        // With non-zero tag
        let result1 = format_sub_cells(&sub_cols, &["Title", "tag"], 30);
        assert!(result1.contains("  ")); // Separator present
        assert_eq!(display_width(&result1), 30);

        // With zero-width tag
        let result2 = format_sub_cells(&sub_cols, &["Title", ""], 30);
        assert_eq!(display_width(&result2), 30);
        // No wasted separator space
    }

    #[test]
    fn sub_column_alignment() {
        let sub_cols = SubColumns::new(
            vec![
                SubCol::fill(), // left-aligned by default
                SubCol::fixed(10).right(),
            ],
            " ",
        )
        .unwrap();

        let result = format_sub_cells(&sub_cols, &["Left", "Right"], 30);
        // "Left" should be left-aligned (padded right)
        assert!(result.starts_with("Left"));
        // "Right" should be right-aligned (padded left)
        assert!(result.ends_with("     Right"));
        assert_eq!(display_width(&result), 30);
    }

    #[test]
    fn sub_column_grower_truncation() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::fixed(15)], " ").unwrap();

        // Parent=25, fixed=15, sep=1, grower=9
        // Title "A very long title that exceeds" is 30 chars, gets truncated to 9
        let result = format_sub_cells(
            &sub_cols,
            &["A very long title that exceeds", "fixed-col"],
            25,
        );
        assert_eq!(display_width(&result), 25);
        assert!(result.contains("…")); // Truncation marker
    }

    #[test]
    fn sub_column_style_application() {
        let sub_cols = SubColumns::new(
            vec![SubCol::fill(), SubCol::bounded(0, 20).right().style("tag")],
            " ",
        )
        .unwrap();

        let result = format_sub_cells(&sub_cols, &["Title", "feature"], 40);
        assert!(result.contains("[tag]"));
        assert!(result.contains("[/tag]"));
        assert!(result.contains("feature"));
    }

    #[test]
    fn sub_column_grower_zero_width() {
        // When non-grower widths eat everything, the fixed col is clamped
        // so total still fits: grower(0) + sep(1) + fixed(19) = 20
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::fixed(20)], " ").unwrap();

        let widths = resolve_sub_widths(&sub_cols, &["title", "fixed"], 20);
        assert_eq!(widths[0], 0); // Grower gets nothing
        assert_eq!(widths[1], 19); // Clamped: 20 - 1 sep = 19 available

        // Output still has correct width
        let result = format_sub_cells(&sub_cols, &["title", "fixed"], 20);
        assert_eq!(display_width(&result), 20);
    }

    #[test]
    fn sub_column_all_empty() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 20)], " ").unwrap();

        let result = format_sub_cells(&sub_cols, &["", ""], 30);
        assert_eq!(display_width(&result), 30);
    }

    #[test]
    fn sub_column_plain_string_fallback() {
        // When format_row gets a plain string for a sub-column column,
        // it should wrap it as Single and use the grower
        let (spec, _) = padz_spec();
        let formatter = TabularFormatter::new(&spec, 60);

        let row = formatter.format_row(&["1.", "Just a title", "4d"]);
        // Should still produce valid output
        assert_eq!(display_width(&row), 60);
        assert!(row.contains("Just a title"));
    }

    #[test]
    fn sub_column_format_row_cells_api() {
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 15).right()], " ").unwrap();

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(3)))
            .column(Column::new(Width::Fill).sub_columns(sub_cols))
            .separator("  ")
            .build();

        let formatter = TabularFormatter::new(&spec, 50);

        // Row with tag
        let row1 = formatter.format_row_cells(&[
            CellValue::Single("1."),
            CellValue::Sub(vec!["Title", "[bug]"]),
        ]);
        assert_eq!(display_width(&row1), 50);
        assert!(row1.contains("Title"));
        assert!(row1.contains("[bug]"));

        // Row without tag
        let row2 = formatter.format_row_cells(&[
            CellValue::Single("2."),
            CellValue::Sub(vec!["Longer Title Here", ""]),
        ]);
        assert_eq!(display_width(&row2), 50);
        assert!(row2.contains("Longer Title Here"));
    }

    #[test]
    fn sub_column_via_template() {
        use minijinja::Environment;

        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 15).right()], " ").unwrap();

        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(4)))
            .column(Column::new(Width::Fill).sub_columns(sub_cols))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 50);

        let mut env = Environment::new();
        env.add_template("test", "{{ t.row(['1.', ['My Title', '[tag]']]) }}")
            .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! { t => Value::from_object(formatter) })
            .unwrap();

        assert_eq!(display_width(&output), 50);
        assert!(output.contains("My Title"));
        assert!(output.contains("[tag]"));
    }

    #[test]
    fn sub_column_multiple_rows_alignment() {
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 15).right()], " ").unwrap();

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(4)))
            .column(Column::new(Width::Fill).sub_columns(sub_cols))
            .column(Column::new(Width::Fixed(4)).right())
            .separator("  ")
            .build();

        let formatter = TabularFormatter::new(&spec, 60);

        let rows = [
            vec![
                CellValue::Single("1."),
                CellValue::Sub(vec!["GitHub integration", "[feature]"]),
                CellValue::Single("8h"),
            ],
            vec![
                CellValue::Single("2."),
                CellValue::Sub(vec!["Bug : Static", "[bug]"]),
                CellValue::Single("4d"),
            ],
            vec![
                CellValue::Single("3."),
                CellValue::Sub(vec!["Fixing Layout of Image Nav", ""]),
                CellValue::Single("4d"),
            ],
        ];

        for (i, row) in rows.iter().enumerate() {
            let output = formatter.format_row_cells(row);
            assert_eq!(
                display_width(&output),
                60,
                "Row {} has wrong width: '{}'",
                i,
                output
            );
        }
    }
    // ============================================================================
    // BBCode-aware width tests (issue #104)
    // ============================================================================

    #[test]
    fn format_value_bbcode_preserves_tags_when_fitting() {
        let overflow = Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        };
        // "hello" is 5 visible chars, column is 10 — tags should be preserved
        let result = format_value("[bold]hello[/bold]", 10, Align::Left, &overflow, None);
        let stripped = standout_bbparser::strip_tags(&result);
        assert_eq!(display_width(&stripped), 10, "visible width should be 10");
        assert!(
            result.contains("[bold]hello[/bold]"),
            "tags should be preserved when content fits"
        );
    }

    #[test]
    fn format_value_bbcode_truncation() {
        let overflow = Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        };
        // "[red]hello world[/red]" has 11 visible chars, column is 8
        let result = format_value("[red]hello world[/red]", 8, Align::Left, &overflow, None);
        let stripped = standout_bbparser::strip_tags(&result);
        assert_eq!(
            display_width(&stripped),
            8,
            "truncated output should be exactly 8 visible columns"
        );
    }

    #[test]
    fn format_value_bbcode_right_align() {
        let overflow = Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        };
        let result = format_value("[dim]hi[/dim]", 6, Align::Right, &overflow, None);
        let stripped = standout_bbparser::strip_tags(&result);
        assert_eq!(display_width(&stripped), 6);
        // Should have leading spaces then the tagged content
        assert!(result.contains("[dim]hi[/dim]"));
        assert!(result.starts_with("    "));
    }

    #[test]
    fn format_value_bbcode_with_style() {
        let overflow = Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        };
        // BBCode input + column style applied
        let result = format_value("[dim]ok[/dim]", 8, Align::Left, &overflow, Some("green"));
        let stripped = standout_bbparser::strip_tags(&result);
        assert_eq!(display_width(&stripped), 8);
        // Should have outer [green]...[/green] wrapper
        assert!(result.starts_with("[green]"));
        assert!(result.ends_with("[/green]"));
    }

    #[test]
    fn format_cell_lines_bbcode_wrap() {
        let col = Column::new(Width::Fixed(10)).overflow(Overflow::Wrap { indent: 0 });
        // "[bold]hello world foo[/bold]" → 15 visible chars, width 10
        let result = format_cell_lines("[bold]hello world foo[/bold]", 10, &col);
        match result {
            CellOutput::Multi(lines) => {
                for line in &lines {
                    let stripped = standout_bbparser::strip_tags(line);
                    assert!(
                        display_width(&stripped) <= 10,
                        "wrapped line '{}' exceeds column width (visible: {})",
                        line,
                        display_width(&stripped)
                    );
                }
            }
            CellOutput::Single(s) => {
                let stripped = standout_bbparser::strip_tags(&s);
                assert!(display_width(&stripped) <= 10, "single line should fit");
            }
        }
    }

    #[test]
    fn format_cell_lines_bbcode_fits_preserves_tags() {
        let col = Column::new(Width::Fixed(10)).overflow(Overflow::Wrap { indent: 0 });
        // "hi" is 2 visible chars, fits in 10
        let result = format_cell_lines("[bold]hi[/bold]", 10, &col);
        match result {
            CellOutput::Single(s) => {
                assert!(
                    s.contains("[bold]hi[/bold]"),
                    "tags should be preserved when content fits"
                );
                let stripped = standout_bbparser::strip_tags(&s);
                assert_eq!(display_width(&stripped), 10);
            }
            _ => panic!("expected Single output"),
        }
    }

    #[test]
    fn cell_output_line_bbcode_padding() {
        // Simulate a styled CellOutput with BBCode
        let output = CellOutput::Single("[green]ok[/green]".to_string());
        let line = output.line(0, 8, Align::Left);
        let stripped = standout_bbparser::strip_tags(&line);
        assert_eq!(
            display_width(&stripped),
            8,
            "CellOutput::line should pad to correct visible width"
        );
    }

    #[test]
    fn resolve_sub_widths_bbcode() {
        use crate::tabular::{SubCol, SubColumns};
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 30).right()], " ").unwrap();
        // Second value has BBCode — "[tag]" is 5 visible chars
        let widths = resolve_sub_widths(&sub_cols, &["Title", "[dim][tag][/dim]"], 30);
        // [dim][tag][/dim] visible width is 5, so bounded should resolve to 5
        assert_eq!(
            widths[1], 5,
            "bounded sub-col should use visible width, not raw string length"
        );
        assert_eq!(
            widths[0] + widths[1] + 1, // +1 for separator " "
            30,
            "widths + separator should equal parent width"
        );
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::tabular::{display_width, SubCol, SubColumns};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn sub_column_output_width_equals_parent(
            parent_width in 10usize..100,
            title_len in 0usize..50,
            tag_len in 0usize..30,
            bounded_max in 5usize..30,
        ) {
            let sub_cols = SubColumns::new(
                vec![SubCol::fill(), SubCol::bounded(0, bounded_max)],
                " ",
            ).unwrap();

            let title: String = "x".repeat(title_len);
            let tag: String = "y".repeat(tag_len);
            let values: Vec<&str> = vec![&title, &tag];

            let result = format_sub_cells(&sub_cols, &values, parent_width);
            prop_assert_eq!(
                display_width(&result),
                parent_width,
                "sub-cell output must exactly fill parent width. Got '{}' (dw={}), expected {}",
                result, display_width(&result), parent_width
            );
        }

        #[test]
        fn sub_column_non_grower_respects_bounds(
            parent_width in 30usize..100,
            min_w in 0usize..10,
            max_w_offset in 1usize..20,
            content_len in 0usize..40,
        ) {
            let max_w = min_w + max_w_offset; // ensure max > min
            let sub_cols = SubColumns::new(
                vec![SubCol::fill(), SubCol::bounded(min_w, max_w)],
                " ",
            ).unwrap();

            let content: String = "z".repeat(content_len);
            let values = vec!["title", content.as_str()];
            let widths = resolve_sub_widths(&sub_cols, &values, parent_width);

            let bounded_width = widths[1];
            prop_assert!(
                bounded_width >= min_w,
                "bounded width {} < min {}", bounded_width, min_w
            );
            prop_assert!(
                bounded_width <= max_w,
                "bounded width {} > max {}", bounded_width, max_w
            );
        }

        #[test]
        fn sub_column_width_arithmetic(
            parent_width in 10usize..100,
            fixed_width in 1usize..15,
            title_len in 0usize..50,
        ) {
            let sub_cols = SubColumns::new(
                vec![SubCol::fill(), SubCol::fixed(fixed_width)],
                "  ",
            ).unwrap();

            let title: String = "t".repeat(title_len);
            let values = vec![title.as_str(), "fixed"];
            let widths = resolve_sub_widths(&sub_cols, &values, parent_width);

            let sep_width = display_width(&sub_cols.separator);
            // Grower is always visible; only zero-width non-growers are skipped.
            // For 2 sub-cols with grower at [0], the visible count includes the
            // grower plus any non-zero non-grower columns.
            let visible_non_growers: usize = if widths[1] > 0 { 1 } else { 0 };
            let visible_count: usize = visible_non_growers + 1; // +1 for grower
            let sep_overhead = visible_count.saturating_sub(1) * sep_width;
            let total: usize = widths.iter().sum::<usize>() + sep_overhead;

            prop_assert_eq!(
                total, parent_width,
                "widths {:?} + sep_overhead {} != parent {}",
                widths, sep_overhead, parent_width
            );
        }

        #[test]
        fn sub_column_output_three_sub_cols(
            parent_width in 20usize..100,
            prefix_len in 0usize..20,
            tag_len in 0usize..15,
        ) {
            let sub_cols = SubColumns::new(
                vec![
                    SubCol::bounded(0, 10),
                    SubCol::fill(),
                    SubCol::bounded(0, 15).right(),
                ],
                " ",
            ).unwrap();

            let prefix: String = "p".repeat(prefix_len);
            let tag: String = "t".repeat(tag_len);
            let values = vec![prefix.as_str(), "middle content", tag.as_str()];

            let result = format_sub_cells(&sub_cols, &values, parent_width);
            prop_assert_eq!(
                display_width(&result),
                parent_width,
                "three sub-cols output must fill parent width"
            );
        }
    }
}
