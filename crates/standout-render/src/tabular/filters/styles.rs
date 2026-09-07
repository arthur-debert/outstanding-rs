use super::*;
use minijinja::context;

use crate::tabular::filters::test_data::setup_env;
use standout_bbparser::{TagTransform, UnknownTagBehavior};
use std::collections::HashMap;

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
fn filter_style_as_keeps_a_bracketed_value_literal_inside_its_style() {
    let mut env = setup_env();
    env.add_template("test", "{{ value | style_as('error') }}")
        .unwrap();
    let result = env
        .get_template("test")
        .unwrap()
        .render(context!(value => "missing [severity_map] table"))
        .unwrap();
    assert_eq!(result, r"[error]missing \[severity_map\] table[/error]");

    let styles = HashMap::from([("error".to_string(), console::Style::new().bold())]);
    let _window = crate::diagnostics::begin_capture();
    assert_eq!(
        crate::diagnostics::resolve_tags(
            &result,
            styles,
            TagTransform::Remove,
            UnknownTagBehavior::Strip,
        ),
        "missing [severity_map] table"
    );
    assert!(crate::diagnostics::unresolved_in_current_window().is_empty());
}

#[test]
fn filter_style_as_keeps_ansi_in_its_value_zero_width_and_unsplit() {
    let mut env = setup_env();
    env.add_template("measure", "{{ value | style_as('row') | display_width }}")
        .unwrap();
    env.add_template(
        "truncate",
        "{{ value | style_as('row') | truncate_at(8, 'end', '') }}",
    )
    .unwrap();
    let value = Value::from(crate::FormattedText::from_ansi_sgr(
        "\u{1b}[31m[boom] alpha\u{1b}[0m",
    ));

    let width = env
        .get_template("measure")
        .unwrap()
        .render(context!(value))
        .unwrap();
    assert_eq!(width, "12", "the ANSI sequences carry no visible width");

    let truncated = env
        .get_template("truncate")
        .unwrap()
        .render(context!(value))
        .unwrap();
    assert!(
        crate::FormattedText::from_ansi_sgr(&truncated)
            .nodes()
            .iter()
            .any(|node| {
                matches!(node, standout_types::PresentationNode::Styled {
                    style: standout_types::PresentationStyle::Sgr(style), ..
                } if style.foreground == Some(standout_types::SgrColor::Indexed(1)))
            }),
        "truncation preserves the imported red style: {truncated:?}"
    );
    let sequences_are_whole = |text: &str| {
        standout_bbparser::ansi::ansi_units(text)
            .all(|unit| !unit.is_escape || unit.text.ends_with('m'))
    };
    assert!(
        sequences_are_whole(&truncated),
        "truncation split a sequence: {truncated:?}"
    );

    let resolved = crate::diagnostics::resolve_tags(
        &truncated,
        HashMap::new(),
        TagTransform::Remove,
        UnknownTagBehavior::Strip,
    );
    assert_eq!(
        console::strip_ansi_codes(&resolved),
        "[boom] a",
        "an unknown outer style is stripped, leaving the value's own ANSI"
    );
    assert!(
        sequences_are_whole(&resolved),
        "tag resolution split a sequence: {resolved:?}"
    );

    env.add_template("plain", "{{ value | style_as('row') }}")
        .unwrap();
    let ansi_only = env
        .get_template("plain")
        .unwrap()
        .render(context!(value => "\u{1b}[31malpha\u{1b}[0m"))
        .unwrap();
    assert_eq!(
        standout_bbparser::strip_tags(&ansi_only),
        r"\u{1b}[31malpha\u{1b}[0m"
    );
}

#[test]
fn filter_style_as_rejects_a_name_that_would_invent_a_tag() {
    let mut env = setup_env();
    env.add_template("dynamic", "{{ value | style_as(name) }}")
        .unwrap();

    for name in [
        "[error]",
        "error]",
        "a/b",
        "/error",
        "Error",
        "1st",
        "two words",
        "name.with.dot",
        "err\u{7}or",
        "-error",
    ] {
        let error = env
            .get_template("dynamic")
            .unwrap()
            .render(context!(value => "text", name => name))
            .unwrap_err()
            .to_string();
        assert!(error.contains("style_as"), "{error}");
        assert!(error.contains(name), "{error}");
    }

    for name in ["error", "my-style2", "_private"] {
        let rendered = env
            .get_template("dynamic")
            .unwrap()
            .render(context!(value => "text", name => name))
            .unwrap();
        assert_eq!(rendered, format!("[{name}]text[/{name}]"));
    }
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
            .render(context!(value => Value::from(crate::FormattedText::text("+32").styled("additions").unwrap().append("/").append(crate::FormattedText::text("-0").styled("deletions").unwrap()).append("/32"))))
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
            .render(context!(value => Value::from(crate::FormattedText::text("hi").styled("bold").unwrap())))
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
            .render(context!(value => Value::from(crate::FormattedText::text("hi").styled("bold").unwrap())))
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
            .render(context!(value => Value::from(crate::FormattedText::text("hello world").styled("bold").unwrap())))
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
            .render(context!(value => Value::from(crate::FormattedText::text("日本語").styled("match").unwrap())))
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
            .render(context!(value => Value::from(crate::FormattedText::text("hello").styled("bold").unwrap())))
            .unwrap();
    assert_eq!(result, "[bold]hello[/bold]");
}

#[test]
fn filter_display_width_bbcode() {
    let mut env = setup_env();
    env.add_template("test", "{{ value | display_width }}")
        .unwrap();
    let result = env
            .get_template("test")
            .unwrap()
            .render(context!(value => Value::from(crate::FormattedText::text("hello").styled("bold").unwrap())))
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
            .render(context!(value => Value::from(crate::FormattedText::text("hi").styled("bold").unwrap())))
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
            .render(context!(value => Value::from(crate::FormattedText::text("hi").styled("bold").unwrap())))
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
            .render(context!(value => Value::from(crate::FormattedText::text("hi").styled("bold").unwrap())))
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
            .render(context!(value => Value::from(crate::FormattedText::text("hello world").styled("bold").unwrap())))
            .unwrap();
    assert_eq!(
        visible_width_with_policy(&result, crate::AmbiguousWidth::Narrow),
        8
    );
    assert_eq!(result, "[bold]hello w[/bold]…");
}
