use std::borrow::Cow;

use serde_json::{Map, Value};
use standout_bbparser::ansi::ansi_units;

use crate::error::RenderError;

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

pub fn rgb_to_truecolor(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    rgb
}

pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    truncate_to_width_with_policy(s, max_width, crate::AmbiguousWidth::Narrow)
}

pub fn truncate_to_width_with_policy(
    s: &str,
    max_width: usize,
    policy: crate::AmbiguousWidth,
) -> String {
    let calculator = crate::WidthCalculator::new(policy);
    if max_width == 0 && calculator.visible_width(s) > 0 {
        return "…".to_string();
    }
    calculator.truncate_visible(s, max_width, "…", crate::width::VisibleTruncateAt::End)
}

pub fn escape_style_tags(text: Cow<'_, str>) -> Cow<'_, str> {
    if !text.contains(['[', ']']) {
        return text;
    }
    let plain_bracket =
        ansi_units(&text).any(|unit| !unit.is_escape && unit.text.contains(['[', ']']));
    if !plain_bracket {
        return text;
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    for unit in ansi_units(&text) {
        // An escaped `\[` no longer introduces a CSI sequence, so the width and
        // truncation machinery would count the sequence body as visible text.
        if unit.is_escape {
            escaped.push_str(unit.text);
            continue;
        }
        for character in unit.text.chars() {
            if character == '[' || character == ']' {
                escaped.push('\\');
            }
            escaped.push(character);
        }
    }
    Cow::Owned(escaped)
}

/// `value` is one flat record (scalar values) or an array of them; anything
/// else is a [`RenderError`] pointing at `CsvProjection`. Columns are the keys
/// in first-seen order; a missing or null key is an empty cell.
pub fn csv_records(value: &Value) -> Result<(Vec<String>, Vec<Vec<String>>), RenderError> {
    let records = match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| flat_record(item, &format!("[{index}]")))
            .collect::<Result<Vec<_>, _>>()?,
        record => vec![flat_record(record, "")?],
    };
    let mut headers: Vec<String> = Vec::new();
    for record in &records {
        for key in record.keys() {
            if !headers.contains(key) {
                headers.push(key.clone());
            }
        }
    }
    let rows = records
        .iter()
        .map(|record| {
            headers
                .iter()
                .map(|header| record.get(header).map(scalar_cell).unwrap_or_default())
                .collect()
        })
        .collect();
    Ok((headers, rows))
}

/// The CSV document for `value`, under the rule [`csv_records`] states.
pub fn write_csv(value: &Value) -> Result<String, RenderError> {
    let (headers, rows) = csv_records(value)?;
    let mut writer = csv::Writer::from_writer(Vec::new());
    if !headers.is_empty() {
        writer.write_record(&headers)?;
    }
    for row in rows {
        writer.write_record(&row)?;
    }
    Ok(String::from_utf8(writer.into_inner()?)?)
}

fn flat_record<'a>(value: &'a Value, at: &str) -> Result<&'a Map<String, Value>, RenderError> {
    let Value::Object(record) = value else {
        return Err(not_a_flat_record(at, value));
    };
    for (key, field) in record {
        if matches!(field, Value::Array(_) | Value::Object(_)) {
            let path = if at.is_empty() {
                key.clone()
            } else {
                format!("{at}.{key}")
            };
            return Err(not_a_flat_record(&path, field));
        }
    }
    Ok(record)
}

fn not_a_flat_record(path: &str, value: &Value) -> RenderError {
    let kind = match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    };
    let subject = if path.is_empty() {
        "the document".to_string()
    } else {
        format!("`{path}`")
    };
    RenderError::SerializationError(format!(
        "CSV output takes a flat record or an array of flat records, and {subject} is {kind}; \
         declare the columns with a CsvProjection"
    ))
}

fn scalar_cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
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
    fn truncation_closes_an_ansi_style_it_cuts() {
        assert_eq!(
            truncate_to_width("\u{1b}[31malpha beta gamma\u{1b}[0m", 8),
            "\u{1b}[31malpha b\u{1b}[0m…"
        );
        assert_eq!(
            truncate_to_width("[row]\u{1b}[31malpha beta gamma\u{1b}[0m[/row]", 8),
            "[row]\u{1b}[31malpha b\u{1b}[0m[/row]…"
        );
    }

    // `standout`'s config command carried its own escaper, spelled below as
    // `bracket_replace`. This pins that the surviving function still produces
    // what that one did for the inputs it ever saw, and names the one input
    // class where they part company — the reason only one of them is left.
    #[test]
    fn the_shared_escaper_agrees_with_the_bracket_replacement_it_replaced() {
        fn bracket_replace(text: &str) -> String {
            text.replace('[', "\\[").replace(']', "\\]")
        }

        for text in [
            "",
            "term.color",
            "/home/user/.config/app/config.toml",
            "term.color = auto",
            "value [with] brackets",
            "[unclosed",
            "\\[already escaped\\]",
            "nested [[double]]",
        ] {
            assert_eq!(
                escape_style_tags(Cow::Borrowed(text)),
                bracket_replace(text),
                "{text:?}"
            );
        }

        let with_ansi = "\u{1b}[31mred\u{1b}[0m [tag]";
        assert_eq!(
            escape_style_tags(Cow::Borrowed(with_ansi)),
            "\u{1b}[31mred\u{1b}[0m \\[tag\\]"
        );
        assert_eq!(
            bracket_replace(with_ansi),
            "\u{1b}\\[31mred\u{1b}\\[0m \\[tag\\]"
        );
    }

    #[test]
    fn csv_records_keeps_a_records_declared_column_order() {
        let data = serde_json::json!({"name": "Alice", "age": 30, "admin": true, "note": null});
        let (headers, rows) = csv_records(&data).unwrap();
        assert_eq!(headers, vec!["name", "age", "admin", "note"]);
        assert_eq!(rows, vec![vec!["Alice", "30", "true", ""]]);
    }

    #[test]
    fn csv_records_unions_the_columns_of_an_array_of_records() {
        let data = serde_json::json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "email": "bob@example.com"}
        ]);
        let (headers, rows) = csv_records(&data).unwrap();
        assert_eq!(headers, vec!["name", "age", "email"]);
        assert_eq!(rows[0], vec!["Alice", "30", ""]);
        assert_eq!(rows[1], vec!["Bob", "", "bob@example.com"]);
    }

    #[test]
    fn csv_records_accepts_an_empty_array_as_a_headerless_document() {
        let (headers, rows) = csv_records(&serde_json::json!([])).unwrap();
        assert!(headers.is_empty());
        assert!(rows.is_empty());
        assert_eq!(write_csv(&serde_json::json!([])).unwrap(), "");
    }

    #[test]
    fn write_csv_quotes_cells_the_way_a_reader_expects() {
        let data = serde_json::json!([
            {"name": "a, \"quoted\"", "count": 2},
            {"name": "plain", "count": null}
        ]);
        assert_eq!(
            write_csv(&data).unwrap(),
            "name,count\n\"a, \"\"quoted\"\"\",2\nplain,\n"
        );
    }

    #[test]
    fn a_nested_value_is_a_render_error_naming_csv_projection() {
        let cases = [
            (
                serde_json::json!({"name": "Alice", "tags": ["a"]}),
                "`tags` is an array",
            ),
            (
                serde_json::json!({"user": {"name": "Bob"}}),
                "`user` is an object",
            ),
            (
                serde_json::json!([{"name": "x"}, {"name": "y", "items": [1]}]),
                "`[1].items` is an array",
            ),
            (serde_json::json!([{"ok": 1}, 2]), "`[1]` is a number"),
            (serde_json::json!(42), "the document is a number"),
            (serde_json::json!("text"), "the document is a string"),
            (serde_json::json!(null), "the document is null"),
        ];
        for (data, subject) in cases {
            let error = csv_records(&data).unwrap_err().to_string();
            assert!(error.contains(subject), "{data}: {error}");
            assert!(error.contains("CsvProjection"), "{data}: {error}");
        }
    }
}
