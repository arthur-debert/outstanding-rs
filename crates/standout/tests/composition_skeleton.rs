use clap::Command;
use console::Style;
use serde_json::json;
use standout::cli::FnHandler;
use standout::cli::{App, CompletedRun, DispatchResult, Output};
use standout::ColorPolicy;
use standout::EmbeddedTemplates;
use standout::{AmbiguousWidth, ColorMode, IconMode, InputSources, TargetProperties, Theme};

const TEMPLATES: &[(&str, &str)] = &[
    ("list", "[tone]{{ msg }}[/tone]"),
    ("show", "[tone]{{ msg }}[/tone]"),
];

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

fn run_list(app: &App, args: &[&str]) -> CompletedRun {
    app.run_with(
        list_command(),
        args.iter().copied(),
        capable_target(),
        InputSources::from_process(),
    )
}

#[test]
fn styled_list_output_file_receives_raw_without_ansi() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.txt");
    let theme = Theme::new().add("tone", Style::new().red().force_styling(true));
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"msg": "hello"})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let stdout = run_list(
        &app,
        &[
            "app",
            "list",
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

    let terminal = run_list(&app, &["app", "list"]);
    let rendered = terminal.output().expect("terminal list should render");
    assert!(
        rendered.contains("\x1b["),
        "a color-capable terminal must keep ANSI, got {rendered:?}"
    );
}

#[test]
fn silent_list_rejects_rendered_data_through_run_with() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
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
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
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
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_list(&app, &["app", "list"]);
    let output = result
        .output()
        .expect("a structured-only command serializes with no --output");
    let value: serde_json::Value = serde_json::from_str(output).unwrap();
    assert_eq!(value["name"], "Ada");
}

#[test]
fn structured_only_list_rejects_term_debug_through_run_with() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_list(&app, &["app", "list", "--output=term-debug"]);
    match result.into_outcome() {
        DispatchResult::Error(error) => {
            let message = error.to_string();
            assert!(
                message.contains("command `list` is declared structured-only"),
                "{message}"
            );
        }
        other => panic!("expected structured-only list to reject term-debug, got {other:?}"),
    }
}

#[test]
fn styled_show_goes_through_render_request() {
    let theme = Theme::new().add("tone", Style::new().red());
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"msg": "hello"})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let result = app.run_with_color(
        show_command(),
        ["app", "show"],
        capable_target(),
        ColorPolicy::Always,
        InputSources::from_process(),
    );
    let rendered = result.output().expect("show should render");
    assert!(
        rendered.contains("\x1b["),
        "every command's colored render goes through render_request force_styling, got {rendered:?}"
    );
    assert!(rendered.contains("hello"), "got {rendered:?}");
}
