use proptest::prelude::*;
use standout_render::{
    render_with_output, ColorPolicy, FormattedText, RenderData, Representation, Theme,
};

const KEY_EXPRESSIONS: [&str; 6] = [
    "raw_key",
    "typed_key",
    "raw_key | style_as('heading')",
    "key_macro()",
    "captured_key",
    "key_macro() | style_as('heading')",
];

const SETUP: &str = "{% macro key_macro() %}[heading]{{ raw_key }}[/heading]{% endmacro %}{% set captured_key %}[heading]{{ raw_key }}[/heading]{% endset %}";

fn render(source: &str, key: &str, value: &str) -> String {
    let data = RenderData::from_iter([
        ("raw_key", RenderData::from(key)),
        (
            "typed_key",
            FormattedText::text(key).styled("heading").unwrap().into(),
        ),
        (
            "value",
            FormattedText::text(value).styled("heading").unwrap().into(),
        ),
    ]);
    render_with_output(
        &format!("{SETUP}{source}"),
        &data,
        &Theme::new().add("heading", console::Style::new().bold()),
        Representation::Human,
        ColorPolicy::Always,
    )
    .unwrap()
}

#[test]
fn formatted_map_keys_match_plain_lookup_without_flattening_values() {
    for key in KEY_EXPRESSIONS {
        assert_eq!(
            render(
                &format!("{{% set item = {{{key}: value}} %}}{{{{ item[raw_key] }}}}"),
                "name",
                "[payload]"
            ),
            render("{{ value }}", "name", "[payload]"),
            "{key}"
        );
    }
}

#[test]
fn formatted_subscripts_find_plain_map_keys_without_flattening_values() {
    for key in KEY_EXPRESSIONS {
        assert_eq!(
            render(
                &format!("{{% set item = {{raw_key: value}} %}}{{{{ item[{key}] }}}}"),
                "name",
                "[payload]"
            ),
            render("{{ value }}", "name", "[payload]"),
            "{key}"
        );
    }
}

#[test]
fn attribute_filters_use_plain_keys_and_keep_formatted_results() {
    for key in KEY_EXPRESSIONS {
        for lookup in [
            format!("item | attr({key})"),
            format!("item | attr(*[{key}])"),
            format!("[item] | map(attribute={key}) | first"),
            format!("[item] | map(**{{'attribute': {key}}}) | first"),
            format!("[item] | sort(attribute={key}) | first | attr(raw_key)"),
            format!("[item] | sort(**{{'attribute': {key}}}) | first | attr(raw_key)"),
            format!("[item] | unique(attribute={key}) | first | attr(raw_key)"),
            format!("[item] | unique(**{{'attribute': {key}}}) | first | attr(raw_key)"),
            format!("([item] | groupby({key}) | first).list[0][raw_key]"),
            format!("([item] | groupby(attribute={key}) | first).list[0][raw_key]"),
            format!("([item] | groupby(*[{key}]) | first).list[0][raw_key]"),
            format!("([item] | groupby(**{{'attribute': {key}}}) | first).list[0][raw_key]"),
        ] {
            assert_eq!(
                render(
                    &format!("{{% set item = {{raw_key: value}} %}}{{{{ {lookup} }}}}"),
                    "name",
                    "[payload]"
                ),
                render("{{ value }}", "name", "[payload]"),
                "{lookup}"
            );
        }
    }
}

#[test]
fn comparisons_project_nested_leaves_without_changing_displayed_values() {
    assert_eq!(
        render("{{ raw_key in [typed_key] }}|{{ typed_key in [typed_key] }}|{{ {'x': [typed_key]} == {'x': [raw_key]} }}|{{ typed_key }}", "name", "payload"),
        format!("true|true|true|{}", render("{{ typed_key }}", "name", "payload"))
    );
}

#[test]
fn map_filter_names_project_without_projecting_filter_values() {
    for key in KEY_EXPRESSIONS {
        assert_eq!(
            render(
                &format!("{{{{ ['ok'] | map(*[{key}]) | first }}}}"),
                "upper",
                "payload"
            ),
            "OK"
        );
    }
    assert_eq!(
        render(
            "{{ [value] | map('style_as', 'heading') | first }}",
            "name",
            "[payload]"
        ),
        render("{{ value | style_as('heading') }}", "name", "[payload]")
    );
}

#[test]
fn comparison_projection_keeps_lazy_membership_short_circuiting() {
    use minijinja::Value;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let visited = Arc::new(AtomicUsize::new(0));
    let values = Value::make_object_iterable(visited.clone(), |visited| {
        Box::new((0..1_000_000_000).map(move |_| {
            visited.fetch_add(1, Ordering::Relaxed);
            Value::from(FormattedText::text("name").styled("heading").unwrap())
        }))
    });
    let environment = standout_render::template::new_environment();
    assert_eq!(
        environment
            .render_str(
                "{{ 'name' in (values | __standout_plain_for_comparison) }}",
                minijinja::context!(values)
            )
            .unwrap(),
        "true"
    );
    assert_eq!(visited.load(Ordering::Relaxed), 1);
}

#[test]
fn comparison_projection_preserves_same_object_equality_without_iteration() {
    use minijinja::Value;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let visited = Arc::new(AtomicUsize::new(0));
    let values = Value::make_object_iterable(visited.clone(), |visited| {
        Box::new((0..8).map(move |_| {
            visited.fetch_add(1, Ordering::Relaxed);
            Value::from(FormattedText::text("name").styled("heading").unwrap())
        }))
    });
    let environment = standout_render::template::new_environment();
    for source in [
        "{{ values == values }}",
        "{{ (values | __standout_plain_for_comparison) == (values | __standout_plain_for_comparison) }}",
    ] {
        assert_eq!(environment.render_str(source, minijinja::context!(values => values.clone())).unwrap(), "true");
        assert_eq!(visited.load(Ordering::Relaxed), 0, "{source}");
    }
}

#[test]
fn absent_keys_differ_from_present_undefined_values() {
    for key in ["raw_key", "typed_key"] {
        for (name, expected) in [("a", "true"), ("b", "false")] {
            assert_eq!(
                render(&format!("{{{{ {key} in {{'a': missing}} }}}}"), name, ""),
                expected
            );
            assert_eq!(
                render(&format!("{{{{ {key} in {{'a': 1}} }}}}"), name, ""),
                expected
            );
        }
    }
    assert_eq!(
        render(
            "{{ {'a': missing} == {'b': missing} }}|{{ {'a': missing} == {'a': missing} }}",
            "",
            ""
        ),
        "false|true"
    );
}

const COMPARISON_TESTS: [(&str, bool); 16] = [
    ("eq", true),
    ("equalto", true),
    ("==", true),
    ("ne", false),
    ("!=", false),
    ("lt", false),
    ("lessthan", false),
    ("<", false),
    ("le", true),
    ("<=", true),
    ("gt", false),
    ("greaterthan", false),
    (">", false),
    ("ge", true),
    (">=", true),
    ("in", true),
];

#[test]
fn comparison_test_aliases_agree_with_operators_and_preserve_selected_values() {
    let selected = render("{{ typed_key }}", "name", "payload");
    for (name, expected) in COMPARISON_TESTS {
        let rhs = if name == "in" { "[raw_key]" } else { "raw_key" };
        if name.chars().all(char::is_alphabetic) {
            assert_eq!(
                render(
                    &format!("{{{{ typed_key is {name}({rhs}) }}}}"),
                    "name",
                    "payload"
                ),
                expected.to_string(),
                "{name}"
            );
        }
        for (filter, keep) in [("select", expected), ("reject", !expected)] {
            assert_eq!(
                render(
                    &format!("{{{{ [typed_key] | {filter}('{name}', {rhs}) | first }}}}"),
                    "name",
                    "payload"
                ),
                if keep { selected.as_str() } else { "" },
                "{filter} {name}"
            );
        }
        assert_eq!(render(&format!("{{{{ [{{'key': typed_key}}] | selectattr('key', '{name}', {rhs}) | map(attribute='key') | first }}}}"), "name", "payload"), if expected { selected.as_str() } else { "" }, "selectattr {name}");
    }
    assert_eq!(
        render(
            "{{ typed_key is sameas(raw_key) }}|{{ typed_key is string }}",
            "name",
            "payload"
        ),
        "false|false"
    );
}

fn ordinary_data() -> impl Strategy<Value = serde_json::Value> {
    let scalar = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        (-8_i64..=8).prop_map(serde_json::Value::from),
        "[a-c]{0,4}".prop_map(serde_json::Value::String),
    ];
    scalar.prop_recursive(3, 16, 3, |child| {
        prop_oneof![
            prop::collection::vec(child.clone(), 0..4).prop_map(serde_json::Value::Array),
            prop::collection::btree_map("[a-c]{1,2}", child, 0..4)
                .prop_map(|map| serde_json::Value::Object(map.into_iter().collect())),
        ]
    })
}

fn keys() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec!["a", "9", "é", "中", "[", "]", "\\", "'", " "]),
        0..10,
    )
    .prop_map(|parts| format!("k{}", parts.concat()))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn ordinary_comparisons_agree_with_minijinja(left in ordinary_data(), right in ordinary_data()) {
        let original = minijinja::Environment::new();
        let adapted = standout_render::template::new_environment();
        for (operator, right) in [("==", &right), ("!=", &right), ("==", &left), ("!=", &left), ("in", &right), ("not in", &right)] {
            if matches!(operator, "in" | "not in") && !matches!(right, serde_json::Value::Array(_) | serde_json::Value::Object(_) | serde_json::Value::String(_)) {
                continue;
            }
            let context = minijinja::context!(left => &left, right => right);
            let expected = original.compile_expression(&format!("left {operator} right")).unwrap().eval(context.clone()).map(|value| value.is_true()).map_err(|error| error.kind());
            let actual = adapted.compile_expression(&format!("(left | __standout_plain_for_comparison) {operator} (right | __standout_plain_for_comparison)")).unwrap().eval(context).map(|value| value.is_true()).map_err(|error| error.kind());
            prop_assert_eq!(actual, expected, "left={:?}, right={:?}, operator={}", left, right, operator);
        }
        for (name, _) in COMPARISON_TESTS {
            let context = minijinja::context!(left => &left, right => &right);
            let source = format!("[left] | select('{name}', right) | list | length");
            let expected = original.compile_expression(&source).unwrap().eval(context.clone()).map(|value| value.to_string()).map_err(|error| error.kind());
            let actual = adapted.compile_expression(&source).unwrap().eval(context).map(|value| value.to_string()).map_err(|error| error.kind());
            prop_assert_eq!(actual, expected, "left={:?}, right={:?}, test={}", left, right, name);
        }
    }

    #[test]
    fn textual_key_positions_agree_and_preserve_typed_values(
        key in keys(),
        payload in prop::collection::vec(prop::sample::select(vec!["a", "[heading]", "[/heading]", "\\", "é", "中", "\x1b[2J", "\0"]), 0..8).prop_map(|parts| parts.concat()),
        insertion in 0..KEY_EXPRESSIONS.len(),
        lookup in 0..KEY_EXPRESSIONS.len(),
    ) {
        let insert = KEY_EXPRESSIONS[insertion];
        let lookup = KEY_EXPRESSIONS[lookup];
        let setup = format!("{{% set item = {{{insert}: value}} %}}");
        let expected = render("{{ value }}", &key, &payload);
        for operation in [format!("item[{lookup}]"), format!("item | attr({lookup})"), format!("[item] | map(attribute={lookup}) | first")] {
            let source = format!("{setup}{{{{ {operation} }}}}");
            prop_assert_eq!(render(&source, &key, &payload), expected.as_str(), "{}", source);
        }
        let membership = format!("{setup}{{{{ {lookup} in item }}}}|{{{{ {lookup} == raw_key }}}}");
        prop_assert_eq!(render(&membership, &key, &payload), "true|true");
    }
}
