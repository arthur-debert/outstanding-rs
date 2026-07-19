//! Utility functions for text processing and color conversion.

use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Converts an RGB triplet to the nearest ANSI 256-color palette index.
///
/// # Example
///
/// ```rust
/// use standout_render::rgb_to_ansi256;
///
/// // Pure red maps to ANSI 196
/// assert_eq!(rgb_to_ansi256((255, 0, 0)), 196);
///
/// // Pure green maps to ANSI 46
/// assert_eq!(rgb_to_ansi256((0, 255, 0)), 46);
/// ```
pub fn rgb_to_ansi256((r, g, b): (u8, u8, u8)) -> u8 {
    if r == g && g == b {
        if r < 8 {
            16
        } else if r > 248 {
            231
        } else {
            232 + ((r as u16 - 8) * 24 / 247) as u8
        }
    } else {
        let red = (r as u16 * 5 / 255) as u8;
        let green = (g as u16 * 5 / 255) as u8;
        let blue = (b as u16 * 5 / 255) as u8;
        16 + 36 * red + 6 * green + blue
    }
}

/// Placeholder helper for true-color output.
///
/// Currently returns the RGB triplet unchanged so it can be handed
/// to future true-color aware APIs.
pub fn rgb_to_truecolor(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    rgb
}

/// Truncates a string to fit within a maximum display width, adding ellipsis if needed.
///
/// Uses Unicode width calculations for proper handling of CJK and other wide
/// characters. Semantic style tags are zero-width and remain balanced around
/// the retained prefix. If the string fits within `max_width`, it is returned
/// unchanged. If truncation is needed, characters are removed from the end and
/// replaced with `…` (ellipsis).
///
/// # Arguments
///
/// * `s` - The string to truncate
/// * `max_width` - Maximum display width (in terminal columns)
///
/// # Example
///
/// ```rust
/// use standout_render::truncate_to_width;
///
/// assert_eq!(truncate_to_width("Hello", 10), "Hello");
/// assert_eq!(truncate_to_width("Hello World", 6), "Hello…");
/// ```
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    truncate_to_width_with_policy(s, max_width, crate::AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`truncate_to_width`].
pub fn truncate_to_width_with_policy(
    s: &str,
    max_width: usize,
    policy: crate::AmbiguousWidth,
) -> String {
    let calculator = crate::WidthCalculator::new(policy);
    // Preserve this utility's historical width-zero behavior. Tabular
    // truncation uses the strict width-bounded interface directly.
    if max_width == 0 && calculator.visible_width(s) > 0 {
        return "…".to_string();
    }
    calculator.truncate_visible(s, max_width, "…", crate::width::VisibleTruncateAt::End)
}

/// Serializes data to XML, handling all serializable types.
///
/// Named structs serialize directly (using the struct name as root element).
/// Map-like types are wrapped in a `<data>` root tag with keys sanitized to
/// valid XML element names. Primitive types (strings, numbers, booleans) are
/// wrapped as `<data><value>...</value></data>`. Null values produce an empty
/// `<data/>` element.
pub fn serialize_to_xml<T: Serialize + ?Sized>(data: &T) -> Result<String, quick_xml::DeError> {
    // Direct serialization works for named structs (keys are known valid)
    if let Ok(xml) = quick_xml::se::to_string(data) {
        return Ok(xml);
    }
    // For types that need a root element (maps, primitives, arrays),
    // convert to JSON Value, sanitize keys, and serialize with root tag
    let value = serde_json::to_value(data).unwrap_or(serde_json::Value::Null);
    let sanitized = sanitize_xml_keys(&value);
    match sanitized {
        serde_json::Value::Object(_) => quick_xml::se::to_string_with_root("data", &sanitized),
        serde_json::Value::Null => quick_xml::se::to_string_with_root(
            "data",
            &serde_json::Value::Object(serde_json::Map::new()),
        ),
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            quick_xml::se::to_string_with_root("data", &serde_json::Value::Object(map))
        }
    }
}

/// Recursively sanitizes JSON object keys to be valid XML element names.
fn sanitize_xml_keys(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (key, val) in map {
                let safe_key = sanitize_xml_name(key);
                new_map.insert(safe_key, sanitize_xml_keys(val));
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(sanitize_xml_keys).collect())
        }
        other => other.clone(),
    }
}

/// Ensures a string is a valid XML element name.
///
/// XML names must start with a letter or underscore. Subsequent characters
/// may be letters, digits, hyphens, underscores, or periods. Invalid
/// characters are replaced with underscores.
fn sanitize_xml_name(name: &str) -> String {
    if name.is_empty() {
        return "_".to_string();
    }
    let mut result = String::with_capacity(name.len() + 1);
    for (i, c) in name.chars().enumerate() {
        if i == 0 {
            if c.is_ascii_alphabetic() || c == '_' {
                result.push(c);
            } else {
                result.push('_');
                if c.is_ascii_alphanumeric() {
                    result.push(c);
                }
            }
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
            result.push(c);
        } else {
            result.push('_');
        }
    }
    result
}

/// Flattens a JSON Value into a list of records for CSV export.
///
/// Returns a tuple of `(headers, rows)`, where rows are vectors of strings corresponding to headers.
///
/// - If `value` is an Array, each element becomes a row.
/// - If `value` is an Object, it becomes a single row.
/// - Nested objects are flattened with dot notation.
/// - Arrays inside objects are flattened with indexed keys (e.g., items.0, items.1).
pub fn flatten_json_for_csv(value: &Value) -> (Vec<String>, Vec<Vec<String>>) {
    let mut rows: Vec<BTreeMap<String, String>> = Vec::new();

    match value {
        Value::Array(arr) => {
            for item in arr {
                rows.push(flatten_single_item(item));
            }
        }
        _ => {
            rows.push(flatten_single_item(value));
        }
    }

    // Collect all unique keys
    let mut headers_set = BTreeSet::new();
    for row in &rows {
        for key in row.keys() {
            headers_set.insert(key.clone());
        }
    }
    let headers: Vec<String> = headers_set.into_iter().collect();

    // Map rows to value lists based on headers
    let mut data = Vec::new();
    for row in rows {
        let mut row_data = Vec::new();
        for header in &headers {
            row_data.push(row.get(header).cloned().unwrap_or_default());
        }
        data.push(row_data);
    }

    (headers, data)
}

fn flatten_single_item(value: &Value) -> BTreeMap<String, String> {
    let mut acc = BTreeMap::new();
    flatten_recursive(value, "", &mut acc);
    acc
}

fn flatten_recursive(value: &Value, prefix: &str, acc: &mut BTreeMap<String, String>) {
    match value {
        Value::Null => {}
        Value::Bool(b) => {
            let key = if prefix.is_empty() { "value" } else { prefix };
            acc.insert(key.to_string(), b.to_string());
        }
        Value::Number(n) => {
            let key = if prefix.is_empty() { "value" } else { prefix };
            acc.insert(key.to_string(), n.to_string());
        }
        Value::String(s) => {
            let key = if prefix.is_empty() { "value" } else { prefix };
            acc.insert(key.to_string(), s.clone());
        }
        Value::Array(arr) => {
            let key_prefix = if prefix.is_empty() { "value" } else { prefix };
            for (i, item) in arr.iter().enumerate() {
                let indexed_key = format!("{}.{}", key_prefix, i);
                flatten_recursive(item, &indexed_key, acc);
            }
        }
        Value::Object(map) => {
            if map.is_empty() {
                let key = if prefix.is_empty() { "value" } else { prefix };
                acc.insert(key.to_string(), "{}".to_string());
            } else {
                for (k, v) in map {
                    let new_key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    flatten_recursive(v, &new_key, acc);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_ansi256_grayscale() {
        assert_eq!(rgb_to_ansi256((0, 0, 0)), 16);
        assert_eq!(rgb_to_ansi256((255, 255, 255)), 231);
        let mid = rgb_to_ansi256((128, 128, 128));
        assert!((232..=255).contains(&mid));
    }

    #[test]
    fn test_rgb_to_ansi256_color_cube() {
        assert_eq!(rgb_to_ansi256((255, 0, 0)), 196);
        assert_eq!(rgb_to_ansi256((0, 255, 0)), 46);
        assert_eq!(rgb_to_ansi256((0, 0, 255)), 21);
    }

    #[test]
    fn test_truncate_to_width_no_truncation() {
        assert_eq!(truncate_to_width("Hello", 10), "Hello");
        assert_eq!(truncate_to_width("Hello", 5), "Hello");
    }

    #[test]
    fn test_truncate_to_width_with_truncation() {
        assert_eq!(truncate_to_width("Hello World", 6), "Hello…");
        assert_eq!(truncate_to_width("Hello World", 7), "Hello …");
    }

    #[test]
    fn test_truncate_to_width_empty() {
        assert_eq!(truncate_to_width("", 5), "");
    }

    #[test]
    fn test_truncate_to_width_exact_fit() {
        assert_eq!(truncate_to_width("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_to_width_one_over() {
        assert_eq!(truncate_to_width("123456", 5), "1234…");
    }

    #[test]
    fn test_truncate_to_width_zero_width() {
        assert_eq!(truncate_to_width("Hello", 0), "…");
    }

    #[test]
    fn test_truncate_to_width_one_width() {
        assert_eq!(truncate_to_width("Hello", 1), "…");
    }

    #[test]
    fn test_truncate_to_width_preserves_semantic_style() {
        assert_eq!(
            truncate_to_width("[match]Hello World[/match]", 6),
            "[match]Hello[/match]…"
        );
    }

    #[test]
    fn test_serialize_to_xml_named_struct() {
        #[derive(serde::Serialize)]
        struct User {
            name: String,
            age: u32,
        }

        let data = User {
            name: "Alice".into(),
            age: 30,
        };
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<User>"));
        assert!(xml.contains("<name>Alice</name>"));
        assert!(xml.contains("<age>30</age>"));
    }

    #[test]
    fn test_serialize_to_xml_json_object() {
        let data = serde_json::json!({"name": "test", "count": 42});
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data>"));
        assert!(xml.contains("<name>test</name>"));
        assert!(xml.contains("<count>42</count>"));
    }

    #[test]
    fn test_serialize_to_xml_nested_object() {
        let data = serde_json::json!({"user": {"name": "Bob", "age": 25}});
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data>"));
        assert!(xml.contains("<user>"));
        assert!(xml.contains("<name>Bob</name>"));
    }

    #[test]
    fn test_serialize_to_xml_with_array_field() {
        let data = serde_json::json!({"tags": ["a", "b", "c"]});
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data>"));
        assert!(xml.contains("<tags>a</tags>"));
    }

    #[test]
    fn test_serialize_to_xml_empty_object() {
        let data = serde_json::json!({});
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data"));
    }

    #[test]
    fn test_serialize_to_xml_hashmap() {
        let mut data = std::collections::HashMap::new();
        data.insert("key", "value");
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data>"));
        assert!(xml.contains("<key>value</key>"));
    }

    #[test]
    fn test_serialize_to_xml_null() {
        let data = serde_json::Value::Null;
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data"));
    }

    #[test]
    fn test_serialize_to_xml_bare_string() {
        let data = serde_json::json!("hello");
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data>"));
        assert!(xml.contains("<value>hello</value>"));
    }

    #[test]
    fn test_serialize_to_xml_bare_number() {
        let data = serde_json::json!(42);
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data>"));
        assert!(xml.contains("<value>42</value>"));
    }

    #[test]
    fn test_serialize_to_xml_bare_bool() {
        let data = serde_json::json!(true);
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data>"));
        assert!(xml.contains("<value>true</value>"));
    }

    #[test]
    fn test_serialize_to_xml_bare_array() {
        let data = serde_json::json!(["a", "b", "c"]);
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data>"));
        assert!(xml.contains("<value>"));
    }

    #[test]
    fn test_serialize_to_xml_numeric_keys() {
        let data = serde_json::json!({"0": "zero", "1": "one"});
        let xml = serialize_to_xml(&data).unwrap();
        assert!(xml.contains("<data>"));
        // Keys starting with digits get prefixed with underscore
        assert!(xml.contains("<_0>zero</_0>"));
        assert!(xml.contains("<_1>one</_1>"));
    }

    #[test]
    fn test_sanitize_xml_name_valid() {
        assert_eq!(sanitize_xml_name("name"), "name");
        assert_eq!(sanitize_xml_name("_private"), "_private");
        assert_eq!(sanitize_xml_name("item-1"), "item-1");
        assert_eq!(sanitize_xml_name("a.b"), "a.b");
    }

    #[test]
    fn test_sanitize_xml_name_digit_start() {
        assert_eq!(sanitize_xml_name("0"), "_0");
        assert_eq!(sanitize_xml_name("1abc"), "_1abc");
        assert_eq!(sanitize_xml_name("42"), "_42");
    }

    #[test]
    fn test_sanitize_xml_name_empty() {
        assert_eq!(sanitize_xml_name(""), "_");
    }

    #[test]
    fn test_sanitize_xml_name_special_chars() {
        assert_eq!(sanitize_xml_name("a b"), "a_b");
        assert_eq!(sanitize_xml_name("a@b"), "a_b");
    }

    // =========================================================================
    // flatten_json_for_csv tests
    // =========================================================================

    #[test]
    fn test_flatten_csv_simple_object() {
        let data = serde_json::json!({"name": "Alice", "age": 30});
        let (headers, rows) = flatten_json_for_csv(&data);
        assert_eq!(headers, vec!["age", "name"]);
        assert_eq!(rows, vec![vec!["30", "Alice"]]);
    }

    #[test]
    fn test_flatten_csv_array_of_objects() {
        let data = serde_json::json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]);
        let (headers, rows) = flatten_json_for_csv(&data);
        assert_eq!(headers, vec!["age", "name"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["30", "Alice"]);
        assert_eq!(rows[1], vec!["25", "Bob"]);
    }

    #[test]
    fn test_flatten_csv_nested_objects() {
        let data = serde_json::json!({"user": {"name": "Alice", "age": 30}});
        let (headers, rows) = flatten_json_for_csv(&data);
        assert_eq!(headers, vec!["user.age", "user.name"]);
        assert_eq!(rows, vec![vec!["30", "Alice"]]);
    }

    #[test]
    fn test_flatten_csv_array_field() {
        let data = serde_json::json!({"name": "Alice", "tags": ["a", "b", "c"]});
        let (headers, rows) = flatten_json_for_csv(&data);
        assert!(headers.contains(&"name".to_string()));
        assert!(headers.contains(&"tags.0".to_string()));
        assert!(headers.contains(&"tags.1".to_string()));
        assert!(headers.contains(&"tags.2".to_string()));
        // Check values
        let name_idx = headers.iter().position(|h| h == "name").unwrap();
        let t0_idx = headers.iter().position(|h| h == "tags.0").unwrap();
        let t1_idx = headers.iter().position(|h| h == "tags.1").unwrap();
        let t2_idx = headers.iter().position(|h| h == "tags.2").unwrap();
        assert_eq!(rows[0][name_idx], "Alice");
        assert_eq!(rows[0][t0_idx], "a");
        assert_eq!(rows[0][t1_idx], "b");
        assert_eq!(rows[0][t2_idx], "c");
    }

    #[test]
    fn test_flatten_csv_nested_array_of_objects() {
        let data = serde_json::json!({
            "items": [
                {"name": "x", "value": 1},
                {"name": "y", "value": 2}
            ]
        });
        let (headers, _rows) = flatten_json_for_csv(&data);
        assert!(headers.contains(&"items.0.name".to_string()));
        assert!(headers.contains(&"items.0.value".to_string()));
        assert!(headers.contains(&"items.1.name".to_string()));
        assert!(headers.contains(&"items.1.value".to_string()));
    }

    #[test]
    fn test_flatten_csv_empty_array_field() {
        let data = serde_json::json!({"name": "Alice", "tags": []});
        let (headers, rows) = flatten_json_for_csv(&data);
        assert_eq!(headers, vec!["name"]);
        assert_eq!(rows, vec![vec!["Alice"]]);
    }

    #[test]
    fn test_flatten_csv_mixed_array_rows() {
        // Array of objects where some have arrays and some don't
        let data = serde_json::json!([
            {"name": "Alice", "tags": ["x"]},
            {"name": "Bob"}
        ]);
        let (headers, rows) = flatten_json_for_csv(&data);
        assert!(headers.contains(&"name".to_string()));
        assert!(headers.contains(&"tags.0".to_string()));
        assert_eq!(rows.len(), 2);
        // Bob's tags.0 should be empty
        let t0_idx = headers.iter().position(|h| h == "tags.0").unwrap();
        assert_eq!(rows[1][t0_idx], "");
    }

    #[test]
    fn test_flatten_csv_bare_primitive() {
        let data = serde_json::json!(42);
        let (headers, rows) = flatten_json_for_csv(&data);
        assert_eq!(headers, vec!["value"]);
        assert_eq!(rows, vec![vec!["42"]]);
    }
}
