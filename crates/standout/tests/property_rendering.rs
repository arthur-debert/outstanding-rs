use clap::Command;
use console::Style;
use proptest::prelude::*;
use serde_json::{json, Value};
use standout::cli::{App, DispatchResult, Output, RunErrorKind};
use standout::{OutputMode, Theme};
use standout_test::invariants::{
    assert_no_unresolved_tag_markers_in_page, assert_styling_preserves_layout_in_pages,
};

fn output_mode_strategy() -> impl Strategy<Value = OutputMode> {
    prop_oneof![
        Just(OutputMode::Auto),
        Just(OutputMode::Term),
        Just(OutputMode::Text),
        Just(OutputMode::TermDebug),
        Just(OutputMode::Json),
        Just(OutputMode::Yaml),
        Just(OutputMode::Xml),
        Just(OutputMode::Csv),
    ]
}

const TEMPLATES: &[(&str, &str)] = &[
    ("plain", "{{ data }}"),
    ("titled", "[title]{{ data }}[/title]"),
    (
        "highlighted",
        "[highlight]Output: [title]{{ data }}[/title][/highlight]",
    ),
];

#[derive(Debug, Clone)]
struct TemplateCase {
    name: &'static str,
}

fn template_strategy() -> impl Strategy<Value = TemplateCase> {
    prop_oneof![
        Just(TemplateCase { name: "plain" }),
        Just(TemplateCase { name: "titled" }),
        Just(TemplateCase {
            name: "highlighted",
        }),
    ]
}

#[derive(Debug, Clone)]
struct ThemeCase {
    theme: Option<Theme>,
}

fn theme_strategy() -> impl Strategy<Value = ThemeCase> {
    prop_oneof![
        Just(ThemeCase { theme: None }),
        Just(ThemeCase {
            theme: Some(Theme::new()),
        }),
        Just(ThemeCase {
            theme: Some(Theme::new().add("title", Style::new().bold())),
        }),
        Just(ThemeCase {
            theme: Some(Theme::new().add("highlight", Style::new().cyan())),
        }),
        Just(ThemeCase {
            theme: Some(
                Theme::new()
                    .add("title", Style::new().bold())
                    .add("highlight", Style::new().cyan())
                    .add("error", Style::new().red().bold())
            ),
        }),
    ]
}

fn json_data_strategy() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<f64>().prop_map(|f| json!(f)),
        "[a-zA-Z0-9]*".prop_map(Value::String),
    ];
    leaf.prop_recursive(4, 64, 10, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..10).prop_map(Value::Array),
            prop::collection::hash_map("[a-zA-Z0-9]*", inner, 0..10)
                .prop_map(|m| { Value::Object(m.into_iter().collect()) })
        ]
    })
}

fn dispatch(
    mode: OutputMode,
    theme: &ThemeCase,
    template: &TemplateCase,
    data: &Value,
) -> DispatchResult {
    let payload = if mode.is_structured() {
        data.clone()
    } else {
        json!({ "data": data })
    };
    let builder = App::builder()
        .templates(standout::EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "test",
            standout::cli::FnHandler::new(move |_m, _ctx| Ok(Output::Render(payload.clone()))),
            |cfg| cfg.template_name(template.name),
        )
        .unwrap();

    let builder = match &theme.theme {
        Some(t) => builder.theme(t.clone()),
        None => builder,
    };

    let app = builder.build().expect("Failed to build app");
    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    app.dispatch(matches, mode).into_outcome()
}

fn validate_structured_output(output: &str, mode: OutputMode) {
    match mode {
        OutputMode::Json => {
            let parsed: Result<Value, _> = serde_json::from_str(output);
            assert!(
                parsed.is_ok(),
                "JSON output should be parseable: {}",
                output
            );
        }
        OutputMode::Yaml => {
            let parsed: Result<Value, _> = serde_yaml::from_str(output);
            assert!(
                parsed.is_ok(),
                "YAML output should be parseable: {}",
                output
            );
        }
        OutputMode::Xml => {
            assert!(!output.is_empty(), "XML output should not be empty");
            let mut reader = quick_xml::Reader::from_str(output);
            loop {
                match reader.read_event() {
                    Ok(quick_xml::events::Event::Eof) => break,
                    Ok(_) => {}
                    Err(err) => panic!("XML output should parse: {err}\n{output}"),
                }
            }
        }
        OutputMode::Csv => {
            assert!(!output.is_empty(), "CSV output should not be empty");
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(false)
                .flexible(false)
                .from_reader(output.as_bytes());
            for record in reader.records() {
                record.unwrap_or_else(|err| panic!("CSV output should parse: {err}\n{output}"));
            }
        }
        _ => {}
    }
}

fn assert_agrees_with_text_mode(
    styled_page: &str,
    theme: &ThemeCase,
    template: &TemplateCase,
    data: &Value,
) {
    let text = dispatch(OutputMode::Text, theme, template, data);
    let Some(text_page) = text.output() else {
        panic!("Text mode must render the same input: {text:?}");
    };
    assert_styling_preserves_layout_in_pages(&console::strip_ansi_codes(styled_page), text_page);
}

proptest! {
    #[test]
    fn rendering_upholds_the_invariants(
        mode in output_mode_strategy(),
        theme in theme_strategy(),
        template in template_strategy(),
        data in json_data_strategy()
    ) {
        let result = dispatch(mode, &theme, &template, &data);

        prop_assert!(
            result.is_handled() || result.is_error(),
            "expected Handled or Error, got {:?}",
            result
        );

        if mode.is_structured() {
            match mode {
                OutputMode::Json | OutputMode::Yaml => {
                    prop_assert!(
                        result.is_handled(),
                        "{:?} must serialize any generated value, got {:?}",
                        mode,
                        result
                    );
                }
                _ => {
                    prop_assert!(
                        result.is_handled()
                            || result.error_kind() == Some(RunErrorKind::Render),
                        "{:?} must serialize or refuse with a Render error, got {:?}",
                        mode,
                        result
                    );
                }
            }
            if let Some(output) = result.output() {
                validate_structured_output(output, mode);
            }
        } else {
            prop_assert!(
                result.is_handled(),
                "a valid template must render, got {:?}",
                result
            );

            let output = result.output().expect("a handled render has output");
            assert_no_unresolved_tag_markers_in_page(output);
            if matches!(mode, OutputMode::Term | OutputMode::Auto) {
                assert_agrees_with_text_mode(output, &theme, &template, &data);
            }
        }
    }

    #[test]
    fn no_theme_gap_corrupts_a_page_or_splits_the_modes(
        theme in theme_strategy(),
        template in template_strategy(),
        data in json_data_strategy()
    ) {
        let term = dispatch(OutputMode::Term, &theme, &template, &data);
        prop_assert!(term.is_handled(), "expected a render, got {:?}", term);
        let term_page = term.output().unwrap();
        assert_no_unresolved_tag_markers_in_page(term_page);
        assert_agrees_with_text_mode(term_page, &theme, &template, &data);
    }
}
