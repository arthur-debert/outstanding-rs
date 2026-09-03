use clap::{Arg, Command};
use clapfig::{Clapfig, SearchPath};
use serde::{Deserialize, Serialize};
use serde_json::json;
use standout::cli::{
    App, AppBuilder, CommandContextInput, DiagnosticKind, ExitStatus, FnHandler, MissingConfig,
    Output, RunErrorKind, TermSettings,
};
use standout::{EmbeddedTemplates, OutputMode, SetupError};
use standout_test::{serial, TestHarness};

const TEMPLATES: &[(&str, &str)] = &[("show", "index at {{ index_dir }}")];

#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]
struct FixtureConfig {
    #[clapfig(default = "/compiled")]
    index_dir: String,
    term: TermSettings,
}

fn fixture_builder() -> clapfig::TypedBuilder<FixtureConfig> {
    Clapfig::typed::<FixtureConfig>()
        .app_name("cfgapp")
        .search_paths(vec![SearchPath::Cwd])
}

fn show_command(builder: AppBuilder) -> AppBuilder {
    builder
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "show",
            FnHandler::new(|_matches, ctx| {
                let config: &FixtureConfig = ctx.config()?;
                Ok(Output::Render(json!({ "index_dir": config.index_dir })))
            }),
            |cfg| cfg.template_name("show"),
        )
        .unwrap()
}

fn configured_app() -> App {
    show_command(App::builder())
        .config(fixture_builder())
        .term_settings(|config: &FixtureConfig| &config.term)
        .config_override_flag("set")
        .build()
        .unwrap()
}

fn cfgapp() -> Command {
    Command::new("cfgapp").subcommand(Command::new("show"))
}

const BAD_FILE: &str = "index_dir = \"/from-file\"\nbogus_key = 1\n";

#[test]
#[serial]
fn a_file_value_reaches_the_handler() {
    let result = TestHarness::new()
        .fixture("cfgapp.toml", "index_dir = \"/from-file\"\n")
        .run(&configured_app(), cfgapp(), ["cfgapp", "show"]);

    result.assert_success();
    result.assert_stdout_contains("index at /from-file");
}

#[test]
#[serial]
fn an_env_value_reaches_the_handler() {
    let result = TestHarness::new()
        .fixture("cfgapp.toml", "index_dir = \"/from-file\"\n")
        .env("CFGAPP__INDEX_DIR", "/from-env")
        .run(&configured_app(), cfgapp(), ["cfgapp", "show"]);

    result.assert_success();
    result.assert_stdout_contains("index at /from-env");
}

#[test]
#[serial]
fn the_override_flag_reaches_the_handler_above_env() {
    let result = TestHarness::new()
        .fixture("cfgapp.toml", "index_dir = \"/from-file\"\n")
        .env("CFGAPP__INDEX_DIR", "/from-env")
        .run(
            &configured_app(),
            cfgapp(),
            ["cfgapp", "show", "--set", "index_dir=/from-flag"],
        );

    result.assert_success();
    result.assert_stdout_contains("index at /from-flag");
}

#[test]
#[serial]
fn a_malformed_override_pair_is_a_usage_error() {
    let result = TestHarness::new().run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "show", "--set", "index_dir"],
    );

    result.assert_error_kind(RunErrorKind::ClapUsage);
    result.assert_error_contains("KEY=VALUE");
}

#[test]
#[serial]
fn a_bad_key_exits_one_with_clapfigs_message() {
    let result = TestHarness::new().fixture("cfgapp.toml", BAD_FILE).run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "show"],
    );

    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_kind(RunErrorKind::Config);
    result.assert_error_contains("bogus_key");
    result.assert_error_contains("cfgapp.toml");
}

#[test]
#[serial]
fn a_bad_key_under_json_is_a_diagnostic_with_file_and_line() {
    let result = TestHarness::new().fixture("cfgapp.toml", BAD_FILE).run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "show", "--output", "json"],
    );

    result.assert_exit_status(ExitStatus::FAILURE);
    let diagnostic = result.expect_diagnostic();
    assert_eq!(diagnostic.kind, DiagnosticKind::Config);
    assert!(
        format!("{}\n{}", diagnostic.summary, diagnostic.detail).contains("bogus_key"),
        "{diagnostic:?}"
    );
    let range = diagnostic
        .range
        .expect("an unknown key carries its position");
    assert!(
        range.filename.ends_with("cfgapp.toml"),
        "{}",
        range.filename
    );
    assert_eq!(range.start.line, 2);
}

#[test]
#[serial]
fn help_and_usage_errors_never_resolve_the_config() {
    let help = TestHarness::new().fixture("cfgapp.toml", BAD_FILE).run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "--help"],
    );
    help.assert_success();
    help.assert_stdout_contains("show");

    let usage = TestHarness::new().fixture("cfgapp.toml", BAD_FILE).run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "show", "--bogus"],
    );
    usage.assert_error_kind(RunErrorKind::ClapUsage);
}

#[test]
#[serial]
fn an_unregistered_command_never_resolves_the_config() {
    let result = TestHarness::new().fixture("cfgapp.toml", BAD_FILE).run(
        &configured_app(),
        cfgapp().subcommand(Command::new("other")),
        ["cfgapp", "other"],
    );

    assert!(result.is_no_match());
}

const TERM_JSON: &str = "index_dir = \"/from-file\"\n[term]\noutput = \"json\"\n";

#[test]
#[serial]
fn term_output_decides_the_mode_when_the_flag_is_absent() {
    let bare = TestHarness::new().fixture("cfgapp.toml", TERM_JSON).run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "show"],
    );
    bare.assert_success();
    assert_eq!(bare.output_mode(), OutputMode::Json);
    let document: serde_json::Value = serde_json::from_str(bare.stdout()).unwrap();
    assert_eq!(document["index_dir"], "/from-file");

    let flagged = TestHarness::new().fixture("cfgapp.toml", TERM_JSON).run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "show", "--output", "term"],
    );
    flagged.assert_success();
    assert_eq!(flagged.output_mode(), OutputMode::Term);
    flagged.assert_stdout_contains("index at /from-file");

    let help = TestHarness::new().fixture("cfgapp.toml", TERM_JSON).run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "--help"],
    );
    help.assert_success();
    assert_eq!(help.output_mode(), OutputMode::Auto);
}

#[test]
#[serial]
fn term_settings_are_not_read_without_the_accessor() {
    let app = show_command(App::builder())
        .config(fixture_builder())
        .build()
        .unwrap();
    let result = TestHarness::new().fixture("cfgapp.toml", TERM_JSON).run(
        &app,
        cfgapp(),
        ["cfgapp", "show"],
    );

    result.assert_success();
    assert_eq!(result.output_mode(), OutputMode::Auto);
}

#[test]
#[serial]
fn reading_config_without_configuring_it_is_a_typed_error() {
    let app = show_command(App::builder()).build().unwrap();
    let result = TestHarness::new().run(&app, cfgapp(), ["cfgapp", "show"]);

    result.assert_error_kind(RunErrorKind::Handler);
    result.assert_error_contains(
        &MissingConfig {
            type_name: std::any::type_name::<FixtureConfig>(),
        }
        .to_string(),
    );
}

#[test]
#[serial]
fn the_override_flag_refuses_a_name_the_application_declares() {
    let result = TestHarness::new().run(
        &configured_app(),
        cfgapp().arg(Arg::new("set").long("set")),
        ["cfgapp", "show"],
    );

    result.assert_error_kind(RunErrorKind::ClapUsage);
    result.assert_error_contains("config_override_flag(\"set\")");
}

#[test]
fn config_options_without_a_config_fail_build() {
    let accessor = show_command(App::builder())
        .term_settings(|config: &FixtureConfig| &config.term)
        .build()
        .err();
    assert!(
        matches!(accessor, Some(SetupError::Config(_))),
        "{accessor:?}"
    );

    let flag = show_command(App::builder())
        .config_override_flag("set")
        .build()
        .err();
    assert!(matches!(flag, Some(SetupError::Config(_))), "{flag:?}");

    let taken = show_command(App::builder())
        .config(fixture_builder())
        .config_override_flag("output")
        .build()
        .err();
    assert!(matches!(taken, Some(SetupError::Config(_))), "{taken:?}");
}

#[test]
fn an_accessor_over_another_type_fails_build() {
    #[derive(Debug, Serialize, Deserialize, clapfig::Schema)]
    struct Other {
        term: TermSettings,
    }
    let result = show_command(App::builder())
        .config(fixture_builder())
        .term_settings(|other: &Other| &other.term)
        .build()
        .err();
    assert!(matches!(result, Some(SetupError::Config(_))), "{result:?}");
}
