use minijinja::{context, Environment};
use serde_json::json;
use standout_render::template::{new_environment, register_filters, MiniJinjaEngine};
use standout_render::TemplateEngine;

fn env() -> Environment<'static> {
    let mut env = new_environment();
    register_filters(&mut env);
    env
}

fn render(template: &str) -> String {
    env()
        .render_str(
            template,
            context! {
                flag => true,
                off => false,
                nothing => (),
                flags => vec![Some(true), Some(false), None],
            },
        )
        .expect("template renders")
}

#[test]
fn interpolation() {
    assert_eq!(render("{{ flag }}"), "true");
    assert_eq!(render("{{ off }}"), "false");
    assert_eq!(render("{{ nothing }}"), "none");
}

#[test]
fn control_flow_bindings() {
    assert_eq!(
        render("{% for f in flags %}{{ f }},{% endfor %}"),
        "true,false,none,"
    );
    assert_eq!(render("{% set v = flag %}{{ v }}"), "true");
    assert_eq!(render("{{ flag if flag else off }}"), "true");
}

#[test]
fn string_filter() {
    assert_eq!(render("{{ flag | string }}"), "true");
    assert_eq!(render("{{ off | string }}"), "false");
    assert_eq!(render("{{ nothing | string }}"), "none");
    assert_eq!(render("{{ 'kept' | string }}"), "kept");
    assert_eq!(render("{{ 42 | string }}"), "42");
}

#[test]
fn join_filter() {
    assert_eq!(render("{{ flags | join(',') }}"), "true,false,none");
    assert_eq!(render("{{ ['a', 'b'] | join('-') }}"), "a-b");
    assert_eq!(render("{{ [1, 2] | join }}"), "12");
}

#[test]
fn nl_filter() {
    assert_eq!(render("{{ flag | nl }}"), "true\n");
    assert_eq!(render("{{ nothing | nl }}"), "none\n");
}

#[test]
fn sequence_and_map_literals() {
    assert_eq!(render("{{ flags }}"), "[true, false, none]");
    assert_eq!(render("{{ [1, 'a', flag] }}"), r#"[1, "a", true]"#);
    assert_eq!(render(r#"{{ {"k": flag} }}"#), r#"{"k": true}"#);
    assert_eq!(
        render(r#"{{ {"k": [flag, {"deep": nothing}]} }}"#),
        r#"{"k": [true, {"deep": none}]}"#
    );
}

#[test]
fn tabular_cells() {
    let row = render(
        r#"{% set t = tabular([{"width": 6}, {"width": 6}], separator="|") %}{{ t.row([flag, nothing]) }}"#,
    );
    assert_eq!(row, "true  |none  ");

    let bordered = render(
        r#"{% set t = table([{"width": 6}, {"width": 6}], separator="|") %}{{ t.row([off, nothing]) }}"#,
    );
    assert_eq!(bordered, "false |none  ");
}

#[test]
fn tabular_column_options_render_the_same_way() {
    assert_eq!(
        render(
            r#"{% set t = tabular([{"width": 6, "null_repr": off}]) %}{{ t.row_from({"a": none}) }}"#
        ),
        "false "
    );
    assert_eq!(
        render(r#"{% set t = table([{"width": 6}], header=[flag]) %}{{ t.header_row() }}"#),
        "true  "
    );
}

#[test]
fn width_and_padding_filters() {
    assert_eq!(render("{{ flag | display_width }}"), "4");
    assert_eq!(render("{{ nothing | pad_left(6) }}"), "  none");
    assert_eq!(render("{{ off | pad_right(6) }}"), "false ");
    assert_eq!(render("{{ nothing | col(6) }}"), "none  ");
    assert_eq!(render("{{ flag | style_as('ok') }}"), "[ok]true[/ok]");
    assert_eq!(render("{{ off | truncate_at(3) }}"), "fa…");
}

#[test]
fn minijinja_engine_runtime_path() {
    let engine = MiniJinjaEngine::new();
    let data = json!({ "flag": true, "off": false, "nothing": null });
    assert_eq!(
        engine
            .render_template("{{ flag }} {{ off }} {{ nothing }}", &data)
            .unwrap(),
        "true false none"
    );
}

#[test]
fn bare_environment_plus_register_filters() {
    let mut env = Environment::new();
    register_filters(&mut env);
    assert_eq!(
        env.render_str("{{ v }} {{ v|string }}", context! { v => true })
            .unwrap(),
        "true true"
    );
}

/// `~` formats its operands inside minijinja's evaluator, which exposes no
/// hook; pinned so a minijinja change that closes the gap is noticed.
#[test]
fn concatenation_operator_is_a_known_gap() {
    let concatenated = render("{{ 'x' ~ flag }}");
    assert!(
        concatenated == "xtrue" || concatenated == "xTrue",
        "unexpected `~` rendering: {concatenated}"
    );
    assert_eq!(render("{{ 'x' ~ flag|string }}"), "xtrue");
    assert_eq!(render("{{ 'x' }}{{ flag }}"), "xtrue");
}
