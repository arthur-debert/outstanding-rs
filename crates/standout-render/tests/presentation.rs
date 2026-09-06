use minijinja::{context, Value};
use serde::Serialize;
use standout_render::{
    render_with_output, validate_template, ColorPolicy, FormattedText, Representation, Theme,
};

fn theme() -> Theme {
    Theme::new().add("heading", console::Style::new().bold())
}

#[derive(Serialize)]
struct Data<T> {
    value: T,
}

fn output(template: &str, value: impl Serialize, colors: ColorPolicy) -> String {
    render_with_output(
        template,
        &Data { value },
        &theme(),
        Representation::Human,
        colors,
    )
    .unwrap()
}

#[test]
fn ordinary_interpolation_preserves_brackets_and_escapes_terminal_controls() {
    let value = "[draft].txt[/heading]\\\x1b[2J\x1b]52;c;payload\x07\x1bPquery\x1b\\\r\x00\x7f\u{9b}31m\x1b[\n\t";
    let expected = "[draft].txt[/heading]\\\\u{1b}[2J\\u{1b}]52;c;payload\\u{7}\\u{1b}Pquery\\u{1b}\\\\u{d}\\u{0}\\u{7f}\\u{9b}31m\\u{1b}[\n\t";
    assert_eq!(
        output(
            "[heading]before[/heading]{{ value }}tail",
            value,
            ColorPolicy::Never
        ),
        format!("before{expected}tail")
    );
    let colored = output(
        "[heading]before[/heading]{{ value }}tail",
        value,
        ColorPolicy::Always,
    );
    assert_eq!(
        console::strip_ansi_codes(&colored),
        format!("before{expected}tail")
    );
    assert!(!colored.contains("\x1b[2J"));
    assert!(!colored.contains("\x1b]"));
    assert!(!colored.contains("\x1bP"));
}

#[test]
fn styled_text_children_remain_literal_and_do_not_style_the_tail() {
    let value = FormattedText::text("[/heading][draft].txt\\\x1b[2J")
        .styled("heading")
        .unwrap();
    let expected = "[/heading][draft].txt\\\\u{1b}[2J";
    assert_eq!(
        output("{{ value }}tail", &value, ColorPolicy::Never),
        format!("{expected}tail")
    );
    let colored = output("{{ value }}tail", &value, ColorPolicy::Always);
    assert!(colored.contains("\x1b[1m"));
    assert!(colored.ends_with("\x1b[0mtail"));
    assert_eq!(
        console::strip_ansi_codes(&colored),
        format!("{expected}tail")
    );
}

#[test]
fn ansi_import_preserves_supported_style_and_displays_other_sequences() {
    let source = "\x1b[1;38;2;1;2;3m[draft]\x1b[0m\x1b[2J\x1b]x\x1b[31my\x07";
    let value = FormattedText::from_ansi_sgr(source);
    let expected = "[draft]\\u{1b}[2J\\u{1b}]x\\u{1b}[31my\\u{7}";
    assert_eq!(output("{{ value }}", &value, ColorPolicy::Never), expected);
    let colored = output("{{ value }}tail", &value, ColorPolicy::Always);
    assert_eq!(
        console::strip_ansi_codes(&colored),
        format!("{expected}tail")
    );
    assert!(colored.contains('\x1b'));
    assert!(!colored.contains("\x1b[2J"));
    assert!(!colored.contains("\x1b]"));
}

#[test]
fn joins_preserve_explicit_styles_and_concatenation_projects_to_text() {
    let value = FormattedText::text("[draft]").styled("heading").unwrap();
    let joined = output(
        "{{ [value, '[tail]'] | join('\\\\') }}",
        &value,
        ColorPolicy::Always,
    );
    assert!(joined.contains("\x1b[1m"));
    assert_eq!(console::strip_ansi_codes(&joined), "[draft]\\[tail]");
    let concatenated = output("{{ value ~ '[tail]' }}", &value, ColorPolicy::Always);
    assert_eq!(concatenated, "[draft][tail]");
    let composed = value.append("[tail]");
    let output = output("{{ value }}", composed, ColorPolicy::Always);
    assert!(output.contains("\x1b[1m"));
    assert_eq!(console::strip_ansi_codes(&output), "[draft][tail]");
}

#[test]
fn width_truncation_and_padding_measure_the_escaped_visible_text() {
    let value = "\x1b[2J";
    assert_eq!(
        output("{{ value | display_width }}", value, ColorPolicy::Never),
        "9"
    );
    assert_eq!(
        output("{{ value | truncate_at(7) }}", value, ColorPolicy::Never),
        "\\u{1b}…"
    );
    assert_eq!(
        output("{{ value | pad_left(12) }}", value, ColorPolicy::Never),
        "   \\u{1b}[2J"
    );
    assert_eq!(
        output("{{ value | pad_right(12) }}", value, ColorPolicy::Never),
        "\\u{1b}[2J   "
    );
    let formatted = FormattedText::text("[draft]").styled("heading").unwrap();
    assert_eq!(
        output(
            "{{ value | display_width }}",
            &formatted,
            ColorPolicy::Never
        ),
        "7"
    );
    let truncated = output(
        "{{ value | truncate_at(5) }}tail",
        &formatted,
        ColorPolicy::Always,
    );
    assert_eq!(console::strip_ansi_codes(&truncated), "[dra…tail");
    assert!(truncated.contains("\x1b[1m"));
    let (_, unstyled_tail) = truncated.rsplit_once("\x1b[0m").unwrap();
    assert!(!unstyled_tail.contains('\x1b'));
    assert!(unstyled_tail.ends_with("tail"));
}

#[test]
fn macros_and_captures_preserve_formatting_without_reinterpreting_children() {
    let value = "[/heading][draft]\\\x1b[2J";
    let expected = "[/heading][draft]\\\\u{1b}[2J";
    for template in [
        "{% macro show(v) %}[heading]{{ v }}[/heading]{% endmacro %}{{ show(value) }}tail",
        "{% set captured %}[heading]{{ value }}[/heading]{% endset %}{{ captured }}tail",
        "{% macro wrap() %}[heading]{{ caller() }}[/heading]{% endmacro %}{% call wrap() %}{{ value }}{% endcall %}tail",
        "{% macro show(v) %}[heading]{{ v }}[/heading]{% endmacro %}{{ [show(value)] | join }}tail",
    ] {
        let rendered = output(template, value, ColorPolicy::Always);
        assert_eq!(
            console::strip_ansi_codes(&rendered),
            format!("{expected}tail")
        );
        assert!(rendered.contains("\x1b[1m"));
        assert!(rendered.ends_with("\x1b[0mtail"));
    }
}

#[test]
fn string_operations_use_plain_projection_and_keep_result_literal() {
    let value = FormattedText::text("[draft]").styled("heading").unwrap();
    assert_eq!(
        output("{{ value[0:4] }}", &value, ColorPolicy::Always),
        "[dra"
    );
    assert_eq!(
        output(
            "{{ value | replace('draft', 'other') }}",
            &value,
            ColorPolicy::Always
        ),
        "[other]"
    );
    assert_eq!(
        output("{{ value | string }}", &value, ColorPolicy::Always),
        "[draft]"
    );
    assert_eq!(
        output(
            "{% macro show() %}[heading]{{ value }}[/heading]{% endmacro %}{{ show()[0:4] }}",
            &value,
            ColorPolicy::Always
        ),
        "[dra"
    );
}

#[test]
fn safe_metadata_and_html_filters_do_not_authorize_terminal_formatting() {
    let source = "[heading]x[/heading]\x1b[2J";
    let safe = Value::from_safe_string(source.to_owned());
    for template in [
        "{{ value }}",
        "{{ value | safe }}",
        "{{ value | escape }}",
        "{{ value | e }}",
    ] {
        assert_eq!(
            output(template, &safe, ColorPolicy::Always),
            "[heading]x[/heading]\\u{1b}[2J"
        );
    }
}

#[test]
fn bare_environment_does_not_promote_safe_string_ingress() {
    let mut environment = standout_render::template::new_environment();
    standout_render::template::register_filters(&mut environment);
    let source = "[heading]x[/heading]\x1b[2J";
    let rendered = environment
        .render_str(
            "{{ value }}",
            context!(value => Value::from_safe_string(source.into())),
        )
        .unwrap();
    assert_eq!(
        standout_bbparser::strip_tags(&rendered),
        "[heading]x[/heading]\\u{1b}[2J"
    );
}

#[test]
fn unknown_value_tags_do_not_fail_template_validation() {
    let source = "[missing]value[/missing][/heading]\\\x1b[2J";
    assert!(validate_template(
        "[heading]{{ value }}[/heading]",
        &context!(value => source),
        &theme()
    )
    .is_ok());
    assert!(validate_template("[missing]value[/missing]", &context!(), &theme()).is_err());
}

#[test]
fn imported_sgr_is_valid_presentation_without_a_theme_entry() {
    let value = FormattedText::from_ansi_sgr("\x1b[31mred\x1b[0m");
    assert!(validate_template("{{ value }}", &Data { value }, &theme()).is_ok());
}

#[test]
fn formatted_children_have_plain_spelling_when_a_container_is_interpolated() {
    let value = vec![FormattedText::text("[draft]").styled("heading").unwrap()];
    assert_eq!(
        output("{{ value }}", &value, ColorPolicy::Never),
        "[\"[draft]\"]"
    );
}

#[test]
fn every_c0_and_c1_control_uses_the_multiline_policy() {
    let value: String = (0..=0x9f)
        .filter_map(char::from_u32)
        .filter(|character| character.is_control())
        .collect();
    let mut expected = String::new();
    for character in value.chars() {
        if matches!(character, '\n' | '\t') {
            expected.push(character);
        } else {
            expected.push_str(&format!("\\u{{{:x}}}", character as u32));
        }
    }
    assert_eq!(output("{{ value }}", &value, ColorPolicy::Never), expected);
}

#[test]
fn dynamic_style_names_are_validated_and_verbatim_is_absent() {
    for style in ["bad]name", "bad/name", "x\x1b", "Heading"] {
        let result = render_with_output(
            "{{ value | style_as(style) }}",
            &context!(value => "[draft]", style),
            &theme(),
            Representation::Human,
            ColorPolicy::Never,
        );
        assert!(result.is_err());
    }
    let error = render_with_output(
        "{{ value | verbatim }}",
        &context!(value => "text"),
        &theme(),
        Representation::Human,
        ColorPolicy::Never,
    )
    .unwrap_err();
    assert!(error.to_string().contains("verbatim"));
}

#[test]
fn private_use_characters_and_noncharacters_have_no_reserved_meaning() {
    let mut value = String::new();
    for range in [
        0xe000..=0xf8ff,
        0xf0000..=0xffffd,
        0x100000..=0x10fffd,
        0xfdd0..=0xfdef,
    ] {
        value.extend(range.filter_map(char::from_u32));
    }
    for plane in 0..=16 {
        value.push(char::from_u32((plane << 16) | 0xfffe).unwrap());
        value.push(char::from_u32((plane << 16) | 0xffff).unwrap());
    }
    assert_eq!(output("{{ value }}", &value, ColorPolicy::Never), value);
}

#[test]
fn nested_iteration_and_tables_keep_cells_literal() {
    let values = vec![vec!["[draft]", "\x1b[2J"]];
    assert_eq!(
        output(
            "{% for row in value %}{% for cell in row %}{{ cell }}|{% endfor %}{% endfor %}",
            &values,
            ColorPolicy::Never
        ),
        "[draft]|\\u{1b}[2J|"
    );
    let template =
        "{% set t = tabular([{'width': 10}, {'width': 10}], separator='|') %}{{ t.row(value[0]) }}";
    assert_eq!(
        output(template, &values, ColorPolicy::Never),
        "[draft]   |\\u{1b}[2J "
    );
}

#[test]
fn structured_output_preserves_data_and_projects_formatted_values_to_plain_text() {
    let plain = "[draft]\x1b[2J";
    let formatted = FormattedText::from_ansi_sgr("\x1b[31m[draft]\x1b[0m\x1b[2J");
    let data =
        standout_render::RenderData::from_serialize(serde_json::json!({"plain": plain})).unwrap();
    let mut data = data;
    data.as_object_mut().unwrap().insert(
        "formatted".into(),
        standout_render::RenderData::Formatted(formatted),
    );
    let json = render_with_output(
        "unused",
        &data,
        &theme(),
        Representation::Json,
        ColorPolicy::Never,
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap(),
        serde_json::json!({"plain": plain, "formatted": plain})
    );
}

#[test]
fn literal_fragments_cannot_combine_with_interpolated_data() {
    let data = serde_json::json!({"yes": true, "value": "heading"});
    for template in [
        "{% if yes %}[{% else %}]{% endif %}{{ value }}]evil[/heading]",
        "\\{{ value }}",
        "{% for item in [1] %}[{% endfor %}{{ value }}]evil[/heading]",
        "{% macro prefix() %}[{% endmacro %}{{ prefix() }}{{ value }}]evil[/heading]",
    ] {
        let error = render_with_output(
            template,
            &data,
            &theme(),
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap_err();
        assert!(error.to_string().contains("literal fragments"), "{error}");
    }
    assert_eq!(
        output(
            r"\\{{ value }}",
            "[heading]evil[/heading]",
            ColorPolicy::Never
        ),
        r"\[heading]evil[/heading]"
    );
    assert_eq!(
        output(r"\[{{ value }}]", "heading", ColorPolicy::Never),
        "[heading]"
    );
}

#[test]
fn simple_engine_rejects_interpolation_inside_incomplete_literal_escapes() {
    use standout_render::template::SimpleEngine;
    use standout_render::{RenderData, TemplateEngine};
    let engine = SimpleEngine::new();
    let data =
        RenderData::from_serialize(serde_json::json!({"value":"[heading]evil[/heading]"})).unwrap();
    assert!(engine.render_template(r"\{value}", &data).is_err());
    assert!(engine.render_template("[{value}]", &data).is_err());
    assert_eq!(
        standout_bbparser::strip_tags(&engine.render_template(r"\\{value}", &data).unwrap()),
        r"\[heading]evil[/heading]"
    );
}

#[test]
fn loaded_templates_share_capture_and_literal_validation() {
    use standout_render::{MiniJinjaEngine, RenderData, TemplateEngine};
    let mut engine = MiniJinjaEngine::new();
    assert!(engine.add_template("invalid", "[").is_err());
    engine
        .add_template(
            "library",
            "{% macro show(value) %}[heading]{{ value }}[/heading]{% endmacro %}",
        )
        .unwrap();
    engine
        .add_template(
            "parent",
            "{% import 'library' as lib %}{{ lib.show(value) }}tail",
        )
        .unwrap();
    let data =
        RenderData::from_serialize(serde_json::json!({"value":"[/heading][draft]"})).unwrap();
    let markup = engine.render_named("parent", &data).unwrap();
    assert_eq!(
        standout_bbparser::strip_tags(&markup),
        "[/heading][draft]tail"
    );
}

#[test]
fn table_borders_measure_literal_separator_width() {
    for separator in ["[", "]", "\\"] {
        let rendered = output(
            "{% set t = table([{'width': 1}, {'width': 1}], separator=value, border='ascii') %}{{ t.top_border() }}\n{{ t.row(['a', 'b']) }}\n{{ t.bottom_border() }}",
            separator,
            ColorPolicy::Never,
        );
        let widths: Vec<_> = rendered.lines().map(console::measure_text_width).collect();
        assert_eq!(widths, vec![5, 5, 5], "{rendered:?}");
    }
}

#[test]
fn row_from_preserves_nested_formatted_values_and_renderer_fragments() {
    let formatted = FormattedText::text("[draft]").styled("heading").unwrap();
    for table in ["tabular", "table"] {
        for expression in ["value", "'[draft]' | style_as('heading')", "captured"] {
            let template = "{% set captured %}[heading]\\[draft\\][/heading]{% endset %}{% set t = TABLE([{'key': 'nested.0', 'width': 7}]) %}{{ t.row_from({'nested': [EXPRESSION]}) }}tail"
                .replace("TABLE", table)
                .replace("EXPRESSION", expression);
            let rendered = output(&template, &formatted, ColorPolicy::Always);
            assert!(
                rendered.contains("\x1b[1m"),
                "{table}/{expression}: {rendered:?}"
            );
            assert_eq!(console::strip_ansi_codes(&rendered), "[draft]tail");
            assert!(rendered.ends_with("\x1b[0mtail"), "{rendered:?}");
        }
    }
}

#[test]
fn context_values_preserve_nested_formatted_text() {
    use standout_render::context::{ContextRegistry, RenderContext};
    use standout_render::{render_with_context, render_with_vars, RenderData, StyleMode};
    let formatted = FormattedText::text("[draft]").styled("heading").unwrap();
    let rendered = render_with_vars(
        "{{ extra }}tail",
        &(),
        &theme(),
        Representation::Human,
        ColorPolicy::Always,
        [("extra", RenderData::Formatted(formatted.clone()))],
    )
    .unwrap();
    assert!(rendered.contains("\x1b[1m"), "{rendered:?}");
    assert_eq!(console::strip_ansi_codes(&rendered), "[draft]tail");
    let mut registry = ContextRegistry::new();
    registry.add_provider("extra", move |_: &RenderContext| {
        RenderData::from_serialize(std::collections::HashMap::from([(
            "items",
            vec![formatted.clone()],
        )]))
        .unwrap()
    });
    let theme = theme();
    let data = RenderData::Null;
    let context = RenderContext::new(Representation::Human, StyleMode::Ansi, None, &theme, &data);
    let rendered = render_with_context(
        "{{ extra.items[0] }}tail",
        &data,
        &theme,
        Representation::Human,
        ColorPolicy::Always,
        &registry,
        &context,
        None,
    )
    .unwrap();
    assert!(rendered.contains("\x1b[1m"), "{rendered:?}");
    assert_eq!(console::strip_ansi_codes(&rendered), "[draft]tail");
}

#[test]
fn semantic_style_names_never_synthesize_sgr() {
    let name = "_standout_sgr_255_i1_none";
    let theme = Theme::new().add(name, console::Style::new());
    let formatted = FormattedText::text("text").styled(name).unwrap();
    assert_eq!(
        render_with_output(
            "{{ value }}",
            &Data { value: formatted },
            &theme,
            Representation::Human,
            ColorPolicy::Always
        )
        .unwrap(),
        "text"
    );
    for template in [
        "{{ 'text' | style_as(value) }}",
        "{% set t = tabular([{'width': 4, 'style': value}]) %}{{ t.row(['text']) }}",
        "{% set t = table([{'width': 4}], header=['text'], header_style=value) %}{{ t.header_row() }}",
    ] {
        let output = render_with_output(template, &Data { value: name }, &theme, Representation::Human, ColorPolicy::Always).unwrap();
        assert!(!output.contains('\x1b'), "{output:?}");
        assert!(output.contains("text"));
    }
}

#[test]
fn authored_style_names_never_synthesize_sgr() {
    use standout_render::template::SimpleEngine;
    use standout_render::{MiniJinjaEngine, RenderData, TemplateEngine};
    let name = "_standout_sgr_255_i1_none";
    let styles = Theme::new()
        .add(name, console::Style::new())
        .resolve_styles(None);
    for engine in [
        &MiniJinjaEngine::new() as &dyn TemplateEngine,
        &SimpleEngine::new(),
    ] {
        let raw = engine
            .render_template(
                "[_standout_sgr_255_i1_none]text[/_standout_sgr_255_i1_none]",
                &RenderData::Null,
            )
            .unwrap();
        let output = standout_render::template::apply_style_tags(
            &raw,
            &styles,
            standout_render::StyleMode::Ansi,
        );
        assert_eq!(output, "text");
    }
}

#[test]
fn simple_engine_rejects_trailing_incomplete_literal_fragments() {
    use standout_render::template::SimpleEngine;
    use standout_render::{RenderData, TemplateEngine};
    let engine = SimpleEngine::new();
    for source in ["[", "\\", "text[", "{value}\\"] {
        assert!(
            engine.render_template(source, &RenderData::Null).is_err(),
            "{source:?}"
        );
    }
    for source in [r"\[", r"\\"] {
        assert!(engine.render_template(source, &RenderData::Null).is_ok());
    }
}
