//! MiniJinja filters and functions for tabular output.
//!
//! This module provides filters and global functions that can be used in templates
//! to format values for columnar display.
//!
//! ## Filters
//!
//! - `col(width, ...)` - Format value to fit column width
//! - `pad_left(width)` - Right-align with padding
//! - `pad_right(width)` - Left-align with padding
//! - `truncate_at(width, pos, ellipsis)` - Truncate at position
//! - `display_width` - Get display width of a string
//! - `style_as(style)` - Wrap value in style tags
//!
//! ## Global Functions
//!
//! - `tabular(columns, separator=?, width=?)` - Create a TabularFormatter
//! - `table(columns, border=?, header=?, header_style=?)` - Create a Table
//!
//! ### Column Definition Format
//!
//! Columns are specified as dictionaries with these keys:
//! - `width`: Number (fixed), `"fill"`, or `{"min": n, "max": m}` (bounded)
//! - `align`: `"left"` (default), `"right"`, or `"center"`
//! - `truncate`: `"end"` (default), `"start"`, or `"middle"`
//! - `key`: Field name for struct extraction
//! - `header`: Header text for this column
//! - `style`: Style name to wrap cell content
//!
//! ### Example
//!
//! ```jinja
//! {% set fmt = tabular([
//!     {"width": 10, "key": "name"},
//!     {"width": "fill", "key": "description"},
//!     {"width": 8, "align": "right", "key": "count"}
//! ], separator="  ") %}
//!
//! {% for item in items %}
//! {{ fmt.row([item.name, item.description, item.count]) }}
//! {% endfor %}
//! ```

use minijinja::{Environment, Value};

use super::decorator::{BorderStyle, Table};
use super::formatter::TabularFormatter;
use super::types::{Align, Column, Overflow, TabularSpec, TruncateAt, Width};
use super::util::{
    display_width, pad_center, pad_left, pad_right, truncate_end, truncate_middle, truncate_start,
};

/// Register all tabular-related filters on a MiniJinja environment.
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
/// use outstanding::tabular::filters::register_tabular_filters;
///
/// let mut env = Environment::new();
/// register_tabular_filters(&mut env);
/// ```
pub fn register_tabular_filters(env: &mut Environment<'static>) {
    // col filter: {{ value | col(width) }} or {{ value | col(width, align="right", truncate="middle") }}
    // "fill" support (Option B): {{ value | col("fill", width=80) }}
    env.add_filter(
        "col",
        |value: Value,
         width_val: Value,
         kwargs: minijinja::value::Kwargs|
         -> Result<String, minijinja::Error> {
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
            let truncate = kwargs
                .get::<Option<String>>("truncate")?
                .unwrap_or_default();
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
        |value: Value,
         width: usize,
         position: Option<String>,
         ellipsis: Option<String>|
         -> String {
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

    // style_as filter: {{ value | style_as("error") }} => [error]value[/error]
    env.add_filter("style_as", |value: Value, style: String| -> String {
        let text = value.to_string();
        if style.is_empty() {
            text
        } else {
            format!("[{}]{}[/{}]", style, text, style)
        }
    });

    // Register global functions for creating formatters
    register_table_functions(env);
}

/// Register global functions for creating table formatters.
fn register_table_functions(env: &mut Environment<'static>) {
    // tabular(columns, separator=?, width=?) -> TabularFormatter
    env.add_function(
        "tabular",
        |columns: Value, kwargs: minijinja::value::Kwargs| -> Result<Value, minijinja::Error> {
            let cols = parse_columns(&columns)?;
            let separator = kwargs
                .get::<Option<String>>("separator")?
                .unwrap_or_default();
            let width = kwargs.get::<Option<usize>>("width")?.unwrap_or(80);
            kwargs.assert_all_used()?;

            let mut builder = TabularSpec::builder();
            for col in cols {
                builder = builder.column(col);
            }
            if !separator.is_empty() {
                builder = builder.separator(&separator);
            }

            let spec = builder.build();
            let formatter = TabularFormatter::new(&spec, width);
            Ok(Value::from_object(formatter))
        },
    );

    // table(columns, border=?, header=?, header_style=?, width=?) -> Table
    env.add_function(
        "table",
        |columns: Value, kwargs: minijinja::value::Kwargs| -> Result<Value, minijinja::Error> {
            let cols = parse_columns(&columns)?;
            let separator = kwargs
                .get::<Option<String>>("separator")?
                .unwrap_or_default();
            let border = kwargs.get::<Option<String>>("border")?.unwrap_or_default();
            let header = kwargs.get::<Option<Value>>("header")?;
            let header_style = kwargs.get::<Option<String>>("header_style")?;
            let row_separator = kwargs
                .get::<Option<bool>>("row_separator")?
                .unwrap_or(false);
            let width = kwargs.get::<Option<usize>>("width")?.unwrap_or(80);
            kwargs.assert_all_used()?;

            let mut builder = TabularSpec::builder();
            for col in cols {
                builder = builder.column(col);
            }
            if !separator.is_empty() {
                builder = builder.separator(&separator);
            }

            let spec = builder.build();
            let mut table = Table::new(spec, width).border(parse_border_style(&border));

            // Set header if provided
            if let Some(h) = header {
                let headers: Vec<String> = h
                    .try_iter()
                    .map_err(|_| {
                        minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            "header must be an array of strings",
                        )
                    })?
                    .map(|v| v.to_string())
                    .collect();
                table = table.header(headers);
            }

            // Set header style if provided
            if let Some(style) = header_style {
                table = table.header_style(style);
            }

            // Set row separator if enabled
            if row_separator {
                table = table.row_separator(true);
            }

            Ok(Value::from_object(table))
        },
    );
}

/// Parse column definitions from a template array value.
fn parse_columns(columns: &Value) -> Result<Vec<Column>, minijinja::Error> {
    let iter = columns.try_iter().map_err(|_| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "columns must be an array",
        )
    })?;

    let mut result = Vec::new();
    for col_val in iter {
        let col = parse_column(&col_val)?;
        result.push(col);
    }
    Ok(result)
}

/// Parse a single column definition from a template value.
fn parse_column(value: &Value) -> Result<Column, minijinja::Error> {
    // Get width - required
    let width_val = value.get_attr("width").map_err(|_| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "column must have a 'width' attribute",
        )
    })?;

    let width = parse_width(&width_val)?;
    let mut col = Column::new(width);

    // Optional: align
    if let Ok(align_val) = value.get_attr("align") {
        if !align_val.is_none() && !align_val.is_undefined() {
            col = col.align(parse_align(&align_val.to_string()));
        }
    }

    // Optional: truncate
    if let Ok(truncate_val) = value.get_attr("truncate") {
        if !truncate_val.is_none() && !truncate_val.is_undefined() {
            col = col.truncate(parse_truncate(&truncate_val.to_string()));
        }
    }

    // Optional: key
    if let Ok(key_val) = value.get_attr("key") {
        if !key_val.is_none() && !key_val.is_undefined() {
            col = col.key(key_val.to_string());
        }
    }

    // Optional: header
    if let Ok(header_val) = value.get_attr("header") {
        if !header_val.is_none() && !header_val.is_undefined() {
            col = col.header(header_val.to_string());
        }
    }

    // Optional: style
    if let Ok(style_val) = value.get_attr("style") {
        if !style_val.is_none() && !style_val.is_undefined() {
            col = col.style(style_val.to_string());
        }
    }

    // Optional: null_repr
    if let Ok(null_val) = value.get_attr("null_repr") {
        if !null_val.is_none() && !null_val.is_undefined() {
            col = col.null_repr(null_val.to_string());
        }
    }

    // Optional: anchor
    if let Ok(anchor_val) = value.get_attr("anchor") {
        if !anchor_val.is_none()
            && !anchor_val.is_undefined()
            && anchor_val.to_string().to_lowercase() == "right"
        {
            col = col.anchor_right();
        }
    }

    // Optional: overflow
    if let Ok(overflow_val) = value.get_attr("overflow") {
        if !overflow_val.is_none() && !overflow_val.is_undefined() {
            col = col.overflow(parse_overflow(&overflow_val)?);
        }
    }

    Ok(col)
}

/// Parse an overflow specification from a template value.
fn parse_overflow(value: &Value) -> Result<Overflow, minijinja::Error> {
    // String shorthand: "truncate", "wrap", "clip", "expand"
    if let Some(s) = value.as_str() {
        return Ok(match s.to_lowercase().as_str() {
            "wrap" => Overflow::wrap(),
            "clip" => Overflow::Clip,
            "expand" => Overflow::Expand,
            "truncate_start" => Overflow::truncate(TruncateAt::Start),
            "truncate_middle" => Overflow::truncate(TruncateAt::Middle),
            _ => Overflow::truncate(TruncateAt::End), // "truncate" or "truncate_end"
        });
    }

    // Object form: {"truncate": {"at": "middle", "marker": "..."}} or {"wrap": {"indent": 2}}
    if let Ok(truncate_obj) = value.get_attr("truncate") {
        if !truncate_obj.is_none() && !truncate_obj.is_undefined() {
            let at = if let Ok(at_val) = truncate_obj.get_attr("at") {
                parse_truncate(&at_val.to_string())
            } else {
                TruncateAt::End
            };
            let marker = if let Ok(marker_val) = truncate_obj.get_attr("marker") {
                if !marker_val.is_none() && !marker_val.is_undefined() {
                    marker_val.to_string()
                } else {
                    "…".to_string()
                }
            } else {
                "…".to_string()
            };
            return Ok(Overflow::truncate_with_marker(at, marker));
        }
    }

    if let Ok(wrap_obj) = value.get_attr("wrap") {
        if !wrap_obj.is_none() && !wrap_obj.is_undefined() {
            let indent = if let Ok(indent_val) = wrap_obj.get_attr("indent") {
                indent_val.as_usize().unwrap_or(0)
            } else {
                0
            };
            return Ok(Overflow::wrap_with_indent(indent));
        }
    }

    // Default to truncate
    Ok(Overflow::default())
}

/// Parse a width specification from a template value.
fn parse_width(value: &Value) -> Result<Width, minijinja::Error> {
    // Number -> Fixed width
    if let Some(n) = value.as_i64() {
        return Ok(Width::Fixed(n as usize));
    }

    // String "fill" or "Nfr" (fractional) -> Fill or Fraction
    if let Some(s) = value.as_str() {
        if s == "fill" {
            return Ok(Width::Fill);
        }

        // Check for fractional syntax: "2fr", "1fr", etc.
        if let Some(num_part) = s.strip_suffix("fr") {
            if let Ok(n) = num_part.parse::<usize>() {
                return Ok(Width::Fraction(n));
            }
        }

        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "unknown width string: '{}' (use number, 'fill', 'Nfr', or object)",
                s
            ),
        ));
    }

    // Object with min and/or max -> Bounded
    let min_result = value.get_attr("min");
    let max_result = value.get_attr("max");

    let has_min = min_result.is_ok()
        && !min_result.as_ref().unwrap().is_none()
        && !min_result.as_ref().unwrap().is_undefined();
    let has_max = max_result.is_ok()
        && !max_result.as_ref().unwrap().is_none()
        && !max_result.as_ref().unwrap().is_undefined();

    if has_min || has_max {
        let min_val = if has_min {
            Some(min_result.unwrap().as_usize().ok_or_else(|| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "min must be a number",
                )
            })?)
        } else {
            None
        };

        let max_val = if has_max {
            Some(max_result.unwrap().as_usize().ok_or_else(|| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "max must be a number",
                )
            })?)
        } else {
            None
        };

        return Ok(Width::Bounded {
            min: min_val,
            max: max_val,
        });
    }

    // Object with fraction -> Fraction
    if let Ok(frac) = value.get_attr("fraction") {
        let frac_val = frac.as_usize().ok_or_else(|| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "fraction must be a number",
            )
        })?;
        return Ok(Width::Fraction(frac_val));
    }

    Err(minijinja::Error::new(
        minijinja::ErrorKind::InvalidOperation,
        "width must be a number, 'fill', or object with min/max or fraction",
    ))
}

/// Parse alignment from string.
fn parse_align(s: &str) -> Align {
    match s.to_lowercase().as_str() {
        "right" => Align::Right,
        "center" => Align::Center,
        _ => Align::Left,
    }
}

/// Parse truncation position from string.
fn parse_truncate(s: &str) -> TruncateAt {
    match s.to_lowercase().as_str() {
        "start" => TruncateAt::Start,
        "middle" => TruncateAt::Middle,
        _ => TruncateAt::End,
    }
}

/// Parse border style from string.
fn parse_border_style(s: &str) -> BorderStyle {
    match s.to_lowercase().as_str() {
        "ascii" => BorderStyle::Ascii,
        "light" => BorderStyle::Light,
        "heavy" => BorderStyle::Heavy,
        "double" => BorderStyle::Double,
        "rounded" => BorderStyle::Rounded,
        _ => BorderStyle::None,
    }
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
        register_tabular_filters(&mut env);
        env
    }

    #[test]
    fn filter_col_basic() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10) }}").unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hello"))
            .unwrap();
        assert_eq!(result, "hello     ");
    }

    #[test]
    fn filter_col_truncate() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(8) }}").unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert_eq!(result, "hello w…");
    }

    #[test]
    fn filter_col_right_align() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10, align='right') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "42"))
            .unwrap();
        assert_eq!(result, "        42");
    }

    #[test]
    fn filter_col_center_align() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10, align='center') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hi"))
            .unwrap();
        assert_eq!(result, "    hi    ");
    }

    #[test]
    fn filter_col_truncate_middle() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10, truncate='middle') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "abcdefghijklmno"))
            .unwrap();
        assert_eq!(display_width(&result), 10);
        assert!(result.contains("…"));
    }

    #[test]
    fn filter_col_custom_ellipsis() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10, ellipsis='...') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert!(result.contains("..."));
    }

    #[test]
    fn filter_pad_left() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | pad_left(8) }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "42"))
            .unwrap();
        assert_eq!(result, "      42");
    }

    #[test]
    fn filter_pad_right() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | pad_right(8) }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hi"))
            .unwrap();
        assert_eq!(result, "hi      ");
    }

    #[test]
    fn filter_pad_center() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | pad_center(8) }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hi"))
            .unwrap();
        assert_eq!(result, "   hi   ");
    }

    #[test]
    fn filter_truncate_at_end() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | truncate_at(8) }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert_eq!(result, "hello w…");
    }

    #[test]
    fn filter_truncate_at_start() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | truncate_at(8, 'start') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert!(result.starts_with("…"));
        assert_eq!(display_width(&result), 8);
    }

    #[test]
    fn filter_truncate_at_middle() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | truncate_at(8, 'middle') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert!(result.contains("…"));
        assert_eq!(display_width(&result), 8);
    }

    #[test]
    fn filter_truncate_at_custom_ellipsis() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | truncate_at(10, 'end', '...') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hello world"))
            .unwrap();
        assert!(result.contains("..."));
    }

    #[test]
    fn filter_display_width() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | display_width }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
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
        env.add_template("test", "{{ value | col('fill', width=10) }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hello"))
            .unwrap();
        assert_eq!(result, "hello     ");
    }

    #[test]
    fn filter_col_fill_missing_width_fails() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col('fill') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "hello"));
        assert!(result.is_err());
    }

    #[test]
    fn filter_in_loop() {
        let mut env = setup_env();
        env.add_template("test", r#"{% for item in items %}{{ item.name | col(10) }}  {{ item.value | col(5, align='right') }}
{% endfor %}"#).unwrap();

        let items = vec![
            Item {
                name: "foo",
                value: "1",
            },
            Item {
                name: "bar",
                value: "22",
            },
            Item {
                name: "bazqux",
                value: "333",
            },
        ];

        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(items => items))
            .unwrap();

        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("foo       "));
        assert!(lines[1].starts_with("bar       "));
    }

    #[test]
    fn filter_style_as() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | style_as('error') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "Error message"))
            .unwrap();
        assert_eq!(result, "[error]Error message[/error]");
    }

    #[test]
    fn filter_style_as_empty() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | style_as('') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "text"))
            .unwrap();
        assert_eq!(result, "text");
    }

    #[test]
    fn filter_style_as_combined_with_col() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10) | style_as('header') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "Name"))
            .unwrap();
        assert_eq!(result, "[header]Name      [/header]");
    }

    // ============================================================================
    // Template Function Tests (Phase 9)
    // ============================================================================

    #[test]
    fn function_tabular_basic() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 10}, {"width": 8}], separator="  ") %}{{ fmt.row(["Hello", "World"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert_eq!(result, "Hello       World   ");
    }

    #[test]
    fn function_tabular_in_loop() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 8}, {"width": 6}], separator="  ") %}{% for item in items %}{{ fmt.row([item.name, item.value]) }}
{% endfor %}"#,
        )
        .unwrap();

        let items = vec![
            Item {
                name: "Alice",
                value: "100",
            },
            Item {
                name: "Bob",
                value: "200",
            },
        ];

        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(items => items))
            .unwrap();

        assert!(result.contains("Alice"));
        assert!(result.contains("Bob"));
    }

    #[test]
    fn function_tabular_fill_width() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 5}, {"width": "fill"}], separator="  ", width=20) %}{{ fmt.row(["A", "B"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Total 20, first col 5, sep 2, fill col = 13
        assert_eq!(display_width(&result), 20);
    }

    #[test]
    fn function_tabular_right_align() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 10, "align": "right"}]) %}{{ fmt.row(["42"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert_eq!(result, "        42");
    }

    #[test]
    fn function_tabular_with_style() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 10, "style": "name"}]) %}{{ fmt.row(["Alice"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert!(result.contains("[name]"));
        assert!(result.contains("[/name]"));
    }

    #[test]
    fn function_table_basic() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set tbl = table([{"width": 10}, {"width": 8}], separator="  ") %}{{ tbl.row(["Hello", "World"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // No border, just content
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
    }

    #[test]
    fn function_table_with_border() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set tbl = table([{"width": 10}, {"width": 8}], border="light") %}{{ tbl.row(["Hello", "World"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Should have light border characters
        assert!(result.starts_with('│'));
        assert!(result.ends_with('│'));
    }

    #[test]
    fn function_table_with_header() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set tbl = table([{"width": 10}, {"width": 8}], header=["Name", "Value"]) %}{{ tbl.header_row() }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert!(result.contains("Name"));
        assert!(result.contains("Value"));
    }

    #[test]
    fn function_table_separator_row() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set tbl = table([{"width": 10}, {"width": 8}], border="light") %}{{ tbl.separator_row() }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert!(result.contains('─'));
        assert!(result.starts_with('├'));
    }

    #[test]
    fn function_table_render_all() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set tbl = table([{"width": 10}, {"width": 8}], border="light", header=["Name", "Val"]) %}{{ tbl.render_all([["Alice", "100"], ["Bob", "200"]]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();

        let lines: Vec<&str> = result.lines().collect();
        // Should have borders, header, separator, data rows
        assert!(lines.len() >= 5);
        assert!(result.contains("Alice"));
        assert!(result.contains("Bob"));
    }

    #[test]
    fn function_table_with_header_style() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set tbl = table([{"width": 10}], header=["Name"], header_style="title") %}{{ tbl.header_row() }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert!(result.contains("[title]"));
        assert!(result.contains("[/title]"));
    }

    #[test]
    fn function_tabular_with_anchor() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 5}, {"width": 5, "anchor": "right"}], separator=" ", width=30) %}{{ fmt.row(["L", "R"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Total width 30, left col at start, right col at end
        assert_eq!(display_width(&result), 30);
        assert!(result.starts_with("L    "));
        assert!(result.ends_with("R    "));
    }

    #[test]
    fn function_tabular_overflow_clip() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 5, "overflow": "clip"}]) %}{{ fmt.row(["Hello World"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Clip truncates without ellipsis
        assert_eq!(result, "Hello");
        assert!(!result.contains("…"));
    }

    #[test]
    fn function_tabular_overflow_wrap() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 8, "overflow": "wrap"}]) %}{{ fmt.row(["This wraps"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Content fits in 8 chars with wrap mode
        assert_eq!(display_width(&result), 8);
    }

    #[test]
    fn function_tabular_overflow_truncate_middle() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 10, "overflow": "truncate_middle"}]) %}{{ fmt.row(["abcdefghijklmno"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert_eq!(display_width(&result), 10);
        assert!(result.contains("…"));
        // Middle truncate keeps start and end
        assert!(result.starts_with("abcd"));
    }

    #[test]
    fn function_tabular_overflow_object_truncate() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 10, "overflow": {"truncate": {"at": "start", "marker": "..."}}}]) %}{{ fmt.row(["Hello World"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert!(result.starts_with("..."));
        assert_eq!(display_width(&result), 10);
    }

    #[test]
    fn function_tabular_overflow_object_wrap() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 10, "overflow": {"wrap": {"indent": 2}}}]) %}{{ fmt.row(["Short"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Content fits, so just padded
        assert_eq!(display_width(&result), 10);
    }

    #[test]
    fn function_tabular_width_min_only() {
        let mut env = setup_env();
        // Two columns: fixed + min-only bounded
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": 10}, {"width": {"min": 15}}], separator="  ", width=50) %}{{ fmt.row(["A", "B"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Total 50, first col 10, sep 2, bounded gets rest (38) which is >= min 15
        assert_eq!(display_width(&result), 50);
    }

    #[test]
    fn function_tabular_width_max_only() {
        let mut env = setup_env();
        // Test that max-only bounded column works (capped by max when competing with fill)
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": {"max": 10}}, {"width": "fill"}], separator="  ", width=50) %}{{ fmt.row(["Hello World Test", "B"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Total 50, max-only bounded capped at 10, fill takes rest
        assert_eq!(display_width(&result), 50);
    }

    #[test]
    fn function_tabular_width_min_max() {
        let mut env = setup_env();
        // Bounded column with both min and max, competing with fill
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": {"min": 10, "max": 20}}, {"width": "fill"}], separator="  ", width=50) %}{{ fmt.row(["Hello", "World"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Total 50, bounded fits content "Hello" (5) but uses min 10, fill takes rest
        assert_eq!(display_width(&result), 50);
    }

    #[test]
    fn function_tabular_width_fraction_string() {
        let mut env = setup_env();
        // Two fraction columns: 2fr and 1fr (2:1 ratio)
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": "2fr"}, {"width": "1fr"}], separator="  ", width=35) %}{{ fmt.widths }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Total 35, sep 2, content 33, ratio 2:1 -> [22, 11]
        assert!(result.contains("22"));
        assert!(result.contains("11"));
    }

    #[test]
    fn function_tabular_width_fraction_object() {
        let mut env = setup_env();
        // Fraction via object syntax
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": {"fraction": 3}}, {"width": {"fraction": 1}}], separator="  ", width=42) %}{{ fmt.widths }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        // Total 42, sep 2, content 40, ratio 3:1 -> [30, 10]
        assert!(result.contains("30"));
        assert!(result.contains("10"));
    }

    #[test]
    fn function_table_row_from() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set tbl = table([{"width": 10, "key": "name"}, {"width": 8, "key": "status"}], separator="  ") %}{{ tbl.row_from(item) }}"#,
        )
        .unwrap();

        #[derive(Serialize)]
        struct TestItem {
            name: &'static str,
            status: &'static str,
        }

        let item = TestItem {
            name: "Alice",
            status: "active",
        };

        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(item => item))
            .unwrap();
        assert!(result.contains("Alice"));
        assert!(result.contains("active"));
    }

    #[test]
    fn function_table_with_row_separator() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set tbl = table([{"width": 10}, {"width": 8}], border="light", row_separator=true) %}{{ tbl.render_all([["A", "1"], ["B", "2"]]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();

        let lines: Vec<&str> = result.lines().collect();
        // Should have: top, row A, separator, row B, bottom
        // Count separator lines (├...┤)
        let sep_count = lines.iter().filter(|l| l.starts_with('├')).count();
        assert!(sep_count >= 1, "Expected at least 1 separator between rows");
    }
}
