//! MiniJinja filters (`col`, `pad_left`, `pad_right`, `pad_center`,
//! `truncate_at`, `display_width`, `style_as`) and global functions
//! (`tabular(columns, ...)`, `table(columns, ...)`) for columnar output.
//!
//! Both functions take `rows` — the row arrays about to be rendered — and
//! size every `{min, max}` column to the widest cell they hold, the
//! whole-table measurement a formatter that sees one row at a time cannot do
//! on its own; `table()` measures its `header` alongside them.
//!
//! Columns are defined as dicts with `width` (a number, `"fill"`, `"Nfr"`,
//! or `{min, max}`/`{fraction}`), `align`, `truncate`, `key`, `header`,
//! `style`, `null_repr`, `anchor`, `overflow`, and `sub_columns`. BBCode-style
//! style tags (`[bold]...[/bold]`) are treated as zero-width throughout: width
//! measurement excludes tag bytes, and tags stay balanced after padding or
//! truncation.

use minijinja::{Environment, Value};

use super::decorator::{BorderStyle, Table};
use super::formatter::TabularFormatter;
use super::traits::Tabular;
use super::types::{
    Align, Column, Overflow, SubColumn, SubColumns, TabularSpec, TruncateAt, Width,
};
#[cfg(test)]
use super::util::display_width;
use super::util::{
    truncate_visible_end_with_policy, truncate_visible_middle_with_policy,
    truncate_visible_start_with_policy, visible_width_with_policy,
};
use crate::template::stringify;
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

pub fn register_tabular_filters(env: &mut Environment<'static>) {
    register_tabular_filters_with_policy(env, crate::AmbiguousWidth::Narrow);
}

pub fn register_tabular_filters_with_policy(
    env: &mut Environment<'static>,
    policy: crate::AmbiguousWidth,
) {
    register_tabular_filters_with_source(env, RenderWidthSource::new(policy));
}

pub(crate) fn register_tabular_filters_with_source(
    env: &mut Environment<'static>,
    widths: RenderWidthSource,
) {
    let col_policy = widths.clone();
    env.add_filter(
        "col",
        move |value: Value,
              width_val: Value,
              kwargs: minijinja::value::Kwargs|
              -> Result<String, minijinja::Error> {
            let text = stringify(&value).into_owned();

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

            Ok(format_col_with_policy(
                &text,
                width,
                &align,
                &truncate,
                &ellipsis,
                col_policy.ambiguous_width(),
            ))
        },
    );

    let pad_left_policy = widths.clone();
    env.add_filter("pad_left", move |value: Value, width: usize| -> String {
        let text = stringify(&value).into_owned();
        let visible_width = visible_width_with_policy(&text, pad_left_policy.ambiguous_width());
        if visible_width >= width {
            text
        } else {
            format!("{}{}", " ".repeat(width - visible_width), text)
        }
    });

    let pad_right_policy = widths.clone();
    env.add_filter("pad_right", move |value: Value, width: usize| -> String {
        let text = stringify(&value).into_owned();
        let visible_width = visible_width_with_policy(&text, pad_right_policy.ambiguous_width());
        if visible_width >= width {
            text
        } else {
            format!("{}{}", text, " ".repeat(width - visible_width))
        }
    });

    let pad_center_policy = widths.clone();
    env.add_filter("pad_center", move |value: Value, width: usize| -> String {
        let text = stringify(&value).into_owned();
        let visible_width = visible_width_with_policy(&text, pad_center_policy.ambiguous_width());
        if visible_width >= width {
            text
        } else {
            let padding = width - visible_width;
            let left_pad = padding / 2;
            let right_pad = padding - left_pad;
            format!("{}{}{}", " ".repeat(left_pad), text, " ".repeat(right_pad))
        }
    });

    let truncate_policy = widths.clone();
    env.add_filter(
        "truncate_at",
        move |value: Value,
              width: usize,
              position: Option<String>,
              ellipsis: Option<String>|
              -> String {
            let text = stringify(&value).into_owned();
            let pos = position.as_deref().unwrap_or("end");
            let ell = ellipsis.as_deref().unwrap_or("…");

            match pos {
                "start" => truncate_visible_start_with_policy(
                    &text,
                    width,
                    ell,
                    truncate_policy.ambiguous_width(),
                ),
                "middle" => truncate_visible_middle_with_policy(
                    &text,
                    width,
                    ell,
                    truncate_policy.ambiguous_width(),
                ),
                _ => truncate_visible_end_with_policy(
                    &text,
                    width,
                    ell,
                    truncate_policy.ambiguous_width(),
                ),
            }
        },
    );

    let display_policy = widths.clone();
    env.add_filter("display_width", move |value: Value| -> usize {
        visible_width_with_policy(&stringify(&value), display_policy.ambiguous_width())
    });

    env.add_filter("style_as", |value: Value, style: String| -> String {
        let text = stringify(&value).into_owned();
        if style.is_empty() {
            text
        } else {
            format!("[{}]{}[/{}]", style, text, style)
        }
    });

    register_table_functions(env, widths);
}

fn register_table_functions(env: &mut Environment<'static>, widths: RenderWidthSource) {
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
                builder = builder.separator(&separator);
            }

            let spec = builder.build();
            let policy = tabular_widths.ambiguous_width();
            let formatter = match rows {
                Some(rows) => {
                    let data = measurable_rows(&spec.columns, &rows, "tabular")?;
                    let resolved = spec.resolve_widths_from_data_with_policy(width, &data, policy);
                    TabularFormatter::from_resolved_with_width_and_policy(
                        &spec, resolved, width, policy,
                    )
                }
                None => TabularFormatter::with_ambiguous_width(&spec, width, policy),
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
                builder = builder.separator(&separator);
            }

            let spec = builder.build();
            let columns = spec.columns.clone();
            let mut table = Table::with_ambiguous_width(
                spec,
                width,
                table_widths.ambiguous_width(),
            )
            .border(parse_border_style(&border));

            let mut headers: Option<Vec<String>> = None;
            if let Some(h) = header {
                let parsed: Vec<String> = h
                    .try_iter()
                    .map_err(|_| {
                        minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            "header must be an array of strings",
                        )
                    })?
                    .map(|v| stringify(&v).into_owned())
                    .collect();
                headers = Some(parsed.clone());
                table = table.header(parsed);
            }

            if let Some(rows) = rows {
                let mut data = measurable_rows(&columns, &rows, "table")?;
                if let Some(headers) = headers {
                    data.push(measurable_row(&columns, headers));
                }
                table = table.sized_to_data(&data);
            }

            if let Some(style) = header_style {
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
                            table = table.row_styles(even, odd);
                        }
                        _ => {
                            if let Ok(iter) = rs.try_iter() {
                                let names: Vec<String> = iter.map(|v| v.to_string()).collect();
                                if names.len() == 2 {
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

/// Flattens the `rows=` argument into the cell strings the width resolver
/// measures. A column carrying `sub_columns` measures as empty: its
/// sub-columns are resolved per row against the parent's width, so the whole
/// table has no single content width to grow it to.
fn measurable_rows(
    columns: &[Column],
    rows: &Value,
    function: &str,
) -> Result<Vec<Vec<String>>, minijinja::Error> {
    let iter = rows.try_iter().map_err(|_| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("{}() rows must be an array of row arrays", function),
        )
    })?;

    Ok(iter
        .map(|row| {
            let cells: Vec<String> = match row.try_iter() {
                Ok(cells) => cells.map(|cell| stringify(&cell).into_owned()).collect(),
                Err(_) => vec![stringify(&row).into_owned()],
            };
            measurable_row(columns, cells)
        })
        .collect())
}

fn measurable_row(columns: &[Column], cells: Vec<String>) -> Vec<String> {
    cells
        .into_iter()
        .enumerate()
        .map(|(index, cell)| {
            if columns
                .get(index)
                .and_then(|column| column.sub_columns.as_ref())
                .is_some()
            {
                String::new()
            } else {
                cell
            }
        })
        .collect()
}

fn parse_columns(columns: &Value) -> Result<Vec<Column>, minijinja::Error> {
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

fn format_col_with_policy(
    text: &str,
    width: usize,
    align: &str,
    truncate: &str,
    ellipsis: &str,
    policy: crate::AmbiguousWidth,
) -> String {
    if width == 0 {
        return String::new();
    }

    let visible_width = visible_width_with_policy(text, policy);

    if visible_width > width {
        let truncated = match truncate {
            "start" => truncate_visible_start_with_policy(text, width, ellipsis, policy),
            "middle" => truncate_visible_middle_with_policy(text, width, ellipsis, policy),
            _ => truncate_visible_end_with_policy(text, width, ellipsis, policy),
        };
        pad_col_visible(&truncated, width, align, policy)
    } else {
        pad_col_visible(text, width, align, policy)
    }
}

fn pad_col_visible(text: &str, width: usize, align: &str, policy: crate::AmbiguousWidth) -> String {
    let padding = width.saturating_sub(visible_width_with_policy(text, policy));
    match align {
        "right" => format!("{}{}", " ".repeat(padding), text),
        "center" => {
            let left = padding / 2;
            format!("{}{}{}", " ".repeat(left), text, " ".repeat(padding - left))
        }
        _ => format!("{}{}", text, " ".repeat(padding)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;
    use serde::Serialize;

    fn setup_env() -> Environment<'static> {
        let mut env = crate::template::new_environment();
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

    #[test]
    fn filter_col_bbcode_no_truncation() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(16, align='center') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[additions]+32[/additions]/[deletions]-0[/deletions]/32"))
            .unwrap();
        assert!(result.contains("+32"));
        assert!(result.contains("-0"));
        assert!(result.contains("[additions]"));
        assert!(result.contains("[/deletions]"));
        assert_eq!(
            visible_width_with_policy(&result, crate::AmbiguousWidth::Narrow),
            16
        );
    }

    #[test]
    fn filter_col_bbcode_padding_left_align() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10) }}").unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[bold]hi[/bold]"))
            .unwrap();
        assert!(result.contains("[bold]hi[/bold]"));
        assert_eq!(result, "[bold]hi[/bold]        ");
        assert_eq!(
            visible_width_with_policy(&result, crate::AmbiguousWidth::Narrow),
            10
        );
    }

    #[test]
    fn filter_col_bbcode_padding_right_align() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(10, align='right') }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[bold]hi[/bold]"))
            .unwrap();
        assert!(result.starts_with("        "));
        assert!(result.contains("[bold]hi[/bold]"));
        assert_eq!(result, "        [bold]hi[/bold]");
    }

    #[test]
    fn filter_col_bbcode_truncation() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(5) }}").unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[bold]hello world[/bold]"))
            .unwrap();
        assert_eq!(
            visible_width_with_policy(&result, crate::AmbiguousWidth::Narrow),
            5
        );
        assert_eq!(result, "[bold]hell[/bold]…");
    }

    #[test]
    fn filter_col_pads_after_wide_styled_truncation() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(4) }}").unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[match]日本語[/match]"))
            .unwrap();

        assert_eq!(result, "[match]日[/match]… ");
        assert_eq!(
            visible_width_with_policy(&result, crate::AmbiguousWidth::Narrow),
            4
        );
    }

    #[test]
    fn filter_col_bbcode_exact_fit() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | col(5) }}").unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[bold]hello[/bold]"))
            .unwrap();
        assert_eq!(result, "[bold]hello[/bold]");
    }

    #[test]
    fn filter_col_no_tags_unchanged() {
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
    fn filter_display_width_bbcode() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | display_width }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[bold]hello[/bold]"))
            .unwrap();
        assert_eq!(result, "5");
    }

    #[test]
    fn filter_pad_left_bbcode() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | pad_left(8) }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[bold]hi[/bold]"))
            .unwrap();
        assert!(result.starts_with("      "));
        assert!(result.contains("[bold]hi[/bold]"));
    }

    #[test]
    fn filter_pad_right_bbcode() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | pad_right(8) }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[bold]hi[/bold]"))
            .unwrap();
        assert!(result.contains("[bold]hi[/bold]"));
        assert_eq!(
            visible_width_with_policy(&result, crate::AmbiguousWidth::Narrow),
            8
        );
    }

    #[test]
    fn filter_pad_center_bbcode() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | pad_center(8) }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[bold]hi[/bold]"))
            .unwrap();
        assert!(result.contains("[bold]hi[/bold]"));
        assert_eq!(
            visible_width_with_policy(&result, crate::AmbiguousWidth::Narrow),
            8
        );
    }

    #[test]
    fn filter_truncate_at_bbcode() {
        let mut env = setup_env();
        env.add_template("test", "{{ value | truncate_at(8) }}")
            .unwrap();
        let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => "[bold]hello world[/bold]"))
            .unwrap();
        assert_eq!(
            visible_width_with_policy(&result, crate::AmbiguousWidth::Narrow),
            8
        );
        assert_eq!(result, "[bold]hello w[/bold]…");
    }

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

    const PR_ROWS: &[[&str; 3]] = &[
        ["#12", "open", "Add pagination"],
        ["#7", "merged", "Fix retry"],
    ];

    fn render_with_rows(template: &str) -> String {
        let mut env = setup_env();
        env.add_template("test", template).unwrap();
        env.get_template("test")
            .unwrap()
            .render(context!(rows => PR_ROWS))
            .unwrap()
    }

    #[test]
    fn function_tabular_without_rows_leaves_bounded_columns_unmeasured() {
        let widths = render_with_rows(
            r#"{% set fmt = tabular([{"width": {"min": 0}}, {"width": {"min": 0}}, {"width": {"min": 0}}], separator="  ", width=60) %}{{ fmt.widths }}"#,
        );
        assert_eq!(widths, "[0, 0, 56]");
    }

    #[test]
    fn function_tabular_rows_sizes_bounded_columns_to_the_widest_cell() {
        let widths = render_with_rows(
            r#"{% set fmt = tabular([{"width": {"min": 0}}, {"width": {"min": 0}}, {"width": {"min": 0}}], separator="  ", width=60, rows=rows) %}{{ fmt.widths }}"#,
        );
        assert_eq!(widths, "[3, 6, 47]");
    }

    #[test]
    fn function_tabular_rows_aligns_every_row_to_the_measured_columns() {
        let rendered = render_with_rows(
            r#"{% set fmt = tabular([{"width": {"min": 0}}, {"width": {"min": 0}}, {"width": {"min": 0}}], separator="  ", width=60, rows=rows) %}{% for row in rows %}{{ fmt.row(row) }}
{% endfor %}"#,
        );
        let lines: Vec<&str> = rendered.lines().map(|line| line.trim_end()).collect();
        assert_eq!(
            lines,
            vec!["#12  open    Add pagination", "#7   merged  Fix retry"]
        );
    }

    #[test]
    fn function_tabular_rows_respects_a_max_bound() {
        let widths = render_with_rows(
            r#"{% set fmt = tabular([{"width": {"min": 0, "max": 2}}, {"width": {"min": 0}}, {"width": "fill"}], separator="  ", width=60, rows=rows) %}{{ fmt.widths }}"#,
        );
        assert_eq!(widths, "[2, 6, 48]");
    }

    #[test]
    fn function_tabular_rows_leaves_a_sub_column_parent_unmeasured() {
        let widths = render_with_rows(
            r#"{% set fmt = tabular([{"width": {"min": 4}}, {"width": {"min": 0}, "sub_columns": {"columns": [{"width": "fill"}, {"width": 6}]}}], separator="  ", width=40, rows=rows) %}{{ fmt.widths }}"#,
        );
        assert_eq!(widths, "[4, 34]");
    }

    #[test]
    fn function_table_rows_measures_the_header_too() {
        let rendered = render_with_rows(
            r#"{% set t = table([{"width": {"min": 0}}, {"width": {"min": 0}}, {"width": "fill"}], separator="  ", width=60, header=["NUMBER", "STATE", "TITLE"], rows=rows) %}{{ t.header_row() }}
{% for row in rows %}{{ t.row(row) }}
{% endfor %}"#,
        );
        let lines: Vec<&str> = rendered.lines().map(|line| line.trim_end()).collect();
        assert_eq!(
            lines,
            vec![
                "NUMBER  STATE   TITLE",
                "#12     open    Add pagination",
                "#7      merged  Fix retry",
            ]
        );
    }

    #[test]
    fn function_tabular_rows_rejects_a_scalar() {
        let mut env = setup_env();
        env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": {"min": 0}}], rows=12) %}{{ fmt.widths }}"#,
        )
        .unwrap();
        let error = env
            .get_template("test")
            .unwrap()
            .render(context!())
            .unwrap_err();
        assert!(
            error.to_string().contains("rows must be an array"),
            "{error}"
        );
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
        assert!(result.starts_with('│'));
        assert!(result.ends_with('│'));
        assert!(result.contains("My Title"));
        assert!(result.contains("[bug]"));
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
