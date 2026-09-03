//! A declared exit status is a signal beside the result, never a failure: the document
//! still goes to stdout and nothing becomes a diagnostic.

use clap::{Arg, ArgAction, Command};
use serde_json::json;
use serial_test::serial;
use standout::cli::{
    App, CommandContext, Diagnostic, ExitStatus, FnHandler, HandlerResult, Output, RunErrorKind,
    SuccessKind,
};
use standout::views::{list_view, ListViewResult};
use standout::{EmbeddedTemplates, Representation};
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[
    ("plan", "{{ changes }} to add"),
    ("list", "{{ items | length }} items"),
];

fn command() -> Command {
    Command::new("app")
        .subcommand(
            Command::new("plan")
                .arg(Arg::new("config").long("config").required(true))
                .arg(
                    Arg::new("detailed-exitcode")
                        .long("detailed-exitcode")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("list").arg(Arg::new("empty").long("empty").action(ArgAction::SetTrue)),
        )
        .subcommand(Command::new("binary-signal"))
}

fn plan(matches: &clap::ArgMatches, _ctx: &CommandContext) -> HandlerResult<serde_json::Value> {
    let changes = match matches.get_one::<String>("config").unwrap().as_str() {
        "none" => 0,
        "two" => 2,
        _ => {
            return Err(Diagnostic::error("config line 2 does not parse")
                .detail("expected `resource <name> <state>`")
                .range("main.tfl", 2, 1)
                .into())
        }
    };
    let output = Output::Render(json!({ "changes": changes }));
    if matches.get_flag("detailed-exitcode") && changes > 0 {
        Ok(output.with_exit_status(ExitStatus::from(2)))
    } else {
        Ok(output)
    }
}

fn list(matches: &clap::ArgMatches, _ctx: &CommandContext) -> HandlerResult<ListViewResult<u32>> {
    let items = if matches.get_flag("empty") {
        vec![]
    } else {
        vec![1, 2]
    };
    Ok(list_view(items).empty_exit_status(3).output())
}

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with("plan", FnHandler::new(plan), |cfg| cfg)
        .unwrap()
        .command_with("list", FnHandler::new(list), |cfg| cfg)
        .unwrap()
        .command_with(
            "binary-signal",
            FnHandler::new(|_, _| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![1, 2, 3],
                    filename: "out.bin".into(),
                }
                .with_exit_status(ExitStatus::from(2)))
            }),
            |cfg| cfg.binary(),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn run(mode: Representation, args: &[&str]) -> standout_test::TestResult {
    TestHarness::new()
        .output_mode(mode)
        .run(&app(), command(), args)
}

const MODES: [Representation; 3] = [
    Representation::Human,
    Representation::Json,
    Representation::Csv,
];

#[test]
#[serial]
fn a_plan_with_nothing_to_do_exits_zero() {
    for mode in MODES {
        let result = run(
            mode,
            &["app", "plan", "--config", "none", "--detailed-exitcode"],
        );
        result.assert_success();
        result.assert_exit_status(ExitStatus::SUCCESS);
        result.assert_stderr_empty();
        assert_eq!(
            result.success_kind(),
            Some(SuccessKind::Command),
            "{mode:?}"
        );
        assert!(
            result.stdout().contains('0'),
            "{mode:?}: {}",
            result.stdout()
        );
    }
}

#[test]
#[serial]
fn a_plan_with_changes_exits_two_and_is_still_a_success() {
    for mode in MODES {
        let result = run(
            mode,
            &["app", "plan", "--config", "two", "--detailed-exitcode"],
        );
        result.assert_success();
        result.assert_exit_status(ExitStatus::from(2));
        result.assert_stderr_empty();
        assert_eq!(result.error_kind(), None, "{mode:?}");
        assert!(
            result.diagnostic().is_none(),
            "{mode:?}: a declared status is not a diagnostic:\n{}",
            result.stdout()
        );
        match mode {
            Representation::Human => result.assert_stdout_eq("2 to add"),
            Representation::Json => assert_eq!(
                serde_json::from_str::<serde_json::Value>(result.stdout()).unwrap(),
                json!({ "changes": 2 })
            ),
            _ => result.assert_stdout_eq("changes\n2\n"),
        }
    }

    let undeclared = run(Representation::Human, &["app", "plan", "--config", "two"]);
    undeclared.assert_exit_status(ExitStatus::SUCCESS);
    undeclared.assert_stdout_eq("2 to add");
}

#[test]
#[serial]
fn a_failed_plan_exits_one() {
    for mode in MODES {
        let result = run(
            mode,
            &["app", "plan", "--config", "broken", "--detailed-exitcode"],
        );
        result.assert_error_kind(RunErrorKind::Handler);
        result.assert_exit_status(ExitStatus::FAILURE);
        if mode == Representation::Human {
            result.assert_stderr_contains("main.tfl:2:1: config line 2 does not parse");
        } else {
            result.assert_stderr_empty();
            let diagnostic = result.expect_diagnostic();
            assert_eq!(diagnostic.summary, "config line 2 does not parse");
            assert_eq!(diagnostic.range.unwrap().start.line, 2);
        }
    }
}

#[test]
#[serial]
fn a_list_declares_its_empty_status_only_when_empty() {
    let empty = run(Representation::Json, &["app", "list", "--empty"]);
    empty.assert_success();
    empty.assert_exit_status(ExitStatus::from(3));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(empty.stdout()).unwrap(),
        json!({ "schema_version": 1, "items": [] })
    );

    let filled = run(Representation::Human, &["app", "list"]);
    filled.assert_exit_status(ExitStatus::SUCCESS);
    filled.assert_stdout_eq("2 items");
}

#[test]
#[serial]
fn a_status_on_binary_output_is_a_render_error() {
    let result = run(Representation::Human, &["app", "binary-signal"]);
    result.assert_error_kind(RunErrorKind::Render);
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_contains("exit status 2 was declared on binary output");
}
