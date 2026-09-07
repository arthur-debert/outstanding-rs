use super::{Column, Width};
use crate::template::presentation::escape_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Decorations {
    pub column_sep: String,
    pub row_prefix: String,
    pub row_suffix: String,
}

impl Decorations {
    pub fn with_separator(sep: impl Into<String>) -> Self {
        Decorations {
            column_sep: sep.into(),
            row_prefix: String::new(),
            row_suffix: String::new(),
        }
    }

    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.column_sep = sep.into();
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.row_prefix = prefix.into();
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.row_suffix = suffix.into();
        self
    }

    pub fn overhead(&self, num_columns: usize) -> usize {
        self.overhead_with_policy(num_columns, crate::AmbiguousWidth::Narrow)
    }

    pub fn overhead_with_policy(&self, num_columns: usize, policy: crate::AmbiguousWidth) -> usize {
        self.prepared_text().prepared_overhead(num_columns, policy)
    }

    pub(crate) fn prepared_text(&self) -> Self {
        Self {
            column_sep: escape_text(&self.column_sep),
            row_prefix: escape_text(&self.row_prefix),
            row_suffix: escape_text(&self.row_suffix),
        }
    }

    pub(crate) fn prepared_overhead(
        &self,
        num_columns: usize,
        policy: crate::AmbiguousWidth,
    ) -> usize {
        use crate::tabular::visible_width_with_policy;
        let prefix_width = visible_width_with_policy(&self.row_prefix, policy);
        let suffix_width = visible_width_with_policy(&self.row_suffix, policy);
        let sep_width = visible_width_with_policy(&self.column_sep, policy);
        let sep_count = num_columns.saturating_sub(1);
        prefix_width + suffix_width + (sep_width * sep_count)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlatDataSpec {
    pub columns: Vec<Column>,
    pub decorations: Decorations,
}

impl FlatDataSpec {
    pub(crate) fn prepared_text(&self) -> Self {
        Self {
            columns: self.columns.iter().map(Column::prepared_text).collect(),
            decorations: self.decorations.prepared_text(),
        }
    }
}

impl FlatDataSpec {
    pub fn new(columns: Vec<Column>) -> Self {
        FlatDataSpec {
            columns,
            decorations: Decorations::default(),
        }
    }

    pub fn builder() -> FlatDataSpecBuilder {
        FlatDataSpecBuilder::default()
    }

    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }

    pub fn has_fill_column(&self) -> bool {
        self.columns.iter().any(|c| matches!(c.width, Width::Fill))
    }

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
        v => Some(v.to_string()),
    }
}

#[derive(Clone, Debug, Default)]
pub struct FlatDataSpecBuilder {
    columns: Vec<Column>,
    decorations: Decorations,
}

impl FlatDataSpecBuilder {
    pub fn column(mut self, column: Column) -> Self {
        self.columns.push(column);
        self
    }

    pub fn columns(mut self, columns: impl IntoIterator<Item = Column>) -> Self {
        self.columns.extend(columns);
        self
    }

    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.decorations.column_sep = sep.into();
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.decorations.row_prefix = prefix.into();
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.decorations.row_suffix = suffix.into();
        self
    }

    pub fn decorations(mut self, decorations: Decorations) -> Self {
        self.decorations = decorations;
        self
    }

    pub fn build(self) -> FlatDataSpec {
        FlatDataSpec {
            columns: self.columns,
            decorations: self.decorations,
        }
    }
}

pub type TabularSpec = FlatDataSpec;
pub type TabularSpecBuilder = FlatDataSpecBuilder;

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(dec.overhead(3), 8);
        assert_eq!(dec.overhead(1), 4);
        assert_eq!(dec.overhead(0), 4);
    }

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
            .column(Column::new(Width::Fixed(10)).key("missing.field"))
            .build();

        let row = spec.extract_row(&json);
        assert_eq!(row[0], "Alice");
        assert_eq!(row[1], "30");
        assert_eq!(row[2], "admin");
        assert_eq!(row[3], "-");
    }

    #[test]
    fn extract_header_row() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).header("Name").key("name"))
            .column(Column::new(Width::Fixed(5)).key("age"))
            .column(Column::new(Width::Fixed(10)))
            .build();

        let header = spec.extract_header();
        assert_eq!(header[0], "Name");
        assert_eq!(header[1], "age");
        assert_eq!(header[2], "");
    }
}
