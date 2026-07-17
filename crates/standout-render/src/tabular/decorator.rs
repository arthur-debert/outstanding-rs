//! Table decorator for adding borders, headers, and formatting.
//!
//! This module provides a `Table` type that wraps a `TabularSpec` and adds
//! decorative elements like borders, headers, and separators.
//!
//! # Example
//!
//! ```rust
//! use standout_render::tabular::{Table, TabularSpec, Col, BorderStyle};
//!
//! let spec = TabularSpec::builder()
//!     .column(Col::fixed(20))
//!     .column(Col::fixed(10))
//!     .column(Col::fixed(8))
//!     .separator("  ")
//!     .build();
//!
//! let table = Table::new(spec, 80)
//!     .border(BorderStyle::Light)
//!     .header(vec!["Name", "Status", "Count"]);
//!
//! // Render header
//! println!("{}", table.header_row());
//! println!("{}", table.separator_row());
//!
//! // Render data rows
//! println!("{}", table.row(&["Alice", "Active", "42"]));
//! println!("{}", table.row(&["Bob", "Pending", "17"]));
//!
//! // Or render everything at once
//! let data = vec![
//!     vec!["Alice", "Active", "42"],
//!     vec!["Bob", "Pending", "17"],
//! ];
//! println!("{}", table.render(&data));
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};

use super::formatter::{CellValue, OwnedCellValue, TabularFormatter};
use super::traits::{Tabular, TabularRow};
use super::types::{FlatDataSpec, TabularSpec};
use super::util::display_width_with_policy;
use crate::{AmbiguousWidth, WidthCalculator};

/// Border style for table decoration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderStyle {
    /// No borders.
    #[default]
    None,
    /// ASCII borders: +, -, |
    Ascii,
    /// Light Unicode box-drawing characters: ┌, ─, ┐, │, └, ┘, ├, ┼, ┤, ┬, ┴
    Light,
    /// Heavy Unicode box-drawing characters: ┏, ━, ┓, ┃, ┗, ┛, ┣, ╋, ┫, ┳, ┻
    Heavy,
    /// Double-line Unicode box-drawing: ╔, ═, ╗, ║, ╚, ╝, ╠, ╬, ╣, ╦, ╩
    Double,
    /// Rounded corners with light lines: ╭, ─, ╮, │, ╰, ╯, ├, ┼, ┤, ┬, ┴
    Rounded,
}

impl BorderStyle {
    /// Get the box-drawing characters for this border style.
    ///
    /// Returns a tuple of (horizontal, vertical, top_left, top_right, bottom_left,
    /// bottom_right, left_t, cross, right_t, top_t, bottom_t).
    fn chars(&self) -> BorderChars {
        match self {
            BorderStyle::None => BorderChars::empty(),
            BorderStyle::Ascii => BorderChars {
                horizontal: '-',
                vertical: '|',
                top_left: '+',
                top_right: '+',
                bottom_left: '+',
                bottom_right: '+',
                left_t: '+',
                cross: '+',
                right_t: '+',
                top_t: '+',
                bottom_t: '+',
            },
            BorderStyle::Light => BorderChars {
                horizontal: '─',
                vertical: '│',
                top_left: '┌',
                top_right: '┐',
                bottom_left: '└',
                bottom_right: '┘',
                left_t: '├',
                cross: '┼',
                right_t: '┤',
                top_t: '┬',
                bottom_t: '┴',
            },
            BorderStyle::Heavy => BorderChars {
                horizontal: '━',
                vertical: '┃',
                top_left: '┏',
                top_right: '┓',
                bottom_left: '┗',
                bottom_right: '┛',
                left_t: '┣',
                cross: '╋',
                right_t: '┫',
                top_t: '┳',
                bottom_t: '┻',
            },
            BorderStyle::Double => BorderChars {
                horizontal: '═',
                vertical: '║',
                top_left: '╔',
                top_right: '╗',
                bottom_left: '╚',
                bottom_right: '╝',
                left_t: '╠',
                cross: '╬',
                right_t: '╣',
                top_t: '╦',
                bottom_t: '╩',
            },
            BorderStyle::Rounded => BorderChars {
                horizontal: '─',
                vertical: '│',
                top_left: '╭',
                top_right: '╮',
                bottom_left: '╰',
                bottom_right: '╯',
                left_t: '├',
                cross: '┼',
                right_t: '┤',
                top_t: '┬',
                bottom_t: '┴',
            },
        }
    }
}

/// Box-drawing characters for a border style.
#[derive(Clone, Copy, Debug)]
struct BorderChars {
    horizontal: char,
    vertical: char,
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    left_t: char,
    cross: char,
    right_t: char,
    top_t: char,
    bottom_t: char,
}

impl BorderChars {
    fn empty() -> Self {
        BorderChars {
            horizontal: ' ',
            vertical: ' ',
            top_left: ' ',
            top_right: ' ',
            bottom_left: ' ',
            bottom_right: ' ',
            left_t: ' ',
            cross: ' ',
            right_t: ' ',
            top_t: ' ',
            bottom_t: ' ',
        }
    }
}

/// A decorated table with borders, headers, and separators.
///
/// Supports alternating row styles (odd/even) via [`row_styles`](Table::row_styles).
/// When set, data rows are automatically wrapped in `[style]...[/style]` tags
/// that alternate between the two style names.
pub struct Table {
    /// The underlying formatter.
    formatter: TabularFormatter,
    /// Original specification, retained so border selection can re-resolve Wide layouts.
    spec: FlatDataSpec,
    /// Requested width passed to the constructor.
    requested_width: usize,
    /// Column headers.
    headers: Option<Vec<String>>,
    /// Border style.
    border: BorderStyle,
    /// Header style name (for styling header cells).
    header_style: Option<String>,
    /// Whether to add separators between data rows.
    row_separator: bool,
    /// Alternating row style names: (odd_style, even_style).
    /// Row 0 uses even, row 1 uses odd, etc.
    row_styles: Option<(String, String)>,
    /// Counter for tracking data row index (for alternating styles).
    row_counter: AtomicUsize,
}

impl Clone for Table {
    fn clone(&self) -> Self {
        Self {
            formatter: self.formatter.clone(),
            spec: self.spec.clone(),
            requested_width: self.requested_width,
            headers: self.headers.clone(),
            border: self.border,
            header_style: self.header_style.clone(),
            row_separator: self.row_separator,
            row_styles: self.row_styles.clone(),
            row_counter: AtomicUsize::new(self.row_counter.load(Ordering::Relaxed)),
        }
    }
}

impl std::fmt::Debug for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Table")
            .field("formatter", &self.formatter)
            .field("requested_width", &self.requested_width)
            .field("headers", &self.headers)
            .field("border", &self.border)
            .field("header_style", &self.header_style)
            .field("row_separator", &self.row_separator)
            .field("row_styles", &self.row_styles)
            .field("row_counter", &self.row_counter.load(Ordering::Relaxed))
            .finish()
    }
}

impl Table {
    /// Create a new table with the given spec and total width.
    pub fn new(spec: TabularSpec, total_width: usize) -> Self {
        Self::with_ambiguous_width(spec, total_width, AmbiguousWidth::Narrow)
    }

    /// Creates a table with an explicit ambiguous-width policy.
    pub fn with_ambiguous_width(
        spec: TabularSpec,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        let formatter = TabularFormatter::with_ambiguous_width(&spec, total_width, policy);
        Table {
            formatter,
            spec,
            requested_width: total_width,
            headers: None,
            border: BorderStyle::None,
            header_style: None,
            row_separator: false,
            row_styles: None,
            row_counter: AtomicUsize::new(0),
        }
    }

    /// Create a table from a raw FlatDataSpec.
    pub fn from_spec(spec: &FlatDataSpec, total_width: usize) -> Self {
        Self::from_spec_with_ambiguous_width(spec, total_width, AmbiguousWidth::Narrow)
    }

    /// Creates a table from a raw spec with an explicit policy.
    pub fn from_spec_with_ambiguous_width(
        spec: &FlatDataSpec,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        Self::with_ambiguous_width(spec.clone(), total_width, policy)
    }

    /// Create a table from a type that implements `Tabular`.
    ///
    /// This constructor uses the `TabularSpec` generated by the `#[derive(Tabular)]`
    /// macro to configure the table.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout_render::tabular::{Tabular, Table, BorderStyle};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize, Tabular)]
    /// #[tabular(separator = " | ")]
    /// struct Task {
    ///     #[col(width = 8, header = "ID")]
    ///     id: String,
    ///     #[col(width = "fill", header = "Title")]
    ///     title: String,
    /// }
    ///
    /// let table = Table::from_type::<Task>(80)
    ///     .header_from_columns()
    ///     .border(BorderStyle::Light);
    /// ```
    pub fn from_type<T: Tabular>(total_width: usize) -> Self {
        Self::from_type_with_ambiguous_width::<T>(total_width, AmbiguousWidth::Narrow)
    }

    /// Creates a table from a `Tabular` type with an explicit policy.
    pub fn from_type_with_ambiguous_width<T: Tabular>(
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        let spec = T::tabular_spec();
        Self::with_ambiguous_width(spec, total_width, policy)
    }

    /// Set the border style.
    pub fn border(mut self, border: BorderStyle) -> Self {
        self.border = border;
        self.rebuild_formatter_for_border();
        self
    }

    fn rebuild_formatter_for_border(&mut self) {
        let policy = self.formatter.ambiguous_width();
        let mut formatter_width = self.requested_width;

        // Preserve the legacy Narrow layout exactly. Under Wide, Unicode border
        // glyphs occupy two cells, so the requested table width becomes a hard
        // maximum and the formatter receives the remaining interior width.
        if policy == AmbiguousWidth::Wide
            && matches!(
                self.border,
                BorderStyle::Light
                    | BorderStyle::Heavy
                    | BorderStyle::Double
                    | BorderStyle::Rounded
            )
        {
            let calculator = WidthCalculator::new(policy);
            let vertical = self.border.chars().vertical;
            formatter_width =
                formatter_width.saturating_sub(calculator.char_width(vertical).saturating_mul(2));
        }

        self.formatter =
            TabularFormatter::with_ambiguous_width(&self.spec, formatter_width, policy);
        if policy == AmbiguousWidth::Wide && self.border != BorderStyle::None {
            self.formatter.limit_to_width(formatter_width);
        }
    }

    /// Set the column headers.
    pub fn header<S: Into<String>, I: IntoIterator<Item = S>>(mut self, headers: I) -> Self {
        self.headers = Some(headers.into_iter().map(|s| s.into()).collect());
        self
    }

    /// Set headers automatically from column specifications.
    ///
    /// For each column, uses (in order of preference):
    /// 1. The `header` field if set
    /// 2. The `key` field if set
    /// 3. The `name` field if set
    /// 4. Empty string
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let spec = TabularSpec::builder()
    ///     .column(Col::fixed(8).header("ID"))
    ///     .column(Col::min(10).key("author").header("Author"))
    ///     .column(Col::fill().named("message"))  // Uses name as fallback
    ///     .build();
    ///
    /// let table = Table::new(spec, 80)
    ///     .header_from_columns()  // Headers: ["ID", "Author", "message"]
    ///     .border(BorderStyle::Light);
    /// ```
    pub fn header_from_columns(mut self) -> Self {
        self.headers = Some(self.formatter.extract_headers());
        self
    }

    /// Set the header style name.
    pub fn header_style(mut self, style: impl Into<String>) -> Self {
        self.header_style = Some(style.into());
        self
    }

    /// Enable row separators between data rows.
    pub fn row_separator(mut self, enable: bool) -> Self {
        self.row_separator = enable;
        self
    }

    /// Set alternating row styles for even and odd data rows.
    ///
    /// When set, each data row is wrapped in `[style]...[/style]` tags
    /// that alternate between the two style names. Row 0 uses `even_style`,
    /// row 1 uses `odd_style`, and so on.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let table = Table::new(spec, 80)
    ///     .row_styles("table_row_even", "table_row_odd");
    /// ```
    pub fn row_styles(
        mut self,
        even_style: impl Into<String>,
        odd_style: impl Into<String>,
    ) -> Self {
        self.row_styles = Some((odd_style.into(), even_style.into()));
        self
    }

    /// Get the border style.
    pub fn get_border(&self) -> BorderStyle {
        self.border
    }

    /// Get the number of columns.
    pub fn num_columns(&self) -> usize {
        self.formatter.num_columns()
    }

    /// Format a data row.
    pub fn row<S: AsRef<str>>(&self, values: &[S]) -> String {
        let content = self.formatter.format_row(values);
        self.wrap_data_row(&content)
    }

    /// Format a data row with sub-column support.
    ///
    /// Cells that correspond to columns with sub-columns should be
    /// [`CellValue::Sub`]; all others should be [`CellValue::Single`].
    pub fn row_cells(&self, values: &[CellValue<'_>]) -> String {
        let content = self.formatter.format_row_cells(values);
        self.wrap_data_row(&content)
    }

    /// Format a data row by extracting values from a serializable struct.
    ///
    /// This method extracts field values based on each column's `key` or `name`.
    /// See [`TabularFormatter::row_from`] for details on field extraction.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct Record { name: String, status: String }
    ///
    /// let table = Table::new(spec, 80);
    /// let record = Record { name: "Alice".into(), status: "active".into() };
    /// println!("{}", table.row_from(&record));
    /// ```
    pub fn row_from<T: serde::Serialize>(&self, value: &T) -> String {
        let content = self.formatter.row_from(value);
        self.wrap_data_row(&content)
    }

    /// Format a data row using the `TabularRow` trait.
    ///
    /// This method uses the optimized `to_row()` implementation generated by
    /// `#[derive(TabularRow)]`, avoiding JSON serialization overhead.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout_render::tabular::{TabularRow, Tabular, Table, BorderStyle};
    ///
    /// #[derive(Tabular, TabularRow)]
    /// #[tabular(separator = " | ")]
    /// struct Task {
    ///     #[col(width = 8)]
    ///     id: String,
    ///     #[col(width = "fill")]
    ///     title: String,
    /// }
    ///
    /// let table = Table::from_type::<Task>(80).border(BorderStyle::Light);
    /// let task = Task {
    ///     id: "TSK-001".to_string(),
    ///     title: "Implement feature".to_string(),
    /// };
    ///
    /// println!("{}", table.row_from_trait(&task));
    /// ```
    pub fn row_from_trait<T: TabularRow>(&self, value: &T) -> String {
        let content = self.formatter.row_from_trait(value);
        self.wrap_data_row(&content)
    }

    /// Format the header row.
    pub fn header_row(&self) -> String {
        match &self.headers {
            Some(headers) => {
                // Format the headers first (handles truncation/padding)
                let content = self.formatter.format_row(headers);

                // Apply style after formatting to avoid style tags being truncated
                let styled_content = if let Some(style) = &self.header_style {
                    format!("[{}]{}[/{}]", style, content, style)
                } else {
                    content
                };

                self.wrap_row(&styled_content)
            }
            None => String::new(),
        }
    }

    /// Generate a horizontal separator row.
    pub fn separator_row(&self) -> String {
        self.horizontal_line(LineType::Middle)
    }

    /// Generate the top border row.
    pub fn top_border(&self) -> String {
        self.horizontal_line(LineType::Top)
    }

    /// Generate the bottom border row.
    pub fn bottom_border(&self) -> String {
        self.horizontal_line(LineType::Bottom)
    }

    /// Wrap a data row with alternating style (if set) and borders.
    fn wrap_data_row(&self, content: &str) -> String {
        let bordered = self.wrap_row(content);
        if let Some((odd_style, even_style)) = &self.row_styles {
            let index = self.row_counter.fetch_add(1, Ordering::Relaxed);
            let style = if index.is_multiple_of(2) {
                even_style
            } else {
                odd_style
            };
            format!("[{}]{}[/{}]", style, bordered, style)
        } else {
            bordered
        }
    }

    /// Wrap a row content with vertical borders.
    fn wrap_row(&self, content: &str) -> String {
        if self.border == BorderStyle::None {
            return content.to_string();
        }

        let chars = self.border.chars();
        format!("{}{}{}", chars.vertical, content, chars.vertical)
    }

    /// Generate a horizontal line (top, middle, or bottom).
    fn horizontal_line(&self, line_type: LineType) -> String {
        if self.border == BorderStyle::None {
            return String::new();
        }

        let chars = self.border.chars();
        let widths = self.formatter.widths();

        // Calculate total content width
        let content_width: usize = widths.iter().sum();
        let sep_width = display_width_with_policy(
            &self.formatter_separator(),
            self.formatter.ambiguous_width(),
        );
        let num_seps = widths.len().saturating_sub(1);
        let total_content = if self.formatter.ambiguous_width() == AmbiguousWidth::Wide
            && matches!(
                self.border,
                BorderStyle::Light
                    | BorderStyle::Heavy
                    | BorderStyle::Double
                    | BorderStyle::Rounded
            ) {
            self.formatter.rendered_width()
        } else {
            content_width + (num_seps * sep_width)
        };

        let (left, _joint, right) = match line_type {
            LineType::Top => (chars.top_left, chars.top_t, chars.top_right),
            LineType::Middle => (chars.left_t, chars.cross, chars.right_t),
            LineType::Bottom => (chars.bottom_left, chars.bottom_t, chars.bottom_right),
        };

        let mut line = String::new();
        line.push(left);

        for (i, &width) in widths.iter().enumerate() {
            if i > 0 {
                // Add joint for separator
                for _ in 0..sep_width {
                    line.push(chars.horizontal);
                }
                // The joint replaces the middle horizontal char
                // Actually, for proper box drawing, we need joint at column boundaries
            }
            for _ in 0..width {
                line.push(chars.horizontal);
            }
        }

        // Add separators between columns
        // For simplicity, we'll just draw a continuous line
        // A more sophisticated version would place joints at column boundaries
        let horizontal_width = WidthCalculator::new(self.formatter.ambiguous_width())
            .char_width(chars.horizontal)
            .max(1);
        line = format!(
            "{}{}{}",
            left,
            std::iter::repeat_n(chars.horizontal, total_content / horizontal_width)
                .collect::<String>(),
            right
        );

        line
    }

    /// Get the separator string from formatter.
    fn formatter_separator(&self) -> String {
        // Access separator through the Object trait
        use minijinja::value::{Object, Value};
        use std::sync::Arc;
        let arc_formatter = Arc::new(self.formatter.clone());
        arc_formatter
            .get_value(&Value::from("separator"))
            .map(|v| v.to_string())
            .unwrap_or_default()
    }

    /// Render the complete table with all rows.
    ///
    /// Includes top border, header (if set), separator, data rows, and bottom border.
    pub fn render<S: AsRef<str>>(&self, rows: &[Vec<S>]) -> String {
        self.row_counter.store(0, Ordering::Relaxed);
        let mut output = Vec::new();

        // Top border
        let top = self.top_border();
        if !top.is_empty() {
            output.push(top);
        }

        // Header
        let header = self.header_row();
        if !header.is_empty() {
            output.push(header);

            // Separator after header
            let sep = self.separator_row();
            if !sep.is_empty() {
                output.push(sep);
            }
        }

        // Data rows (with optional separators between them)
        let separator = if self.row_separator {
            let sep = self.separator_row();
            if sep.is_empty() {
                None
            } else {
                Some(sep)
            }
        } else {
            None
        };

        for (i, row) in rows.iter().enumerate() {
            if i > 0 {
                if let Some(ref sep) = separator {
                    output.push(sep.clone());
                }
            }
            output.push(self.row(row));
        }

        // Bottom border
        let bottom = self.bottom_border();
        if !bottom.is_empty() {
            output.push(bottom);
        }

        output.join("\n")
    }
}

/// Type of horizontal line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineType {
    Top,
    Middle,
    Bottom,
}

// ============================================================================
// MiniJinja Object Implementation
// ============================================================================

impl minijinja::value::Object for Table {
    fn get_value(self: &std::sync::Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        match key.as_str()? {
            "num_columns" => Some(minijinja::Value::from(self.num_columns())),
            "border" => Some(minijinja::Value::from(format!("{:?}", self.get_border()))),
            _ => None,
        }
    }

    fn enumerate(self: &std::sync::Arc<Self>) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&["num_columns", "border"])
    }

    fn call_method(
        self: &std::sync::Arc<Self>,
        _state: &minijinja::State,
        name: &str,
        args: &[minijinja::Value],
    ) -> Result<minijinja::Value, minijinja::Error> {
        match name {
            "row" => {
                // row([value1, value2, ...]) - format a data row
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "row() requires an array of values",
                    ));
                }

                let values_arg = &args[0];

                if self.formatter.has_sub_columns() {
                    // Sub-column aware path: detect nested arrays
                    let outer_iter = match values_arg.try_iter() {
                        Ok(iter) => iter,
                        Err(_) => {
                            let values = vec![values_arg.to_string()];
                            return Ok(minijinja::Value::from(self.row(&values)));
                        }
                    };

                    let mut owned_values: Vec<OwnedCellValue> = Vec::new();
                    for (i, v) in outer_iter.enumerate() {
                        let is_sub_col = self
                            .formatter
                            .columns()
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

                    let cell_values: Vec<CellValue<'_>> = owned_values
                        .iter()
                        .map(|ov| match ov {
                            OwnedCellValue::Single(s) => CellValue::Single(s.as_str()),
                            OwnedCellValue::Sub(v) => {
                                CellValue::Sub(v.iter().map(|s| s.as_str()).collect())
                            }
                        })
                        .collect();

                    let formatted = self.row_cells(&cell_values);
                    Ok(minijinja::Value::from(formatted))
                } else {
                    let values: Vec<String> = match values_arg.try_iter() {
                        Ok(iter) => iter.map(|v| v.to_string()).collect(),
                        Err(_) => vec![values_arg.to_string()],
                    };

                    let formatted = self.row(&values);
                    Ok(minijinja::Value::from(formatted))
                }
            }
            "row_from" => {
                // row_from(object) - format a row by extracting values from an object
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "row_from() requires an object argument",
                    ));
                }

                // Convert MiniJinja Value to serde_json::Value for field extraction
                let json_value = minijinja::value::Value::from_serialize(&args[0]);
                let formatted = self.formatter.row_from(&json_value);
                Ok(minijinja::Value::from(self.wrap_data_row(&formatted)))
            }
            "header_row" => {
                // header_row() - format the header row
                Ok(minijinja::Value::from(self.header_row()))
            }
            "separator_row" => {
                // separator_row() - format a separator row
                Ok(minijinja::Value::from(self.separator_row()))
            }
            "top_border" => {
                // top_border() - format the top border
                Ok(minijinja::Value::from(self.top_border()))
            }
            "bottom_border" => {
                // bottom_border() - format the bottom border
                Ok(minijinja::Value::from(self.bottom_border()))
            }
            "render_all" => {
                // render_all(rows) - render complete table with all rows
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "render_all() requires an array of rows",
                    ));
                }

                let rows_iter = args[0].try_iter().map_err(|_| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "render_all() requires an array of rows",
                    )
                })?;

                let rows: Vec<Vec<String>> = rows_iter
                    .map(|row| {
                        row.try_iter()
                            .map(|iter| iter.map(|v| v.to_string()).collect())
                            .unwrap_or_else(|_| vec![row.to_string()])
                    })
                    .collect();

                let formatted = Table::render(self, &rows);
                Ok(minijinja::Value::from(formatted))
            }
            _ => Err(minijinja::Error::new(
                minijinja::ErrorKind::UnknownMethod,
                format!("Table has no method '{}'", name),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::Col;
    use crate::WidthCalculator;

    fn simple_spec() -> TabularSpec {
        TabularSpec::builder()
            .column(Col::fixed(10))
            .column(Col::fixed(8))
            .separator("  ")
            .build()
    }

    #[test]
    fn table_no_border() {
        let table = Table::new(simple_spec(), 80);
        let row = table.row(&["Hello", "World"]);
        // No border, just formatted content
        assert!(!row.contains('│'));
        assert!(row.contains("Hello"));
    }

    #[test]
    fn table_with_ascii_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Ascii);
        let row = table.row(&["Hello", "World"]);
        // Should have vertical bars
        assert!(row.starts_with('|'));
        assert!(row.ends_with('|'));
    }

    #[test]
    fn table_with_light_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Light);
        let row = table.row(&["Hello", "World"]);
        // Should have light box characters
        assert!(row.starts_with('│'));
        assert!(row.ends_with('│'));
    }

    #[test]
    fn table_with_heavy_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Heavy);
        let row = table.row(&["Hello", "World"]);
        assert!(row.starts_with('┃'));
        assert!(row.ends_with('┃'));
    }

    #[test]
    fn table_with_double_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Double);
        let row = table.row(&["Hello", "World"]);
        assert!(row.starts_with('║'));
        assert!(row.ends_with('║'));
    }

    #[test]
    fn table_with_rounded_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Rounded);
        let row = table.row(&["Hello", "World"]);
        assert!(row.starts_with('│'));
        assert!(row.ends_with('│'));
    }

    #[test]
    fn table_header_row() {
        let table = Table::new(simple_spec(), 80)
            .border(BorderStyle::Light)
            .header(vec!["Name", "Status"]);

        let header = table.header_row();
        assert!(header.contains("Name"));
        assert!(header.contains("Status"));
        assert!(header.starts_with('│'));
    }

    #[test]
    fn table_header_with_style() {
        let table = Table::new(simple_spec(), 80)
            .header(vec!["Name", "Status"])
            .header_style("header");

        let header = table.header_row();
        assert!(header.contains("[header]"));
        assert!(header.contains("[/header]"));
    }

    #[test]
    fn table_no_header() {
        let table = Table::new(simple_spec(), 80);
        let header = table.header_row();
        assert!(header.is_empty());
    }

    #[test]
    fn table_separator_row() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Light);
        let sep = table.separator_row();
        assert!(sep.contains('─'));
        assert!(sep.starts_with('├'));
        assert!(sep.ends_with('┤'));
    }

    #[test]
    fn table_top_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Light);
        let top = table.top_border();
        assert!(top.contains('─'));
        assert!(top.starts_with('┌'));
        assert!(top.ends_with('┐'));
    }

    #[test]
    fn table_bottom_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Light);
        let bottom = table.bottom_border();
        assert!(bottom.contains('─'));
        assert!(bottom.starts_with('└'));
        assert!(bottom.ends_with('┘'));
    }

    #[test]
    fn table_render_full() {
        let table = Table::new(simple_spec(), 80)
            .border(BorderStyle::Light)
            .header(vec!["Name", "Value"]);

        let data = vec![vec!["Alice", "100"], vec!["Bob", "200"]];

        let output = table.render(&data);
        let lines: Vec<&str> = output.lines().collect();

        // Should have: top border, header, separator, 2 data rows, bottom border
        assert!(lines.len() >= 5);

        // Top border
        assert!(lines[0].starts_with('┌'));
        // Header
        assert!(lines[1].contains("Name"));
        // Separator
        assert!(lines[2].starts_with('├'));
        // Data rows
        assert!(lines[3].contains("Alice"));
        assert!(lines[4].contains("Bob"));
        // Bottom border
        assert!(lines[5].starts_with('└'));
    }

    #[test]
    fn table_render_no_border() {
        let table = Table::new(simple_spec(), 80).header(vec!["Name", "Value"]);

        let data = vec![vec!["Alice", "100"]];

        let output = table.render(&data);
        let lines: Vec<&str> = output.lines().collect();

        // No borders, just header and data
        assert!(lines.len() >= 2);
        assert!(lines[0].contains("Name"));
        assert!(lines[1].contains("Alice"));
    }

    #[test]
    fn border_style_default() {
        assert_eq!(BorderStyle::default(), BorderStyle::None);
    }

    #[test]
    fn table_accessors() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Ascii);

        assert_eq!(table.get_border(), BorderStyle::Ascii);
        assert_eq!(table.num_columns(), 2);
    }

    #[test]
    fn table_row_from() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Record {
            name: String,
            status: String,
        }

        let spec = TabularSpec::builder()
            .column(Col::fixed(10).key("name"))
            .column(Col::fixed(8).key("status"))
            .separator("  ")
            .build();

        let table = Table::new(spec, 80);
        let record = Record {
            name: "Alice".to_string(),
            status: "active".to_string(),
        };

        let row = table.row_from(&record);
        assert!(row.contains("Alice"));
        assert!(row.contains("active"));
    }

    #[test]
    fn table_row_from_with_border() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Item {
            id: u32,
            value: String,
        }

        let spec = TabularSpec::builder()
            .column(Col::fixed(5).key("id"))
            .column(Col::fixed(10).key("value"))
            .build();

        let table = Table::new(spec, 80).border(BorderStyle::Light);
        let item = Item {
            id: 42,
            value: "test".to_string(),
        };

        let row = table.row_from(&item);
        assert!(row.starts_with('│'));
        assert!(row.ends_with('│'));
        assert!(row.contains("42"));
        assert!(row.contains("test"));
    }

    #[test]
    fn table_row_separator_option() {
        let spec = TabularSpec::builder()
            .column(Col::fixed(10))
            .column(Col::fixed(8))
            .build();

        let table = Table::new(spec, 80)
            .border(BorderStyle::Light)
            .row_separator(true);

        let data = vec![vec!["A", "1"], vec!["B", "2"], vec!["C", "3"]];
        let output = table.render(&data);
        let lines: Vec<&str> = output.lines().collect();

        // Should have: top, A, sep, B, sep, C, bottom = 7 lines
        // Count separator lines between data rows
        let sep_count = lines.iter().filter(|l| l.starts_with('├')).count();
        assert_eq!(sep_count, 2, "Expected 2 separators between 3 rows");
    }

    #[test]
    fn table_row_separator_disabled_by_default() {
        let spec = TabularSpec::builder()
            .column(Col::fixed(10))
            .column(Col::fixed(8))
            .build();

        let table = Table::new(spec, 80).border(BorderStyle::Light);

        let data = vec![vec!["A", "1"], vec!["B", "2"]];
        let output = table.render(&data);
        let lines: Vec<&str> = output.lines().collect();

        // No separators between data rows (only after header if present)
        // Lines: top, A, B, bottom = 4 lines
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn table_header_from_columns_with_header_field() {
        let spec = TabularSpec::builder()
            .column(Col::fixed(10).header("Name"))
            .column(Col::fixed(8).header("Status"))
            .separator("  ")
            .build();

        let table = Table::new(spec, 80)
            .header_from_columns()
            .border(BorderStyle::Light);

        let header = table.header_row();
        assert!(header.contains("Name"));
        assert!(header.contains("Status"));
    }

    #[test]
    fn table_header_from_columns_fallback_to_key() {
        let spec = TabularSpec::builder()
            .column(Col::fixed(10).key("user_name"))
            .column(Col::fixed(8).key("status"))
            .separator("  ")
            .build();

        let table = Table::new(spec, 80).header_from_columns();

        let header = table.header_row();
        assert!(header.contains("user_name"));
        assert!(header.contains("status"));
    }

    #[test]
    fn table_header_from_columns_fallback_to_name() {
        let spec = TabularSpec::builder()
            .column(Col::fixed(10).named("column1"))
            .column(Col::fixed(8).named("column2"))
            .separator("  ")
            .build();

        let table = Table::new(spec, 80).header_from_columns();

        let header = table.header_row();
        assert!(header.contains("column1"));
        assert!(header.contains("column2"));
    }

    #[test]
    fn table_header_from_columns_priority_order() {
        // header > key > name
        let spec = TabularSpec::builder()
            .column(Col::fixed(10).header("Header").key("key").named("name"))
            .column(Col::fixed(10).key("key_only").named("name_only"))
            .column(Col::fixed(10).named("name_only2"))
            .separator("  ")
            .build();

        let table = Table::new(spec, 80).header_from_columns();

        let header = table.header_row();
        assert!(header.contains("Header")); // header takes precedence
        assert!(header.contains("key_only")); // key is fallback when no header
        assert!(header.contains("name_only2")); // name is fallback when no key
    }

    #[test]
    fn table_header_from_columns_in_render() {
        let spec = TabularSpec::builder()
            .column(Col::fixed(10).header("Name"))
            .column(Col::fixed(8).header("Value"))
            .separator("  ")
            .build();

        let table = Table::new(spec, 80)
            .header_from_columns()
            .border(BorderStyle::Light);

        let data = vec![vec!["Alice", "100"]];
        let output = table.render(&data);

        // Should have header row with proper values
        assert!(output.contains("Name"));
        assert!(output.contains("Value"));
        assert!(output.contains("Alice"));
        assert!(output.contains("100"));
    }

    #[test]
    fn unicode_borders_honor_wide_maximum_without_ascii_fallback() {
        let styles = [
            (BorderStyle::Light, '┌'),
            (BorderStyle::Heavy, '┏'),
            (BorderStyle::Double, '╔'),
            (BorderStyle::Rounded, '╭'),
        ];
        let calculator = WidthCalculator::new(AmbiguousWidth::Wide);

        for (style, expected_corner) in styles {
            for requested in [20, 21] {
                let spec = TabularSpec::builder().column(Col::fill()).build();
                let table = Table::with_ambiguous_width(spec, requested, AmbiguousWidth::Wide)
                    .border(style);
                let rendered = table.render(&[vec!["≈Δ"]]);

                for line in rendered.lines() {
                    let width = calculator.visible_width(line);
                    assert!(
                        width <= requested,
                        "{style:?} line exceeded {requested}: {line}"
                    );
                    assert!(
                        requested - width <= 1,
                        "{style:?} underfilled {requested}: {line}"
                    );
                }
                assert!(table.top_border().starts_with(expected_corner));
                assert!(
                    !rendered.contains('+'),
                    "must not substitute ASCII: {rendered}"
                );
            }
        }
    }

    #[test]
    fn narrow_unicode_border_layout_remains_compatible() {
        let calculator = WidthCalculator::new(AmbiguousWidth::Narrow);

        for style in [
            BorderStyle::Light,
            BorderStyle::Heavy,
            BorderStyle::Double,
            BorderStyle::Rounded,
        ] {
            let spec = TabularSpec::builder().column(Col::fill()).build();
            let default = Table::new(spec.clone(), 21)
                .border(style)
                .render(&[vec!["≈Δ"]]);
            let explicit = Table::with_ambiguous_width(spec, 21, AmbiguousWidth::Narrow)
                .border(style)
                .render(&[vec!["≈Δ"]]);

            assert_eq!(default, explicit);
            assert!(default
                .lines()
                .all(|line| calculator.visible_width(line) == 23));
        }
    }
}
