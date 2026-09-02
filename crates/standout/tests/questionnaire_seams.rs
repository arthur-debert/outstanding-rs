//! An app-defined answer-sheet format and a configurable confirmation gate, driven through `App`.

use clap::Command;
use serde_json::json;
use standout::cli::{
    App, CommandContext, CommandContextInput, Confirmation, ConfirmationAcceptance, DispatchResult,
    FnHandler, HandlerResult, Output,
};
use standout::input::questionnaire::{
    AnswerSheetDiagnostic, AnswerSheetFormat, Questionnaire as RuntimeQuestionnaire, RawAnswers,
};
use standout::EmbeddedTemplates;
use standout_test::{serial, TestHarness, TestResult};

const TEMPLATES: &[(&str, &str)] = &[("entry", "{{ name }}/{{ region }}")];

#[derive(Debug, Clone, PartialEq, Eq, standout::Questionnaire)]
#[question(id = "formlike.entry")]
struct EntryAnswers {
    /// What is your name?
    name: String,

    /// Which region?
    #[question(default = "us")]
    region: String,
}

/// No framework preamble: a tagged question line, the answer beneath.
const SPEC_SHEET: &str = "Your name <id:name>\nada\n\nWhich region? <id:region>\neu\n";

struct SpecSheet;

impl AnswerSheetFormat for SpecSheet {
    fn parse(
        &self,
        questionnaire: &RuntimeQuestionnaire,
        text: &str,
    ) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        questionnaire.parse_answer_sheet_body(text)
    }
}

fn entry(_matches: &clap::ArgMatches, ctx: &CommandContext) -> HandlerResult<serde_json::Value> {
    let answers: &EntryAnswers = ctx.questionnaire()?;
    Ok(Output::Render(
        json!({ "name": answers.name, "region": answers.region }),
    ))
}

fn command() -> Command {
    Command::new("formlike")
        .subcommand_required(true)
        .subcommand(Command::new("entry"))
}

fn app(app_defined_sheet: bool, confirmation: Option<Confirmation>) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with("entry", FnHandler::new(entry), move |cfg| {
            let mut cfg = cfg.questionnaire::<EntryAnswers>();
            if app_defined_sheet {
                cfg = cfg.answer_sheet_format(SpecSheet);
            }
            if let Some(confirmation) = confirmation {
                cfg = cfg.confirmation(confirmation);
            }
            cfg
        })
        .unwrap()
        .build()
        .unwrap()
}

fn spec_sheet_app() -> App {
    app(true, None)
}

fn framework_sheet_app() -> App {
    app(false, None)
}

fn spec_sheet_app_gated_by(confirmation: Confirmation) -> App {
    app(true, Some(confirmation))
}

fn error_text(result: &TestResult) -> String {
    match result.outcome() {
        DispatchResult::Error(error) => error.to_string(),
        other => panic!("expected error, got {other:?}"),
    }
}

#[test]
#[serial(questionnaire)]
fn the_spec_sheet_the_preamble_rejected_now_answers_the_command() {
    let result = TestHarness::new().fixture("answers.txt", SPEC_SHEET).run(
        &spec_sheet_app(),
        command(),
        ["formlike", "entry", "--answers", "answers.txt", "--yes"],
    );

    result.assert_success();
    assert_eq!(result.stdout(), "ada/eu");
}

#[test]
#[serial(questionnaire)]
fn the_spec_sheet_arrives_the_same_way_through_piped_stdin() {
    let result = TestHarness::new().piped_stdin(SPEC_SHEET).run(
        &spec_sheet_app(),
        command(),
        ["formlike", "entry", "--answers", "-", "--yes"],
    );

    result.assert_success();
    assert_eq!(result.stdout(), "ada/eu");
}

#[test]
#[serial(questionnaire)]
fn a_blank_answer_in_the_spec_sheet_takes_the_declared_default() {
    let result = TestHarness::new()
        .fixture("answers.txt", "Your name <id:name>\nada\n")
        .run(
            &spec_sheet_app(),
            command(),
            ["formlike", "entry", "--answers", "answers.txt", "--yes"],
        );

    result.assert_success();
    assert_eq!(result.stdout(), "ada/us");
}

#[test]
#[serial(questionnaire)]
fn a_missing_required_answer_names_the_question_it_belongs_to() {
    let result = TestHarness::new()
        .fixture("answers.txt", "Which region? <id:region>\neu\n")
        .run(
            &spec_sheet_app(),
            command(),
            ["formlike", "entry", "--answers", "answers.txt", "--yes"],
        );

    let error = error_text(&result);
    assert!(error.contains("name"), "{error}");
}

#[test]
#[serial(questionnaire)]
fn without_the_seam_the_framework_sheet_is_still_the_format() {
    let result = TestHarness::new().fixture("answers.txt", SPEC_SHEET).run(
        &framework_sheet_app(),
        command(),
        ["formlike", "entry", "--answers", "answers.txt", "--yes"],
    );

    let error = error_text(&result);
    assert!(error.contains("#! standout-answers 1"), "{error}");
}

#[test]
#[serial(questionnaire)]
fn the_gate_takes_y_when_the_app_says_y_or_yes() {
    let harness = TestHarness::new()
        .fixture("answers.txt", SPEC_SHEET)
        .fixture("terminal.txt", "y\n");
    let terminal = harness.tempdir().unwrap().join("terminal.txt");
    let terminal_arg = terminal.to_str().unwrap().to_string();

    let result = harness
        .env("STANDOUT_QUESTIONNAIRE_TERMINAL", terminal_arg)
        .run(
            &spec_sheet_app_gated_by(
                Confirmation::default().acceptance(ConfirmationAcceptance::YesOrY),
            ),
            command(),
            ["formlike", "entry", "--answers", "answers.txt"],
        );

    result.assert_success();
    assert_eq!(result.stdout(), "ada/eu");
}

#[test]
#[serial(questionnaire)]
fn the_default_gate_still_declines_a_bare_y() {
    let harness = TestHarness::new()
        .fixture("answers.txt", SPEC_SHEET)
        .fixture("terminal.txt", "y\n");
    let terminal = harness.tempdir().unwrap().join("terminal.txt");
    let terminal_arg = terminal.to_str().unwrap().to_string();

    let result = harness
        .env("STANDOUT_QUESTIONNAIRE_TERMINAL", terminal_arg)
        .run(
            &spec_sheet_app(),
            command(),
            ["formlike", "entry", "--answers", "answers.txt"],
        );

    let error = error_text(&result);
    assert!(error.contains("confirmation declined"), "{error}");
}

#[test]
#[serial(questionnaire)]
fn an_empty_acceptance_word_is_not_confirmed_by_a_bare_enter() {
    let harness = TestHarness::new()
        .fixture("answers.txt", SPEC_SHEET)
        .fixture("terminal.txt", "\n");
    let terminal = harness.tempdir().unwrap().join("terminal.txt");
    let terminal_arg = terminal.to_str().unwrap().to_string();

    let result = harness
        .env("STANDOUT_QUESTIONNAIRE_TERMINAL", terminal_arg)
        .run(
            &spec_sheet_app_gated_by(
                Confirmation::default().acceptance(ConfirmationAcceptance::Word(String::new())),
            ),
            command(),
            ["formlike", "entry", "--answers", "answers.txt"],
        );

    let error = error_text(&result);
    assert!(error.contains("confirmation declined"), "{error}");
}

#[test]
#[serial(questionnaire)]
fn a_disabled_gate_runs_without_an_attended_terminal() {
    let result = TestHarness::new()
        .fixture("answers.txt", SPEC_SHEET)
        .env("STANDOUT_QUESTIONNAIRE_TERMINAL", "absent")
        .run(
            &spec_sheet_app_gated_by(
                Confirmation::default().acceptance(ConfirmationAcceptance::Disabled),
            ),
            command(),
            ["formlike", "entry", "--answers", "answers.txt"],
        );

    result.assert_success();
    assert_eq!(result.stdout(), "ada/eu");
}
