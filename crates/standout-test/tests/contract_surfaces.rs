//! The versioned contract surfaces of `docs/spec/parity-machine-contract.md`
//! (D4, D9) as a process observes them: the app-declared envelope, the
//! framework documents carrying `schema_version`, and the help document.
//! The two `brewlike` cases tagged `group = "schema-version"` in
//! `corpus/archetypes/brewlike/acceptance.toml` are mirrored here verbatim.

use clap::{Arg, ArgAction, Command};
use serde::Serialize;
use serde_json::{json, Value};
use serial_test::serial;
use standout::cli::{
    App, Diagnostic, DiagnosticKind, FnHandler, HandlerResult, Output, RunErrorKind, SuccessKind,
};
use standout::views::list_view;
use standout::{ContractSurface, EmbeddedTemplates, OutputMode};
use standout_test::TestHarness;

#[derive(Serialize, ContractSurface)]
#[contract(schema_version = 1)]
struct Listing {
    items: Vec<Item>,
}

#[derive(Serialize)]
struct Item {
    name: &'static str,
}

#[derive(Serialize, ContractSurface)]
#[contract(schema_version = 1)]
#[serde(transparent)]
struct Formulae(Vec<FormulaRecord>);

#[derive(Serialize)]
struct FormulaRecord {
    name: &'static str,
    installed: &'static str,
    latest: &'static str,
    outdated: bool,
}

const TEMPLATES: &[(&str, &str)] = &[
    (
        "list",
        "{% for item in data.items %}{{ item.name }}\n{% endfor %}",
    ),
    ("tasks", "{% for item in items %}{{ item }}\n{% endfor %}"),
    ("brew-list", "{% for f in data %}{{ f.name }}\n{% endfor %}"),
    ("deps", "{{ data }}"),
    ("fail", "never"),
];

fn command() -> Command {
    Command::new("app")
        .about("A demo")
        .subcommand(Command::new("list").about("Every item"))
        .subcommand(Command::new("tasks").about("Every task"))
        .subcommand(Command::new("fail"))
        .subcommand(
            Command::new("nest").about("A level").subcommand(
                Command::new("leaf")
                    .about("The leaf")
                    .arg(Arg::new("formula").required(true).help("Which one"))
                    .arg(
                        Arg::new("tree")
                            .long("tree")
                            .short('t')
                            .action(ArgAction::SetTrue)
                            .help("As a tree"),
                    ),
            ),
        )
}

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_, _| {
                Ok(Output::Render(
                    Listing {
                        items: vec![Item { name: "a" }, Item { name: "b" }],
                    }
                    .envelope(),
                ))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "tasks",
            FnHandler::new(|_, _| {
                Ok(Output::Render(
                    list_view(vec!["write", "ship"]).intro("Today").build(),
                ))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "fail",
            FnHandler::new(|_, _| -> HandlerResult<Value> {
                Err(Diagnostic::error("refused").detail("why").into())
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

fn run(mode: OutputMode, args: &[&str]) -> standout_test::TestResult {
    TestHarness::new()
        .output_mode(mode)
        .run(&app(), command(), args)
}

fn json_of(result: &standout_test::TestResult) -> Value {
    serde_json::from_str(result.stdout())
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}):\n{}", result.stdout()))
}

fn yaml_of(result: &standout_test::TestResult) -> Value {
    serde_yaml::from_str(result.stdout())
        .unwrap_or_else(|error| panic!("stdout is not YAML ({error}):\n{}", result.stdout()))
}

#[test]
#[serial]
fn an_enveloped_view_stamps_its_version_beside_the_data() {
    let result = run(OutputMode::Json, &["app", "list"]);
    result.assert_success();
    assert_eq!(
        json_of(&result),
        json!({"schema_version": 1, "data": {"items": [{"name": "a"}, {"name": "b"}]}})
    );
    assert!(
        result
            .stdout()
            .starts_with("{\n  \"schema_version\": 1,\n  \"data\": {"),
        "{}",
        result.stdout()
    );
    result.assert_stderr_empty();
}

#[test]
#[serial]
fn a_template_reads_the_envelope_through_its_data_key() {
    let result = run(OutputMode::Text, &["app", "list"]);
    result.assert_success();
    assert_eq!(result.stdout().trim_end(), "a\nb");
}

#[test]
#[serial]
fn the_list_view_document_carries_the_key_under_json_and_yaml() {
    let json = run(OutputMode::Json, &["app", "tasks"]);
    json.assert_success();
    assert_eq!(
        json_of(&json),
        json!({"schema_version": 1, "items": ["write", "ship"], "intro": "Today"})
    );
    assert!(
        json.stdout()
            .starts_with("{\n  \"schema_version\": 1,\n  \"items\": ["),
        "{}",
        json.stdout()
    );
    drop(json);

    let yaml = run(OutputMode::Yaml, &["app", "tasks"]);
    yaml.assert_success();
    assert!(
        yaml.stdout().starts_with("schema_version: 1\nitems:\n"),
        "{}",
        yaml.stdout()
    );
    assert_eq!(
        yaml_of(&yaml),
        json!({"schema_version": 1, "items": ["write", "ship"], "intro": "Today"})
    );
}

#[test]
#[serial]
fn the_root_help_document_under_json_and_yaml() {
    let json = run(OutputMode::Json, &["app", "--help"]);
    json.assert_success();
    assert_eq!(json.success_kind(), Some(SuccessKind::ClapHelp));
    let document = json_of(&json);
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["name"], "app");
    assert_eq!(document["path"], json!(["app"]));
    let usage = document["usage"].as_str().unwrap();
    assert!(
        usage.starts_with("app [OPTIONS]") && usage.contains("<COMMAND>"),
        "{usage}"
    );
    assert_eq!(document["about"], "A demo");
    let subcommands: Vec<&str> = document["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(subcommands, ["list", "tasks", "fail", "nest", "help"]);
    let output_flag = document["args"]
        .as_array()
        .unwrap()
        .iter()
        .find(|arg| arg["long"] == "--output")
        .expect("the framework's own flag is documented");
    assert_eq!(output_flag["value_name"], "MODE");
    assert_eq!(output_flag["default"], "auto");
    assert!(output_flag["possible_values"]
        .as_array()
        .unwrap()
        .contains(&json!("json")));
    assert!(!json.stdout().contains("USAGE"), "{}", json.stdout());
    json.assert_stderr_empty();
    drop(json);

    let yaml = run(OutputMode::Yaml, &["app", "help"]);
    yaml.assert_success();
    assert_eq!(yaml_of(&yaml), document);
}

#[test]
#[serial]
fn a_nested_help_document_names_its_path_by_flag_and_by_word() {
    let flag = run(OutputMode::Json, &["app", "nest", "leaf", "--help"]);
    flag.assert_success();
    let document = json_of(&flag);
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["name"], "leaf");
    assert_eq!(document["path"], json!(["app", "nest", "leaf"]));
    assert_eq!(document["usage"], "app nest leaf [OPTIONS] <formula>");
    assert_eq!(document["about"], "The leaf");
    assert_eq!(document["subcommands"], json!([]));
    assert_eq!(
        document["args"][0],
        json!({
            "name": "formula", "short": null, "long": null, "value_name": "formula",
            "required": true, "help": "Which one", "default": null, "possible_values": []
        })
    );
    assert_eq!(
        document["args"][1],
        json!({
            "name": "tree", "short": "-t", "long": "--tree", "value_name": null,
            "required": false, "help": "As a tree", "default": null, "possible_values": []
        })
    );
    drop(flag);

    let word = run(OutputMode::Yaml, &["app", "help", "nest", "leaf"]);
    word.assert_success();
    assert_eq!(yaml_of(&word), document);
}

#[test]
#[serial]
fn help_under_csv_is_a_render_diagnostic() {
    let result = run(OutputMode::Csv, &["app", "nest", "leaf", "--help"]);
    result.assert_error_kind(RunErrorKind::Render);
    let diagnostic = result.expect_diagnostic();
    assert_eq!(diagnostic.kind, DiagnosticKind::Render);
    assert!(
        diagnostic.summary.contains("no Csv projection"),
        "{diagnostic:?}"
    );
    result.assert_stderr_empty();
}

#[test]
#[serial]
fn the_diagnostic_carries_the_version_its_trait_declares() {
    let result = run(OutputMode::Json, &["app", "fail"]);
    result.assert_error_kind(RunErrorKind::Handler);
    let diagnostic = result.expect_diagnostic();
    assert_eq!(
        diagnostic.schema_version(),
        <Diagnostic as ContractSurface>::SCHEMA_VERSION
    );
    assert_eq!(json_of(&result)["schema_version"], 1);
    assert_eq!(diagnostic.detail, "why");
}

mod brewlike {
    use super::*;

    fn command() -> Command {
        Command::new("brewlike")
            .about("Query the built-in cellar")
            .subcommand(Command::new("list").about("Every installed formula"))
            .subcommand(
                Command::new("deps")
                    .about("The transitive dependency closure of a formula")
                    .arg(
                        Arg::new("tree")
                            .long("tree")
                            .action(ArgAction::SetTrue)
                            .help("Show the closure as a tree instead of a set."),
                    )
                    .arg(
                        Arg::new("formula")
                            .required(true)
                            .help("The formula to resolve."),
                    ),
            )
    }

    fn app() -> App {
        App::builder()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_, _| {
                    let record = |name, installed, latest, outdated| FormulaRecord {
                        name,
                        installed,
                        latest,
                        outdated,
                    };
                    Ok(Output::Render(
                        Formulae(vec![
                            record("basalt", "2.1.0", "2.1.0", false),
                            record("granite", "1.4.2", "1.5.0", true),
                            record("pebble", "0.9.0", "0.9.0", false),
                            record("quartz", "3.0.1", "3.2.0", true),
                        ])
                        .envelope(),
                    ))
                }),
                |cfg| cfg.template_name("brew-list"),
            )
            .unwrap()
            .command_with(
                "deps",
                FnHandler::new(|_, _| Ok(Output::Render(json!(["pebble", "quartz"])))),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    #[serial]
    fn list_json_payload_carries_the_schema_version() {
        let result =
            TestHarness::new().run(&app(), command(), ["brewlike", "list", "--output", "json"]);
        result.assert_success();
        assert_eq!(
            json_of(&result),
            json!({"schema_version":1,
                   "data":[{"name":"basalt","installed":"2.1.0","latest":"2.1.0","outdated":false},
                           {"name":"granite","installed":"1.4.2","latest":"1.5.0","outdated":true},
                           {"name":"pebble","installed":"0.9.0","latest":"0.9.0","outdated":false},
                           {"name":"quartz","installed":"3.0.1","latest":"3.2.0","outdated":true}]})
        );
        result.assert_stderr_empty();
    }

    #[test]
    #[serial]
    fn machine_help_is_a_versioned_document() {
        let result = TestHarness::new().run(
            &app(),
            command(),
            ["brewlike", "deps", "--help", "--output", "json"],
        );
        result.assert_success();
        assert_eq!(result.success_kind(), Some(SuccessKind::ClapHelp));
        let document = json_of(&result);
        assert_eq!(document["schema_version"], 1);
        let carries_tree = document["args"].as_array().unwrap().iter().any(|element| {
            element
                .as_object()
                .unwrap()
                .values()
                .any(|value| value == "--tree")
        });
        assert!(
            carries_tree,
            "no element names --tree:\n{}",
            result.stdout()
        );
        assert!(!result.stdout().contains("Usage:"), "{}", result.stdout());
        result.assert_stderr_empty();
    }
}
