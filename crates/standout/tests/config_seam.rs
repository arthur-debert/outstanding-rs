use clap::{Arg, ArgMatches, Command};
use clapfig::{Clapfig, SearchPath};
use serde::{Deserialize, Serialize};
use serde_json::json;
use standout::cli::{
    App, AppBuilder, CommandContext, CommandContextInput, DiagnosticKind, Dispatch, DispatchResult,
    ExitStatus, FnHandler, HandlerResult, HelpResult, MissingConfig, Output, RunErrorKind,
    StreamSink, TermSettings,
};
use standout::ColorPolicy;
use standout::{EmbeddedTemplates, InputSources, Representation, SetupError, TemplateRef};
use standout_test::{serial, TestHarness};

const TEMPLATES: &[(&str, &str)] = &[
    ("show", "index at {{ index_dir }}"),
    ("doctor", "config {{ state }}"),
];

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

mod handlers {
    use super::*;

    pub fn doctor(_matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<serde_json::Value> {
        let state = match ctx.config::<FixtureConfig>() {
            Ok(_) => "resolved",
            Err(MissingConfig { .. }) => "missing",
        };
        Ok(Output::Render(json!({ "state": state })))
    }
}

#[derive(clap::Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum DoctorCommands {
    #[dispatch(no_config, template_name = "doctor")]
    Doctor,
}

fn doctor_app(derived: bool) -> App {
    let builder = show_command(App::builder());
    let builder = if derived {
        builder.commands(DoctorCommands::dispatch_config()).unwrap()
    } else {
        builder
            .command_with("doctor", FnHandler::new(handlers::doctor), |cfg| {
                cfg.template_name("doctor").without_config()
            })
            .unwrap()
    };
    builder
        .config(fixture_builder())
        .term_settings(|config: &FixtureConfig| &config.term)
        .config_override_flag("set")
        .build()
        .unwrap()
}

fn cfgapp_with_doctor() -> Command {
    cfgapp().subcommand(Command::new("doctor"))
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
    assert_eq!((range.start.line, range.start.column), (2, 1));
}

#[test]
#[serial]
fn a_malformed_file_under_json_carries_the_parse_position() {
    let result = TestHarness::new()
        .fixture("cfgapp.toml", "index_dir = \"/from-file\"\n= 1\n")
        .run(
            &configured_app(),
            cfgapp(),
            ["cfgapp", "show", "--output", "json"],
        );

    result.assert_exit_status(ExitStatus::FAILURE);
    let diagnostic = result.expect_diagnostic();
    assert_eq!(diagnostic.kind, DiagnosticKind::Config);
    let range = diagnostic
        .range
        .expect("a parse error carries its position");
    assert!(
        range.filename.ends_with("cfgapp.toml"),
        "{}",
        range.filename
    );
    assert_eq!((range.start.line, range.start.column), (2, 1));
}

#[test]
#[serial]
fn a_non_file_unknown_key_under_json_carries_no_range() {
    let result = TestHarness::new().env("CFGAPP__BOGUS_KEY", "1").run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "show", "--output", "json"],
    );

    result.assert_exit_status(ExitStatus::FAILURE);
    let diagnostic = result.expect_diagnostic();
    assert_eq!(diagnostic.kind, DiagnosticKind::Config);
    assert!(diagnostic.range.is_none(), "{diagnostic:?}");
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
    assert_eq!(bare.output_mode(), Representation::Json);
    let document: serde_json::Value = serde_json::from_str(bare.stdout()).unwrap();
    assert_eq!(document["index_dir"], "/from-file");

    let flagged = TestHarness::new().fixture("cfgapp.toml", TERM_JSON).run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "show", "--output", "yaml"],
    );
    flagged.assert_success();
    assert_eq!(flagged.output_mode(), Representation::Yaml);
    flagged.assert_stdout_contains("index_dir: /from-file");

    let help = TestHarness::new().fixture("cfgapp.toml", TERM_JSON).run(
        &configured_app(),
        cfgapp(),
        ["cfgapp", "--help"],
    );
    help.assert_success();
    assert_eq!(help.output_mode(), Representation::Human);
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
    assert_eq!(result.output_mode(), Representation::Human);
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

fn commands_taking_set() -> Vec<Command> {
    vec![
        cfgapp().arg(Arg::new("set").long("set")),
        cfgapp().arg(Arg::new("assign").long("assign").alias("set")),
        cfgapp().arg(Arg::new("assign").long("assign").visible_alias("set")),
        cfgapp().subcommand(Command::new("assign").long_flag("set")),
        cfgapp().subcommand(
            Command::new("assign")
                .long_flag("assign")
                .long_flag_alias("set"),
        ),
        cfgapp().subcommand(Command::new("nested").arg(Arg::new("set").long("set"))),
        cfgapp().arg(Arg::new("_config_override").long("other")),
    ]
}

#[test]
#[serial]
fn the_override_flag_refuses_every_spelling_the_application_declares() {
    for cmd in commands_taking_set() {
        let result = TestHarness::new().run(&configured_app(), cmd, ["cfgapp", "show"]);
        result.assert_error_kind(RunErrorKind::ClapUsage);
        result.assert_error_contains("config_override_flag(\"set\")");
    }
}

#[test]
fn manual_parsing_and_verification_refuse_a_declared_override_flag() {
    for cmd in commands_taking_set() {
        let verified = configured_app().verify_command(&cmd);
        assert!(
            matches!(verified, Err(SetupError::Config(_))),
            "{verified:?}"
        );
        let parsed = configured_app().get_matches_from(
            cmd,
            ["cfgapp", "show"],
            &InputSources::from_process(),
        );
        assert!(matches!(parsed, HelpResult::Error(_)), "{parsed:?}");
    }
}

#[test]
fn the_override_flag_refuses_the_generated_help_and_version_flags() {
    let app = |flag: &str| {
        show_command(App::builder())
            .config(fixture_builder())
            .config_override_flag(flag)
            .build()
            .unwrap()
    };
    let help = app("help").verify_command(&cfgapp());
    assert!(matches!(help, Err(SetupError::Config(_))), "{help:?}");
    let version = app("version").verify_command(&cfgapp().version("1.0"));
    assert!(matches!(version, Err(SetupError::Config(_))), "{version:?}");
    assert!(app("version").verify_command(&cfgapp()).is_ok());
    assert!(app("help")
        .verify_command(&cfgapp().disable_help_flag(true))
        .is_ok());
}

#[test]
fn the_override_flag_refuses_flags_clap_generates_on_descendants() {
    let app = |flag: &str| {
        show_command(App::builder())
            .config(fixture_builder())
            .config_override_flag(flag)
            .build()
            .unwrap()
    };
    let child_version = Command::new("cfgapp").subcommand(Command::new("show").version("1.0"));
    let verified = app("version").verify_command(&child_version);
    assert!(
        matches!(verified, Err(SetupError::Config(_))),
        "{verified:?}"
    );
    let parsed = app("version").get_matches_from(
        child_version,
        ["cfgapp", "show"],
        &InputSources::from_process(),
    );
    assert!(matches!(parsed, HelpResult::Error(_)), "{parsed:?}");

    let grandchild_version = Command::new("cfgapp")
        .subcommand(Command::new("show").subcommand(Command::new("all").version("1.0")));
    let verified = app("version").verify_command(&grandchild_version);
    assert!(
        matches!(verified, Err(SetupError::Config(_))),
        "{verified:?}"
    );

    let disabled_below = Command::new("cfgapp").subcommand(
        Command::new("show")
            .version("1.0")
            .disable_version_flag(true),
    );
    assert!(app("version").verify_command(&disabled_below).is_ok());
    let disabled_help_below = Command::new("cfgapp")
        .disable_help_flag(true)
        .subcommand(Command::new("show").subcommand(Command::new("all")));
    assert!(app("help").verify_command(&disabled_help_below).is_ok());
}

#[test]
#[serial]
fn a_disabled_help_flag_frees_help_for_overrides_on_every_subcommand() {
    let app = show_command(App::builder())
        .config(fixture_builder())
        .config_override_flag("help")
        .build()
        .unwrap();
    let result = TestHarness::new()
        .fixture("cfgapp.toml", "index_dir = \"/from-file\"\n")
        .run(
            &app,
            cfgapp().disable_help_flag(true),
            ["cfgapp", "show", "--help", "index_dir=/from-help"],
        );
    result.assert_success();
    result.assert_stdout_contains("index at /from-help");
}

struct Cwd(std::path::PathBuf);

impl Cwd {
    fn enter(dir: &std::path::Path) -> Self {
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self(previous)
    }
}

impl Drop for Cwd {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

fn parse(app: &App, args: &[&str]) -> ArgMatches {
    match app.get_matches_from(cfgapp(), args, &InputSources::from_process()) {
        HelpResult::Matches(matches) => matches,
        other => panic!("{other:?}"),
    }
}

fn dispatch_parsed(file: &str, args: &[&str]) -> standout::cli::CompletedRun {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cfgapp.toml"), file).unwrap();
    let _cwd = Cwd::enter(dir.path());
    let app = configured_app();
    let matches = parse(&app, args);
    app.dispatch(matches, Representation::Human)
}

fn dispatch_extracted(file: &str, args: &[&str]) -> standout::cli::CompletedRun {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cfgapp.toml"), file).unwrap();
    let _cwd = Cwd::enter(dir.path());
    let app = configured_app();
    let matches = parse(&app, args);
    let output_mode = app.extract_output_mode(&matches);
    app.dispatch(matches, output_mode)
}

#[test]
#[serial]
fn dispatch_takes_term_output_only_when_the_flag_was_not_typed() {
    let bare = dispatch_extracted(TERM_JSON, &["cfgapp", "show"]);
    assert_eq!(bare.output_mode(), Representation::Json);
    let document: serde_json::Value = serde_json::from_str(bare.output().unwrap()).unwrap();
    assert_eq!(document["index_dir"], "/from-file");

    let flagged = dispatch_extracted(TERM_JSON, &["cfgapp", "show", "--output", "yaml"]);
    assert_eq!(flagged.output_mode(), Representation::Yaml);
    assert!(flagged.output().unwrap().contains("index_dir: /from-file"));

    let unset = dispatch_parsed("index_dir = \"/from-file\"\n", &["cfgapp", "show"]);
    assert_eq!(unset.output_mode(), Representation::Human);
}

#[test]
#[serial]
fn run_command_resolves_the_config_like_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("cfgapp.toml");
    std::fs::write(&file, TERM_JSON).unwrap();
    let _cwd = Cwd::enter(dir.path());
    let app = configured_app();
    let run = |args: &[&str]| {
        let matches = parse(&app, args);
        let sub = matches.subcommand_matches("show").unwrap();
        app.run_command(
            "show",
            sub,
            FnHandler::new(|_matches, ctx| {
                let config: &FixtureConfig = ctx.config()?;
                Ok(Output::Render(json!({ "index_dir": config.index_dir })))
            }),
            TemplateRef::Inline("index at {{ index_dir }}".to_string()),
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        )
    };

    let bare = run(&["cfgapp", "show"]).unwrap();
    let document: serde_json::Value = serde_json::from_str(bare.as_text().unwrap()).unwrap();
    assert_eq!(document["index_dir"], "/from-file");

    let flagged = run(&[
        "cfgapp",
        "show",
        "--output",
        "yaml",
        "--set",
        "index_dir=/from-flag",
    ])
    .unwrap();
    assert!(flagged.as_text().unwrap().contains("index_dir: /from-flag"));

    std::fs::write(&file, BAD_FILE).unwrap();
    let failed = run(&["cfgapp", "show"]).unwrap_err();
    assert!(failed.to_string().contains("Config error"), "{failed}");
}

#[test]
#[serial]
fn only_the_resolved_config_answers_as_the_config() {
    let app = show_command(App::builder())
        .command_with(
            "leak",
            FnHandler::new(|_matches, ctx| match ctx.config::<InputSources>() {
                Err(MissingConfig { .. }) => Ok(Output::<serde_json::Value>::Silent),
                Ok(_) => Err(anyhow::anyhow!("InputSources answered as the config")),
            }),
            |cfg| cfg.silent(),
        )
        .unwrap()
        .config(fixture_builder())
        .build()
        .unwrap();
    let result = TestHarness::new()
        .fixture("cfgapp.toml", "index_dir = \"/from-file\"\n")
        .run(
            &app,
            cfgapp().subcommand(Command::new("leak")),
            ["cfgapp", "leak"],
        );
    result.assert_success();
}

#[test]
#[serial]
fn dispatch_resolves_the_config_for_manually_parsed_matches() {
    let from_file = dispatch_parsed("index_dir = \"/from-file\"\n", &["cfgapp", "show"]);
    assert_eq!(from_file.output(), Some("index at /from-file"));

    let from_flag = dispatch_parsed(
        "index_dir = \"/from-file\"\n",
        &["cfgapp", "show", "--set", "index_dir=/from-flag"],
    );
    assert_eq!(from_flag.output(), Some("index at /from-flag"));

    let failed = dispatch_parsed(BAD_FILE, &["cfgapp", "show"]);
    assert!(
        matches!(failed.outcome(), DispatchResult::Error(error) if error.kind() == RunErrorKind::Config),
        "{:?}",
        failed.outcome()
    );
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

#[test]
#[serial]
fn a_command_declining_config_runs_when_the_file_does_not_load() {
    for derived in [false, true] {
        let result = TestHarness::new().fixture("cfgapp.toml", BAD_FILE).run(
            &doctor_app(derived),
            cfgapp_with_doctor(),
            ["cfgapp", "doctor"],
        );

        result.assert_success();
        result.assert_stdout_contains("config missing");
    }
}

#[test]
#[serial]
fn a_command_declining_config_reads_none_from_a_file_that_loads() {
    for derived in [false, true] {
        let result = TestHarness::new()
            .fixture("cfgapp.toml", "index_dir = \"/from-file\"\n")
            .run(
                &doctor_app(derived),
                cfgapp_with_doctor(),
                ["cfgapp", "doctor"],
            );

        result.assert_success();
        result.assert_stdout_contains("config missing");
    }
}

#[test]
#[serial]
fn a_sibling_of_a_declining_command_still_fails_on_the_broken_file() {
    for derived in [false, true] {
        let result = TestHarness::new().fixture("cfgapp.toml", BAD_FILE).run(
            &doctor_app(derived),
            cfgapp_with_doctor(),
            ["cfgapp", "show"],
        );

        result.assert_exit_status(ExitStatus::FAILURE);
        result.assert_error_kind(RunErrorKind::Config);
        result.assert_error_contains("bogus_key");
    }
}
