use minijinja::{Environment, Value};

use super::util::{
    truncate_visible_end_with_policy, truncate_visible_middle_with_policy,
    truncate_visible_start_with_policy, visible_width_with_policy,
};
use crate::template::presentation::{fragment, markup};
fn stringify(value: &minijinja::Value) -> std::borrow::Cow<'_, str> {
    std::borrow::Cow::Owned(markup(value))
}
use crate::width::RenderWidthSource;

mod columns;
mod tables;
use tables::register_table_functions;
pub use tables::{
    formatter_from_type, formatter_from_type_with_ambiguous_width, table_from_type,
    table_from_type_with_ambiguous_width,
};

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
              -> Result<Value, minijinja::Error> {
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
            let ellipsis = crate::template::presentation::escape_text(&ellipsis);

            Ok(fragment(format_col_with_policy(
                &text,
                width,
                &align,
                &truncate,
                &ellipsis,
                col_policy.ambiguous_width(),
            )))
        },
    );

    let pad_left_policy = widths.clone();
    env.add_filter("pad_left", move |value: Value, width: usize| -> Value {
        let text = stringify(&value).into_owned();
        let visible_width = visible_width_with_policy(&text, pad_left_policy.ambiguous_width());
        if visible_width >= width {
            fragment(text)
        } else {
            fragment(format!("{}{}", " ".repeat(width - visible_width), text))
        }
    });

    let pad_right_policy = widths.clone();
    env.add_filter("pad_right", move |value: Value, width: usize| -> Value {
        let text = stringify(&value).into_owned();
        let visible_width = visible_width_with_policy(&text, pad_right_policy.ambiguous_width());
        if visible_width >= width {
            fragment(text)
        } else {
            fragment(format!("{}{}", text, " ".repeat(width - visible_width)))
        }
    });

    let pad_center_policy = widths.clone();
    env.add_filter("pad_center", move |value: Value, width: usize| -> Value {
        let text = stringify(&value).into_owned();
        let visible_width = visible_width_with_policy(&text, pad_center_policy.ambiguous_width());
        if visible_width >= width {
            fragment(text)
        } else {
            let padding = width - visible_width;
            let left_pad = padding / 2;
            let right_pad = padding - left_pad;
            fragment(format!(
                "{}{}{}",
                " ".repeat(left_pad),
                text,
                " ".repeat(right_pad)
            ))
        }
    });

    let truncate_policy = widths.clone();
    env.add_filter(
        "truncate_at",
        move |value: Value,
              width: usize,
              position: Option<String>,
              ellipsis: Option<String>|
              -> Value {
            let text = stringify(&value).into_owned();
            let pos = position.as_deref().unwrap_or("end");
            let ellipsis =
                crate::template::presentation::escape_text(ellipsis.as_deref().unwrap_or("…"));
            let ell = &ellipsis;

            fragment(match pos {
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
            })
        },
    );

    let display_policy = widths.clone();
    env.add_filter("display_width", move |value: Value| -> usize {
        visible_width_with_policy(&stringify(&value), display_policy.ambiguous_width())
    });

    env.add_filter(
        "style_as",
        |value: Value, style: String| -> Result<Value, minijinja::Error> {
            let text = stringify(&value);
            if style.is_empty() {
                return Ok(fragment(text.into_owned()));
            }
            if !standout_bbparser::is_valid_tag_name(&style) {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!(
                        "style_as: `{style}` cannot name a style; a style name is \
                         lowercase ASCII letters, digits, `_` and `-`, starting with \
                         a letter or `_`"
                    ),
                ));
            }
            Ok(fragment(format!("[{}]{}[/{}]", style, text, style)))
        },
    );

    register_table_functions(env, widths);
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

    use minijinja::context;

    use crate::tabular::display_width;
    use crate::tabular::filters::test_data::{setup_env, Item};

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
}

#[cfg(test)]
mod styles;

#[cfg(test)]
mod test_data {
    use super::*;
    use serde::Serialize;
    pub(super) fn setup_env() -> Environment<'static> {
        let mut env = crate::template::new_environment();
        register_tabular_filters(&mut env);
        env
    }

    #[derive(Serialize)]
    pub(super) struct Item {
        pub(super) name: &'static str,
        pub(super) value: &'static str,
    }
}
