use std::path::PathBuf;
use std::sync::Arc;

use serial_test::serial;
use standout_input::env::MockStdin;
use standout_input::questionnaire::{
    AnswerSheetDiagnostic, Questionnaire as RuntimeQuestionnaire, QuestionnaireError,
    QuestionnaireInput, ScalarField, ScalarKind, ValidationDiagnostic,
};
use standout_input::{
    reset_default_prompt_responder, set_default_prompt_responder, PromptResponse, ScriptedResponder,
};
use standout_macros::Questionnaire;

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.profile")]
struct Profile {
    /// What is your project called?
    name: String,

    /// Where should the project live?
    #[question(id = "project.path", default = "/tmp/demo")]
    root: PathBuf,

    /// Use Docker?
    #[question(default = "yes")]
    docker: bool,

    /// Add any setup notes.
    ///
    /// Later paragraphs are reserved and do not render today.
    #[question(prose)]
    notes: String,

    /// Optional nickname?
    nickname: Option<String>,

    /// Optional config path?
    config: Option<PathBuf>,
}

fn hand_built_profile() -> RuntimeQuestionnaire {
    RuntimeQuestionnaire::new(
        "demo.profile",
        vec![
            ScalarField::new("name", "What is your project called?", ScalarKind::String),
            ScalarField::new(
                "project.path",
                "Where should the project live?",
                ScalarKind::Path,
            )
            .with_default("/tmp/demo"),
            ScalarField::new("docker", "Use Docker?", ScalarKind::Bool).with_default("yes"),
            ScalarField::new("notes", "Add any setup notes.", ScalarKind::Text),
            ScalarField::new("nickname", "Optional nickname?", ScalarKind::String).optional(),
            ScalarField::new("config", "Optional config path?", ScalarKind::Path).optional(),
        ],
    )
    .unwrap()
}

fn answer(sheet: &str, id: &str, answer: &str) -> String {
    let tag = format!("<id:{id}>");
    let lines: Vec<&str> = sheet.lines().collect();
    let mut out = Vec::new();
    let mut found = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        out.push(line.to_string());
        i += 1;
        if !found && line.trim_end().ends_with(&tag) {
            found = true;
            if lines.get(i).is_some_and(|next| !next.trim().is_empty()) {
                i += 1;
            }
            out.push(answer.to_string());
        }
    }
    assert!(found, "answer sheet has no question line for {tag}");
    out.join("\n") + "\n"
}

fn edited_sheet(questionnaire: &RuntimeQuestionnaire) -> String {
    let sheet = questionnaire.render_answer_sheet();
    let sheet = answer(&sheet, "name", "demo");
    let sheet = answer(&sheet, "notes", "line one\nline two");
    answer(&sheet, "config", "/etc/demo")
}

fn expected_profile() -> Profile {
    Profile {
        name: "demo".to_string(),
        root: PathBuf::from("/tmp/demo"),
        docker: true,
        notes: "line one\nline two".to_string(),
        nickname: None,
        config: Some(PathBuf::from("/etc/demo")),
    }
}

struct ResponderGuard;

impl ResponderGuard {
    fn install(responses: impl IntoIterator<Item = PromptResponse>) -> Self {
        set_default_prompt_responder(Arc::new(ScriptedResponder::new(responses)));
        Self
    }
}

impl Drop for ResponderGuard {
    fn drop(&mut self) {
        reset_default_prompt_responder();
    }
}

#[test]
fn derived_definition_matches_hand_built_definition_and_fingerprint() {
    let derived = Profile::questionnaire().unwrap();
    let hand_built = hand_built_profile();

    assert_eq!(derived, hand_built);
    assert_eq!(derived.fingerprint(), hand_built.fingerprint());
}

#[derive(Questionnaire)]
#[question(id = "demo.invalid")]
#[allow(dead_code)]
struct InvalidFieldId {
    /// Name?
    #[question(id = "Project.Name")]
    name: String,
}

#[test]
fn derived_definitions_surface_public_builder_errors() {
    let err = InvalidFieldId::questionnaire().unwrap_err();

    assert_eq!(err, QuestionnaireError::InvalidId("Project.Name".into()));
    assert!(err.to_string().contains("Project.Name"));
}

#[test]
fn rendered_sheet_round_trips_to_the_typed_struct() {
    let questionnaire = Profile::questionnaire().unwrap();
    let raw = questionnaire
        .parse_answer_sheet(&edited_sheet(&questionnaire))
        .unwrap();
    let profile = Profile::from_raw_answers(&raw).unwrap();

    assert_eq!(profile, expected_profile());
}

#[test]
fn typed_decode_accepts_file_and_stdin_raw_answers() {
    let questionnaire = Profile::questionnaire().unwrap();
    let sheet = edited_sheet(&questionnaire);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("answers.txt");
    std::fs::write(&path, &sheet).unwrap();

    let from_file = questionnaire.read_answer_sheet_file(&path).unwrap();
    let from_stdin = questionnaire
        .read_answer_sheet_stdin_with(&MockStdin::piped(sheet))
        .unwrap();

    assert_eq!(
        Profile::from_raw_answers(&from_file).unwrap(),
        expected_profile()
    );
    assert_eq!(
        Profile::from_raw_answers(&from_stdin).unwrap(),
        expected_profile()
    );
}

#[test]
#[serial(prompt_responder)]
fn typed_decode_accepts_interactive_raw_answers() {
    let questionnaire = Profile::questionnaire().unwrap();
    let _guard = ResponderGuard::install([
        PromptResponse::text("demo"),
        PromptResponse::Skip,
        PromptResponse::Skip,
        PromptResponse::text("line one\nline two"),
        PromptResponse::Skip,
        PromptResponse::text("/etc/demo"),
    ]);

    let raw = questionnaire.collect_interactive().unwrap();
    assert_eq!(Profile::from_raw_answers(&raw).unwrap(), expected_profile());
}

#[derive(Questionnaire)]
#[question(id = "demo.docs")]
#[allow(dead_code)]
struct WrappedDocs {
    /// Rustfmt may wrap this prompt over
    /// more than one source line.
    ///
    /// This paragraph is reserved.
    name: String,
}

#[derive(Questionnaire)]
#[question(id = "demo.docs")]
#[allow(dead_code)]
struct UnwrappedDocs {
    /// Rustfmt may wrap this prompt over more than one source line.
    ///
    /// This paragraph is reserved too.
    name: String,
}

#[test]
fn doc_comments_use_the_unwrapped_first_paragraph_only() {
    let wrapped = WrappedDocs::questionnaire().unwrap().render_answer_sheet();
    let unwrapped = UnwrappedDocs::questionnaire()
        .unwrap()
        .render_answer_sheet();

    assert_eq!(wrapped, unwrapped);
    assert!(wrapped.contains("Rustfmt may wrap this prompt over more than one source line."));
    assert!(!wrapped.contains("reserved"));
}

#[derive(Debug, Questionnaire)]
#[question(id = "demo.single-line")]
#[allow(dead_code)]
struct SingleLineDefault {
    /// A short value?
    value: String,

    /// Prose value?
    #[question(prose)]
    prose: String,
}

#[test]
fn prose_accepts_multiline_while_unmarked_strings_reject_it() {
    let questionnaire = SingleLineDefault::questionnaire().unwrap();
    let ok_sheet = answer(
        &answer(&questionnaire.render_answer_sheet(), "value", "short"),
        "prose",
        "line one\nline two",
    );
    assert!(SingleLineDefault::from_raw_answers(
        &questionnaire.parse_answer_sheet(&ok_sheet).unwrap()
    )
    .is_ok());

    let bad_sheet = answer(
        &answer(
            &questionnaire.render_answer_sheet(),
            "value",
            "line one\nline two",
        ),
        "prose",
        "text",
    );
    let err =
        SingleLineDefault::from_raw_answers(&questionnaire.parse_answer_sheet(&bad_sheet).unwrap())
            .unwrap_err();
    assert!(matches!(
        err,
        standout_input::questionnaire::QuestionnaireInputError::Validation(diagnostics)
            if matches!(
                diagnostics.as_slice(),
                [ValidationDiagnostic::InvalidValue { id, .. }] if id == "value"
            )
    ));
}

#[test]
fn stale_fingerprint_rejects_the_round_trip() {
    let questionnaire = Profile::questionnaire().unwrap();
    let sheet = edited_sheet(&questionnaire).replace("sha256:", "sha256:0000");
    let diagnostics = questionnaire.parse_answer_sheet(&sheet).unwrap_err();
    assert!(matches!(
        diagnostics.as_slice(),
        [AnswerSheetDiagnostic::FingerprintMismatch { .. }]
    ));
}

#[test]
fn compile_failures_cover_attribute_misuse() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/questionnaire/*.rs");
}
