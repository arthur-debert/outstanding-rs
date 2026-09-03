use std::path::{Path, PathBuf};

use clap::{Arg, Command};
use clapfig::{Clapfig, SearchPath};
use serde::{Deserialize, Serialize};
use serde_json::json;
use standout::cli::{
    App, AppBuilder, CommandContextInput, FnHandler, HelpResult, Output, RunErrorKind, TermSettings,
};
use standout::{EmbeddedTemplates, InputSources, SetupError};
use standout_test::{serial, TestHarness, TestResult};

const TEMPLATES: &[(&str, &str)] = &[("show", "index at {{ index_dir }}")];

#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]
struct FixtureConfig {
    /// Where the index lives.
    #[clapfig(default = "/compiled")]
    index_dir: String,
    #[clapfig(default = 8080)]
    port: i64,
    #[clapfig(default = ["a", "b"])]
    tags: Vec<String>,
    stamp: Option<clapfig::value::Datetime>,
    stamps: Vec<clapfig::value::Datetime>,
    ratio: Option<f64>,
    term: TermSettings,
}

fn fixture_builder(global: &Path) -> clapfig::TypedBuilder<FixtureConfig> {
    Clapfig::typed::<FixtureConfig>()
        .app_name("cfgapp")
        .search_paths(vec![SearchPath::Cwd])
        .persist_scope("local", SearchPath::Cwd)
        .persist_scope("global", SearchPath::Path(global.to_path_buf()))
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

fn configured_builder(root: &Path) -> AppBuilder {
    show_command(App::builder()).config(fixture_builder(&root.join("global")))
}

fn cfgapp() -> Command {
    Command::new("cfgapp").subcommand(Command::new("show"))
}

const FILE: &str = "index_dir = \"/from-file\"\nport = 9000\n";

struct Run {
    result: TestResult,
    root: PathBuf,
}

impl Run {
    fn local_file(&self) -> String {
        std::fs::read_to_string(self.root.join("cfgapp.toml")).unwrap()
    }

    fn global_file(&self) -> String {
        std::fs::read_to_string(self.root.join("global").join("cfgapp.toml")).unwrap()
    }
}

fn run_with(configure: impl FnOnce(AppBuilder) -> AppBuilder, args: &[&str]) -> Run {
    run_file(FILE, configure, args)
}

fn run_file(file: &str, configure: impl FnOnce(AppBuilder) -> AppBuilder, args: &[&str]) -> Run {
    let harness = TestHarness::new()
        .fixture("cfgapp.toml", file)
        .fixture("global/.keep", "");
    let root = harness.tempdir().unwrap().to_path_buf();
    let app = configure(configured_builder(&root)).build().unwrap();
    let result = harness.run(&app, cfgapp(), args);
    Run { result, root }
}

fn run(args: &[&str]) -> Run {
    run_with(|builder| builder, args)
}

#[test]
#[serial]
fn the_config_tree_is_present_and_no_config_command_removes_it() {
    let help = run(&["cfgapp", "help"]).result;
    help.assert_success();
    help.assert_stdout_contains("config");

    run(&["cfgapp", "config", "list"]).result.assert_success();

    let removed = run_with(AppBuilder::no_config_command, &["cfgapp", "config", "list"]).result;
    removed.assert_error_kind(RunErrorKind::ClapUsage);
}

#[test]
fn the_config_command_is_not_installed_without_a_config() {
    let app = show_command(App::builder()).build().unwrap();
    let parsed = app.get_matches_from(
        cfgapp(),
        ["cfgapp", "config", "list"],
        &InputSources::from_process(),
    );
    assert!(matches!(parsed, HelpResult::Error(_)), "{parsed:?}");

    let removed = show_command(App::builder())
        .no_config_command()
        .build()
        .err();
    assert!(
        matches!(removed, Some(SetupError::Config(_))),
        "{removed:?}"
    );
}

#[test]
#[serial]
fn an_app_declared_config_command_is_a_setup_error() {
    let root = tempfile::tempdir().unwrap();
    let silent = || FnHandler::new(|_matches, _ctx| Ok(Output::<serde_json::Value>::Silent));

    for path in ["config", "config.extra"] {
        let registered = configured_builder(root.path())
            .command_with(path, silent(), |cfg| cfg)
            .unwrap()
            .build()
            .err();
        assert!(
            matches!(registered, Some(SetupError::Config(_))),
            "{path}: {registered:?}"
        );
    }

    let app = configured_builder(root.path()).build().unwrap();
    for declared in [
        cfgapp().subcommand(Command::new("config")),
        cfgapp().subcommand(Command::new("settings").alias("config")),
    ] {
        let verified = app.verify_command(&declared);
        assert!(
            matches!(verified, Err(SetupError::Config(_))),
            "{verified:?}"
        );
        let result = TestHarness::new().run(&app, declared, ["cfgapp", "show"]);
        result.assert_error_kind(RunErrorKind::ClapUsage);
        result.assert_error_contains("no_config_command");
    }

    let kept = configured_builder(root.path())
        .no_config_command()
        .build()
        .unwrap();
    assert!(kept
        .verify_command(&cfgapp().subcommand(Command::new("config")))
        .is_ok());
}

#[test]
fn an_override_flag_the_config_tree_takes_is_a_setup_error() {
    let root = tempfile::tempdir().unwrap();
    for flag in ["scope", "file", "force"] {
        let taken = configured_builder(root.path())
            .config_override_flag(flag)
            .build()
            .err();
        assert!(
            matches!(taken, Some(SetupError::Config(_))),
            "{flag}: {taken:?}"
        );
    }
    assert!(configured_builder(root.path())
        .config_override_flag("set")
        .build()
        .is_ok());
}

#[test]
fn a_root_global_the_config_tree_takes_is_a_setup_error() {
    let root = tempfile::tempdir().unwrap();
    let app = configured_builder(root.path()).build().unwrap();
    let globals = || {
        [
            ("--scope", Arg::new("workspace").long("scope")),
            ("--file", Arg::new("target").long("file")),
            ("--force", Arg::new("yes").long("force")),
            ("--scope", Arg::new("zone").long("zone").alias("scope")),
            ("-o", Arg::new("origin").long("origin").short('o')),
            ("id `output`", Arg::new("output").long("out")),
        ]
    };
    for (claim, arg) in globals() {
        let declared = cfgapp().arg(arg.global(true));
        let verified = app.verify_command(&declared);
        assert!(
            matches!(&verified, Err(SetupError::Config(text)) if text.contains(claim)),
            "{claim}: {verified:?}"
        );
        let result = TestHarness::new().run(&app, declared, ["cfgapp", "show"]);
        result.assert_error_kind(RunErrorKind::ClapUsage);
        result.assert_error_contains("no_config_command");
    }

    for (_, arg) in globals() {
        assert!(app.verify_command(&cfgapp().arg(arg)).is_ok());
    }
    let kept = configured_builder(root.path())
        .no_config_command()
        .build()
        .unwrap();
    for (_, arg) in globals() {
        assert!(kept.verify_command(&cfgapp().arg(arg.global(true))).is_ok());
    }
}

#[test]
fn a_framework_flag_the_config_tree_takes_is_a_setup_error() {
    let root = tempfile::tempdir().unwrap();
    type Install = fn(AppBuilder, &str) -> AppBuilder;
    let flags: [(&str, Install); 2] = [
        ("output_flag", |builder, flag| {
            builder.output_flag(Some(flag))
        }),
        ("output_file_flag", |builder, flag| {
            builder.output_file_flag(Some(flag))
        }),
    ];
    for (option, install) in flags {
        for flag in ["scope", "file", "force"] {
            let taken = install(configured_builder(root.path()), flag).build().err();
            assert!(
                matches!(&taken, Some(SetupError::Config(text)) if text.contains(option)),
                "{option}({flag}): {taken:?}"
            );
            assert!(install(configured_builder(root.path()), flag)
                .no_config_command()
                .build()
                .is_ok());
        }
        assert!(install(configured_builder(root.path()), "mode")
            .build()
            .is_ok());
    }
}

#[test]
#[serial]
fn config_json_spells_datetimes_and_non_finite_floats_as_strings() {
    const TYPED: &str =
        "stamp = 1979-05-27T07:32:00Z\nstamps = [1979-05-27, 07:32:00]\nratio = inf\n";
    let json = run_file(
        TYPED,
        |b| b,
        &["cfgapp", "config", "list", "--output", "json"],
    )
    .result;
    json.assert_success();
    let document: serde_json::Value = serde_json::from_str(json.stdout()).unwrap();
    assert_eq!(document["stamp"], json!("1979-05-27T07:32:00Z"));
    assert_eq!(document["stamps"], json!(["1979-05-27", "07:32:00"]));
    assert_eq!(document["ratio"], json!("inf"));

    let one = run_file(
        TYPED,
        |b| b,
        &["cfgapp", "config", "get", "stamp", "--output", "json"],
    )
    .result;
    one.assert_success();
    let document: serde_json::Value = serde_json::from_str(one.stdout()).unwrap();
    assert_eq!(document, json!({ "stamp": "1979-05-27T07:32:00Z" }));
}

#[test]
#[serial]
fn config_list_renders_the_same_entries_in_term_and_json() {
    let term = run(&["cfgapp", "config", "list"]).result;
    term.assert_success();
    term.assert_stdout_contains("index_dir = /from-file");
    term.assert_stdout_contains("port = 9000");
    term.assert_stdout_contains("[\"a\", \"b\"]");
    assert!(!term.stdout().contains('\\'), "{}", term.stdout());

    let bare = run(&["cfgapp", "config"]).result;
    bare.assert_success();
    assert_eq!(bare.stdout(), term.stdout());

    let json = run(&["cfgapp", "config", "list", "--output", "json"]).result;
    json.assert_success();
    let document: serde_json::Value = serde_json::from_str(json.stdout()).unwrap();
    assert_eq!(document["index_dir"], json!("/from-file"));
    assert_eq!(document["port"], json!(9000));
    assert_eq!(document["tags"], json!(["a", "b"]));
}

#[test]
#[serial]
fn config_get_renders_one_typed_entry() {
    let term = run(&["cfgapp", "config", "get", "port"]).result;
    term.assert_success();
    term.assert_stdout_contains("port = 9000");

    let json = run(&["cfgapp", "config", "get", "port", "--output", "json"]).result;
    json.assert_success();
    let document: serde_json::Value = serde_json::from_str(json.stdout()).unwrap();
    assert_eq!(document, json!({ "port": 9000 }));
}

#[test]
#[serial]
fn config_set_writes_the_default_scope_and_confirms() {
    let set = run(&["cfgapp", "config", "set", "index_dir", "/srv/idx"]);
    set.result.assert_success();
    set.result.assert_stdout_contains("index_dir = /srv/idx");
    assert!(
        set.local_file().contains("index_dir = \"/srv/idx\""),
        "{}",
        set.local_file()
    );

    let json = run(&[
        "cfgapp", "config", "set", "port", "1234", "--output", "json",
    ]);
    json.result.assert_success();
    let document: serde_json::Value = serde_json::from_str(json.result.stdout()).unwrap();
    assert_eq!(document, json!({ "key": "port", "value": "1234" }));
    assert!(
        json.local_file().contains("port = 1234"),
        "{}",
        json.local_file()
    );
}

#[test]
#[serial]
fn the_scope_flag_reaches_clapfig() {
    let set = run(&[
        "cfgapp",
        "config",
        "set",
        "index_dir",
        "/srv/idx",
        "--scope",
        "global",
    ]);
    set.result.assert_success();
    assert!(
        set.global_file().contains("index_dir = \"/srv/idx\""),
        "{}",
        set.global_file()
    );
    assert_eq!(set.local_file(), FILE);
}

#[test]
#[serial]
fn config_unset_removes_the_key_and_confirms() {
    let unset = run(&["cfgapp", "config", "unset", "port"]);
    unset.result.assert_success();
    unset.result.assert_stdout_contains("port");
    assert!(
        !unset.local_file().contains("port"),
        "{}",
        unset.local_file()
    );

    let json = run(&["cfgapp", "config", "unset", "port", "--output", "json"]).result;
    json.assert_success();
    let document: serde_json::Value = serde_json::from_str(json.stdout()).unwrap();
    assert_eq!(document, json!({ "key": "port" }));
}

#[test]
#[serial]
fn config_gen_and_schema_are_artifacts() {
    let gen = run(&["cfgapp", "config", "gen"]).result;
    gen.assert_success();
    gen.assert_artifact_to_stdout();
    let template = String::from_utf8(gen.expect_artifact().bytes().to_vec()).unwrap();
    assert!(template.contains("index_dir"), "{template}");
    assert!(template.contains("Where the index lives"), "{template}");

    let schema = run(&["cfgapp", "config", "schema"]).result;
    schema.assert_success();
    schema.assert_artifact_to_stdout();
    let document: serde_json::Value =
        serde_json::from_slice(schema.expect_artifact().bytes()).unwrap();
    assert!(document["properties"]["port"].is_object(), "{document}");
}

#[test]
#[serial]
fn a_written_template_is_a_confirmation() {
    let written = run(&["cfgapp", "config", "gen", "--file", "generated.toml"]);
    written.result.assert_success();
    written.result.assert_stdout_contains("generated.toml");
    assert!(written.root.join("generated.toml").is_file());
}

#[test]
#[serial]
fn a_clapfig_error_takes_the_config_error_path() {
    let result = run(&["cfgapp", "config", "get", "bogus_key"]).result;
    result.assert_error_kind(RunErrorKind::Config);
    result.assert_error_contains("bogus_key");
}
