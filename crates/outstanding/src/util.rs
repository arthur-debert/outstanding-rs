//! Utility functions for text processing and color conversion.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Converts an RGB triplet to the nearest ANSI 256-color palette index.
///
/// # Example
///
/// ```rust
/// use outstanding::rgb_to_ansi256;
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
/// Uses Unicode width calculations for proper handling of CJK and other wide characters.
/// If the string fits within `max_width`, it is returned unchanged. If truncation is
/// needed, characters are removed from the end and replaced with `…` (ellipsis).
///
/// # Arguments
///
/// * `s` - The string to truncate
/// * `max_width` - Maximum display width (in terminal columns)
///
/// # Example
///
/// ```rust
/// use outstanding::truncate_to_width;
///
/// assert_eq!(truncate_to_width("Hello", 10), "Hello");
/// assert_eq!(truncate_to_width("Hello World", 6), "Hello…");
/// ```
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    // If the string fits, return it unchanged
    if s.width() <= max_width {
        return s.to_string();
    }

    let mut result = String::new();
    let mut current_width = 0;
    // Reserve 1 char for ellipsis
    let limit = max_width.saturating_sub(1);

    for c in s.chars() {
        let char_width = c.width().unwrap_or(0);
        if current_width + char_width > limit {
            result.push('…');
            return result;
        }
        result.push(c);
        current_width += char_width;
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
/// - Arrays inside objects are serialized as JSON strings.
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
        Value::Array(_) => {
            // Serialize array as JSON string
            let key = if prefix.is_empty() { "value" } else { prefix };
            acc.insert(key.to_string(), value.to_string());
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
}
