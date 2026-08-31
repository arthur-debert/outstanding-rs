//! [`Table`] wraps a `TabularSpec`/`FlatDataSpec` and adds borders, headers,
//! row separators, and alternating row styles on top of the plain formatted
//! rows. Alternating styles wrap each data row in `[style]...[/style]` tags;
//! `Table` also implements `minijinja::value::Object` so templates can call
//! `.row(...)`, `.header_row()`, etc. directly.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::formatter::{CellValue, OwnedCellValue, TabularFormatter};
use super::traits::{Tabular, TabularRow};
use super::types::{FlatDataSpec, TabularSpec};
use super::util::display_width_with_policy;
use crate::template::stringify;
use crate::{AmbiguousWidth, WidthCalculator};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderStyle {
    #[default]
    None,
    Ascii,
    Light,
    Heavy,
    Double,
    Rounded,
}

impl BorderStyle {
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

pub struct Table {
    formatter: TabularFormatter,
    spec: FlatDataSpec,
    requested_width: usize,
    headers: Option<Vec<String>>,
    border: BorderStyle,
    header_style: Option<String>,
    row_separator: bool,
    row_styles: Option<(String, String)>,
    row_counter: AtomicUsize,
    data_widths: Option<Vec<usize>>,
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
            data_widths: self.data_widths.clone(),
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
    pub fn new(spec: TabularSpec, total_width: usize) -> Self {
        Self::with_ambiguous_width(spec, total_width, AmbiguousWidth::Narrow)
    }

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
            data_widths: None,
        }
    }

    pub fn from_spec(spec: &FlatDataSpec, total_width: usize) -> Self {
        Self::from_spec_with_ambiguous_width(spec, total_width, AmbiguousWidth::Narrow)
    }

    pub fn from_spec_with_ambiguous_width(
        spec: &FlatDataSpec,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        Self::with_ambiguous_width(spec.clone(), total_width, policy)
    }

    pub fn from_type<T: Tabular>(total_width: usize) -> Self {
        Self::from_type_with_ambiguous_width::<T>(total_width, AmbiguousWidth::Narrow)
    }

    pub fn from_type_with_ambiguous_width<T: Tabular>(
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        let spec = T::tabular_spec();
        Self::with_ambiguous_width(spec, total_width, policy)
    }

    pub fn border(mut self, border: BorderStyle) -> Self {
        self.border = border;
        self.rebuild_formatter();
        self
    }

    /// Resizes every `Bounded` column to the widest cell `data` holds for it,
    /// the whole-table measurement `resolve_widths_from_data` performs.
    pub(crate) fn sized_to_data<S: AsRef<str>>(mut self, data: &[Vec<S>]) -> Self {
        let policy = self.formatter.ambiguous_width();
        self.data_widths = Some(self.spec.measure_columns(data, policy));
        self.rebuild_formatter();
        self
    }

    fn rebuild_formatter(&mut self) {
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

        self.formatter = match &self.data_widths {
            Some(measured) => {
                let resolved = self.spec.resolve_widths_measured_with_policy(
                    formatter_width,
                    measured,
                    policy,
                );
                TabularFormatter::from_resolved_with_width_and_policy(
                    &self.spec,
                    resolved,
                    formatter_width,
                    policy,
                )
            }
            None => TabularFormatter::with_ambiguous_width(&self.spec, formatter_width, policy),
        };
        if policy == AmbiguousWidth::Wide && self.border != BorderStyle::None {
            self.formatter.limit_to_width(formatter_width);
        }
    }

    pub fn header<S: Into<String>, I: IntoIterator<Item = S>>(mut self, headers: I) -> Self {
        self.headers = Some(headers.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn header_from_columns(mut self) -> Self {
        self.headers = Some(self.formatter.extract_headers());
        self
    }

    pub fn header_style(mut self, style: impl Into<String>) -> Self {
        self.header_style = Some(style.into());
        self
    }

    pub fn row_separator(mut self, enable: bool) -> Self {
        self.row_separator = enable;
        self
    }

    pub fn row_styles(
        mut self,
        even_style: impl Into<String>,
        odd_style: impl Into<String>,
    ) -> Self {
        self.row_styles = Some((odd_style.into(), even_style.into()));
        self
    }

    pub fn get_border(&self) -> BorderStyle {
        self.border
    }

    pub fn num_columns(&self) -> usize {
        self.formatter.num_columns()
    }

    pub fn row<S: AsRef<str>>(&self, values: &[S]) -> String {
        let content = self.formatter.format_row(values);
        self.wrap_data_row(&content)
    }

    pub fn row_cells(&self, values: &[CellValue<'_>]) -> String {
        let content = self.formatter.format_row_cells(values);
        self.wrap_data_row(&content)
    }

    pub fn row_from<T: serde::Serialize>(&self, value: &T) -> String {
        let content = self.formatter.row_from(value);
        self.wrap_data_row(&content)
    }

    pub fn row_from_trait<T: TabularRow>(&self, value: &T) -> String {
        let content = self.formatter.row_from_trait(value);
        self.wrap_data_row(&content)
    }

    pub fn header_row(&self) -> String {
        match &self.headers {
            Some(headers) => {
                let content = self.formatter.format_row(headers);

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

    pub fn separator_row(&self) -> String {
        self.horizontal_line(LineType::Middle)
    }

    pub fn top_border(&self) -> String {
        self.horizontal_line(LineType::Top)
    }

    pub fn bottom_border(&self) -> String {
        self.horizontal_line(LineType::Bottom)
    }

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

    fn wrap_row(&self, content: &str) -> String {
        if self.border == BorderStyle::None {
            return content.to_string();
        }

        let chars = self.border.chars();
        format!("{}{}{}", chars.vertical, content, chars.vertical)
    }

    fn horizontal_line(&self, line_type: LineType) -> String {
        if self.border == BorderStyle::None {
            return String::new();
        }

        let chars = self.border.chars();
        let widths = self.formatter.widths();

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
                for _ in 0..sep_width {
                    line.push(chars.horizontal);
                }
            }
            for _ in 0..width {
                line.push(chars.horizontal);
            }
        }

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

    fn formatter_separator(&self) -> String {
        use minijinja::value::{Object, Value};
        use std::sync::Arc;
        let arc_formatter = Arc::new(self.formatter.clone());
        arc_formatter
            .get_value(&Value::from("separator"))
            .map(|v| stringify(&v).into_owned())
            .unwrap_or_default()
    }

    pub fn render<S: AsRef<str>>(&self, rows: &[Vec<S>]) -> String {
        self.row_counter.store(0, Ordering::Relaxed);
        let mut output = Vec::new();

        let top = self.top_border();
        if !top.is_empty() {
            output.push(top);
        }

        let header = self.header_row();
        if !header.is_empty() {
            output.push(header);

            let sep = self.separator_row();
            if !sep.is_empty() {
                output.push(sep);
            }
        }

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

        let bottom = self.bottom_border();
        if !bottom.is_empty() {
            output.push(bottom);
        }

        output.join("\n")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineType {
    Top,
    Middle,
    Bottom,
}

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
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "row() requires an array of values",
                    ));
                }

                let values_arg = &args[0];

                if self.formatter.has_sub_columns() {
                    let outer_iter = match values_arg.try_iter() {
                        Ok(iter) => iter,
                        Err(_) => {
                            let values = vec![stringify(values_arg).into_owned()];
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
                                    inner_iter.map(|iv| stringify(&iv).into_owned()).collect();
                                owned_values.push(OwnedCellValue::Sub(sub_vals));
                            } else {
                                owned_values
                                    .push(OwnedCellValue::Single(stringify(&v).into_owned()));
                            }
                        } else {
                            owned_values.push(OwnedCellValue::Single(stringify(&v).into_owned()));
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
                        Ok(iter) => iter.map(|v| stringify(&v).into_owned()).collect(),
                        Err(_) => vec![stringify(values_arg).into_owned()],
                    };

                    let formatted = self.row(&values);
                    Ok(minijinja::Value::from(formatted))
                }
            }
            "row_from" => {
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "row_from() requires an object argument",
                    ));
                }

                let json_value = minijinja::value::Value::from_serialize(&args[0]);
                let formatted = self.formatter.row_from(&json_value);
                Ok(minijinja::Value::from(self.wrap_data_row(&formatted)))
            }
            "header_row" => Ok(minijinja::Value::from(self.header_row())),
            "separator_row" => Ok(minijinja::Value::from(self.separator_row())),
            "top_border" => Ok(minijinja::Value::from(self.top_border())),
            "bottom_border" => Ok(minijinja::Value::from(self.bottom_border())),
            "render_all" => {
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
                            .map(|iter| iter.map(|v| stringify(&v).into_owned()).collect())
                            .unwrap_or_else(|_| vec![stringify(&row).into_owned()])
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
        assert!(!row.contains('│'));
        assert!(row.contains("Hello"));
    }

    #[test]
    fn table_with_ascii_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Ascii);
        let row = table.row(&["Hello", "World"]);
        assert!(row.starts_with('|'));
        assert!(row.ends_with('|'));
    }

    #[test]
    fn table_with_light_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Light);
        let row = table.row(&["Hello", "World"]);
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

        assert!(lines.len() >= 5);

        assert!(lines[0].starts_with('┌'));
        assert!(lines[1].contains("Name"));
        assert!(lines[2].starts_with('├'));
        assert!(lines[3].contains("Alice"));
        assert!(lines[4].contains("Bob"));
        assert!(lines[5].starts_with('└'));
    }

    #[test]
    fn table_render_no_border() {
        let table = Table::new(simple_spec(), 80).header(vec!["Name", "Value"]);

        let data = vec![vec!["Alice", "100"]];

        let output = table.render(&data);
        let lines: Vec<&str> = output.lines().collect();

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
