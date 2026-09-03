//! Declarative projections for machine-readable output.
//!
//! A projection is a presentation-layer view over a canonical serialized
//! response. It never mutates the value used by other representations. Its row
//! source is a dot path into the response, or `.` for the response itself;
//! an array there is one row per element, a record is one row.

use std::fmt;
use std::rc::Rc;

use serde_json::{Map, Value};

use crate::tabular::{Column, FlatDataSpec};

type DerivedCell = Rc<dyn Fn(&Value, &Value) -> Value>;
type SyntheticRow = Rc<dyn Fn(&Value) -> Option<Value>>;

#[derive(Clone)]
pub struct StructuredOutputProjection {
    csv: CsvProjection,
}

impl StructuredOutputProjection {
    pub fn csv(projection: CsvProjection) -> Self {
        Self { csv: projection }
    }

    pub fn csv_projection(&self) -> &CsvProjection {
        &self.csv
    }
}

impl fmt::Debug for StructuredOutputProjection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StructuredOutputProjection")
            .field("csv", &self.csv)
            .finish()
    }
}

#[derive(Clone)]
pub struct CsvProjection {
    row_source: String,
    columns: Vec<ProjectionColumn>,
    synthetic_rows: Vec<SyntheticRow>,
}

impl CsvProjection {
    pub fn builder(row_source: impl Into<String>) -> CsvProjectionBuilder {
        CsvProjectionBuilder {
            row_source: row_source.into(),
            columns: Vec::new(),
            synthetic_rows: Vec::new(),
        }
    }

    pub fn render(&self, root: &Value) -> Result<String, ProjectionError> {
        let rows = value_at_path(root, &self.row_source).ok_or_else(|| {
            ProjectionError::MissingRowSource {
                path: self.row_source.clone(),
            }
        })?;
        let rows: Vec<&Value> = match rows {
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(index, item)| match item {
                    Value::Object(_) => Ok(item),
                    _ => Err(ProjectionError::RowNotARecord {
                        path: self.row_source.clone(),
                        index,
                    }),
                })
                .collect::<Result<_, _>>()?,
            Value::Object(_) => vec![rows],
            _ => {
                return Err(ProjectionError::RowSourceNotRecords {
                    path: self.row_source.clone(),
                })
            }
        };

        let spec = FlatDataSpec::new(
            self.columns
                .iter()
                .enumerate()
                .map(|(index, projection)| {
                    let column = projection.column();
                    let header = column
                        .header
                        .as_deref()
                        .or(column.key.as_deref())
                        .unwrap_or("");

                    column.clone().header(header).key(index.to_string())
                })
                .collect(),
        );

        let mut wtr = csv::Writer::from_writer(Vec::new());
        wtr.write_record(spec.extract_header())?;

        for row in rows {
            wtr.write_record(self.extract_row(&spec, row, root))?;
        }

        for row in self
            .synthetic_rows
            .iter()
            .filter_map(|synthetic| synthetic(root))
        {
            wtr.write_record(self.extract_row(&spec, &row, root))?;
        }

        let bytes = wtr.into_inner().map_err(|error| error.into_error())?;
        Ok(String::from_utf8(bytes)?)
    }

    fn extract_row(&self, spec: &FlatDataSpec, row: &Value, root: &Value) -> Vec<String> {
        let values = self
            .columns
            .iter()
            .enumerate()
            .map(|(index, projection)| {
                let value = match projection {
                    ProjectionColumn::Direct(column) => column
                        .key
                        .as_deref()
                        .and_then(|path| value_at_path(row, path))
                        .cloned()
                        .unwrap_or(Value::Null),
                    ProjectionColumn::Derived { derive, .. } => derive(row, root),
                };
                (index.to_string(), value)
            })
            .collect::<Map<_, _>>();

        spec.extract_row(&Value::Object(values))
    }
}

impl fmt::Debug for CsvProjection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CsvProjection")
            .field("row_source", &self.row_source)
            .field("column_count", &self.columns.len())
            .field("synthetic_row_count", &self.synthetic_rows.len())
            .finish()
    }
}

#[derive(Clone)]
enum ProjectionColumn {
    Direct(Column),
    Derived { column: Column, derive: DerivedCell },
}

impl ProjectionColumn {
    fn column(&self) -> &Column {
        match self {
            Self::Direct(column) | Self::Derived { column, .. } => column,
        }
    }
}

pub struct CsvProjectionBuilder {
    row_source: String,
    columns: Vec<ProjectionColumn>,
    synthetic_rows: Vec<SyntheticRow>,
}

impl CsvProjectionBuilder {
    pub fn column(mut self, column: Column) -> Self {
        self.columns.push(ProjectionColumn::Direct(column));
        self
    }

    pub fn derived_column<F>(mut self, column: Column, derive: F) -> Self
    where
        F: Fn(&Value, &Value) -> Value + 'static,
    {
        self.columns.push(ProjectionColumn::Derived {
            column,
            derive: Rc::new(derive),
        });
        self
    }

    pub fn synthetic_row<F>(self, row: F) -> Self
    where
        F: Fn(&Value) -> Value + 'static,
    {
        self.conditional_row(move |root| Some(row(root)))
    }

    pub fn conditional_row<F>(mut self, row: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + 'static,
    {
        self.synthetic_rows.push(Rc::new(row));
        self
    }

    pub fn build(self) -> CsvProjection {
        CsvProjection {
            row_source: self.row_source,
            columns: self.columns,
            synthetic_rows: self.synthetic_rows,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("projection row source `{path}` was not found")]
    MissingRowSource { path: String },
    #[error("projection row source `{path}` is neither a record nor an array of records")]
    RowSourceNotRecords { path: String },
    #[error("projection row source `{path}` element {index} is not a record")]
    RowNotARecord { path: String, index: usize },
    #[error(transparent)]
    Csv(#[from] csv::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path == "." {
        return Some(value);
    }
    path.split('.')
        .try_fold(value, |current, segment| match current {
            Value::Object(object) => object.get(segment),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tabular::{Column, Width};

    fn column(key: &str, header: &str) -> Column {
        Column::new(Width::default()).key(key).header(header)
    }

    fn projection() -> CsvProjection {
        CsvProjection::builder("report.items")
            .column(column("language", "LANGUAGE"))
            .column(column("counts.code", "CODE"))
            .derived_column(column("net", "NET"), |row, root| {
                json!(
                    row["counts"]["code"].as_i64().unwrap_or(0)
                        - row["counts"]["comments"].as_i64().unwrap_or(0)
                        + root["adjustment"].as_i64().unwrap()
                )
            })
            .synthetic_row(|root| {
                json!({
                    "language": "TOTAL",
                    "counts": {
                        "code": root["totals"]["code"],
                        "comments": root["totals"]["comments"]
                    }
                })
            })
            .conditional_row(|root| {
                (root["skipped"].as_u64().unwrap() > 0).then(|| {
                    json!({
                        "language": "SKIPPED",
                        "counts": { "code": root["skipped"], "comments": 0 }
                    })
                })
            })
            .build()
    }

    #[test]
    fn selects_nested_rows_derives_cells_and_appends_synthetic_rows() {
        let root = json!({
            "report": { "items": [
                { "language": "Rust", "counts": { "code": 12, "comments": 2 } },
                { "language": "Python", "counts": { "code": 8, "comments": 1 } }
            ] },
            "totals": { "code": 20, "comments": 3 },
            "skipped": 2,
            "adjustment": 1
        });

        assert_eq!(
            projection().render(&root).unwrap(),
            "LANGUAGE,CODE,NET\nRust,12,11\nPython,8,8\nTOTAL,20,18\nSKIPPED,2,3\n"
        );
    }

    #[test]
    fn conditional_row_disappears_and_null_repr_matches_flat_data_spec() {
        let root = json!({
            "report": { "items": [{ "language": "Rust", "counts": { "comments": 2 } }] },
            "totals": { "code": null, "comments": 2 },
            "skipped": 0,
            "adjustment": 0
        });

        assert_eq!(
            projection().render(&root).unwrap(),
            "LANGUAGE,CODE,NET\nRust,-,-2\nTOTAL,-,-2\n"
        );
    }

    #[test]
    fn the_document_itself_is_a_row_source_and_a_record_is_one_row() {
        let projection = CsvProjection::builder(".")
            .column(column("summary", "summary"))
            .column(column("range.start.line", "range_line").null_repr(""))
            .build();

        assert_eq!(
            projection
                .render(&json!({ "summary": "bad line", "range": { "start": { "line": 2 } } }))
                .unwrap(),
            "summary,range_line\nbad line,2\n"
        );
        assert_eq!(
            projection.render(&json!({ "summary": "bad" })).unwrap(),
            "summary,range_line\nbad,\n"
        );
        assert_eq!(
            projection
                .render(&json!([{ "summary": "a" }, { "summary": "b" }]))
                .unwrap(),
            "summary,range_line\na,\nb,\n"
        );
        let error = projection.render(&json!("text")).unwrap_err();
        assert!(
            matches!(error, ProjectionError::RowSourceNotRecords { ref path } if path == "."),
            "{error}"
        );
    }

    #[test]
    fn every_array_element_must_be_a_record() {
        let projection = CsvProjection::builder("items")
            .column(column("name", "name"))
            .build();

        let mixed = projection
            .render(&json!({ "items": [{ "name": "ok" }, 42] }))
            .unwrap_err();
        assert!(
            matches!(
                mixed,
                ProjectionError::RowNotARecord { ref path, index: 1 } if path == "items"
            ),
            "{mixed}"
        );
        assert_eq!(
            mixed.to_string(),
            "projection row source `items` element 1 is not a record"
        );

        let primitives = projection.render(&json!({ "items": [1, 2] })).unwrap_err();
        assert!(
            matches!(primitives, ProjectionError::RowNotARecord { index: 0, .. }),
            "{primitives}"
        );

        let nested = projection
            .render(&json!({ "items": [[{ "name": "inner" }]] }))
            .unwrap_err();
        assert!(
            matches!(nested, ProjectionError::RowNotARecord { index: 0, .. }),
            "{nested}"
        );
    }

    #[test]
    fn preserves_key_based_header_fallback_for_direct_and_derived_columns() {
        let projection = CsvProjection::builder("items")
            .column(Column::new(Width::default()).key("language"))
            .derived_column(Column::new(Width::default()).key("net"), |row, _root| {
                json!(row["code"].as_i64().unwrap() - row["comments"].as_i64().unwrap())
            })
            .build();

        assert_eq!(
            projection
                .render(&json!({
                    "items": [{ "language": "Rust", "code": 12, "comments": 2 }]
                }))
                .unwrap(),
            "language,net\nRust,10\n"
        );
    }
}
