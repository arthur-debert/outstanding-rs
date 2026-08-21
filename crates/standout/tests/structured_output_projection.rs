//! App-path coverage for per-command structured-output projections.

use clap::{Arg, Command};
use serde_json::{json, Value};
use standout::cli::hooks::TextOutput;
use standout::cli::{
    App, DispatchResult, ExitStatus, Output, RenderedOutput, RunErrorKind, SuccessKind,
};
use standout::tabular::{Column, Width};
use standout::{CsvProjection, OutputMode, StructuredOutputProjection};

const EXPECTED_CSV: &str =
    "LANGUAGE,FILES,CODE,NET\nRust,3,120,100\nPython,2,70,60\nTOTAL,5,190,160\nSKIPPED,1,-,-\n";

fn column(key: &str, header: &str) -> Column {
    Column::new(Width::default()).key(key).header(header)
}

fn rustloc_projection() -> StructuredOutputProjection {
    StructuredOutputProjection::csv(
        CsvProjection::builder("report.items")
            .column(column("language", "LANGUAGE"))
            .column(column("files", "FILES"))
            .column(column("code", "CODE"))
            .derived_column(column("net", "NET"), |row, _root| {
                match row["language"].as_str() {
                    Some("SKIPPED") => Value::Null,
                    _ => json!(
                        row["code"].as_i64().unwrap_or(0) - row["comments"].as_i64().unwrap_or(0)
                    ),
                }
            })
            .synthetic_row(|root| {
                json!({
                    "language": "TOTAL",
                    "files": root["totals"]["files"],
                    "code": root["totals"]["code"],
                    "comments": root["totals"]["comments"]
                })
            })
            .conditional_row(|root| {
                (root["skipped"]["count"].as_u64().unwrap_or(0) > 0).then(|| {
                    json!({
                        "language": "SKIPPED",
                        "files": root["skipped"]["count"]
                    })
                })
            })
            .build(),
    )
}

fn response() -> Value {
    json!({
        "report": {
            "items": [
                { "language": "Rust", "files": 3, "code": 120, "comments": 20 },
                { "language": "Python", "files": 2, "code": 70, "comments": 10 }
            ]
        },
        "totals": { "files": 5, "code": 190, "comments": 30 },
        "skipped": { "count": 1, "paths": ["vendor/generated.rs"] }
    })
}

fn command() -> Command {
    Command::new("rustloc").subcommand(Command::new("summary"))
}

fn command_with_output() -> Command {
    command().arg(
        Arg::new("_output_mode")
            .long("output")
            .value_name("MODE")
            .global(true)
            .value_parser([
                "auto",
                "term",
                "text",
                "term-debug",
                "json",
                "yaml",
                "xml",
                "csv",
            ])
            .default_value("auto"),
    )
}

fn app() -> App {
    App::builder()
        .command_with(
            "summary",
            |_matches, _ctx| Ok(Output::Render(response())),
            |config| {
                config
                    .template("{{ totals.files }} files / {{ totals.code }} lines")
                    .structured_output_projection(rustloc_projection())
            },
        )
        .unwrap()
        .build()
        .unwrap()
}

fn direct_dispatch(app: &App, mode: OutputMode) -> String {
    let matches = command()
        .try_get_matches_from(["rustloc", "summary"])
        .unwrap();
    let DispatchResult::Handled(output) = app.dispatch(matches, mode).into_outcome() else {
        panic!("expected handled output")
    };
    output.into_string()
}

#[test]
fn run_command_and_dispatch_agree_on_csv_projection() {
    let app = app();
    let matches = command_with_output()
        .try_get_matches_from(["rustloc", "summary", "--output=csv"])
        .unwrap();
    let sub = matches.subcommand_matches("summary").unwrap();
    let via_run_command = app
        .run_command(
            "summary",
            sub,
            |_matches, _ctx| Ok(Output::Render(response())),
            "{{ totals.files }} files / {{ totals.code }} lines",
        )
        .expect("run_command should render csv");

    assert_eq!(via_run_command.as_text(), Some(EXPECTED_CSV));
    assert_eq!(via_run_command.as_raw_text(), Some(EXPECTED_CSV));
    assert_eq!(direct_dispatch(&app, OutputMode::Csv), EXPECTED_CSV);
}

#[test]
fn csv_projection_preserves_canonical_output_in_other_modes() {
    let app = app();

    assert_eq!(direct_dispatch(&app, OutputMode::Csv), EXPECTED_CSV);
    assert_eq!(
        direct_dispatch(&app, OutputMode::Text),
        "5 files / 190 lines"
    );

    let json: Value = serde_json::from_str(&direct_dispatch(&app, OutputMode::Json)).unwrap();
    assert_eq!(json, response());

    let yaml = direct_dispatch(&app, OutputMode::Yaml);
    assert!(yaml.contains("report:"));
    assert!(yaml.contains("paths:"));

    let xml = direct_dispatch(&app, OutputMode::Xml);
    assert!(xml.contains("<report>"));
    assert!(xml.contains("<skipped>"));
}

#[test]
fn commands_without_a_projection_keep_automatic_csv_flattening() {
    let app = App::builder()
        .command(
            "summary",
            |_matches, _ctx| Ok(Output::Render(json!([{ "name": "alpha" }]))),
            "unused",
        )
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(direct_dispatch(&app, OutputMode::Csv), "name\nalpha\n");
}

#[test]
fn run_to_string_and_output_file_use_the_same_projection() {
    let app = app();
    let result = app.run_to_string(command(), ["rustloc", "summary", "--output=csv"]);
    assert_eq!(result.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(result.success_kind(), Some(SuccessKind::Command));
    let DispatchResult::Handled(output) = result.into_outcome() else {
        panic!("expected handled output")
    };
    assert_eq!(output, EXPECTED_CSV);

    let tempdir = tempfile::tempdir().unwrap();
    let output_path = tempdir.path().join("summary.csv");
    let output_arg = format!("--output-file-path={}", output_path.display());
    let file_result = app.run_to_string(
        command(),
        ["rustloc", "summary", "--output=csv", &output_arg],
    );
    assert_eq!(file_result.exit_status(), Some(ExitStatus::SUCCESS));
    let DispatchResult::Handled(stdout) = file_result.into_outcome() else {
        panic!("expected handled output")
    };
    assert!(stdout.is_empty());
    assert_eq!(std::fs::read_to_string(output_path).unwrap(), EXPECTED_CSV);
}

#[test]
fn projection_failures_are_typed_render_errors() {
    let projection = StructuredOutputProjection::csv(
        CsvProjection::builder("missing.items")
            .column(column("language", "LANGUAGE"))
            .build(),
    );
    let app = App::builder()
        .command_with(
            "summary",
            |_matches, _ctx| Ok(Output::Render(response())),
            |config| {
                config
                    .structured_only()
                    .structured_output_projection(projection)
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = app.run_to_string(command(), ["rustloc", "summary", "--output=csv"]);

    assert_eq!(result.exit_status(), Some(ExitStatus::FAILURE));
    assert_eq!(result.error_kind(), Some(RunErrorKind::Render));
    assert!(result.error().unwrap().contains("missing.items"));
}

#[test]
fn projection_runs_between_post_dispatch_and_post_output() {
    let app = App::builder()
        .command_with(
            "summary",
            |_matches, _ctx| Ok(Output::Render(response())),
            |config| {
                config
                    .structured_only()
                    .structured_output_projection(rustloc_projection())
                    .post_dispatch(|_matches, _ctx, mut root| {
                        root["report"]["items"][0]["language"] = json!("Rust (hooked)");
                        Ok(root)
                    })
                    .post_output(|_matches, _ctx, output| {
                        Ok(match output {
                            RenderedOutput::Text(text) => RenderedOutput::Text(TextOutput::new(
                                format!("{}POST-OUTPUT\n", text.formatted),
                                format!("{}POST-OUTPUT\n", text.raw),
                            )),
                            output => output,
                        })
                    })
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let csv = direct_dispatch(&app, OutputMode::Csv);
    assert!(csv.starts_with("LANGUAGE,FILES,CODE,NET\nRust (hooked),"));
    assert!(csv.ends_with("POST-OUTPUT\n"));
}
