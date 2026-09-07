use std::sync::atomic::{AtomicUsize, Ordering};

use super::formatter::{CellValue, MarkupCellValue, OwnedCellValue, TabularFormatter};
use super::traits::{Tabular, TabularRow};
use super::types::{FlatDataSpec, TabularSpec};
use crate::template::presentation::{escape_text, fragment, markup, parse_markup};
fn stringify(value: &minijinja::Value) -> std::borrow::Cow<'_, str> {
    std::borrow::Cow::Owned(markup(value))
}
use crate::{AmbiguousWidth, FormattedText, WidthCalculator};

mod borders;
mod template_object;
pub use borders::BorderStyle;

pub struct Table {
    formatter: TabularFormatter,
    spec: FlatDataSpec,
    requested_width: usize,
    headers: Option<Vec<FormattedText>>,
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
        Self::from_prepared_spec(spec.prepared_text(), total_width, policy)
    }

    pub(crate) fn from_prepared_spec(
        spec: TabularSpec,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        let formatter = TabularFormatter::from_prepared_spec(&spec, total_width, policy);
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

    /// Resizes every `Bounded` column to the widest cell `data` holds for it.
    pub(crate) fn sized_to_data<S: AsRef<str>>(mut self, data: &[Vec<S>]) -> Self {
        let policy = self.formatter.ambiguous_width();
        self.data_widths = Some(self.spec.measure_columns(data, policy));
        self.rebuild_formatter();
        self
    }

    pub fn header<S: Into<String>, I: IntoIterator<Item = S>>(mut self, headers: I) -> Self {
        self.headers = Some(
            headers
                .into_iter()
                .map(|text| FormattedText::text(text.into()))
                .collect(),
        );
        self
    }

    pub fn header_formatted(mut self, headers: impl IntoIterator<Item = FormattedText>) -> Self {
        self.headers = Some(headers.into_iter().collect());
        self
    }

    pub fn header_from_columns(self) -> Self {
        let headers = self.formatter.extract_headers();
        self.header(headers)
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

    pub fn row_formatted(&self, values: &[FormattedText]) -> FormattedText {
        let content = self.formatter.formatted_row_markup(values);
        parse_markup(&self.wrap_data_row(&content))
    }

    fn row_markup<S: AsRef<str>>(&self, values: &[S]) -> String {
        self.wrap_data_row(&self.formatter.format_markup_row(values))
    }

    fn row_markup_cells(&self, values: &[MarkupCellValue<'_>]) -> String {
        self.wrap_data_row(&self.formatter.format_markup_row_cells(values))
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
                let content = self.formatter.formatted_row_markup(headers);

                let styled_content = if let Some(style) = self
                    .header_style
                    .as_ref()
                    .filter(|style| standout_bbparser::is_valid_tag_name(style))
                {
                    format!("[{}]{}[/{}]", style, content, style)
                } else {
                    content
                };

                self.wrap_row(&styled_content)
            }
            None => String::new(),
        }
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
            if standout_bbparser::is_valid_tag_name(style) {
                format!("[{}]{}[/{}]", style, bordered, style)
            } else {
                bordered
            }
        } else {
            bordered
        }
    }

    pub fn render<S: AsRef<str>>(&self, rows: &[Vec<S>]) -> String {
        let rows: Vec<Vec<_>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| escape_text(value.as_ref()))
                    .collect()
            })
            .collect();
        self.render_markup(&rows)
    }

    fn render_markup<S: AsRef<str>>(&self, rows: &[Vec<S>]) -> String {
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
            output.push(self.row_markup(row));
        }

        let bottom = self.bottom_border();
        if !bottom.is_empty() {
            output.push(bottom);
        }

        output.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::Col;

    pub(super) fn simple_spec() -> TabularSpec {
        TabularSpec::builder()
            .column(Col::fixed(10))
            .column(Col::fixed(8))
            .separator("  ")
            .build()
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
}
