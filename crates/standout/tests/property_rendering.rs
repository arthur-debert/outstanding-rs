use clap::Command;
use console::Style;
use proptest::prelude::*;
use serde_json::{json, Value};
use serde_yaml;
use standout::cli::{App, Local, Output, ThreadSafe};
use standout::{OutputMode, Theme};

// Strategy for generating arbitrary OutputMode values
// Per design guidelines: all 8 output modes must be covered
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

// Strategy for generating arbitrary Themes
// Tests: no theme, empty theme, and populated theme with styles
fn theme_strategy() -> impl Strategy<Value = Option<Theme>> {
    prop_oneof![
        Just(None),
        Just(Some(Theme::new())),
        Just(Some(
            Theme::new()
                .add("title", Style::new().bold())
                .add("highlight", Style::new().cyan())
                .add("error", Style::new().red().bold())
        )),
    ]
}

// Strategy for generating template variations
// Tests different rendering paths that work with any JSON input
fn template_strategy() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        // Simple dump - basic MiniJinja path
        Just("{{ . }}"),
        // With style tags - exercises BBParser
        Just("[title]{{ . }}[/title]"),
        // Nested style tags
        Just("[highlight]Output: [title]{{ . }}[/title][/highlight]"),
    ]
}

// Strategy for generating arbitrary JSON data
fn json_data_strategy() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<f64>().prop_map(|f| json!(f)),
        "[a-zA-Z0-9]*".prop_map(Value::String),
    ];
    leaf.prop_recursive(
        4,  // 4 levels deep
        64, // Max size 64 nodes
        10, // Items per collection
        |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..10).prop_map(Value::Array),
                prop::collection::hash_map("[a-zA-Z0-9]*", inner, 0..10)
                    .prop_map(|m| { Value::Object(m.into_iter().collect()) })
            ]
        },
    )
}

/// Helper to validate structured output
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
        // XML and CSV are harder to validate generically, but we verify no panic
        _ => {}
    }
}

proptest! {
    /// Property test for ThreadSafe handler mode
    #[test]
    fn test_threadsafe_rendering_invariants(
        mode in output_mode_strategy(),
        theme in theme_strategy(),
        template in template_strategy(),
        data in json_data_strategy()
    ) {
        let builder = App::<ThreadSafe>::builder()
            .command(
                "test",
                move |_m, _ctx| Ok(Output::Render(data.clone())),
                template,
            ).unwrap();

        let builder = if let Some(t) = theme {
            builder.theme(t)
        } else {
            builder
        };

        let app = builder.build().expect("Failed to build app");

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();

        let result = app.dispatch(matches, mode);

        // Invariants
        assert!(result.is_handled());
        if let Some(output) = result.output() {
            validate_structured_output(output, mode);
        }
    }

    /// Property test for Local handler mode
    /// Per design guidelines: both handler modes must be tested
    #[test]
    fn test_local_rendering_invariants(
        mode in output_mode_strategy(),
        theme in theme_strategy(),
        template in template_strategy(),
        data in json_data_strategy()
    ) {
        let builder = App::<Local>::builder()
            .command(
                "test",
                move |_m, _ctx| Ok(Output::Render(data.clone())),
                template,
            ).unwrap();

        let builder = if let Some(t) = theme {
            builder.theme(t)
        } else {
            builder
        };

        let app = builder.build().expect("Failed to build app");

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();

        let result = app.dispatch(matches, mode);

        // Same invariants as ThreadSafe - feature parity
        assert!(result.is_handled());
        if let Some(output) = result.output() {
            validate_structured_output(output, mode);
        }
    }
}
