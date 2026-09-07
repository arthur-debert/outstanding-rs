use minijinja::value::ValueKind;
use minijinja::{Environment, Value};

use crate::tabular::decorator::{BorderStyle, Table};
use crate::tabular::formatter::TabularFormatter;
use crate::tabular::traits::Tabular;
use crate::tabular::types::{Column, TabularSpec};
use crate::template::presentation::parse_markup;

use super::columns::{parse_columns, validate_style};
use super::stringify;
use crate::width::RenderWidthSource;

const DEFAULT_TABULAR_WIDTH: usize = 80;

fn resolve_tabular_width(
    explicit_width: Option<usize>,
    render_widths: &RenderWidthSource,
) -> usize {
    explicit_width
        .or_else(|| render_widths.terminal_width())
        .unwrap_or(DEFAULT_TABULAR_WIDTH)
}

pub(super) fn register_table_functions(env: &mut Environment<'static>, widths: RenderWidthSource) {
    let tabular_widths = widths.clone();
    env.add_function(
        "tabular",
        move |columns: Value,
              kwargs: minijinja::value::Kwargs|
              -> Result<Value, minijinja::Error> {
            let cols = parse_columns(&columns)?;
            let separator = kwargs
                .get::<Option<String>>("separator")?
                .unwrap_or_default();
            let rows = kwargs.get::<Option<Value>>("rows")?;
            let width =
                resolve_tabular_width(kwargs.get::<Option<usize>>("width")?, &tabular_widths);
            kwargs.assert_all_used()?;

            let mut builder = TabularSpec::builder();
            for col in cols {
                builder = builder.column(col);
            }
            if !separator.is_empty() {
                builder = builder.separator(crate::template::presentation::escape_text(&separator));
            }

            let spec = builder.build();
            let policy = tabular_widths.ambiguous_width();
            let formatter = match rows {
                Some(rows) => {
                    let data = measurable_rows(&spec.columns, &rows, "tabular")?;
                    let resolved = spec.resolve_prepared_widths_from_data(width, &data, policy);
                    TabularFormatter::from_prepared_resolved(&spec, resolved, width, policy)
                }
                None => TabularFormatter::from_prepared_spec(&spec, width, policy),
            };
            Ok(Value::from_object(formatter))
        },
    );

    let table_widths = widths;
    env.add_function(
        "table",
        move |columns: Value, kwargs: minijinja::value::Kwargs| -> Result<Value, minijinja::Error> {
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
            let row_styles = kwargs.get::<Option<Value>>("row_styles")?;
            let rows = kwargs.get::<Option<Value>>("rows")?;
            let width =
                resolve_tabular_width(kwargs.get::<Option<usize>>("width")?, &table_widths);
            kwargs.assert_all_used()?;

            let mut builder = TabularSpec::builder();
            for col in cols {
                builder = builder.column(col);
            }
            if !separator.is_empty() {
                builder = builder.separator(crate::template::presentation::escape_text(&separator));
            }

            let spec = builder.build();
            let columns = spec.columns.clone();
            let mut table = Table::from_prepared_spec(
                spec,
                width,
                table_widths.ambiguous_width(),
            )
            .border(parse_border_style(&border));

            let mut headers: Option<Vec<String>> = None;
            if let Some(h) = header {
                let parsed: Vec<String> = array_items(&h)
                    .ok_or_else(|| {
                        minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            format!("header must be an array of strings, got {}", h.kind()),
                        )
                    })?
                    .iter()
                    .map(|v| stringify(v).into_owned())
                    .collect();
                headers = Some(parsed.clone());
                table = table.header_formatted(parsed.iter().map(|text| parse_markup(text)));
            }

            if let Some(rows) = rows {
                let mut data = measurable_rows(&columns, &rows, "table")?;
                if let Some(headers) = headers {
                    data.push(measurable_row(&columns, headers));
                }
                table = table.sized_to_data(&data);
            }

            if let Some(style) = header_style {
                validate_style(&style)?;
                table = table.header_style(style);
            }

            if row_separator {
                table = table.row_separator(true);
            }

            if let Some(rs) = row_styles {
                if rs.is_true() {
                    match rs.kind() {
                        minijinja::value::ValueKind::Bool => {
                            table = table.row_styles("table_row_even", "table_row_odd");
                        }
                        minijinja::value::ValueKind::String => {
                            let tint = rs.to_string();
                            let even = format!("table_row_even_{}", tint);
                            let odd = format!("table_row_odd_{}", tint);
                            validate_style(&even)?;
                            validate_style(&odd)?;
                            table = table.row_styles(even, odd);
                        }
                        _ => {
                            if let Ok(iter) = rs.try_iter() {
                                let names: Vec<String> = iter.map(|v| v.to_string()).collect();
                                if names.len() == 2 {
                                    validate_style(&names[0])?;
                                    validate_style(&names[1])?;
                                    table = table.row_styles(&names[0], &names[1]);
                                } else {
                                    return Err(minijinja::Error::new(
                                        minijinja::ErrorKind::InvalidOperation,
                                        "row_styles array must have exactly 2 elements: [even_style, odd_style]",
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            Ok(Value::from_object(table))
        },
    );
}

/// Rejects anything that is not an array of arrays, so a mapping or scalar
/// never measures as a one-cell row of its debug rendering.
fn measurable_rows(
    columns: &[Column],
    rows: &Value,
    function: &str,
) -> Result<Vec<Vec<String>>, minijinja::Error> {
    let rows = array_items(rows).ok_or_else(|| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "{function}() rows must be an array of row arrays, got {}",
                rows.kind()
            ),
        )
    })?;

    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let cells = array_items(&row).ok_or_else(|| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!(
                        "{function}() rows must be an array of row arrays, but row {index} is {}",
                        row.kind()
                    ),
                )
            })?;
            Ok(measurable_row(
                columns,
                cells.iter().map(|cell| stringify(cell).into_owned()),
            ))
        })
        .collect()
}

/// `None` for anything but an array; a string's characters are not cells.
fn array_items(value: &Value) -> Option<Vec<Value>> {
    match value.kind() {
        ValueKind::Seq | ValueKind::Iterable => value.try_iter().map(Iterator::collect).ok(),
        _ => None,
    }
}

/// An omitted cell measures as the column's `null_repr`; a `sub_columns`
/// column measures as empty, since its width is resolved per row.
fn measurable_row(columns: &[Column], cells: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut cells = cells.into_iter();
    columns
        .iter()
        .map(|column| {
            let cell = cells.next();
            if column.sub_columns.is_some() {
                String::new()
            } else {
                cell.unwrap_or_else(|| column.null_repr.clone())
            }
        })
        .collect()
}

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

pub fn formatter_from_type<T: Tabular>(width: usize) -> Value {
    formatter_from_type_with_ambiguous_width::<T>(width, crate::AmbiguousWidth::Narrow)
}

pub fn formatter_from_type_with_ambiguous_width<T: Tabular>(
    width: usize,
    policy: crate::AmbiguousWidth,
) -> Value {
    let formatter = TabularFormatter::from_type_with_ambiguous_width::<T>(width, policy);
    Value::from_object(formatter)
}

pub fn table_from_type<T: Tabular>(width: usize, border: BorderStyle, use_headers: bool) -> Value {
    table_from_type_with_ambiguous_width::<T>(
        width,
        border,
        use_headers,
        crate::AmbiguousWidth::Narrow,
    )
}

pub fn table_from_type_with_ambiguous_width<T: Tabular>(
    width: usize,
    border: BorderStyle,
    use_headers: bool,
    policy: crate::AmbiguousWidth,
) -> Value {
    let mut table = Table::from_type_with_ambiguous_width::<T>(width, policy).border(border);
    if use_headers {
        table = table.header_from_columns();
    }
    Value::from_object(table)
}

#[cfg(test)]
mod tests {

    use minijinja::context;
    use serde::Serialize;

    use crate::tabular::display_width;
    use crate::tabular::filters::test_data::{setup_env, Item};

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
        assert_eq!(display_width(&result), 20);
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
        let sep_count = lines.iter().filter(|l| l.starts_with('├')).count();
        assert!(sep_count >= 1, "Expected at least 1 separator between rows");
    }
}

#[cfg(test)]
mod measurement;
