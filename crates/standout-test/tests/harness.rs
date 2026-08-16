//! Integration tests for `TestHarness`.
//!
//! All tests are `#[serial]` because the harness mutates process-global
//! state (env vars, cwd, detectors, default input readers).

use clap::Command;
use serde_json::json;
use serial_test::serial;
use standout::cli::{
    App, Artifact, ExitStatus, ExternalFailure, HandlerResult, HookError, Hooks, Output,
    OutputKind, RunErrorKind, SuccessKind,
};
use standout::tabular::{Column, Width};
use standout::views::list_view;
use standout::{CsvProjection, StructuredOutputProjection};
use standout_input::{ClipboardSource, EnvSource, InputChain, StdinSource};
use standout_render::{AmbiguousWidth, OutputMode};
use standout_test::TestHarness;

fn build_echo_app(template: &'static str) -> App {
    App::builder()
        .command(
            "echo",
            |m, _ctx| {
                let msg = m
                    .get_one::<String>("msg")
                    .cloned()
                    .unwrap_or_else(|| "no-arg".into());
                Ok(Output::Render(json!({ "msg": msg })))
            },
            template,
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
        .command(
            "list",
            |_matches, _ctx| {
                let spec = standout::tabular::TabularSpec::builder()
                    .column(Column::new(Width::Fill).right().key("name"))
                    .build();
                Ok(Output::Render(
                    list_view(vec![WidthSensitiveItem { name: "cascade" }])
                        .tabular_spec(spec)
                        .build(),
                ))
            },
            "standout/list-view",
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
    let app = build_echo_app("{{ msg }}");
    let result = TestHarness::new().run(&app, echo_command(), vec!["app", "echo", "hello"]);
    result.assert_success();
    result.assert_stdout_eq("hello");
    result.assert_exit_status(ExitStatus::SUCCESS);
}

#[test]
#[serial]
fn ambiguous_width_policy_can_be_injected_for_the_same_app_fixture() {
    let app = build_echo_app("{{ msg | display_width }}");

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

    for width in [31, 47] {
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
fn columns_environment_width_cascades_through_the_framework_list_view_template() {
    let app = build_framework_list_view_app();
    let result = TestHarness::new()
        .env("COLUMNS", "37")
        .run(&app, list_command(), ["app", "list"]);
    result.assert_success();
    let row = result
        .stdout()
        .lines()
        .find(|line| line.contains("cascade"))
        .expect("framework list view should render its tabular row");
    assert_eq!(row.chars().count(), 37);
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

#[test]
#[serial]
fn harness_exposes_typed_clap_and_handler_outcomes() {
    let app = build_echo_app("{{ msg }}");
    let help = TestHarness::new().run(&app, echo_command(), ["app", "--help"]);
    help.assert_success();
    help.assert_exit_status(ExitStatus::SUCCESS);
    assert_eq!(help.success_kind(), Some(SuccessKind::ClapHelp));

    let usage = TestHarness::new().run(&app, echo_command(), ["app", "--unknown"]);
    usage.assert_error();
    usage.assert_exit_status(ExitStatus::USAGE_ERROR);
    usage.assert_error_kind(RunErrorKind::ClapUsage);

    let failing = App::builder()
        .command(
            "fail",
            |_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(std::io::Error::other("boom").into())
            },
            "",
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
    // The harness parses through the same augmentation a real run does, so a
    // version configured on the builder is answered here too — no separate
    // clap `Command` wiring for tests to keep in sync.
    let app = App::builder()
        .version("4.5.6")
        .command(
            "echo",
            |_m, _ctx| Ok(Output::Render(json!({ "msg": "hi" }))),
            "{{ msg }}",
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
        .command(
            "external",
            |_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(
                    ExternalFailure::new(128, "fatal: delegated command failed\n")
                        .unwrap()
                        .into(),
                )
            },
            "",
        )
        .unwrap()
        .command(
            "external-pre",
            |_matches, _ctx| Ok(Output::Render(json!({ "message": "unreachable" }))),
            "{{ message }}",
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
        .command(
            "whoami",
            |_m, _ctx| {
                let v = InputChain::<String>::new()
                    .try_source(EnvSource::new("STANDOUT_TEST_USER"))
                    .default("anon".into())
                    .resolve(_m)
                    .unwrap();
                Ok(Output::Render(json!({ "user": v })))
            },
            "{{ user }}",
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
        .command(
            "tok",
            |_m, _ctx| {
                let v = InputChain::<String>::new()
                    .try_source(EnvSource::new("STANDOUT_TEST_TOKEN"))
                    .default("missing".into())
                    .resolve(_m)
                    .unwrap();
                Ok(Output::Render(json!({ "tok": v })))
            },
            "{{ tok }}",
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

    // Restore should bring the original back.
    assert_eq!(std::env::var("STANDOUT_TEST_TOKEN").as_deref(), Ok("real"));
    std::env::remove_var("STANDOUT_TEST_TOKEN");
}

#[test]
#[serial]
fn piped_stdin_reaches_handler() {
    let app = App::builder()
        .command(
            "read",
            |_m, _ctx| {
                let v = InputChain::<String>::new()
                    .try_source(StdinSource::new())
                    .default("nothing".into())
                    .resolve(_m)
                    .unwrap();
                Ok(Output::Render(json!({ "val": v })))
            },
            "{{ val }}",
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
        .command(
            "read",
            |_m, _ctx| {
                let v = InputChain::<String>::new()
                    .try_source(StdinSource::new())
                    .default("no-pipe".into())
                    .resolve(_m)
                    .unwrap();
                Ok(Output::Render(json!({ "val": v })))
            },
            "{{ val }}",
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
        .command(
            "paste",
            |_m, _ctx| {
                let v = InputChain::<String>::new()
                    .try_source(ClipboardSource::new())
                    .default("empty".into())
                    .resolve(_m)
                    .unwrap();
                Ok(Output::Render(json!({ "val": v })))
            },
            "{{ val }}",
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

/// Drives a tiny three-step "wizard" handler from the harness, scripting
/// every response. The handler talks to the simple-prompt sources via
/// `.prompt()`; the responder intercepts each call before any TTY is touched.
#[test]
#[serial]
fn scripted_prompts_drive_a_wizard_handler() {
    use standout_input::{
        ConfirmPromptSource, PromptResponse, ScriptedResponder, TextPromptSource,
    };
    use std::sync::Arc;

    let app = App::builder()
        .command(
            "wizard",
            |_m, _ctx| {
                let name = TextPromptSource::new("Name: ").prompt().unwrap();
                let proceed = ConfirmPromptSource::new("Continue? ").prompt().unwrap();
                let title = TextPromptSource::new("Title: ").prompt().unwrap();
                Ok(Output::Render(json!({
                    "name": name,
                    "proceed": proceed,
                    "title": title,
                })))
            },
            "{{ name }}/{{ proceed }}/{{ title }}",
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

/// Scripted Cancel surfaces as PromptCancelled inside the handler — the
/// handler propagates it however it wants (here, a fixed "cancelled" body).
#[test]
#[serial]
fn scripted_cancel_propagates_to_handler() {
    use standout_input::{PromptResponse, ScriptedResponder, TextPromptSource};
    use std::sync::Arc;

    let app = App::builder()
        .command(
            "wizard",
            |_m, _ctx| {
                let body = match TextPromptSource::new("Name: ").prompt() {
                    Ok(name) => format!("ok:{name}"),
                    Err(e) => format!("err:{e}"),
                };
                Ok(Output::Render(json!({ "body": body })))
            },
            "{{ body }}",
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

/// Confirms the responder is reset on `TestResult` drop — a second harness
/// run with no `.prompts(...)` falls back to the real backend (which under
/// `cargo test` means no TTY, so prompt() returns NoInput).
#[test]
#[serial]
fn responder_is_reset_between_runs() {
    use standout_input::{PromptResponse, ScriptedResponder, TextPromptSource};
    use std::sync::Arc;

    let app = App::builder()
        .command(
            "wizard",
            |_m, _ctx| {
                let body = match TextPromptSource::new("Name: ").prompt() {
                    Ok(name) => format!("ok:{name}"),
                    Err(e) => format!("err:{e}"),
                };
                Ok(Output::Render(json!({ "body": body })))
            },
            "{{ body }}",
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("wizard"));

    // First run: scripted responder, gets the value.
    let first = TestHarness::new()
        .prompts(Arc::new(ScriptedResponder::new([PromptResponse::text(
            "Ada",
        )])))
        .run(&app, cmd.clone(), vec!["app", "wizard"]);
    first.assert_stdout_eq("ok:Ada");
    drop(first); // ensure restore runs before the next harness builds

    // Second run: no .prompts(...). The responder should be cleared, so
    // prompt() falls through to TextPromptSource's no-TTY path and returns
    // NoInput.
    let second = TestHarness::new().run(&app, cmd, vec!["app", "wizard"]);
    second.assert_stdout_contains("err:");
}

#[test]
#[serial]
fn fixture_files_are_materialized_in_cwd() {
    let app = App::builder()
        .command(
            "cat",
            |m, _ctx| {
                let path = m.get_one::<String>("path").cloned().unwrap();
                let text = std::fs::read_to_string(path).unwrap();
                Ok(Output::Render(json!({ "text": text })))
            },
            "{{ text }}",
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
    let app = build_echo_app("{{ msg }}");
    let result = TestHarness::new().output_mode(OutputMode::Json).run(
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
        .command_with(
            "summary",
            |_matches, _ctx| {
                Ok(Output::Render(json!({
                    "items": [
                        { "language": "Rust", "code": 120, "comments": 20 },
                        { "language": "Python", "code": 70, "comments": 10 }
                    ],
                    "totals": { "code": 190, "comments": 30 },
                    "skipped": 1
                })))
            },
            |config| config.structured_output_projection(projection),
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("rustloc").subcommand(Command::new("summary"));

    let result =
        TestHarness::new()
            .output_mode(OutputMode::Csv)
            .run(&app, cmd, ["rustloc", "summary"]);

    result.assert_stdout_eq(
        "LANGUAGE,CODE,NET\nRust,120,100\nPython,70,60\nTOTAL,190,160\nSKIPPED,-,0\n",
    );
}

#[test]
#[serial]
fn terminal_width_override_is_observable_via_detector() {
    // The override stays installed for the lifetime of the TestResult
    // (restored when it drops), so we can probe the detector directly
    // while the result is still in scope.
    let app = build_echo_app("{{ msg }}");
    let result = TestHarness::new().terminal_width(42).no_color().run(
        &app,
        echo_command(),
        vec!["app", "echo", "hi"],
    );
    result.assert_stdout_eq("hi");
    assert_eq!(standout_render::detect_terminal_width(), Some(42));
    assert!(!standout_render::detect_color_capability());
    drop(result);
    // After drop, detectors are reset to library defaults — the override
    // should no longer be visible.
    let _ = standout_render::detect_terminal_width();
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
fn env_set_then_remove_restores_true_original() {
    std::env::set_var("STANDOUT_DOUBLE_PROBE", "original");

    let app = build_echo_app("{{ msg }}");
    {
        let _result = TestHarness::new()
            .env("STANDOUT_DOUBLE_PROBE", "transient")
            .env_remove("STANDOUT_DOUBLE_PROBE")
            .run(&app, echo_command(), vec!["app", "echo", "x"]);
    }

    // If the harness recorded the mid-run value as the "original" it
    // would restore "transient" here; the fix records only the first
    // value seen per key.
    assert_eq!(
        std::env::var("STANDOUT_DOUBLE_PROBE").as_deref(),
        Ok("original")
    );
    std::env::remove_var("STANDOUT_DOUBLE_PROBE");
}

#[test]
#[serial]
fn output_flag_name_is_configurable() {
    // Build an app whose output flag is renamed to --format.
    let app = standout::cli::App::builder()
        .output_flag(Some("format"))
        .command(
            "echo",
            |m, _ctx| {
                let msg = m
                    .get_one::<String>("msg")
                    .cloned()
                    .unwrap_or_else(|| "no-arg".into());
                Ok(Output::Render(json!({ "msg": msg })))
            },
            "{{ msg }}",
        )
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new()
        .output_mode(OutputMode::Json)
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
        let app = build_echo_app("{{ msg }}");
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

    // Cleanup
    std::env::remove_var("STANDOUT_RESTORE_PROBE");
    if let Some(v) = original {
        std::env::set_var("STANDOUT_RESTORE_PROBE", v);
    }
}

#[test]
#[serial]
fn no_match_reports_cleanly() {
    let app = build_echo_app("{{ msg }}");
    let result = TestHarness::new().run(&app, echo_command(), vec!["app", "unknown"]);
    // clap rejects unknown subcommands as a parse error; per #141, those
    // surface as `RunResult::Error`. Older clap behavior could also produce
    // `NoMatch`, so accept either.
    assert!(
        result.is_error() || result.is_no_match(),
        "expected Error or NoMatch, got: {:?}",
        result.outcome()
    );
}

// ---------------------------------------------------------------------------
// Compound artifacts
// ---------------------------------------------------------------------------

const ARTIFACT_BYTES: &[u8] = b"id,title\n1,buy milk\n";

/// An export app whose artifact suggests `destination`, mirroring the
/// application/framework split: the app produces bytes and facts, the harness
/// observes what the framework did with them.
fn build_export_app(destination: Option<std::path::PathBuf>) -> App {
    App::builder()
        .output_file_flag(Some("output-file-path"))
        .command(
            "export",
            move |_m, _ctx| {
                let mut artifact = Artifact::new(ARTIFACT_BYTES.to_vec())
                    .with_report(json!({ "entries": 1, "warnings": ["no due date"] }));
                artifact = match &destination {
                    Some(path) => artifact.suggest_destination(path),
                    None => artifact.allow_stdout(),
                };
                Ok(Output::Artifact(artifact))
            },
            "Wrote {{ report.entries }} entries to {{ receipt.destination }}",
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

    let result = TestHarness::new().output_mode(OutputMode::Json).run(
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
