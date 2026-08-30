use clap::{Arg, Command, Subcommand};
use serde_json::json;
use serial_test::serial;
use standout::cli::{
    App, CommandContext, CommandContextInput, Dispatch, DispatchResult, HandlerResult, Output,
};
use standout::input::questionnaire::QuestionnaireInput;
use standout::input::{DefaultSource, InputChain, PromptResponse, ScriptedResponder};
use standout_test::{TestHarness, TestResult};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
#[derive(Debug, Clone, PartialEq, Eq, standout::Questionnaire)]
#[question(id = "fixture.profile")]
struct FixtureAnswers {
    name: String,
}
#[derive(Clone)]
struct Calls(Arc<AtomicUsize>);
mod handlers {
    use super::*;
    pub fn collect(
        _matches: &clap::ArgMatches,
        ctx: &CommandContext,
    ) -> HandlerResult<serde_json::Value> {
        let calls = ctx.app_state.get_required::<Calls>()?;
        calls.0.fetch_add(1, Ordering::SeqCst);
        let answers: &FixtureAnswers = ctx.questionnaire()?;
        Ok(Output::Render(json!({ "name": answers.name })))
    }
    pub fn other(
        _matches: &clap::ArgMatches,
        ctx: &CommandContext,
    ) -> HandlerResult<serde_json::Value> {
        let calls = ctx.app_state.get_required::<Calls>()?;
        calls.0.fetch_add(1, Ordering::SeqCst);
        Ok(Output::Render(json!({ "name": "other" })))
    }
}
#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(questionnaire = FixtureAnswers, template = "{{ name }}")]
    Collect,
    #[dispatch(template = "{{ name }}")]
    Other,
}
fn command() -> Command {
    Command::new("fixture")
        .subcommand_required(true)
        .subcommand(Command::new("collect"))
        .subcommand(Command::new("other"))
}
fn derived_app(calls: Arc<AtomicUsize>) -> standout::cli::App {
    App::builder()
        .app_state(Calls(calls))
        .commands(Commands::dispatch_config())
        .unwrap()
        .build()
        .unwrap()
}
fn builder_app(calls: Arc<AtomicUsize>) -> standout::cli::App {
    App::builder()
        .app_state(Calls(calls))
        .command_with("collect", handlers::collect, |cfg| {
            cfg.template("{{ name }}").questionnaire::<FixtureAnswers>()
        })
        .unwrap()
        .command("other", handlers::other, "{{ name }}")
        .unwrap()
        .build()
        .unwrap()
}
fn answered_sheet(name: &str) -> String {
    FixtureAnswers::questionnaire()
        .unwrap()
        .render_answer_sheet()
        .replace("<id:name>\n", &format!("<id:name>\n{name}\n"))
}
fn error_text(result: &TestResult) -> String {
    match result.outcome() {
        DispatchResult::Error(error) => error.to_string(),
        other => panic!("expected error, got {other:?}"),
    }
}
#[test]
#[serial(questionnaire)]
fn derive_injects_answers_flag_and_yes_gate() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls.clone());
    let result = TestHarness::new()
        .fixture("answers.txt", answered_sheet("from-file"))
        .run(
            &app,
            command(),
            ["fixture", "collect", "--answers", "answers.txt", "--yes"],
        );
    result.assert_success();
    assert_eq!(result.stdout(), "from-file");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
#[serial(questionnaire)]
fn builder_config_injects_equivalent_answers_surface() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = builder_app(calls.clone());
    let result = TestHarness::new()
        .fixture("answers.txt", answered_sheet("from-builder"))
        .run(
            &app,
            command(),
            ["fixture", "collect", "--answers", "answers.txt", "--yes"],
        );
    result.assert_success();
    assert_eq!(result.stdout(), "from-builder");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
#[serial(questionnaire)]
fn stdin_answers_decode_through_shared_pipeline() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls);
    let result = TestHarness::new()
        .piped_stdin(answered_sheet("from-stdin"))
        .run(
            &app,
            command(),
            ["fixture", "collect", "--answers", "-", "--yes"],
        );
    result.assert_success();
    assert_eq!(result.stdout(), "from-stdin");
}
#[test]
#[serial(questionnaire)]
fn answer_sheet_warnings_are_captured_and_do_not_leak() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls);
    let warned = TestHarness::new()
        .fixture(
            "answers.txt",
            answered_sheet("name with <id:fragment> suffix"),
        )
        .run(
            &app,
            command(),
            ["fixture", "collect", "--answers", "answers.txt", "--yes"],
        );
    warned.assert_success();
    warned.assert_warning_contains("answer sheet answers.txt");
    warned.assert_warning_contains("name");
    let clean = TestHarness::new()
        .prompts(Arc::new(ScriptedResponder::new([PromptResponse::text(
            "interactive",
        )])))
        .run(&app, command(), ["fixture", "collect", "--yes"]);
    clean.assert_success();
    assert!(clean.warnings().is_empty(), "{:?}", clean.warnings());
}
#[test]
#[serial(questionnaire)]
fn terminal_stdin_is_rejected_for_explicit_answers_dash() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls.clone());
    let result = TestHarness::new().interactive_stdin().run(
        &app,
        command(),
        ["fixture", "collect", "--answers", "-", "--yes"],
    );
    assert!(result.stdout().is_empty());
    let error = error_text(&result);
    assert!(
        error.contains("stdin is an interactive terminal"),
        "{error}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
#[serial(questionnaire)]
fn invalid_answer_sheet_does_not_fall_back_to_prompts() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls.clone());
    let result = TestHarness::new()
        .fixture("answers.txt", "")
        .prompts(Arc::new(ScriptedResponder::new([PromptResponse::text(
            "fallback",
        )])))
        .run(
            &app,
            command(),
            ["fixture", "collect", "--answers", "answers.txt", "--yes"],
        );
    let error = error_text(&result);
    assert!(error.contains("answer sheet answers.txt has"), "{error}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
#[serial(questionnaire)]
fn no_flags_collects_interactively_before_handler() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls);
    let result = TestHarness::new()
        .prompts(Arc::new(ScriptedResponder::new([PromptResponse::text(
            "interactive",
        )])))
        .run(&app, command(), ["fixture", "collect", "--yes"]);
    result.assert_success();
    assert_eq!(result.stdout(), "interactive");
}
#[test]
#[serial(questionnaire)]
fn questions_renders_blank_sheet_without_running_handler() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls.clone());
    let result = TestHarness::new().run(&app, command(), ["fixture", "collect", "questions"]);
    result.assert_success();
    assert!(result
        .stdout()
        .contains("#! questionnaire: fixture.profile"));
    assert!(result.stdout().contains("<id:name>"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
#[serial(questionnaire)]
fn questions_writes_file_without_running_handler() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls.clone());
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("questions.txt");
    let output_arg = output.to_str().unwrap();
    let result = TestHarness::new().run(
        &app,
        command(),
        ["fixture", "collect", "questions", "--file", output_arg],
    );
    result.assert_success();
    assert_eq!(result.stdout(), "");
    let written = std::fs::read_to_string(output).unwrap();
    assert!(written.contains("#! questionnaire: fixture.profile"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
#[serial(questionnaire)]
fn questions_rejects_answers_and_yes_combination() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls.clone());
    let result = TestHarness::new()
        .fixture("answers.txt", answered_sheet("x"))
        .run(
            &app,
            command(),
            [
                "fixture",
                "collect",
                "--answers",
                "answers.txt",
                "--yes",
                "questions",
            ],
        );
    let error = error_text(&result);
    assert!(
        error.contains("cannot be combined with --answers or --yes"),
        "{error}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
#[serial(questionnaire)]
fn confirmation_gate_rejects_missing_attended_terminal() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls.clone());
    let result = TestHarness::new()
        .fixture("answers.txt", answered_sheet("from-file"))
        .env("STANDOUT_QUESTIONNAIRE_TERMINAL", "absent")
        .run(
            &app,
            command(),
            ["fixture", "collect", "--answers", "answers.txt"],
        );
    let error = error_text(&result);
    assert!(
        error.contains("confirmation requires an attended terminal"),
        "{error}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
#[serial(questionnaire)]
fn confirmation_gate_accepts_scripted_attended_yes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls);
    let harness = TestHarness::new()
        .fixture("answers.txt", answered_sheet("confirmed"))
        .fixture("terminal.txt", "yes\n");
    let terminal = harness.tempdir().unwrap().join("terminal.txt");
    let terminal_arg = terminal.to_str().unwrap().to_string();
    let result = harness
        .env("STANDOUT_QUESTIONNAIRE_TERMINAL", terminal_arg)
        .run(
            &app,
            command(),
            ["fixture", "collect", "--answers", "answers.txt"],
        );
    result.assert_success();
    assert_eq!(result.stdout(), "confirmed");
}
#[test]
#[serial(questionnaire)]
fn non_questionnaire_command_is_unaffected() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = derived_app(calls.clone());
    let ok = TestHarness::new().run(&app, command(), ["fixture", "other"]);
    ok.assert_success();
    assert_eq!(ok.stdout(), "other");
    let unknown = TestHarness::new().run(&app, command(), ["fixture", "other", "--answers", "x"]);
    let error = error_text(&unknown);
    assert!(error.contains("unexpected argument '--answers'"), "{error}");
}
#[test]
fn reserved_answers_collision_fails_verification() {
    let app = App::builder()
        .command_with("collect", handlers::collect, |cfg| {
            cfg.template("{{ name }}").questionnaire::<FixtureAnswers>()
        })
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("fixture")
        .subcommand(Command::new("collect").arg(Arg::new("answers").long("answers")));
    let error = app.verify_command(&cmd).unwrap_err().to_string();
    assert!(error.contains("reserved name"), "{error}");
    assert!(error.contains("--answers"), "{error}");
}
#[test]
fn reserved_answer_alias_collision_fails_verification() {
    let app = App::builder()
        .command_with("collect", handlers::collect, |cfg| {
            cfg.template("{{ name }}").questionnaire::<FixtureAnswers>()
        })
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("fixture")
        .subcommand(Command::new("collect").arg(Arg::new("other").long("other").alias("answers")));
    let error = app.verify_command(&cmd).unwrap_err().to_string();
    assert!(error.contains("reserved name"), "{error}");
    assert!(error.contains("--answers"), "{error}");
}
#[test]
fn reserved_questions_alias_collision_fails_verification() {
    let app = App::builder()
        .command_with("collect", handlers::collect, |cfg| {
            cfg.template("{{ name }}").questionnaire::<FixtureAnswers>()
        })
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("fixture")
        .subcommand(Command::new("collect").subcommand(Command::new("local").alias("questions")));
    let error = app.verify_command(&cmd).unwrap_err().to_string();
    assert!(error.contains("reserved name"), "{error}");
    assert!(error.contains("questions"), "{error}");
}
#[test]
#[serial(questionnaire)]
fn questionnaire_rejects_existing_input_name() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = App::builder()
        .app_state(Calls(calls.clone()))
        .command_with("collect", handlers::collect, |cfg| {
            cfg.template("{{ name }}")
                .input(
                    "questionnaire",
                    InputChain::new().try_source(DefaultSource::new("already-taken".to_string())),
                )
                .questionnaire::<FixtureAnswers>()
        })
        .unwrap()
        .build()
        .unwrap();
    let result = TestHarness::new()
        .fixture("answers.txt", answered_sheet("from-file"))
        .run(
            &app,
            command(),
            ["fixture", "collect", "--answers", "answers.txt", "--yes"],
        );
    let error = error_text(&result);
    assert!(
        error.contains("reserved for command questionnaires"),
        "{error}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
#[serial(questionnaire)]
fn generic_input_rejects_questionnaire_name_after_questionnaire() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = App::builder()
        .app_state(Calls(calls.clone()))
        .command_with("collect", handlers::collect, |cfg| {
            cfg.template("{{ name }}")
                .questionnaire::<FixtureAnswers>()
                .input(
                    "questionnaire",
                    InputChain::new().try_source(DefaultSource::new("already-taken".to_string())),
                )
        })
        .unwrap()
        .build()
        .unwrap();
    let result = TestHarness::new()
        .fixture("answers.txt", answered_sheet("from-file"))
        .run(
            &app,
            command(),
            ["fixture", "collect", "--answers", "answers.txt", "--yes"],
        );
    let error = error_text(&result);
    assert!(
        error.contains("duplicate input names are not supported"),
        "{error}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
