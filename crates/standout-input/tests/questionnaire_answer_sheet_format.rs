use standout_input::env::MockStdin;
use standout_input::questionnaire::{
    AnswerSheetDiagnostic, AnswerSheetFormat, Questionnaire, RawAnswers, ScalarField, ScalarKind,
    StandoutAnswerSheet,
};

fn questionnaire() -> Questionnaire {
    Questionnaire::new(
        "formlike.entry",
        vec![
            ScalarField::new("name", "What is your name?", ScalarKind::String),
            ScalarField::new("region", "Which region?", ScalarKind::String).with_default("us"),
        ],
    )
    .unwrap()
}

/// No framework preamble: a tagged question line, the answer beneath.
const SPEC_SHEET: &str = "Your name <id:name>\nada\n\nWhich region? <id:region>\neu\n";

struct SpecSheet;

impl AnswerSheetFormat for SpecSheet {
    fn parse(
        &self,
        questionnaire: &Questionnaire,
        text: &str,
    ) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        questionnaire.parse_answer_sheet_body(text)
    }
}

struct KeyedSheet;

impl AnswerSheetFormat for KeyedSheet {
    fn parse(
        &self,
        _questionnaire: &Questionnaire,
        text: &str,
    ) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        let mut answers = RawAnswers::default();
        let mut diagnostics = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match line.split_once('=') {
                Some((path, value)) => answers.set(path.trim(), value.trim()),
                None => diagnostics.push(AnswerSheetDiagnostic::Tag {
                    line: index + 1,
                    message: format!("expected `field = answer`, got {line:?}"),
                }),
            }
        }
        if diagnostics.is_empty() {
            Ok(answers)
        } else {
            Err(diagnostics)
        }
    }
}

#[test]
fn the_default_format_rejects_a_sheet_the_app_spec_defines() {
    let q = questionnaire();

    let diagnostics = StandoutAnswerSheet.parse(&q, SPEC_SHEET).unwrap_err();

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.to_string().contains("#! standout-answers 1")),
        "{diagnostics:?}"
    );
}

#[test]
fn an_app_format_extends_the_tagged_body_parser() {
    let q = questionnaire();

    let raw = SpecSheet.parse(&q, SPEC_SHEET).unwrap();
    let answers = q.decode_answers(&raw).unwrap();

    assert_eq!(answers.get_text("name"), Some("ada"));
    assert_eq!(answers.get_text("region"), Some("eu"));
}

#[test]
fn an_app_format_replaces_the_sheet_shape_entirely() {
    let q = questionnaire();

    let raw = KeyedSheet.parse(&q, "name = ada\nregion = eu\n").unwrap();
    let answers = q.decode_answers(&raw).unwrap();

    assert_eq!(answers.get_text("name"), Some("ada"));
    assert_eq!(answers.get_text("region"), Some("eu"));
}

#[test]
fn an_app_format_reports_its_own_diagnostics() {
    let q = questionnaire();

    let diagnostics = KeyedSheet.parse(&q, "name = ada\nregion eu\n").unwrap_err();

    assert!(
        matches!(&diagnostics[..], [AnswerSheetDiagnostic::Tag { line, message }]
            if *line == 2 && message.contains("expected `field = answer`")),
        "{diagnostics:?}"
    );
}

#[test]
fn a_blank_answer_still_takes_the_declared_default() {
    let q = questionnaire();

    let raw = SpecSheet
        .parse(
            &q,
            "Your name <id:name>\nada\n\nWhich region? <id:region>\n",
        )
        .unwrap();
    let answers = q.decode_answers(&raw).unwrap();

    assert_eq!(answers.get_text("region"), Some("us"));
}

#[test]
fn file_and_stdin_read_through_the_format_they_are_given() {
    let q = questionnaire();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("answers.txt");
    std::fs::write(&path, SPEC_SHEET).unwrap();

    let from_file = q.read_answer_sheet_file(&path, &SpecSheet).unwrap();
    let from_stdin = q
        .read_answer_sheet_stdin(&MockStdin::piped(SPEC_SHEET), &SpecSheet)
        .unwrap();

    assert_eq!(from_file, from_stdin);
    assert_eq!(from_file.get("name"), Some("ada"));
    assert!(q
        .read_answer_sheet_file(&path, &StandoutAnswerSheet)
        .is_err());
}
