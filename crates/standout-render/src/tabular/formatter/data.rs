use super::*;

impl TabularFormatter {
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

    pub fn row_from<T: Serialize>(&self, value: &T) -> String {
        let values = self.extract_values(value);
        let string_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
        self.format_markup_row(&string_refs)
    }

    pub fn row_lines_from<T: Serialize>(&self, value: &T) -> Vec<String> {
        let values = self.extract_values(value);
        let string_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
        self.format_markup_row_lines(&string_refs)
    }

    pub fn row_from_trait<T: TabularRow>(&self, value: &T) -> String {
        let values = value.to_row();
        self.format_row(&values)
    }

    pub fn row_lines_from_trait<T: TabularRow>(&self, value: &T) -> Vec<String> {
        let values = value.to_row();
        self.format_row_lines(&values)
    }

    fn extract_values<T: Serialize>(&self, value: &T) -> Vec<String> {
        let json = match crate::RenderData::from_serialize(value) {
            Ok(v) => v,
            Err(_) => return vec![String::new(); self.columns.len()],
        };

        self.columns
            .iter()
            .map(|col| {
                let key = col.key.as_ref().or(col.name.as_ref());

                match key {
                    Some(k) => extract_field(&json, k),
                    None => col.null_repr.clone(),
                }
            })
            .collect()
    }
}

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

    match current {
        JsonValue::String(s) => crate::template::presentation::escape_text(s),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => String::new(),
        _ => markup(&current.to_template_value()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::Width;

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
        let json = crate::test_data!({
            "name": "Alice",
            "age": 30
        });

        assert_eq!(extract_field(&json, "name"), "Alice");
        assert_eq!(extract_field(&json, "age"), "30");
        assert_eq!(extract_field(&json, "missing"), "");
    }

    #[test]
    fn extract_field_nested() {
        let json = crate::test_data!({
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
        let json = crate::test_data!({
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
        assert!(!lines.is_empty());
    }

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
}
