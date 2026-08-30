use std::sync::{Arc, Mutex};

use standout_input::env::MockStdin;
use standout_input::questionnaire::{
    AnswerSheetDiagnostic, DynamicDefault, Group, Item, Questionnaire, QuestionnaireError,
    ScalarField, ScalarKind, ValidationDiagnostic,
};
use standout_input::{
    InputSources, PromptContext, PromptResponder, PromptResponse, ScriptedResponder,
};

fn questionnaire() -> Questionnaire {
    Questionnaire::new(
        "demo.dynamic",
        vec![
            ScalarField::new("input.value_type", "Value type?", ScalarKind::String)
                .one_of(["string", "bool", "path"]),
            ScalarField::new("input.cardinality", "Cardinality?", ScalarKind::String)
                .one_of(["single", "list", "boolean"])
                .with_dynamic_default(DynamicDefault::new("1", |earlier| {
                    match earlier.get_text("input.value_type") {
                        Some("bool") => "boolean".to_string(),
                        _ => "single".to_string(),
                    }
                })),
        ],
    )
    .unwrap()
}

fn answer(sheet: &str, id: &str, answer: &str) -> String {
    let tag = format!("<id:{id}>");
    let lines: Vec<&str> = sheet.lines().collect();
    let mut out: Vec<String> = Vec::new();
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

struct ResponderGuard {
    sources: InputSources,
}
impl ResponderGuard {
    fn install(responses: impl IntoIterator<Item = PromptResponse>) -> Self {
        Self::install_with(Arc::new(ScriptedResponder::new(responses)))
    }

    fn install_with(responder: Arc<dyn PromptResponder>) -> Self {
        Self {
            sources: InputSources::from_process().with_responder(responder),
        }
    }

    fn sources(&self) -> &InputSources {
        &self.sources
    }
}

#[test]
fn declaring_both_a_static_and_a_dynamic_default_is_an_error() {
    let err = Questionnaire::new(
        "demo",
        vec![ScalarField::new("a", "A?", ScalarKind::String)
            .with_default("static")
            .with_dynamic_default(DynamicDefault::new("1", |_| "dynamic".to_string()))],
    )
    .unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "a"));
    assert!(err
        .to_string()
        .contains("declares both a static and a dynamic default"));
}

#[test]
fn an_empty_dynamic_default_revision_is_an_error() {
    let err = Questionnaire::new(
        "demo",
        vec![ScalarField::new("a", "A?", ScalarKind::String)
            .with_dynamic_default(DynamicDefault::new("", |_| "x".to_string()))],
    )
    .unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "a"));
    assert!(err
        .to_string()
        .contains("attaches a dynamic default with an empty revision"));
}

fn with_revision_and_closure(
    revision: &str,
    compute: impl Fn() -> String + Send + Sync + 'static,
) -> Questionnaire {
    Questionnaire::new(
        "demo",
        vec![ScalarField::new("a", "A?", ScalarKind::String)
            .with_dynamic_default(DynamicDefault::new(revision, move |_| compute()))],
    )
    .unwrap()
}

#[test]
fn changing_the_revision_changes_the_fingerprint() {
    let one = with_revision_and_closure("1", || "x".to_string());
    let two = with_revision_and_closure("2", || "x".to_string());
    assert_ne!(one.fingerprint(), two.fingerprint());
}

#[test]
fn the_closure_itself_does_not_affect_the_fingerprint() {
    let one = with_revision_and_closure("1", || "x".to_string());
    let other = with_revision_and_closure("1", || "an entirely different value".to_string());
    assert_eq!(one.fingerprint(), other.fingerprint());
}

#[test]
fn a_dynamic_default_cannot_collide_with_a_static_default() {
    let dynamic = with_revision_and_closure("v", || "v".to_string());
    let static_default = Questionnaire::new(
        "demo",
        vec![ScalarField::new("a", "A?", ScalarKind::String).with_default("v")],
    )
    .unwrap();
    assert_ne!(dynamic.fingerprint(), static_default.fingerprint());
}

#[test]
fn a_revision_bump_invalidates_previously_rendered_sheets() {
    let old = with_revision_and_closure("1", || "x".to_string());
    let new = with_revision_and_closure("2", || "x".to_string());
    let sheet = answer(&old.render_answer_sheet(), "a", "hello");
    let diagnostics = new.parse_answer_sheet(&sheet).unwrap_err();
    assert!(matches!(
        &diagnostics[..],
        [AnswerSheetDiagnostic::Incompatible { message }] if message.contains("fingerprint")
    ));
}

#[test]
fn dynamic_default_fields_render_with_an_empty_answer_region() {
    let q = questionnaire();
    let sheet = q.render_answer_sheet();
    let lines: Vec<&str> = sheet.lines().collect();
    let question = lines
        .iter()
        .position(|l| l.trim_end().ends_with("<id:input.cardinality>"))
        .unwrap();
    assert!(lines
        .get(question + 1)
        .is_none_or(|next| next.trim().is_empty()));
}

#[test]
fn a_blank_answer_resolves_through_the_computed_default_from_a_sheet() {
    let q = questionnaire();
    let sheet = answer(&q.render_answer_sheet(), "input.value_type", "bool");
    let raw = q
        .read_answer_sheet_stdin_with(&MockStdin::piped(sheet.clone()))
        .unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_text("input.cardinality"), Some("boolean"));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("answers.txt");
    std::fs::write(&path, sheet).unwrap();
    let from_file = q
        .decode_answers(&q.read_answer_sheet_file(&path).unwrap())
        .unwrap();
    assert_eq!(from_file, answers);

    let sheet = answer(&q.render_answer_sheet(), "input.value_type", "string");
    let raw = q
        .read_answer_sheet_stdin_with(&MockStdin::piped(sheet))
        .unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_text("input.cardinality"), Some("single"));
}

#[test]
fn a_blank_interactive_entry_resolves_through_the_computed_default() {
    let q = questionnaire();
    let _guard = ResponderGuard::install([PromptResponse::text("bool"), PromptResponse::Skip]);
    let interactive = q
        .decode_answers(&q.collect_interactive_from(_guard.sources()).unwrap())
        .unwrap();
    assert_eq!(interactive.get_text("input.cardinality"), Some("boolean"));

    let sheet = answer(&q.render_answer_sheet(), "input.value_type", "bool");
    let batch = q
        .decode_answers(
            &q.read_answer_sheet_stdin_with(&MockStdin::piped(sheet))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(interactive, batch);
}

#[test]
fn an_entered_answer_overrides_the_computed_default() {
    let q = questionnaire();
    let _guard =
        ResponderGuard::install([PromptResponse::text("bool"), PromptResponse::text("list")]);
    let answers = q
        .decode_answers(&q.collect_interactive_from(_guard.sources()).unwrap())
        .unwrap();
    assert_eq!(answers.get_text("input.cardinality"), Some("list"));
}

#[test]
fn a_computed_default_that_violates_a_constraint_is_a_diagnostic() {
    let q = Questionnaire::new(
        "demo",
        vec![ScalarField::new("a", "A?", ScalarKind::String)
            .one_of(["x", "y"])
            .with_dynamic_default(DynamicDefault::new("1", |_| "not-a-choice".to_string()))],
    )
    .unwrap();
    let sheet = q.render_answer_sheet();
    let raw = q
        .read_answer_sheet_stdin_with(&MockStdin::piped(sheet))
        .unwrap();
    let diagnostics = q.decode_answers(&raw).unwrap_err();
    assert!(matches!(
        &diagnostics[..],
        [ValidationDiagnostic::Field { id, message }]
            if id == "a" && message.contains("must be one of")
    ));
}

#[test]
fn dynamic_defaults_resolve_per_occurrence_in_repeatable_groups() {
    let q = Questionnaire::new(
        "demo.repeats",
        vec![Item::from(
            Group::new(
                "inputs",
                "Describe an input.",
                vec![
                    ScalarField::new("inputs.value_type", "Type?", ScalarKind::String),
                    ScalarField::new("inputs.cardinality", "Cardinality?", ScalarKind::String)
                        .with_dynamic_default(DynamicDefault::new("1", |earlier| {
                            match earlier.get_text("inputs.value_type") {
                                Some("bool") => "boolean".to_string(),
                                _ => "single".to_string(),
                            }
                        })),
                ],
            )
            .repeatable(1),
        )],
    )
    .unwrap();

    let sheet = q.render_answer_sheet();
    let block_start = sheet.find("Describe an input.").unwrap();
    let block = sheet[block_start..].to_string();
    let first = sheet.replace("<id:inputs.value_type>\n", "<id:inputs.value_type>\nbool\n");
    let second = block.replace(
        "<id:inputs.value_type>\n",
        "<id:inputs.value_type>\nstring\n",
    );
    let document = format!("{first}\n{second}");
    let raw = q
        .read_answer_sheet_stdin_with(&MockStdin::piped(document))
        .unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_text("inputs[0].cardinality"), Some("boolean"));
    assert_eq!(answers.get_text("inputs[1].cardinality"), Some("single"));
}

#[test]
fn a_dependency_on_a_later_or_unknown_field_reads_as_none() {
    let q = Questionnaire::new(
        "demo",
        vec![
            ScalarField::new("a", "A?", ScalarKind::String).with_dynamic_default(
                DynamicDefault::new("1", |earlier| {
                    assert!(earlier.get("b").is_none());
                    assert!(earlier.get("no.such.field").is_none());
                    "fallback".to_string()
                }),
            ),
            ScalarField::new("b", "B?", ScalarKind::String).optional(),
        ],
    )
    .unwrap();
    let sheet = q.render_answer_sheet();
    let raw = q
        .read_answer_sheet_stdin_with(&MockStdin::piped(sheet))
        .unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_text("a"), Some("fallback"));
}

struct RecordingResponder {
    messages: Mutex<Vec<String>>,
    responses: Mutex<std::collections::VecDeque<PromptResponse>>,
}

impl PromptResponder for RecordingResponder {
    fn respond(&self, ctx: PromptContext<'_>) -> PromptResponse {
        self.messages.lock().unwrap().push(ctx.message.to_string());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra prompt")
    }
}

#[test]
fn the_interactive_prompt_displays_the_computed_default() {
    let q = questionnaire();
    let responder = Arc::new(RecordingResponder {
        messages: Mutex::new(Vec::new()),
        responses: Mutex::new(
            [PromptResponse::text("bool"), PromptResponse::Skip]
                .into_iter()
                .collect(),
        ),
    });
    let _guard = ResponderGuard::install_with(responder.clone());
    q.collect_interactive_from(_guard.sources()).unwrap();

    let messages = responder.messages.lock().unwrap();
    let cardinality_prompt = messages
        .iter()
        .find(|m| m.starts_with("Cardinality?"))
        .expect("the cardinality question prompted");
    assert!(
        cardinality_prompt.contains("[default: boolean]"),
        "prompt should show the computed default: {cardinality_prompt:?}"
    );
}
