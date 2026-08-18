use clap::Command;
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, Output};
use standout::views::list_view;
use standout::{OutputMode, Theme};
use standout_test::TestHarness;

#[test]
#[serial]
fn app_template_unknown_tag_degrades_to_text_and_warns() {
    let app = App::builder()
        .command(
            "say",
            |_m, _ctx| Ok(Output::Render(json!({}))),
            "[missing_style]hello[/missing_style]",
        )
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new().output_mode(OutputMode::Term).run(
        &app,
        Command::new("app").subcommand(Command::new("say")),
        ["app", "say"],
    );

    result.assert_success();
    assert_eq!(result.stdout(), "hello");
    assert!(!result.stdout().contains("?]"));
    result.assert_warning_contains("missing_style");
}

#[test]
fn framework_templates_fail_build_when_the_resolved_theme_lacks_their_tags() {
    let result = App::builder().include_framework_styles(false).build();

    let err = match result {
        Ok(_) => panic!("build should reject framework templates without framework styles"),
        Err(err) => err.to_string(),
    };

    assert!(err.contains("framework template"), "{err}");
    assert!(err.contains("standout-muted"), "{err}");
}

#[test]
#[serial]
fn app_theme_overlays_framework_styles_per_tag() {
    let app = App::builder()
        .theme(Theme::new().add("standout-muted", Style::new().red().force_styling(true)))
        .command_with(
            "list",
            |_m, _ctx| {
                Ok(Output::Render(
                    list_view(vec!["one"])
                        .total_count(3)
                        .filter_summary("status=pending")
                        .build(),
                ))
            },
            |config| config.template_name("standout/list-view"),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new().output_mode(OutputMode::Term).run(
        &app,
        Command::new("app").subcommand(Command::new("list")),
        ["app", "list"],
    );

    result.assert_success();
    assert!(
        result.stdout().contains("\x1b[31m"),
        "app override for framework tag should win:\n{}",
        result.stdout()
    );
    assert!(!result.stdout().contains("?]"));
}
