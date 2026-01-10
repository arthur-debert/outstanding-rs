//! MiniJinja filters for tabular output.
//!
//! This module provides filters that can be used in templates to format
//! values for columnar display.

use minijinja::{Environment, Value};

use super::util::{
    display_width, pad_center, pad_left, pad_right, truncate_end, truncate_middle, truncate_start,
};

/// Register all table-related filters on a MiniJinja environment.
///
/// # Filters Added
///
/// - `col(width, ...)` - Format value to fit column width
/// - `pad_left(width)` - Right-align with padding
/// - `pad_right(width)` - Left-align with padding
/// - `truncate_at(width, pos, ellipsis)` - Truncate at position
///
/// # Example
///
/// ```rust,ignore
/// use minijinja::Environment;
/// use outstanding::table::filters::register_table_filters;
///
/// let mut env = Environment::new();
/// register_table_filters(&mut env);
/// ```
pub fn register_table_filters(env: &mut Environment<'static>) {
    // col filter: {{ value | col(width) }} or {{ value | col(width, align="right", truncate="middle") }}
    // "fill" support (Option B): {{ value | col("fill", width=80) }}
    env.add_filter(
        "col",
        |value: Value, width_val: Value, kwargs: minijinja::value::Kwargs| -> Result<String, minijinja::Error> {
            let text = value.to_string();
            
            // Resolve width: can be number or "fill" (requiring 'width' kwarg)
            let width = if let Some(w) = width_val.as_i64() {
                w as usize
            } else if let Some(s) = width_val.as_str() {
                if s == "fill" {
                    kwargs.get::<usize>("width").map_err(|_| {
                        minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            "Using col('fill') requires explicit 'width' argument (e.g. width=80)",
                        )
                    })?
                } else {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        format!("Invalid width string: '{}'. Use number or 'fill'", s),
                    ));
                }
            } else {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "Width valid must be an integer or 'fill'",
                ));
            };

            let align = kwargs.get::<Option<String>>("align")?.unwrap_or_default();
            let truncate = kwargs.get::<Option<String>>("truncate")?.unwrap_or_default();
            let ellipsis = kwargs
                .get::<Option<String>>("ellipsis")?
                .unwrap_or_else(|| "…".to_string());

            kwargs.assert_all_used()?;

            Ok(format_col(&text, width, &align, &truncate, &ellipsis))
        },
    );

    // pad_left filter: {{ value | pad_left(width) }}
    env.add_filter("pad_left", |value: Value, width: usize| -> String {
        pad_left(&value.to_string(), width)
    });

    // pad_right filter: {{ value | pad_right(width) }}
    env.add_filter("pad_right", |value: Value, width: usize| -> String {
        pad_right(&value.to_string(), width)
    });

    // pad_center filter: {{ value | pad_center(width) }}
    env.add_filter("pad_center", |value: Value, width: usize| -> String {
        pad_center(&value.to_string(), width)
    });

    // truncate_at filter: {{ value | truncate_at(width, "middle") }}
    env.add_filter(
        "truncate_at",
        |value: Value, width: usize, position: Option<String>, ellipsis: Option<String>| -> String {
            let text = value.to_string();
            let pos = position.as_deref().unwrap_or("end");
            let ell = ellipsis.as_deref().unwrap_or("…");

            match pos {
                "start" => truncate_start(&text, width, ell),
                "middle" => truncate_middle(&text, width, ell),
                _ => truncate_end(&text, width, ell),
            }
        },
    );

    // display_width filter: {{ value | display_width }}
    env.add_filter("display_width", |value: Value| -> usize {
        display_width(&value.to_string())
    });
}

/// Format a value for a column with specified width, alignment, and truncation.
fn format_col(text: &str, width: usize, align: &str, truncate: &str, ellipsis: &str) -> String {
    if width == 0 {
        return String::new();
    }

    let current_width = display_width(text);

    // Truncate if needed
    let truncated = if current_width > width {
        match truncate {
            "start" => truncate_start(text, width, ellipsis),
            "middle" => truncate_middle(text, width, ellipsis),
            _ => truncate_end(text, width, ellipsis),
        }
    } else {
        text.to_string()
    };

    // Pad to width
    match align {
        "right" => pad_left(&truncated, width),
        "center" => pad_center(&truncated, width),
        _ => pad_right(&truncated, width),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;
    use serde::Serialize;

    fn setup_env() -> Environment<'static> {
        let mut env = Environment::new();
        register_table_filters(&mut env);
        env
    }

    #[test]
    fn filter_col_basic() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10) }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello"))
            .unwrap();
        assert_eq!(result, "hello     ");
    }

    #[test]
    fn filter_col_truncate() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(8) }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert_eq!(result, "hello w…");
    }

    #[test]
    fn filter_col_right_align() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10, align='right') }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "42"))
            .unwrap();
        assert_eq!(result, "        42");
    }

    #[test]
    fn filter_col_center_align() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10, align='center') }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hi"))
            .unwrap();
        assert_eq!(result, "    hi    ");
    }

    #[test]
    fn filter_col_truncate_middle() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10, truncate='middle') }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "abcdefghijklmno"))
            .unwrap();
        assert_eq!(display_width(&result), 10);
        assert!(result.contains("…"));
    }

    #[test]
    fn filter_col_custom_ellipsis() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10, ellipsis='...') }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert!(result.contains("..."));
    }

    #[test]
    fn filter_pad_left() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | pad_left(8) }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "42"))
            .unwrap();
        assert_eq!(result, "      42");
    }

    #[test]
    fn filter_pad_right() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | pad_right(8) }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hi"))
            .unwrap();
        assert_eq!(result, "hi      ");
    }

    #[test]
    fn filter_pad_center() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | pad_center(8) }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hi"))
            .unwrap();
        assert_eq!(result, "   hi   ");
    }

    #[test]
    fn filter_truncate_at_end() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | truncate_at(8) }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert_eq!(result, "hello w…");
    }

    #[test]
    fn filter_truncate_at_start() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | truncate_at(8, 'start') }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert!(result.starts_with("…"));
        assert_eq!(display_width(&result), 8);
    }

    #[test]
    fn filter_truncate_at_middle() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | truncate_at(8, 'middle') }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert!(result.contains("…"));
        assert_eq!(display_width(&result), 8);
    }

    #[test]
    fn filter_truncate_at_custom_ellipsis() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | truncate_at(10, 'end', '...') }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert!(result.contains("..."));
    }

    #[test]
    fn filter_display_width() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | display_width }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello"))
            .unwrap();
        assert_eq!(result, "5");
    }

    #[derive(Serialize)]
    struct Item {
        name: &'static str,
        value: &'static str,
    }

    #[test]
    fn filter_col_fill_option_b() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col('fill', width=10) }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello"))
            .unwrap();
        assert_eq!(result, "hello     ");
    }

    #[test]
    fn filter_col_fill_missing_width_fails() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col('fill') }}").unwrap();
        let result = env.get_template("test").unwrap()
            .render(context!(value => "hello"));
        assert!(result.is_err());
    }

    #[test]
    fn filter_in_loop() {
        let mut env = setup_env();
        env.add_template("test", r#"{% for item in items %}{{ item.name | col(10) }}  {{ item.value | col(5, align='right') }}
{% endfor %}"#).unwrap();

        let items = vec![
            Item { name: "foo", value: "1" },
            Item { name: "bar", value: "22" },
            Item { name: "bazqux", value: "333" },
        ];

        let result = env.get_template("test").unwrap()
            .render(context!(items => items))
            .unwrap();

        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("foo       "));
        assert!(lines[1].starts_with("bar       "));
    }
}
