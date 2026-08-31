//! Dispatch seams an application reaches for: the output-mode fallback, an
//! app-owned exit status and diagnostic, hook diagnostic framing, and the
//! matches a hook receives.

use clap::{Arg, ArgAction, Command};
use serde_json::json;
use standout::cli::{
    App, AppFailure, ExitStatus, FnHandler, HandlerResult, HookError, HookPhase, Hooks, Output,
    RunErrorKind,
};
use standout::{EmbeddedTemplates, OutputMode};
use standout_test::{serial, TestHarness};

const TEMPLATES: &[(&str, &str)] = &[("status", "unit {{ unit }} is {{ state }}")];

// The app decides the mode used when `--output` is absent.

fn systemdlike(fallback: OutputMode) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .output_mode_fallback(fallback)
        .command_with(
            "status",
            FnHandler::new(|_matches, _ctx| {
                Ok(Output::Render(json!({ "unit": "web", "state": "active" })))
            }),
            |cfg| cfg.template_name("status"),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn systemdlike_command() -> Command {
    Command::new("systemdlike").subcommand(Command::new("status"))
}

#[test]
#[serial]
fn the_app_fallback_decides_the_mode_when_the_flag_is_absent() {
    let result = TestHarness::new().run(
        &systemdlike(OutputMode::Json),
        systemdlike_command(),
        ["systemdlike", "status"],
    );

    result.assert_success();
    result.assert_stdout_contains("\"state\": \"active\"");
}

#[test]
#[serial]
fn an_explicit_output_flag_outranks_the_app_fallback() {
    let result = TestHarness::new().run(
        &systemdlike(OutputMode::Json),
        systemdlike_command(),
        ["systemdlike", "status", "--output", "text"],
    );

    result.assert_success();
    result.assert_stdout_eq("unit web is active");
}

#[test]
#[serial]
fn the_default_fallback_is_unchanged_for_an_app_that_sets_none() {
    let result = TestHarness::new().run(
        &systemdlike(OutputMode::Auto),
        systemdlike_command(),
        ["systemdlike", "status"],
    );

    result.assert_success();
    result.assert_stdout_eq("unit web is active");
}

#[test]
#[serial]
fn both_help_spellings_render_in_the_app_fallback_mode() {
    let app = systemdlike(OutputMode::Term);

    let word = TestHarness::new().run(&app, systemdlike_command(), ["systemdlike", "help"]);
    let flag = TestHarness::new().run(&app, systemdlike_command(), ["systemdlike", "--help"]);
    let default_app = TestHarness::new().run(
        &systemdlike(OutputMode::Auto),
        systemdlike_command(),
        ["systemdlike", "--help"],
    );

    assert!(
        word.stdout().contains("\x1b["),
        "`help` should render in the Term fallback, got {:?}",
        word.stdout()
    );
    assert!(
        flag.stdout().contains("\x1b["),
        "`--help` should render in the Term fallback, got {:?}",
        flag.stdout()
    );
    assert!(
        !default_app.stdout().contains("\x1b["),
        "an app that sets no fallback keeps the Auto help, got {:?}",
        default_app.stdout()
    );
}

// A domain error owning both its exit status and its exact stderr line.

fn app_owned_failure_app(status: u8, diagnostic: &'static str) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "view",
            FnHandler::new(move |_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(AppFailure::new(status, diagnostic).unwrap().into())
            }),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn view_command() -> Command {
    Command::new("ghlike").subcommand(Command::new("view"))
}

#[test]
#[serial]
fn a_domain_error_carries_its_own_status_and_verbatim_stderr() {
    let result = TestHarness::new().run(
        &app_owned_failure_app(1, "ghlike: repository not found: demo/gamma\n"),
        view_command(),
        ["ghlike", "view"],
    );

    result.assert_error();
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_kind(RunErrorKind::App);
    result.assert_stdout_eq("");
    result.assert_stderr_eq("ghlike: repository not found: demo/gamma\n");
}

#[test]
#[serial]
fn a_domain_error_can_claim_any_nonzero_status() {
    let result = TestHarness::new().run(
        &app_owned_failure_app(3, "fatal: not a valid object name\n"),
        view_command(),
        ["ghlike", "view"],
    );

    result.assert_error();
    assert_eq!(result.exit_status().map(ExitStatus::code), Some(3));
    result.assert_stderr_eq("fatal: not a valid object name\n");
}

#[test]
#[serial]
fn a_domain_error_can_never_report_shell_success() {
    assert!(AppFailure::new(0, "").is_err());

    let result = TestHarness::new().run(
        &app_owned_failure_app(1, "ghlike: repository not found: demo/gamma\n"),
        view_command(),
        ["ghlike", "view"],
    );

    result.assert_error();
    assert_ne!(result.exit_status(), Some(ExitStatus::SUCCESS));
}

#[test]
#[serial]
fn a_pre_dispatch_guard_reaches_the_same_app_owned_seam() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "view",
            FnHandler::new(|_matches, _ctx| Ok(Output::Render(json!({ "unit": "unreachable" })))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .hooks(
            "view",
            Hooks::new().pre_dispatch(|_, _| {
                Err(HookError::pre_dispatch_app(
                    AppFailure::new(4, "ghlike: not authenticated\n").unwrap(),
                ))
            }),
        )
        .build()
        .unwrap();

    let result = TestHarness::new().run(&app, view_command(), ["ghlike", "view"]);

    result.assert_error();
    assert_eq!(result.exit_status().map(ExitStatus::code), Some(4));
    result.assert_error_kind(RunErrorKind::App);
    result.assert_stderr_eq("ghlike: not authenticated\n");
}

// A hook diagnostic is framed once, not twice.

#[test]
#[serial]
fn a_hook_diagnostic_is_framed_once() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "provision",
            FnHandler::new(|_matches, _ctx| Ok(Output::Render(json!({ "unit": "unreachable" })))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .hooks(
            "provision",
            Hooks::new().pre_dispatch(|_, _| {
                Err(HookError::pre_dispatch(
                    "questionnaire input `questionnaire`: Validation failed: answers required",
                ))
            }),
        )
        .build()
        .unwrap();

    let result = TestHarness::new().run(
        &app,
        Command::new("formlike").subcommand(Command::new("provision")),
        ["formlike", "provision"],
    );

    result.assert_error();
    result.assert_error_kind(RunErrorKind::Hook(HookPhase::PreDispatch));
    result.assert_stderr_eq(
        "Error: hook error (pre-dispatch): questionnaire input `questionnaire`: \
         Validation failed: answers required\n",
    );
}

// A hook reads the subcommand's own flags.

#[test]
#[serial]
fn hooks_read_the_command_s_own_matches() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "provision",
            FnHandler::new(|matches, _ctx| {
                Ok(Output::Render(json!({
                    "unit": matches.get_one::<String>("answers").cloned().unwrap_or_default(),
                    "state": "active",
                })))
            }),
            |cfg| cfg.template_name("status"),
        )
        .unwrap()
        .hooks(
            "provision",
            Hooks::new()
                .pre_dispatch(|matches, _ctx| match matches.get_one::<String>("answers") {
                    Some(_) => Ok(()),
                    None => Err(HookError::pre_dispatch("an answer source is required")),
                })
                .post_output(|matches, _ctx, output| {
                    matches
                        .get_one::<String>("answers")
                        .map(|_| output)
                        .ok_or_else(|| HookError::post_output("post-output lost the matches"))
                }),
        )
        .build()
        .unwrap();

    let command = Command::new("formlike").subcommand(
        Command::new("provision").arg(Arg::new("answers").long("answers").action(ArgAction::Set)),
    );

    let accepted = TestHarness::new().run(
        &app,
        command.clone(),
        ["formlike", "provision", "--answers", "sheet.txt"],
    );
    accepted.assert_success();
    accepted.assert_stdout_eq("unit sheet.txt is active");

    let refused = TestHarness::new().run(&app, command, ["formlike", "provision"]);
    refused.assert_error();
    refused.assert_stderr_eq("Error: hook error (pre-dispatch): an answer source is required\n");
}

#[test]
#[serial_test::serial(questionnaire)]
fn questionnaire_resolution_runs_where_its_call_sits_in_the_hook_chain() {
    use standout::cli::{CommandContext, CommandContextInput};
    use standout::input::questionnaire::QuestionnaireInput;
    use standout_fixtures::derive_surface::ProvisionAnswers;
    use std::cell::RefCell;
    use std::rc::Rc;

    let seen: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let before = seen.clone();
    let after = seen.clone();

    let resolved = |ctx: &CommandContext| ctx.questionnaire::<ProvisionAnswers>().is_ok();

    let app = App::builder()
        .templates(EmbeddedTemplates::new(&[("provision", "{{ host }}")], ""))
        .command_with(
            "provision",
            FnHandler::new(|_matches, ctx: &CommandContext| {
                let answers: &ProvisionAnswers = ctx.questionnaire()?;
                Ok(Output::Render(json!({ "host": answers.host })))
            }),
            move |cfg| {
                let (before, after) = (before.clone(), after.clone());
                cfg.template_name("provision")
                    .pre_dispatch(move |_, ctx| {
                        before.borrow_mut().push(if resolved(ctx) {
                            "before: yes"
                        } else {
                            "before: no"
                        });
                        Ok(())
                    })
                    .questionnaire::<ProvisionAnswers>()
                    .pre_dispatch(move |_, ctx| {
                        after.borrow_mut().push(if resolved(ctx) {
                            "after: yes"
                        } else {
                            "after: no"
                        });
                        Ok(())
                    })
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let sheet = ProvisionAnswers::questionnaire()
        .unwrap()
        .render_answer_sheet()
        .replace("\nlocalhost\n", "\ndb-1\n");

    let result = TestHarness::new()
        .text_output()
        .fixture("answers.txt", sheet)
        .run(
            &app,
            Command::new("provisionctl").subcommand(Command::new("provision")),
            [
                "provisionctl",
                "provision",
                "--answers",
                "answers.txt",
                "--yes",
            ],
        );

    result.assert_success();
    result.assert_stdout_eq("db-1");
    assert_eq!(&*seen.borrow(), &["before: no", "after: yes"]);
}
