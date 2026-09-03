use clap::Command;
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::FnHandler;
use standout::cli::{
    App, Artifact, CommandContextInput, ExitStatus, ExternalFailure, HandlerResult, HookError,
    Hooks, Output, OutputKind, RunErrorKind, SuccessKind,
};
use standout::tabular::{Column, Width};
use standout::views::list_view;
use standout::EmbeddedTemplates;
use standout::{
    ColorMode, CsvProjection, IconDefinition, IconMode, StructuredOutputProjection, Theme,
};
use standout_input::{ClipboardSource, EnvSource, InputChain, StdinSource};
use standout_render::{AmbiguousWidth, Representation};
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[
    ("say", "[tone]{{ icons.mark }}[/tone]"),
    ("echo", "{{ msg }}"),
    ("external-pre", "{{ message }}"),
    ("whoami", "{{ user }}"),
    ("tok", "{{ tok }}"),
    ("read", "{{ val }}"),
    ("paste", "{{ val }}"),
    ("wizard", "{{ name }}/{{ proceed }}/{{ title }}"),
    ("wizard-2", "{{ body }}"),
    ("cat", "{{ text }}"),
    ("echo", "{{ msg }}"),
    ("echo-width", "{{ msg | display_width }}"),
    (
        "export",
        "Wrote {{ report.entries }} entries to {{ receipt.destination }}",
    ),
];
fn build_echo_app(template_name: &'static str) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "echo",
            FnHandler::new(|m: &clap::ArgMatches, _ctx: &_| {
                let msg = m
                    .get_one::<String>("msg")
                    .cloned()
                    .unwrap_or_else(|| "no-arg".into());
                Ok(Output::Render(json!({ "msg": msg })))
            }),
            |cfg| cfg.template_name(template_name),
        )
        .unwrap()
        .build()
        .unwrap()
}
fn echo_command() -> Command {
    Command::new("app")
        .subcommand(Command::new("echo").arg(clap::Arg::new("msg").required(false).index(1)))
}
#[derive(Clone, serde::Serialize)]
struct WidthSensitiveItem {
    name: &'static str,
}
fn build_framework_list_view_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_matches, _ctx| {
                let spec = standout::tabular::TabularSpec::builder()
                    .column(Column::new(Width::Fill).right().key("name"))
                    .build();
                Ok(Output::Render(
                    list_view(vec![WidthSensitiveItem { name: "cascade" }])
                        .tabular_spec(spec)
                        .build(),
                ))
            }),
            |config| config.template_name("standout/list-view"),
        )
        .unwrap()
        .build()
        .unwrap()
}
fn list_command() -> Command {
    Command::new("app").subcommand(Command::new("list"))
}
#[test]
#[serial]
fn simple_handler_returns_rendered_text() {
    let app = build_echo_app("echo");
    let result = TestHarness::new().run(&app, echo_command(), vec!["app", "echo", "hello"]);
    result.assert_success();
    result.assert_stdout_eq("hello");
    result.assert_exit_status(ExitStatus::SUCCESS);
}
#[test]
#[serial]
fn ambiguous_width_policy_can_be_injected_for_the_same_app_fixture() {
    let app = build_echo_app("echo-width");
    let narrow = TestHarness::new()
        .ambiguous_width(AmbiguousWidth::Narrow)
        .run(&app, echo_command(), ["app", "echo", "↦≈Δ"]);
    narrow.assert_stdout_eq("3");
    drop(narrow);
    let wide = TestHarness::new()
        .ambiguous_width(AmbiguousWidth::Wide)
        .run(&app, echo_command(), ["app", "echo", "↦≈Δ"]);
    wide.assert_stdout_eq("5");
}
#[test]
#[serial]
fn terminal_width_cascades_through_the_framework_list_view_template() {
    let app = build_framework_list_view_app();
    for width in [31, 37, 47] {
        let result =
            TestHarness::new()
                .terminal_width(width)
                .run(&app, list_command(), ["app", "list"]);
        result.assert_success();
        let row = result
            .stdout()
            .lines()
            .find(|line| line.contains("cascade"))
            .expect("framework list view should render its tabular row");
        assert_eq!(row.chars().count(), width);
        drop(result);
    }
}
#[test]
#[serial]
fn terminal_width_places_right_aligned_field_at_the_right_edge() {
    let app = build_framework_list_view_app();
    let field = "cascade";
    for width in [80, 120] {
        let result =
            TestHarness::new()
                .terminal_width(width)
                .run(&app, list_command(), ["app", "list"]);
        result.assert_success();
        let row = result
            .stdout()
            .lines()
            .find(|line| line.contains(field))
            .expect("framework list view should render its right-aligned field");
        assert_eq!(row.chars().count(), width);
        assert_eq!(row.find(field), Some(width - field.len()));
        assert!(row.ends_with(field));
        drop(result);
    }
}
#[test]
#[serial]
fn unknown_terminal_width_uses_the_framework_list_view_fallback() {
    let app = build_framework_list_view_app();
    let result = TestHarness::new()
        .no_terminal_width()
        .run(&app, list_command(), ["app", "list"]);
    result.assert_success();
    let row = result
        .stdout()
        .lines()
        .find(|line| line.contains("cascade"))
        .expect("framework list view should render its tabular row");
    assert_eq!(row.chars().count(), 80);
}
fn build_detectable_facts_app() -> App {
    let theme = Theme::new()
        .add_icon("mark", IconDefinition::new("CLASSIC").with_nerdfont("NERD"))
        .add_adaptive(
            "tone",
            Style::new(),
            Some(Style::new().green()),
            Some(Style::new().red()),
        );
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "list",
            FnHandler::new(|_matches, _ctx| {
                let spec = standout::tabular::TabularSpec::builder()
                    .column(Column::new(Width::Fill).right().key("name"))
                    .build();
                Ok(Output::Render(
                    list_view(vec![WidthSensitiveItem { name: "cascade" }])
                        .tabular_spec(spec)
                        .build(),
                ))
            }),
            |config| config.template_name("standout/list-view"),
        )
        .unwrap()
        .build()
        .unwrap()
}
fn detectable_command() -> Command {
    Command::new("app")
        .subcommand(Command::new("say"))
        .subcommand(Command::new("list"))
}
#[test]
#[serial]
fn harness_run_is_independent_of_detected_process_facts() {
    let app = build_detectable_facts_app();
    let cmd = detectable_command();
    let baseline = || TestHarness::new().color_capable_terminal();
    let perturb = || {
        baseline()
            .env("COLUMNS", "37")
            .env("NERD_FONT", "1")
            .env("GTK_THEME", "Adwaita:light")
            .env("COLORFGBG", "0;15")
    };
    let (say_default, say_default_plain) = {
        let result = baseline().run(&app, cmd.clone(), ["app", "say"]);
        result.assert_success();
        (
            result.stdout().to_string(),
            result.stdout_plain().to_string(),
        )
    };
    let list_default = {
        let result = baseline().run(&app, cmd.clone(), ["app", "list"]);
        result.assert_success();
        result.stdout().to_string()
    };
    let say_perturbed = {
        let result = perturb().run(&app, cmd.clone(), ["app", "say"]);
        result.assert_success();
        result.stdout().to_string()
    };
    let list_perturbed = {
        let result = perturb().run(&app, cmd.clone(), ["app", "list"]);
        result.assert_success();
        result.stdout().to_string()
    };
    assert_eq!(say_default, say_perturbed);
    assert_eq!(list_default, list_perturbed);
    assert!(
        say_default_plain.contains("CLASSIC"),
        "unset icon_mode is Classic, got {say_default_plain:?}"
    );
    assert!(
        !say_default_plain.contains("NERD"),
        "NERD_FONT must not select the nerd variant: {say_default_plain:?}"
    );
    let row = list_default
        .lines()
        .find(|line| line.contains("cascade"))
        .expect("framework list view should render its tabular row");
    assert_eq!(
        row.chars().count(),
        80,
        "unset width is None, list-view fallback 80; got {row:?}"
    );
    let say_dark = {
        let result =
            baseline()
                .color_scheme(ColorMode::Dark)
                .run(&app, cmd.clone(), ["app", "say"]);
        result.stdout().to_string()
    };
    let say_light = {
        let result =
            baseline()
                .color_scheme(ColorMode::Light)
                .run(&app, cmd.clone(), ["app", "say"]);
        result.stdout().to_string()
    };
    assert_eq!(
        say_default, say_dark,
        "unset color_scheme is ColorMode::Dark"
    );
    assert_ne!(
        say_default, say_light,
        "Light vs Dark must be visible so scheme independence is meaningful"
    );
    let say_nerd = {
        let result =
            baseline()
                .icon_mode(IconMode::NerdFont)
                .run(&app, cmd.clone(), ["app", "say"]);
        result.stdout().to_string()
    };
    assert_ne!(
        say_default, say_nerd,
        "Classic vs NerdFont must be visible so NERD_FONT independence is meaningful"
    );
}
#[test]
#[serial]
fn harness_exposes_typed_clap_and_handler_outcomes() {
    let app = build_echo_app("echo");
    let help = TestHarness::new().run(&app, echo_command(), ["app", "--help"]);
    help.assert_success();
    help.assert_exit_status(ExitStatus::SUCCESS);
    assert_eq!(help.success_kind(), Some(SuccessKind::ClapHelp));
    let usage = TestHarness::new().run(&app, echo_command(), ["app", "--unknown"]);
    usage.assert_error();
    usage.assert_exit_status(ExitStatus::USAGE_ERROR);
    usage.assert_error_kind(RunErrorKind::ClapUsage);
    let failing = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "fail",
            FnHandler::new(|_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(std::io::Error::other("boom").into())
            }),
            |config| config.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();
    let failing_command = Command::new("app").subcommand(Command::new("fail"));
    let failure = TestHarness::new().run(&failing, failing_command, ["app", "fail"]);
    failure.assert_error();
    failure.assert_exit_status(ExitStatus::FAILURE);
    failure.assert_error_kind(RunErrorKind::Handler);
}
#[test]
#[serial]
fn harness_answers_a_version_declared_on_the_builder() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .version("4.5.6")
        .command_with(
            "echo",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({ "msg": "hi" })))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let result = TestHarness::new().run(&app, echo_command(), ["app", "--version"]);
    result.assert_success();
    result.assert_exit_status(ExitStatus::SUCCESS);
    assert_eq!(result.success_kind(), Some(SuccessKind::ClapVersion));
    result.assert_stdout_contains("4.5.6");
}
#[test]
#[serial]
fn harness_exposes_external_failure_payload_status_and_origin() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "external",
            FnHandler::new(|_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(
                    ExternalFailure::new(128, "fatal: delegated command failed\n")
                        .unwrap()
                        .into(),
                )
            }),
            |config| config.structured_only(),
        )
        .unwrap()
        .command_with(
            "external-pre",
            FnHandler::new(|_matches, _ctx| {
                Ok(Output::Render(json!({ "message": "unreachable" })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .hooks(
            "external-pre",
            Hooks::new().pre_dispatch(|_, _| {
                Err(HookError::pre_dispatch_external(
                    ExternalFailure::new(128, "fatal: pre-dispatch failed\n").unwrap(),
                ))
            }),
        )
        .build()
        .unwrap();
    let command = Command::new("app")
        .subcommand(Command::new("external"))
        .subcommand(Command::new("external-pre"));
    let handler = TestHarness::new().run(&app, command.clone(), ["app", "external"]);
    handler.assert_error();
    handler.assert_exit_status(ExternalFailure::new(128, "").unwrap().exit_status());
    handler.assert_error_kind(RunErrorKind::External);
    assert_eq!(handler.error(), Some("fatal: delegated command failed\n"));
    handler.assert_stdout_eq("");
    drop(handler);
    let pre_dispatch = TestHarness::new().run(&app, command, ["app", "external-pre"]);
    pre_dispatch.assert_error();
    pre_dispatch.assert_exit_status(ExternalFailure::new(128, "").unwrap().exit_status());
    pre_dispatch.assert_error_kind(RunErrorKind::External);
    assert_eq!(pre_dispatch.error(), Some("fatal: pre-dispatch failed\n"));
    pre_dispatch.assert_stdout_eq("");
}
#[test]
#[serial]
fn env_var_visible_to_handler() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "whoami",
            FnHandler::new(|_m, _ctx| {
                let v = InputChain::<String>::new()
                    .try_source(EnvSource::new("STANDOUT_TEST_USER"))
                    .default("anon".into())
                    .resolve(_m)
                    .unwrap();
                Ok(Output::Render(json!({ "user": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("whoami"));
    let result = TestHarness::new().env("STANDOUT_TEST_USER", "arthur").run(
        &app,
        cmd,
        vec!["app", "whoami"],
    );
    result.assert_stdout_eq("arthur");
}
#[test]
#[serial]
fn env_remove_hides_existing_value() {
    std::env::set_var("STANDOUT_TEST_TOKEN", "real");
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "tok",
            FnHandler::new(|_m, _ctx| {
                let v = InputChain::<String>::new()
                    .try_source(EnvSource::new("STANDOUT_TEST_TOKEN"))
                    .default("missing".into())
                    .resolve(_m)
                    .unwrap();
                Ok(Output::Render(json!({ "tok": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("tok"));
    {
        let result =
            TestHarness::new()
                .env_remove("STANDOUT_TEST_TOKEN")
                .run(&app, cmd, vec!["app", "tok"]);
        result.assert_stdout_eq("missing");
    }
    assert_eq!(std::env::var("STANDOUT_TEST_TOKEN").as_deref(), Ok("real"));
    std::env::remove_var("STANDOUT_TEST_TOKEN");
}
#[test]
#[serial]
fn piped_stdin_reaches_handler() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "read",
            FnHandler::new(|_m, ctx| {
                let v = InputChain::<String>::new()
                    .try_source(StdinSource::new())
                    .default("nothing".into())
                    .resolve_from(_m, ctx.input_sources())
                    .unwrap();
                Ok(Output::Render(json!({ "val": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("read"));
    let result = TestHarness::new()
        .piped_stdin("piped-in")
        .run(&app, cmd, vec!["app", "read"]);
    result.assert_stdout_eq("piped-in");
}
#[test]
#[serial]
fn interactive_stdin_falls_through_to_default() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "read",
            FnHandler::new(|_m, ctx| {
                let v = InputChain::<String>::new()
                    .try_source(StdinSource::new())
                    .default("no-pipe".into())
                    .resolve_from(_m, ctx.input_sources())
                    .unwrap();
                Ok(Output::Render(json!({ "val": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("read"));
    let result = TestHarness::new()
        .interactive_stdin()
        .run(&app, cmd, vec!["app", "read"]);
    result.assert_stdout_eq("no-pipe");
}
#[test]
#[serial]
fn clipboard_reaches_handler() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "paste",
            FnHandler::new(|_m, ctx| {
                let v = InputChain::<String>::new()
                    .try_source(ClipboardSource::new())
                    .default("empty".into())
                    .resolve_from(_m, ctx.input_sources())
                    .unwrap();
                Ok(Output::Render(json!({ "val": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("paste"));
    let result =
        TestHarness::new()
            .clipboard("clipboard-content")
            .run(&app, cmd, vec!["app", "paste"]);
    result.assert_stdout_eq("clipboard-content");
}
#[test]
#[serial]
fn scripted_prompts_drive_a_wizard_handler() {
    use standout_input::{
        ConfirmPromptSource, PromptResponse, ScriptedResponder, TextPromptSource,
    };
    use std::sync::Arc;
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "wizard",
            FnHandler::new(|_m, ctx| {
                let sources = ctx.input_sources();
                let name = TextPromptSource::new("Name: ")
                    .prompt_from(sources)
                    .unwrap();
                let proceed = ConfirmPromptSource::new("Continue? ")
                    .prompt_from(sources)
                    .unwrap();
                let title = TextPromptSource::new("Title: ")
                    .prompt_from(sources)
                    .unwrap();
                Ok(Output::Render(json!({
                    "name": name,
                    "proceed": proceed,
                    "title": title,
                })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("wizard"));
    let responder = Arc::new(ScriptedResponder::new([
        PromptResponse::text("Ada"),
        PromptResponse::Bool(true),
        PromptResponse::text("Engineer"),
    ]));
    let result = TestHarness::new()
        .prompts(responder)
        .run(&app, cmd, vec!["app", "wizard"]);
    result.assert_stdout_eq("Ada/true/Engineer");
}
#[test]
#[serial]
fn scripted_cancel_propagates_to_handler() {
    use standout_input::{PromptResponse, ScriptedResponder, TextPromptSource};
    use std::sync::Arc;
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "wizard",
            FnHandler::new(|_m, ctx| {
                let body = match TextPromptSource::new("Name: ").prompt_from(ctx.input_sources()) {
                    Ok(name) => format!("ok:{name}"),
                    Err(e) => format!("err:{e}"),
                };
                Ok(Output::Render(json!({ "body": body })))
            }),
            |cfg| cfg.template_name("wizard-2"),
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("wizard"));
    let responder = Arc::new(ScriptedResponder::new([PromptResponse::Cancel]));
    let result = TestHarness::new()
        .prompts(responder)
        .run(&app, cmd, vec!["app", "wizard"]);
    result.assert_stdout_contains("err:");
    result.assert_stdout_contains("cancelled");
}
#[test]
#[serial]
fn responder_is_reset_between_runs() {
    use standout_input::{PromptResponse, ScriptedResponder, TextPromptSource};
    use std::sync::Arc;
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "wizard",
            FnHandler::new(|_m, ctx| {
                let body = match TextPromptSource::new("Name: ").prompt_from(ctx.input_sources()) {
                    Ok(name) => format!("ok:{name}"),
                    Err(e) => format!("err:{e}"),
                };
                Ok(Output::Render(json!({ "body": body })))
            }),
            |cfg| cfg.template_name("wizard-2"),
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("wizard"));
    let first = TestHarness::new()
        .prompts(Arc::new(ScriptedResponder::new([PromptResponse::text(
            "Ada",
        )])))
        .run(&app, cmd.clone(), vec!["app", "wizard"]);
    first.assert_stdout_eq("ok:Ada");
    drop(first);
    let second = TestHarness::new().run(&app, cmd, vec!["app", "wizard"]);
    second.assert_stdout_contains("err:");
}
#[test]
#[serial]
fn fixture_files_are_materialized_in_cwd() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "cat",
            FnHandler::new(|m, _ctx| {
                let path = m.get_one::<String>("path").cloned().unwrap();
                let text = std::fs::read_to_string(path).unwrap();
                Ok(Output::Render(json!({ "text": text })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app")
        .subcommand(Command::new("cat").arg(clap::Arg::new("path").required(true).index(1)));
    let result = TestHarness::new()
        .fixture("notes/todo.txt", "- buy milk\n- write tests\n")
        .run(&app, cmd, vec!["app", "cat", "notes/todo.txt"]);
    result.assert_stdout_contains("buy milk");
    result.assert_stdout_contains("write tests");
}
#[test]
#[serial]
fn output_mode_override_forces_json() {
    let app = build_echo_app("echo");
    let result = TestHarness::new().output_mode(Representation::Json).run(
        &app,
        echo_command(),
        vec!["app", "echo", "hello"],
    );
    let out = result.stdout();
    assert!(out.contains("\"msg\""));
    assert!(out.contains("\"hello\""));
}
#[test]
#[serial]
fn rustloc_fixture_uses_configured_csv_projection() {
    let projection = StructuredOutputProjection::csv(
        CsvProjection::builder("items")
            .column(
                Column::new(Width::default())
                    .key("language")
                    .header("LANGUAGE"),
            )
            .column(Column::new(Width::default()).key("code").header("CODE"))
            .derived_column(
                Column::new(Width::default()).key("net").header("NET"),
                |row, _root| {
                    json!(row["code"].as_i64().unwrap_or(0) - row["comments"].as_i64().unwrap_or(0))
                },
            )
            .synthetic_row(|root| {
                json!({
                    "language": "TOTAL",
                    "code": root["totals"]["code"],
                    "comments": root["totals"]["comments"]
                })
            })
            .conditional_row(|root| {
                (root["skipped"].as_u64().unwrap_or(0) > 0)
                    .then(|| json!({ "language": "SKIPPED" }))
            })
            .build(),
    );
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "summary",
            FnHandler::new(|_matches, _ctx| {
                Ok(Output::Render(json!({
                    "items": [
                        { "language": "Rust", "code": 120, "comments": 20 },
                        { "language": "Python", "code": 70, "comments": 10 }
                    ],
                    "totals": { "code": 190, "comments": 30 },
                    "skipped": 1
                })))
            }),
            |config| {
                config
                    .structured_only()
                    .structured_output_projection(projection)
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("rustloc").subcommand(Command::new("summary"));
    let result =
        TestHarness::new()
            .output_mode(Representation::Csv)
            .run(&app, cmd, ["rustloc", "summary"]);
    result.assert_stdout_eq(
        "LANGUAGE,CODE,NET\nRust,120,100\nPython,70,60\nTOTAL,190,160\nSKIPPED,-,0\n",
    );
}
#[test]
#[serial]
fn terminal_width_override_does_not_install_a_detector() {
    let app = build_echo_app("echo");
    let result = TestHarness::new()
        .terminal_width(42)
        .stdout_is_terminal(false)
        .run(&app, echo_command(), vec!["app", "echo", "hi"]);
    result.assert_stdout_eq("hi");
}
#[test]
#[serial]
#[should_panic(expected = "absolute")]
fn fixture_rejects_absolute_path() {
    let _ = TestHarness::new().fixture("/etc/passwd", "nope");
}
#[test]
#[serial]
#[should_panic(expected = "..")]
fn fixture_rejects_parent_dir_escape() {
    let _ = TestHarness::new().fixture("../outside", "nope");
}
#[test]
#[serial]
#[should_panic(expected = "..")]
fn relative_cwd_rejects_parent_dir_escape() {
    let _ = TestHarness::new().cwd("../outside");
}
#[test]
#[serial]
#[should_panic(expected = "..")]
fn relative_cwd_rejects_nested_parent_dir_escape() {
    let _ = TestHarness::new().cwd("proj/../../outside");
}
#[test]
#[serial]
fn env_set_then_remove_restores_true_original() {
    std::env::set_var("STANDOUT_DOUBLE_PROBE", "original");
    let app = build_echo_app("echo");
    {
        let _result = TestHarness::new()
            .env("STANDOUT_DOUBLE_PROBE", "transient")
            .env_remove("STANDOUT_DOUBLE_PROBE")
            .run(&app, echo_command(), vec!["app", "echo", "x"]);
    }
    assert_eq!(
        std::env::var("STANDOUT_DOUBLE_PROBE").as_deref(),
        Ok("original")
    );
    std::env::remove_var("STANDOUT_DOUBLE_PROBE");
}
#[test]
#[serial]
fn output_flag_name_is_configurable() {
    let app = standout::cli::App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .output_flag(Some("format"))
        .command_with(
            "echo",
            FnHandler::new(|m, _ctx| {
                let msg = m
                    .get_one::<String>("msg")
                    .cloned()
                    .unwrap_or_else(|| "no-arg".into());
                Ok(Output::Render(json!({ "msg": msg })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let result = TestHarness::new()
        .output_mode(Representation::Json)
        .output_flag_name("format")
        .run(&app, echo_command(), vec!["app", "echo", "hello"]);
    let out = result.stdout();
    assert!(out.contains("\"msg\""), "expected JSON output, got: {out}");
    assert!(out.contains("\"hello\""));
}
#[test]
#[serial]
fn overrides_are_restored_on_drop() {
    let original = std::env::var("STANDOUT_RESTORE_PROBE").ok();
    std::env::set_var("STANDOUT_RESTORE_PROBE", "before");
    {
        let app = build_echo_app("echo");
        let _result = TestHarness::new()
            .env("STANDOUT_RESTORE_PROBE", "during")
            .env("STANDOUT_BRAND_NEW", "new")
            .run(&app, echo_command(), vec!["app", "echo", "x"]);
    }
    assert_eq!(
        std::env::var("STANDOUT_RESTORE_PROBE").as_deref(),
        Ok("before")
    );
    assert!(std::env::var("STANDOUT_BRAND_NEW").is_err());
    std::env::remove_var("STANDOUT_RESTORE_PROBE");
    if let Some(v) = original {
        std::env::set_var("STANDOUT_RESTORE_PROBE", v);
    }
}
#[test]
#[serial]
fn no_match_reports_cleanly() {
    let app = build_echo_app("echo");
    let result = TestHarness::new().run(&app, echo_command(), vec!["app", "unknown"]);
    assert!(
        result.is_error() || result.is_no_match(),
        "expected Error or NoMatch, got: {:?}",
        result.outcome()
    );
}
const ARTIFACT_BYTES: &[u8] = b"id,title\n1,buy milk\n";
fn build_export_app(destination: Option<std::path::PathBuf>) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .output_file_flag(Some("output-file-path"))
        .command_with(
            "export",
            FnHandler::new(move |_m, _ctx| {
                let mut artifact = Artifact::new(ARTIFACT_BYTES.to_vec())
                    .with_report(json!({ "entries": 1, "warnings": ["no due date"] }));
                artifact = match &destination {
                    Some(path) => artifact.suggest_destination(path),
                    None => artifact.allow_stdout(),
                };
                Ok(Output::Artifact(artifact))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
fn export_command() -> Command {
    Command::new("app").subcommand(Command::new("export"))
}
#[test]
#[serial]
fn harness_asserts_bytes_destinations_receipt_and_report() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let app = build_export_app(Some(path.clone()));
    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);
    result.assert_success();
    result.assert_exit_status(ExitStatus::SUCCESS);
    result.assert_artifact_bytes(ARTIFACT_BYTES);
    result.assert_artifact_suggested_destination(&path);
    result.assert_artifact_written_to(&path);
    result.assert_artifact_report_contains("Wrote 1 entries to");
    result.assert_artifact_report_contains(&path.display().to_string());
    assert_eq!(result.artifact_bytes(), Some(ARTIFACT_BYTES));
    assert_eq!(
        result.artifact().unwrap().receipt().byte_count(),
        ARTIFACT_BYTES.len()
    );
    assert_eq!(std::fs::read(&path).unwrap(), ARTIFACT_BYTES);
}
#[test]
#[serial]
fn harness_asserts_the_stdout_artifact_destination() {
    let app = build_export_app(None);
    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);
    result.assert_success();
    result.assert_artifact_to_stdout();
    result.assert_artifact_report_contains("Wrote 1 entries to -");
    assert!(result.artifact_destination().unwrap().is_stdout());
}
#[test]
#[serial]
fn harness_asserts_the_report_data_in_structured_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let app = build_export_app(Some(path.clone()));
    let result = TestHarness::new().output_mode(Representation::Json).run(
        &app,
        export_command(),
        ["app", "export"],
    );
    let report: serde_json::Value =
        serde_json::from_str(result.artifact_report().unwrap()).unwrap();
    assert_eq!(report["report"]["entries"], json!(1));
    assert_eq!(report["report"]["warnings"][0], json!("no due date"));
    assert_eq!(
        report["receipt"]["destination"],
        json!(path.display().to_string())
    );
    assert_eq!(report["receipt"]["stdout"], json!(false));
}
#[test]
#[serial]
fn harness_asserts_a_typed_artifact_write_failure() {
    let dir = tempfile::tempdir().unwrap();
    let unwritable = dir.path().join("missing").join("export.csv");
    let app = build_export_app(Some(unwritable));
    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);
    result.assert_error();
    result.assert_error_kind(RunErrorKind::FinalWrite(OutputKind::Artifact));
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_contains("Error writing artifact");
    assert!(
        result.artifact().is_none(),
        "a failed write produces no report"
    );
}
fn build_pwd_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "echo",
            FnHandler::new(|_m, _ctx| {
                let dir = std::env::current_dir().unwrap();
                Ok(Output::Render(json!({ "msg": dir.to_string_lossy() })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
#[test]
#[serial]
fn relative_cwd_runs_inside_the_tempdir() {
    let app = build_pwd_app();
    let result =
        TestHarness::new()
            .cwd("proj/nested")
            .run(&app, echo_command(), vec!["app", "echo"]);
    let reported = std::path::PathBuf::from(result.stdout().trim())
        .canonicalize()
        .unwrap();
    let temp_root = std::env::temp_dir().canonicalize().unwrap();
    assert!(reported.starts_with(&temp_root), "{reported:?}");
    assert!(reported.ends_with("proj/nested"), "{reported:?}");
}
#[test]
#[serial]
fn relative_cwd_lands_beside_fixtures() {
    let app = build_pwd_app();
    let harness = TestHarness::new()
        .fixture("proj/todos.txt", "x\n")
        .cwd("proj");
    let expected = harness
        .tempdir()
        .unwrap()
        .canonicalize()
        .unwrap()
        .join("proj");
    let result = harness.run(&app, echo_command(), vec!["app", "echo"]);
    let reported = std::path::PathBuf::from(result.stdout().trim())
        .canonicalize()
        .unwrap();
    assert_eq!(reported, expected);
    assert!(reported.join("todos.txt").is_file());
}
#[test]
#[serial]
fn absolute_cwd_is_used_as_given() {
    let app = build_pwd_app();
    let dir = tempfile::tempdir().unwrap();
    let expected = dir.path().canonicalize().unwrap();
    let result = TestHarness::new()
        .cwd(dir.path())
        .run(&app, echo_command(), vec!["app", "echo"]);
    let reported = std::path::PathBuf::from(result.stdout().trim())
        .canonicalize()
        .unwrap();
    assert_eq!(reported, expected);
}
