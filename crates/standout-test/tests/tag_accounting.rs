use clap::Command;
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::FnHandler;
use standout::cli::{App, DispatchResult, Output};
use standout::EmbeddedTemplates;
use standout::{AmbiguousWidth, ColorMode, IconMode, TargetProperties};
use standout::Theme;
use standout_fixtures::downstream;
use standout_render::{Representation, TagResolution};
use standout_test::{TestHarness, TestResult};

const TEMPLATES: &[(&str, &str)] = &[
    ("say", "[headline]hello[/headline]"),
    ("emit", "[inner_missing]from the inner run[/inner_missing]"),
    ("say-2", "[headline]{{ embedded }}[/headline]"),
    ("say-3", "[headline]nothing from the inner run[/headline]"),
];
fn fixture_help(mode: Representation) -> TestResult {
    let fixture = downstream().build();
    TestHarness::new()
        .terminal_width(80)
        .stdout_is_terminal(false)
        .output_mode(mode)
        .run(fixture.app(), fixture.command(), ["lookma", "--help"])
}
fn piped_target() -> TargetProperties {
    TargetProperties {
        width: Some(80),
        stdout_is_terminal: false,
        stderr_is_terminal: false,
        stdout_color_capability: false,
        stderr_color_capability: false,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    }
}
fn undefined_tag_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
fn say_command() -> Command {
    Command::new("app").subcommand(Command::new("say"))
}
#[test]
#[serial]
fn the_structural_record_names_a_tag_no_marker_would_reveal() {
    let result = TestHarness::new().stdout_is_terminal(false).run(
        &undefined_tag_app(),
        say_command(),
        ["app", "say"],
    );
    assert_eq!(
        result.stdout(),
        "hello",
        "Text mode erases the tag, so the page carries no evidence"
    );
    assert_eq!(
        result.unresolved_tag_names(),
        ["headline"],
        "the collapsed view names the offending tag once"
    );
    assert!(
        result
            .unresolved_tags()
            .iter()
            .all(|error| error.tag == "headline"),
        "nothing else is blamed: {:?}",
        result.unresolved_tags()
    );
    let defined: Vec<&str> = result
        .tag_resolutions()
        .iter()
        .find(|pass| !pass.is_clean())
        .map(|pass| pass.defined_tags().iter().map(String::as_str).collect())
        .unwrap_or_default();
    assert!(
        defined.contains(&"node"),
        "the unclean pass reports the resolved theme, got {defined:?}"
    );
    assert_eq!(
        result.tag_resolutions().len(),
        2,
        "a handled run applies the style-tag pass twice over the same template \
         output — once for the page, once unstyled for a pipe — and each pass \
         is recorded with the transform it ran under"
    );
}
#[test]
#[serial]
fn standalone_renders_neither_accumulate_nor_reach_a_later_run() {
    let theme = Theme::new().add("node", Style::new().cyan());
    for _ in 0..100 {
        standout_render::render("[headline]hello[/headline]", &json!({}), &theme)
            .expect("the standalone render succeeds");
    }
    let result = fixture_help(Representation::Human);
    result.assert_success();
    assert!(
        !result.tag_resolutions().is_empty(),
        "the run's own passes are still recorded"
    );
    assert!(
        result.unresolved_tag_names().is_empty(),
        "the standalone renders' unresolved `headline` must not be read as this \
         run's, got {:?}",
        result.unresolved_tag_names()
    );
    assert!(result.tag_resolutions().iter().all(TagResolution::is_clean));
}
#[test]
#[serial]
fn term_output_degrades_unresolved_tags_without_hiding_the_structural_record() {
    let result = TestHarness::new().color_capable_terminal().run(
        &undefined_tag_app(),
        say_command(),
        ["app", "say"],
    );
    assert_eq!(result.stdout(), "hello");
    assert_eq!(result.unresolved_tag_names(), ["headline"]);
}
fn embedded_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command_with(
            "emit",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
fn nesting_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| {
                let inner = embedded_app().run_with(
                    Command::new("inner").subcommand(Command::new("emit")),
                    ["inner", "emit"],
                    piped_target(),
                    standout::InputSources::from_process(),
                );
                let embedded = match inner.outcome() {
                    DispatchResult::Handled(output) => output.as_str().to_string(),
                    other => panic!("the inner run must succeed, got {:?}", other),
                };
                Ok(Output::Render(json!({ "embedded": embedded })))
            }),
            |cfg| cfg.template_name("say-2"),
        )
        .unwrap()
        .build()
        .unwrap()
}
#[test]
#[serial]
fn a_nested_run_cannot_hide_its_unresolved_tags_from_the_outer_one() {
    let result = TestHarness::new().stdout_is_terminal(false).run(
        &nesting_app(),
        say_command(),
        ["app", "say"],
    );
    result.assert_success();
    assert_eq!(
        result.stdout(),
        "from the inner run",
        "Text mode erases both tags, so neither page carries evidence"
    );
    assert_eq!(
        result.unresolved_tag_names(),
        ["inner_missing", "headline"],
        "the outer run accounts for the nested run's passes as well as its own, \
         in the order they ran"
    );
    assert_eq!(
        result
            .tag_resolutions()
            .iter()
            .map(TagResolution::nesting_depth)
            .collect::<Vec<_>>(),
        [1, 1, 0, 0],
        "the inner run's passes are marked as having come from one level in, \
         and each run's page is rendered twice"
    );
}
fn discarding_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("headline", Style::new().bold()))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| {
                let discarded = embedded_app().run_with(
                    Command::new("inner").subcommand(Command::new("emit")),
                    ["inner", "emit"],
                    piped_target(),
                    standout::InputSources::from_process(),
                );
                assert!(
                    matches!(discarded.outcome(), DispatchResult::Handled(_)),
                    "the inner run must succeed"
                );
                Ok(Output::Render(json!({})))
            }),
            |cfg| cfg.template_name("say-3"),
        )
        .unwrap()
        .build()
        .unwrap()
}
#[test]
#[serial]
fn a_discarded_nested_run_is_reported_and_distinguishable() {
    let result = TestHarness::new().stdout_is_terminal(false).run(
        &discarding_app(),
        say_command(),
        ["app", "say"],
    );
    result.assert_success();
    assert_eq!(
        result.stdout(),
        "nothing from the inner run",
        "the discarded run left nothing on this page"
    );
    assert_eq!(
        result.unresolved_tag_names(),
        ["inner_missing"],
        "the run is still reported as having rendered a tag its theme lacks"
    );
    assert!(
        result
            .tag_resolutions()
            .iter()
            .filter(|pass| !pass.is_clean())
            .all(|pass| pass.nesting_depth() == 1),
        "the unresolved tag is attributed to the nested run"
    );
    let own: Vec<&str> = result
        .tag_resolutions()
        .iter()
        .filter(|pass| pass.nesting_depth() == 0)
        .flat_map(TagResolution::unresolved_tag_names)
        .collect();
    assert!(
        own.is_empty(),
        "and page scope is one filter away: this run's own renders are clean"
    );
}
