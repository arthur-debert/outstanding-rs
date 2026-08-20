//! Walking-skeleton request path: `list` through `run_with`.
//!
//! These tests cover review findings that only show up on the `list` adapter:
//! split formatted/raw output, and glue-side template absence.

use clap::Command;
use console::Style;
use serde_json::json;
use standout::cli::{App, DispatchResult, Output, RunResult};
use standout::{AmbiguousWidth, ColorMode, IconMode, InputSources, TargetProperties, Theme};

fn list_command() -> Command {
    Command::new("app").subcommand(Command::new("list"))
}

fn show_command() -> Command {
    Command::new("app").subcommand(Command::new("show"))
}

fn capable_target() -> TargetProperties {
    TargetProperties {
        width: Some(80),
        stdout_is_terminal: true,
        stderr_is_terminal: true,
        stdout_color_capability: true,
        stderr_color_capability: true,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    }
}

fn run_list(app: &App, args: &[&str]) -> RunResult {
    app.run_with(
        list_command(),
        args.iter().copied(),
        capable_target(),
        InputSources::from_process(),
    )
}

#[test]
fn styled_list_term_output_file_receives_raw_without_ansi() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.txt");
    let theme = Theme::new().add("tone", Style::new().red().force_styling(true));
    let app = App::builder()
        .theme(theme)
        .command(
            "list",
            |_m, _ctx| Ok(Output::Render(json!({"msg": "hello"}))),
            "[tone]{{ msg }}[/tone]",
        )
        .unwrap()
        .build()
        .unwrap();

    let stdout = run_list(
        &app,
        &[
            "app",
            "list",
            "--output=term",
            &format!("--output-file-path={}", path.display()),
        ],
    );
    assert!(
        stdout.is_handled(),
        "expected handled silent file write, got {stdout:?}"
    );
    let file = std::fs::read_to_string(&path).unwrap();
    assert!(
        !file.contains("\x1b["),
        "output file must receive raw text, got {file:?}"
    );
    assert!(file.contains("hello"), "got {file:?}");

    let terminal = run_list(&app, &["app", "list", "--output=term"]);
    let rendered = terminal.output().expect("terminal list should render");
    assert!(
        rendered.contains("\x1b["),
        "terminal --output=term must keep ANSI, got {rendered:?}"
    );
}

#[test]
fn silent_list_rejects_rendered_data_through_run_with() {
    let app = App::builder()
        .command_with(
            "list",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |cfg| cfg.silent(),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_list(&app, &["app", "list"]);
    match result.into_outcome() {
        DispatchResult::Error(error) => {
            let message = error.to_string();
            assert!(
                message.contains("command `list` is declared silent"),
                "{message}"
            );
        }
        other => panic!("expected silent list to reject Render data, got {other:?}"),
    }
}

#[test]
fn binary_list_rejects_rendered_data_through_run_with() {
    let app = App::builder()
        .command_with(
            "list",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |cfg| cfg.binary(),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_list(&app, &["app", "list"]);
    match result.into_outcome() {
        DispatchResult::Error(error) => {
            let message = error.to_string();
            assert!(
                message.contains("command `list` is declared binary"),
                "{message}"
            );
        }
        other => panic!("expected binary list to reject Render data, got {other:?}"),
    }
}

#[test]
fn structured_only_list_serializes_json_through_run_with() {
    let app = App::builder()
        .command_with(
            "list",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_list(&app, &["app", "list"]);
    let output = result.output().expect("structured-only Auto serializes");
    let value: serde_json::Value = serde_json::from_str(output).unwrap();
    assert_eq!(value["name"], "Ada");
}

#[test]
fn structured_only_list_rejects_term_through_run_with() {
    let app = App::builder()
        .command_with(
            "list",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_list(&app, &["app", "list", "--output=term"]);
    match result.into_outcome() {
        DispatchResult::Error(error) => {
            let message = error.to_string();
            assert!(
                message.contains("command `list` is declared structured-only"),
                "{message}"
            );
        }
        other => panic!("expected structured-only list to reject term, got {other:?}"),
    }
}

#[test]
fn styled_show_term_goes_through_render_request() {
    let theme = Theme::new().add("tone", Style::new().red());
    let app = App::builder()
        .theme(theme)
        .command(
            "show",
            |_m, _ctx| Ok(Output::Render(json!({"msg": "hello"}))),
            "[tone]{{ msg }}[/tone]",
        )
        .unwrap()
        .build()
        .unwrap();

    let result = app.run_with(
        show_command(),
        ["app", "show", "--output=term"],
        capable_target(),
        InputSources::from_process(),
    );
    let rendered = result.output().expect("show should render");
    assert!(
        rendered.contains("\x1b["),
        "every command's Term render goes through render_request force_styling, got {rendered:?}"
    );
    assert!(rendered.contains("hello"), "got {rendered:?}");
}
