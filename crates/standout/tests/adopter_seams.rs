
use clap::{Arg, ArgAction, Command};
use serde_json::json;
use standout::cli::{
    App, AppFailure, ExitStatus, FnHandler, HandlerResult, HookError, HookPhase, Hooks, Output,
    RunErrorKind,
};
use standout::ColorPolicy;
use standout::{EmbeddedTemplates, Representation};
use standout_test::{serial, TestHarness};

const TEMPLATES: &[(&str, &str)] = &[("status", "unit {{ unit }} is {{ state }}")];

fn systemdlike(fallback: Representation) -> App {
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
        &systemdlike(Representation::Json),
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
        &systemdlike(Representation::Json),
        systemdlike_command(),
        ["systemdlike", "status", "--output", "yaml"],
    );

    result.assert_success();
    result.assert_stdout_contains("state: active");
}

#[test]
#[serial]
fn the_default_fallback_is_unchanged_for_an_app_that_sets_none() {
    let result = TestHarness::new().run(
        &systemdlike(Representation::Human),
        systemdlike_command(),
        ["systemdlike", "status"],
    );

    result.assert_success();
    result.assert_stdout_eq("unit web is active");
}

#[test]
#[serial]
fn both_help_spellings_render_the_human_page_on_a_terminal() {
    let app = systemdlike(Representation::Human);

    let word = TestHarness::new().color_capable_terminal().run(
        &app,
        systemdlike_command(),
        ["systemdlike", "help"],
    );
    let flag = TestHarness::new().color_capable_terminal().run(
        &app,
        systemdlike_command(),
        ["systemdlike", "--help"],
    );
    let piped = TestHarness::new().run(&app, systemdlike_command(), ["systemdlike", "--help"]);

    assert!(
        word.stdout().contains("\x1b["),
        "`help` should color on a terminal, got {:?}",
        word.stdout()
    );
    assert!(
        flag.stdout().contains("\x1b["),
        "`--help` should color on a terminal, got {:?}",
        flag.stdout()
    );
    assert!(
        !piped.stdout().contains("\x1b["),
        "the same page through a pipe carries no escapes, got {:?}",
        piped.stdout()
    );
}

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
        .color(ColorPolicy::Never)
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

fn framed_failure_app(status: u8, diagnostic: &'static str) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "view",
            FnHandler::new(move |_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(AppFailure::new(status, diagnostic).unwrap().framed().into())
            }),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap()
}

#[test]
#[serial]
fn a_framed_domain_error_keeps_its_status_and_takes_the_handler_framing() {
    let result = TestHarness::new().run(
        &framed_failure_app(2, "proiectio: drift detected"),
        view_command(),
        ["proiectio", "view"],
    );

    result.assert_error();
    result.assert_error_kind(RunErrorKind::App);
    assert_eq!(result.exit_status().map(ExitStatus::code), Some(2));
    result.assert_stdout_eq("");
    result.assert_stderr_eq("Error: proiectio: drift detected\n");
}

#[test]
#[serial]
fn a_framed_domain_error_is_a_stdout_document_with_stderr_silent() {
    use standout::cli::DiagnosticKind;

    let result = TestHarness::new().run(
        &framed_failure_app(2, "proiectio: drift detected"),
        view_command(),
        ["proiectio", "view", "--output", "json"],
    );

    result.assert_error();
    result.assert_error_kind(RunErrorKind::App);
    assert_eq!(result.exit_status().map(ExitStatus::code), Some(2));
    result.assert_stderr_empty();

    let diagnostic = result.expect_diagnostic();
    assert_eq!(diagnostic.kind, DiagnosticKind::App);
    assert_eq!(diagnostic.summary, "proiectio: drift detected");
    assert_eq!(diagnostic.detail, "");
}

#[test]
#[serial]
fn an_unframed_domain_error_keeps_its_verbatim_stderr_under_both_representations() {
    use standout::cli::DiagnosticKind;

    let human = TestHarness::new().run(
        &app_owned_failure_app(2, "proiectio: drift detected\n"),
        view_command(),
        ["proiectio", "view"],
    );
    human.assert_error();
    assert_eq!(human.exit_status().map(ExitStatus::code), Some(2));
    human.assert_stdout_eq("");
    human.assert_stderr_eq("proiectio: drift detected\n");

    let structured = TestHarness::new().run(
        &app_owned_failure_app(2, "proiectio: drift detected\n"),
        view_command(),
        ["proiectio", "view", "--output", "json"],
    );
    structured.assert_error();
    assert_eq!(structured.exit_status().map(ExitStatus::code), Some(2));
    structured.assert_stderr_eq("proiectio: drift detected\n");

    let diagnostic = structured.expect_diagnostic();
    assert_eq!(diagnostic.kind, DiagnosticKind::App);
    assert_eq!(diagnostic.summary, "proiectio: drift detected");
    assert_eq!(diagnostic.detail, "proiectio: drift detected\n");
}

#[test]
#[serial]
fn a_framed_domain_error_carries_no_terminal_escape_sequence() {
    let result = TestHarness::new().run(
        &framed_failure_app(2, "proiectio: \u{1b}]0;pwned\u{7}drift"),
        view_command(),
        ["proiectio", "view"],
    );

    result.assert_error();
    result.assert_stderr_eq("Error: proiectio: \\u{1b}]0;pwned\\u{7}drift\n");
}

fn usage_status_app(usage_exit_status: Option<u8>) -> App {
    let mut builder = App::builder().templates(EmbeddedTemplates::new(TEMPLATES, ""));
    if let Some(status) = usage_exit_status {
        builder = builder.usage_exit_status(status);
    }
    builder
        .command_with(
            "view",
            FnHandler::new(|_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(anyhow::anyhow!("proiectio: could not read the archive"))
            }),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap()
}

#[test]
#[serial]
fn a_clap_rejection_exits_two_for_an_app_that_names_no_usage_status() {
    let result = TestHarness::new().run(
        &usage_status_app(None),
        view_command(),
        ["proiectio", "view", "--unknown"],
    );

    result.assert_error();
    result.assert_error_kind(RunErrorKind::ClapUsage);
    result.assert_exit_status(ExitStatus::USAGE_ERROR);
}

#[test]
#[serial]
fn a_clap_rejection_exits_with_the_status_the_app_named() {
    let result = TestHarness::new().run(
        &usage_status_app(Some(1)),
        view_command(),
        ["proiectio", "view", "--unknown"],
    );

    result.assert_error();
    result.assert_error_kind(RunErrorKind::ClapUsage);
    result.assert_exit_status(ExitStatus::FAILURE);
    assert!(
        result.stderr().contains("unexpected argument '--unknown'"),
        "the clap prose is unchanged, got {:?}",
        result.stderr()
    );
}

#[test]
#[serial]
fn a_named_usage_status_leaves_every_other_failure_alone() {
    let result = TestHarness::new().run(
        &usage_status_app(Some(2)),
        view_command(),
        ["proiectio", "view"],
    );

    result.assert_error();
    result.assert_error_kind(RunErrorKind::Handler);
    result.assert_exit_status(ExitStatus::FAILURE);
}

#[test]
#[serial]
fn a_named_usage_status_reaches_the_stdout_document_too() {
    use standout::cli::DiagnosticKind;

    let result = TestHarness::new().run(
        &usage_status_app(Some(1)),
        view_command(),
        ["proiectio", "view", "--unknown", "--output", "json"],
    );

    result.assert_error();
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_stderr_empty();
    assert_eq!(result.expect_diagnostic().kind, DiagnosticKind::ClapUsage);
}

#[test]
fn a_usage_status_of_zero_refuses_to_build() {
    let built = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .usage_exit_status(0)
        .build();

    let Err(error) = built else {
        panic!("a usage status of zero must not build");
    };
    assert!(
        error.to_string().contains("usage_exit_status(0)"),
        "got {error}"
    );
}
