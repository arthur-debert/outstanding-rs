use minijinja::Value;

use crate::tabular::types::{Align, Column, Overflow, SubColumn, SubColumns, TruncateAt, Width};

use super::stringify;

pub(super) fn validate_style(name: &str) -> Result<(), minijinja::Error> {
    if name.is_empty() || standout_bbparser::is_valid_tag_name(name) {
        return Ok(());
    }
    Err(minijinja::Error::new(
        minijinja::ErrorKind::InvalidOperation,
        format!("invalid style name: {name:?}"),
    ))
}

pub(super) fn parse_columns(columns: &Value) -> Result<Vec<Column>, minijinja::Error> {
    let columns = columns
        .get_attr("columns")
        .ok()
        .filter(|value| !value.is_undefined() && !value.is_none())
        .unwrap_or_else(|| columns.clone());

    let iter = columns.try_iter().map_err(|_| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "columns must be an array or a tabular spec",
        )
    })?;

    let mut result = Vec::new();
    for col_val in iter {
        let col = parse_column(&col_val)?;
        result.push(col);
    }
    Ok(result)
}

fn parse_column(value: &Value) -> Result<Column, minijinja::Error> {
    let width_val = value.get_attr("width").map_err(|_| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "column must have a 'width' attribute",
        )
    })?;

    let width = parse_width(&width_val)?;
    let mut col = Column::new(width);

    if let Ok(align_val) = value.get_attr("align") {
        if !align_val.is_none() && !align_val.is_undefined() {
            col = col.align(parse_align(&align_val.to_string()));
        }
    }

    if let Ok(truncate_val) = value.get_attr("truncate") {
        if !truncate_val.is_none() && !truncate_val.is_undefined() {
            col = col.truncate(parse_truncate(&truncate_val.to_string()));
        }
    }

    if let Ok(key_val) = value.get_attr("key") {
        if !key_val.is_none() && !key_val.is_undefined() {
            col = col.key(key_val.to_string());
        }
    }

    if let Ok(header_val) = value.get_attr("header") {
        if !header_val.is_none() && !header_val.is_undefined() {
            col = col.header(stringify(&header_val).into_owned());
        }
    }

    if let Ok(style_val) = value.get_attr("style") {
        if !style_val.is_none() && !style_val.is_undefined() {
            validate_style(&style_val.to_string())?;
            col = col.style(style_val.to_string());
        }
    }

    if let Ok(null_val) = value.get_attr("null_repr") {
        if !null_val.is_none() && !null_val.is_undefined() {
            col = col.null_repr(stringify(&null_val).into_owned());
        }
    }

    if let Ok(anchor_val) = value.get_attr("anchor") {
        if !anchor_val.is_none()
            && !anchor_val.is_undefined()
            && anchor_val.to_string().to_lowercase() == "right"
        {
            col = col.anchor_right();
        }
    }

    if let Ok(overflow_val) = value.get_attr("overflow") {
        if !overflow_val.is_none() && !overflow_val.is_undefined() {
            col = col.overflow(parse_overflow(&overflow_val)?);
        }
    }

    if let Ok(sub_val) = value.get_attr("sub_columns") {
        if !sub_val.is_none() && !sub_val.is_undefined() {
            col = col.sub_columns(parse_sub_columns(&sub_val)?);
        }
    }

    Ok(col)
}

fn parse_overflow(value: &Value) -> Result<Overflow, minijinja::Error> {
    if let Some(s) = value.as_str() {
        return Ok(match s.to_lowercase().as_str() {
            "wrap" => Overflow::wrap(),
            "clip" => Overflow::Clip,
            "expand" => Overflow::Expand,
            "truncate_start" => Overflow::truncate(TruncateAt::Start),
            "truncate_middle" => Overflow::truncate(TruncateAt::Middle),
            _ => Overflow::truncate(TruncateAt::End),
        });
    }

    if let Ok(truncate_obj) = value.get_attr("truncate") {
        if !truncate_obj.is_none() && !truncate_obj.is_undefined() {
            let at = if let Ok(at_val) = truncate_obj.get_attr("at") {
                parse_truncate(&at_val.to_string())
            } else {
                TruncateAt::End
            };
            let marker = if let Ok(marker_val) = truncate_obj.get_attr("marker") {
                if !marker_val.is_none() && !marker_val.is_undefined() {
                    stringify(&marker_val).into_owned()
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

    Ok(Overflow::default())
}

fn parse_sub_columns(value: &Value) -> Result<SubColumns, minijinja::Error> {
    let cols_val = value.get_attr("columns").map_err(|_| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "sub_columns must have a 'columns' attribute",
        )
    })?;

    let iter = cols_val.try_iter().map_err(|_| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "sub_columns.columns must be an array",
        )
    })?;

    let mut columns = Vec::new();
    for col_val in iter {
        columns.push(parse_sub_column(&col_val)?);
    }

    let separator = value
        .get_attr("separator")
        .ok()
        .filter(|v| !v.is_none() && !v.is_undefined())
        .map(|v| stringify(&v).into_owned())
        .unwrap_or_else(|| " ".to_string());

    SubColumns::new(columns, separator)
        .map_err(|e| minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e))
}

fn parse_sub_column(value: &Value) -> Result<SubColumn, minijinja::Error> {
    let width = if let Ok(width_val) = value.get_attr("width") {
        if !width_val.is_none() && !width_val.is_undefined() {
            parse_width(&width_val)?
        } else {
            Width::Fill
        }
    } else {
        Width::Fill
    };

    let mut sub_col = SubColumn::new(width);

    if let Ok(align_val) = value.get_attr("align") {
        if !align_val.is_none() && !align_val.is_undefined() {
            sub_col = sub_col.align(parse_align(&align_val.to_string()));
        }
    }

    if let Ok(overflow_val) = value.get_attr("overflow") {
        if !overflow_val.is_none() && !overflow_val.is_undefined() {
            sub_col = sub_col.overflow(parse_overflow(&overflow_val)?);
        }
    }

    if let Ok(style_val) = value.get_attr("style") {
        if !style_val.is_none() && !style_val.is_undefined() {
            validate_style(&style_val.to_string())?;
            sub_col = sub_col.style(style_val.to_string());
        }
    }

    if let Ok(null_val) = value.get_attr("null_repr") {
        if !null_val.is_none() && !null_val.is_undefined() {
            sub_col = sub_col.null_repr(stringify(&null_val).into_owned());
        }
    }

    Ok(sub_col)
}

fn parse_width(value: &Value) -> Result<Width, minijinja::Error> {
    if let Some(n) = value.as_i64() {
        return Ok(Width::Fixed(n as usize));
    }

    if let Some(s) = value.as_str() {
        if s == "fill" {
            return Ok(Width::Fill);
        }

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

fn parse_align(s: &str) -> Align {
    match s.to_lowercase().as_str() {
        "right" => Align::Right,
        "center" => Align::Center,
        _ => Align::Left,
    }
}

fn parse_truncate(s: &str) -> TruncateAt {
    match s.to_lowercase().as_str() {
        "start" => TruncateAt::Start,
        "middle" => TruncateAt::Middle,
        _ => TruncateAt::End,
    }
}

#[cfg(test)]
mod tests {

    use minijinja::context;

    use crate::tabular::display_width;
    use crate::tabular::filters::test_data::setup_env;

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
        assert_eq!(display_width(&result), 10);
    }

    #[test]
    fn function_tabular_width_min_only() {
        let mut env = setup_env();
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
        assert_eq!(display_width(&result), 50);
    }

    #[test]
    fn function_tabular_width_max_only() {
        let mut env = setup_env();
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
        assert_eq!(display_width(&result), 50);
    }

    #[test]
    fn function_tabular_width_min_max() {
        let mut env = setup_env();
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
        assert_eq!(display_width(&result), 50);
    }

    #[test]
    fn function_tabular_width_fraction_string() {
        let mut env = setup_env();
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
        assert!(result.contains("22"));
        assert!(result.contains("11"));
    }

    #[test]
    fn function_tabular_width_fraction_object() {
        let mut env = setup_env();
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
        assert!(result.contains("30"));
        assert!(result.contains("10"));
    }

    #[test]
    fn function_tabular_sub_columns_basic() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([
                {"width": 4},
                {"width": "fill", "sub_columns": {
                    "columns": [
                        {"width": "fill"},
                        {"width": {"min": 0, "max": 20}, "align": "right"}
                    ],
                    "separator": " "
                }},
                {"width": 4, "align": "right"}
            ], separator="  ", width=60) %}{{ fmt.row(["1.", ["Gallery Navigation", "[feature]"], "4d"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        let result = standout_bbparser::strip_tags(&result);
        assert!(result.contains("Gallery Navigation"));
        assert!(result.contains("[feature]"));
        assert!(result.contains("1."));
        assert!(result.contains("4d"));
        assert_eq!(display_width(&result), 60);
    }

    #[test]
    fn function_tabular_sub_columns_empty_tag() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([
                {"width": "fill", "sub_columns": {
                    "columns": [
                        {"width": "fill"},
                        {"width": {"min": 0, "max": 20}, "align": "right"}
                    ],
                    "separator": " "
                }}
            ], width=40) %}{{ fmt.row([["Title only", ""]]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert!(result.contains("Title only"));
        assert_eq!(display_width(&result), 40);
    }

    #[test]
    fn function_tabular_sub_columns_plain_string_fallback() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([
                {"width": "fill", "sub_columns": {
                    "columns": [{"width": "fill"}, {"width": {"min": 0, "max": 10}}],
                    "separator": " "
                }}
            ], width=30) %}{{ fmt.row(["just a string"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert_eq!(display_width(&result), 30);
    }

    #[test]
    fn function_tabular_sub_columns_with_style() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([
                {"width": "fill", "sub_columns": {
                    "columns": [
                        {"width": "fill"},
                        {"width": {"min": 0, "max": 20}, "align": "right", "style": "tag"}
                    ],
                    "separator": " "
                }}
            ], width=40) %}{{ fmt.row([["Title", "feature"]]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        assert!(result.contains("[tag]"));
        assert!(result.contains("feature"));
        assert!(result.contains("[/tag]"));
    }

    #[test]
    fn function_table_sub_columns_with_border() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set tbl = table([
                {"width": 4},
                {"width": "fill", "sub_columns": {
                    "columns": [
                        {"width": "fill"},
                        {"width": {"min": 0, "max": 15}, "align": "right"}
                    ],
                    "separator": " "
                }},
                {"width": 4}
            ], border="light", separator="  ", width=50) %}{{ tbl.row(["1.", ["My Title", "[bug]"], "2d"]) }}"#,
        )
        .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap();
        let result = standout_bbparser::strip_tags(&result);
        assert!(result.starts_with('│'));
        assert!(result.ends_with('│'));
        assert!(result.contains("My Title"));
        assert!(result.contains("[bug]"));
    }
}
