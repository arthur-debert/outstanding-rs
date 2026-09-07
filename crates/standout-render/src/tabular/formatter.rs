use crate::RenderData as JsonValue;
use minijinja::value::{Enumerator, Object, Value};
use serde::Serialize;
use std::sync::Arc;

use super::resolve::ResolvedWidths;
use super::traits::TabularRow;
use super::types::{
    Align, Anchor, Column, FlatDataSpec, Overflow, SubColumns, TabularSpec, TruncateAt, Width,
};
use super::util::{
    truncate_visible_end_with_policy, truncate_visible_middle_with_policy,
    truncate_visible_start_with_policy, visible_width_with_policy, wrap_visible_indent_with_policy,
};
use crate::template::presentation::{escape_text, fragment, markup, parse_markup};
fn stringify(value: &minijinja::Value) -> std::borrow::Cow<'_, str> {
    std::borrow::Cow::Owned(markup(value))
}
use crate::{AmbiguousWidth, FormattedText};

#[derive(Clone, Debug)]
pub struct TabularFormatter {
    columns: Vec<Column>,
    widths: Vec<usize>,
    separator: String,
    prefix: String,
    suffix: String,
    total_width: usize,
    ambiguous_width: AmbiguousWidth,
}

impl TabularFormatter {
    pub fn new(spec: &FlatDataSpec, total_width: usize) -> Self {
        Self::with_ambiguous_width(spec, total_width, AmbiguousWidth::Narrow)
    }

    pub fn with_ambiguous_width(
        spec: &FlatDataSpec,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        Self::from_prepared_spec(&spec.prepared_text(), total_width, policy)
    }

    pub(crate) fn from_prepared_spec(
        spec: &FlatDataSpec,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        let resolved = spec.resolve_prepared_widths(total_width, policy);
        Self::from_prepared_resolved(spec, resolved, total_width, policy)
    }

    pub fn from_resolved(spec: &FlatDataSpec, resolved: ResolvedWidths) -> Self {
        let content_width: usize = resolved.widths.iter().sum();
        let overhead = spec.decorations.overhead(resolved.widths.len());
        let total_width = content_width + overhead;
        Self::from_resolved_with_width(spec, resolved, total_width)
    }

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

    pub fn from_resolved_with_width_and_policy(
        spec: &FlatDataSpec,
        resolved: ResolvedWidths,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        Self::from_prepared_resolved(&spec.prepared_text(), resolved, total_width, policy)
    }

    pub(crate) fn from_prepared_resolved(
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

    pub fn with_widths(columns: Vec<Column>, widths: Vec<usize>) -> Self {
        Self::with_widths_and_ambiguous_width(columns, widths, AmbiguousWidth::Narrow)
    }

    pub fn with_widths_and_ambiguous_width(
        columns: Vec<Column>,
        widths: Vec<usize>,
        policy: AmbiguousWidth,
    ) -> Self {
        let total_width = widths.iter().sum();
        TabularFormatter {
            columns: columns.iter().map(Column::prepared_text).collect(),
            widths,
            separator: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            total_width,
            ambiguous_width: policy,
        }
    }

    pub fn from_type<T: super::traits::Tabular>(total_width: usize) -> Self {
        Self::from_type_with_ambiguous_width::<T>(total_width, AmbiguousWidth::Narrow)
    }

    pub fn from_type_with_ambiguous_width<T: super::traits::Tabular>(
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> Self {
        let spec: TabularSpec = T::tabular_spec();
        Self::with_ambiguous_width(&spec, total_width, policy)
    }

    pub fn total_width(mut self, width: usize) -> Self {
        self.total_width = width;
        self
    }

    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.separator = escape_text(&sep.into());
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = escape_text(&prefix.into());
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = escape_text(&suffix.into());
        self
    }

    pub fn format_row<S: AsRef<str>>(&self, values: &[S]) -> String {
        let values: Vec<_> = values
            .iter()
            .map(|value| escape_text(value.as_ref()))
            .collect();
        self.format_markup_row(&values)
    }

    pub fn format_formatted_row(&self, values: &[FormattedText]) -> FormattedText {
        parse_markup(&self.formatted_row_markup(values))
    }

    pub(crate) fn formatted_row_markup(&self, values: &[FormattedText]) -> String {
        let values: Vec<_> = values
            .iter()
            .map(|value| markup(&Value::from(value.clone())))
            .collect();
        self.format_markup_row(&values)
    }

    pub fn format_row_cells(&self, values: &[CellValue<'_>]) -> String {
        let values: Vec<_> = values.iter().map(CellValue::to_markup).collect();
        let cells: Vec<_> = values.iter().map(OwnedCellValue::as_borrowed).collect();
        self.format_markup_row_cells(&cells)
    }

    pub fn format_row_lines<S: AsRef<str>>(&self, values: &[S]) -> Vec<String> {
        let values: Vec<_> = values
            .iter()
            .map(|value| escape_text(value.as_ref()))
            .collect();
        self.format_markup_row_lines(&values)
    }

    pub fn format_formatted_row_lines(&self, values: &[FormattedText]) -> Vec<FormattedText> {
        let values: Vec<_> = values
            .iter()
            .map(|value| markup(&Value::from(value.clone())))
            .collect();
        self.format_markup_row_lines(&values)
            .iter()
            .map(|line| parse_markup(line))
            .collect()
    }

    pub(crate) fn format_markup_row<S: AsRef<str>>(&self, values: &[S]) -> String {
        if self.columns.iter().any(|c| c.sub_columns.is_some()) {
            let cell_values: Vec<MarkupCellValue<'_>> = values
                .iter()
                .map(|s| MarkupCellValue::Single(s.as_ref()))
                .collect();
            return self.format_markup_row_cells(&cell_values);
        }

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
            let value = values.get(i).map(|s| s.as_ref()).unwrap_or(&col.null_repr);

            let formatted = format_cell_with_policy(value, width, col, self.ambiguous_width);
            result.push_str(&formatted);
        }

        result.push_str(&self.suffix);
        result
    }

    pub(crate) fn format_markup_row_cells(&self, values: &[MarkupCellValue<'_>]) -> String {
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
                let sub_values: Vec<&str> = match values.get(i) {
                    Some(MarkupCellValue::Sub(v)) => v.clone(),
                    Some(MarkupCellValue::Single(s)) => vec![s],
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
                let value = match values.get(i) {
                    Some(MarkupCellValue::Single(s)) => *s,
                    Some(MarkupCellValue::Sub(v)) => v.first().copied().unwrap_or(&col.null_repr),
                    None => &col.null_repr,
                };
                let formatted = format_cell_with_policy(value, width, col, self.ambiguous_width);
                result.push_str(&formatted);
            }
        }

        result.push_str(&self.suffix);
        result
    }

    fn calculate_anchor_gap(&self) -> (usize, usize) {
        let transition = self
            .columns
            .iter()
            .position(|c| c.anchor == Anchor::Right)
            .unwrap_or(self.columns.len());

        if transition == 0 || transition == self.columns.len() {
            return (0, transition);
        }

        let prefix_width = visible_width_with_policy(&self.prefix, self.ambiguous_width);
        let suffix_width = visible_width_with_policy(&self.suffix, self.ambiguous_width);
        let sep_width = visible_width_with_policy(&self.separator, self.ambiguous_width);
        let content_width: usize = self.widths.iter().sum();
        let num_seps = self.columns.len().saturating_sub(1);
        let current_total = prefix_width + content_width + (num_seps * sep_width) + suffix_width;

        if current_total >= self.total_width {
            (0, transition)
        } else {
            let extra = self.total_width - current_total;
            (extra + sep_width, transition)
        }
    }

    pub fn format_rows<S: AsRef<str>>(&self, rows: &[Vec<S>]) -> Vec<String> {
        rows.iter().map(|row| self.format_row(row)).collect()
    }

    pub(crate) fn format_markup_row_lines<S: AsRef<str>>(&self, values: &[S]) -> Vec<String> {
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

        let max_lines = cell_outputs
            .iter()
            .map(|c| c.line_count())
            .max()
            .unwrap_or(1);

        if max_lines == 1 {
            return vec![self.format_markup_row(values)];
        }

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

    pub fn column_width(&self, index: usize) -> Option<usize> {
        self.widths.get(index).copied()
    }

    pub fn widths(&self) -> &[usize] {
        &self.widths
    }

    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }

    pub fn has_sub_columns(&self) -> bool {
        self.columns.iter().any(|c| c.sub_columns.is_some())
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn ambiguous_width(&self) -> AmbiguousWidth {
        self.ambiguous_width
    }

    pub(crate) fn rendered_width(&self) -> usize {
        self.widths.iter().sum::<usize>()
            + visible_width_with_policy(&self.prefix, self.ambiguous_width)
            + visible_width_with_policy(&self.suffix, self.ambiguous_width)
            + self.columns.len().saturating_sub(1)
                * visible_width_with_policy(&self.separator, self.ambiguous_width)
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
}

mod cell_lines;
mod cells;
mod data;
mod subcolumns;
mod template_object;
use cell_lines::format_cell_lines_with_policy;
pub use cell_lines::CellOutput;
use cells::format_cell_with_policy;
pub use cells::CellValue;
pub(crate) use cells::{MarkupCellValue, OwnedCellValue};
use subcolumns::format_sub_cells_with_policy;

#[cfg(test)]
mod layout;

#[cfg(test)]
mod tests {
    pub(super) fn simple_spec() -> FlatDataSpec {
        FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator(" | ")
            .build()
    }

    use super::*;
    use crate::tabular::{display_width, Width};

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

        let formatter = TabularFormatter::new(&spec, 30);
        let _output = formatter.format_row(&["abc", "middle", "xyz"]);

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
        let output = formatter.format_formatted_row(&[FormattedText::from_ansi_sgr(styled)]);

        let output = crate::render_with_output(
            "{{ value }}",
            &crate::RenderData::Object([("value".into(), output.into())].into_iter().collect()),
            &crate::Theme::new(),
            crate::Representation::Human,
            crate::ColorPolicy::Always,
        )
        .unwrap();
        assert!(output.contains('\x1b'));
        assert_eq!(console::strip_ansi_codes(&output), "red       ");
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
}
