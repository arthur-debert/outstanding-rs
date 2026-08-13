//! Collection-adapter tests through the real abstractions: named files,
//! explicit stdin (via the injectable stdin reader), and interactive
//! prompting (via the prompt-responder seam and the mock terminal) — no real
//! terminal required. Covers EOF, cancellation, malformed documents,
//! defaults, conditions, retry-without-loss, and cross-source equivalence.

use std::sync::Arc;

use serial_test::serial;
use standout_input::env::{reset_default_stdin_reader, set_default_stdin_reader, MockStdin};
use standout_input::questionnaire::{
    AnswerSheetDiagnostic, Group, Item, Questionnaire, ScalarField, ScalarKind,
};
use standout_input::{
    reset_default_prompt_responder, set_default_prompt_responder, InputError, MockTerminal,
    PromptResponse, ScriptedResponder,
};

fn questionnaire() -> Questionnaire {
    Questionnaire::new(
        "demo.collect",
        vec![
            ScalarField::new("project.name", "Project name?", ScalarKind::String),
            ScalarField::new("project.docker", "Use Docker?", ScalarKind::Bool).with_default("no"),
            ScalarField::new("project.docker_image", "Base image?", ScalarKind::String)
                .active_when("project.docker", "yes"),
            ScalarField::new("project.notes", "Notes?", ScalarKind::Text).optional(),
        ],
    )
    .unwrap()
}

/// Set `answer` as the answer text directly below the question line tagged
/// `id`, replacing a rendered pre-filled default line when one is present.
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
            // A non-blank line right below the question is a pre-filled
            // default: the answer replaces it.
            if lines.get(i).is_some_and(|next| !next.trim().is_empty()) {
                i += 1;
            }
            out.push(answer.to_string());
        }
    }
    assert!(found, "answer sheet has no question line for {tag}");
    out.join("\n") + "\n"
}

/// An edited sheet: name answered, docker default kept, notes blank.
fn edited_sheet(q: &Questionnaire) -> String {
    answer(&q.render_answer_sheet(), "project.name", "demo")
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

struct StdinGuard;
impl StdinGuard {
    fn install(mock: MockStdin) -> Self {
        set_default_stdin_reader(Arc::new(mock));
        Self
    }
}
impl Drop for StdinGuard {
    fn drop(&mut self) {
        reset_default_stdin_reader();
    }
}

// ============================================================================
// Named-file adapter
// ============================================================================

#[test]
fn file_adapter_reads_one_complete_sheet() {
    let q = questionnaire();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("answers.txt");
    std::fs::write(&path, edited_sheet(&q)).unwrap();

    let raw = q.read_answer_sheet_file(&path).unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_text("project.name"), Some("demo"));
    assert_eq!(answers.get_bool("project.docker"), Some(false));
}

#[test]
fn unreadable_file_is_a_diagnostic_not_a_panic() {
    let q = questionnaire();
    let diagnostics = q
        .read_answer_sheet_file("/nonexistent/answers.txt")
        .unwrap_err();
    assert!(matches!(
        &diagnostics[..],
        [AnswerSheetDiagnostic::UnreadableDocument { .. }]
    ));
}

#[test]
fn malformed_file_reports_parse_diagnostics() {
    let q = questionnaire();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("answers.txt");
    std::fs::write(&path, "not an answer sheet").unwrap();

    let diagnostics = q.read_answer_sheet_file(&path).unwrap_err();
    assert!(matches!(
        &diagnostics[..],
        [AnswerSheetDiagnostic::MalformedPreamble { .. }, ..]
    ));
}

// ============================================================================
// Explicit-stdin adapter
// ============================================================================

#[test]
fn stdin_adapter_reads_one_complete_sheet() {
    let q = questionnaire();
    let raw = q
        .read_answer_sheet_stdin_with(&MockStdin::piped(edited_sheet(&q)))
        .unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_text("project.name"), Some("demo"));
}

#[test]
fn stdin_adapter_rejects_an_interactive_terminal() {
    let q = questionnaire();
    let diagnostics = q
        .read_answer_sheet_stdin_with(&MockStdin::terminal())
        .unwrap_err();
    assert!(matches!(
        &diagnostics[..],
        [AnswerSheetDiagnostic::UnreadableDocument { .. }]
    ));
}

#[test]
#[serial(default_stdin)]
fn stdin_adapter_honors_the_process_default_reader() {
    let q = questionnaire();
    let _guard = StdinGuard::install(MockStdin::piped(edited_sheet(&q)));
    let raw = q.read_answer_sheet_stdin().unwrap();
    assert_eq!(raw.get("project.name"), Some("demo"));
}

#[test]
fn stale_fingerprint_reaches_the_adapter_caller() {
    let q = questionnaire();
    let sheet = edited_sheet(&q).replace("sha256:", "sha256:0000");
    let diagnostics = q
        .read_answer_sheet_stdin_with(&MockStdin::piped(sheet))
        .unwrap_err();
    assert!(matches!(
        &diagnostics[..],
        [AnswerSheetDiagnostic::FingerprintMismatch { .. }]
    ));
}

// ============================================================================
// Interactive adapter (prompt-responder seam)
// ============================================================================

#[test]
#[serial(prompt_responder)]
fn interactive_collection_walks_active_fields_in_order() {
    let q = questionnaire();
    // name, docker=yes, image, notes — all four prompts fire.
    let _guard = ResponderGuard::install([
        PromptResponse::text("demo"),
        PromptResponse::text("yes"),
        PromptResponse::text("debian:stable"),
        PromptResponse::text("multi word notes"),
    ]);
    let raw = q.collect_interactive().unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_text("project.name"), Some("demo"));
    assert_eq!(answers.get_bool("project.docker"), Some(true));
    assert_eq!(
        answers.get_text("project.docker_image"),
        Some("debian:stable")
    );
    assert_eq!(answers.get_text("project.notes"), Some("multi word notes"));
}

#[test]
#[serial(prompt_responder)]
fn interactive_collection_skips_inactive_fields_without_prompting() {
    let q = questionnaire();
    // Three prompts only: name, docker=no, notes. No image prompt — the
    // ScriptedResponder would panic on a fourth request.
    let _guard = ResponderGuard::install([
        PromptResponse::text("demo"),
        PromptResponse::text("no"),
        PromptResponse::Skip,
    ]);
    let raw = q.collect_interactive().unwrap();
    assert_eq!(raw.get("project.docker_image"), None);
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get("project.docker_image"), None);
    assert_eq!(answers.get("project.notes"), None);
}

#[test]
#[serial(prompt_responder)]
fn interactive_blank_resolves_defaults_and_omission() {
    let q = questionnaire();
    // Skip = blank entry: docker resolves its default, notes is omitted.
    let _guard = ResponderGuard::install([
        PromptResponse::text("demo"),
        PromptResponse::Skip,
        PromptResponse::Skip,
    ]);
    let raw = q.collect_interactive().unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_bool("project.docker"), Some(false));
    assert_eq!(answers.get("project.notes"), None);
}

#[test]
#[serial(prompt_responder)]
fn interactive_failures_retry_locally_without_losing_earlier_answers() {
    let q = questionnaire();
    // The docker question fails twice (bad bool, then a required blank has a
    // default so blank *succeeds* — use the name question instead for the
    // required-blank retry) before the loop accepts an answer.
    let _guard = ResponderGuard::install([
        PromptResponse::Skip,          // name: blank, required, no default -> retry
        PromptResponse::text("demo"),  // name: accepted
        PromptResponse::text("maybe"), // docker: not a bool -> retry
        PromptResponse::text("yes"),   // docker: accepted
        PromptResponse::text("debian:stable"), // image
        PromptResponse::Skip,          // notes
    ]);
    let raw = q.collect_interactive().unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    // The earlier accepted answer survived the later retries.
    assert_eq!(answers.get_text("project.name"), Some("demo"));
    assert_eq!(answers.get_bool("project.docker"), Some(true));
}

#[test]
#[serial(prompt_responder)]
fn interactive_cancellation_aborts_collection() {
    let q = questionnaire();
    let _guard = ResponderGuard::install([PromptResponse::text("demo"), PromptResponse::Cancel]);
    let err = q.collect_interactive().unwrap_err();
    assert!(matches!(err, InputError::PromptCancelled));
}

// ============================================================================
// Interactive adapter (mock terminal)
// ============================================================================

#[test]
#[serial(prompt_responder)]
fn interactive_collection_runs_over_an_injected_terminal() {
    let q = questionnaire();
    let terminal = Arc::new(MockTerminal::with_responses([
        "demo",
        "yes",
        "debian:stable",
        "some notes",
    ]));
    let raw = q.collect_interactive_with_terminal(terminal).unwrap();
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_text("project.name"), Some("demo"));
    assert_eq!(
        answers.get_text("project.docker_image"),
        Some("debian:stable")
    );
}

#[test]
#[serial(prompt_responder)]
fn interactive_eof_is_cancellation() {
    let q = questionnaire();
    let err = q
        .collect_interactive_with_terminal(Arc::new(MockTerminal::eof()))
        .unwrap_err();
    assert!(matches!(err, InputError::PromptCancelled));
}

#[test]
#[serial(prompt_responder)]
fn interactive_collection_refuses_a_non_terminal_without_a_responder() {
    let q = questionnaire();
    let err = q
        .collect_interactive_with_terminal(Arc::new(MockTerminal::non_terminal()))
        .unwrap_err();
    assert!(matches!(err, InputError::NoInput));
}

// ============================================================================
// Interactive adapter: repeatable groups
// ============================================================================

/// A repeatable group (minimum 1, maximum 3) with a per-occurrence
/// conditional field.
fn repeatable_questionnaire() -> Questionnaire {
    Questionnaire::new(
        "demo.repeats",
        vec![Item::from(
            Group::new(
                "inputs",
                "Describe an input.",
                vec![
                    ScalarField::new("inputs.name", "Name?", ScalarKind::String),
                    ScalarField::new("inputs.flag", "Expose a flag?", ScalarKind::Bool)
                        .with_default("no"),
                    ScalarField::new("inputs.flag_name", "Flag name?", ScalarKind::String)
                        .active_when("inputs.flag", "yes"),
                ],
            )
            .repeatable(1)
            .max_occurrences(3),
        )],
    )
    .unwrap()
}

#[test]
#[serial(prompt_responder)]
fn interactive_collection_walks_repeatable_occurrences() {
    let q = repeatable_questionnaire();
    // Occurrence 0: name, flag=yes, flag_name. Add another? yes.
    // Occurrence 1: name, flag=no (skip), flag_name skipped (inactive).
    // Add another? no.
    let _guard = ResponderGuard::install([
        PromptResponse::text("alpha"),
        PromptResponse::text("yes"),
        PromptResponse::text("alpha-flag"),
        PromptResponse::text("yes"), // add another?
        PromptResponse::text("beta"),
        PromptResponse::Skip,       // flag: default "no"
        PromptResponse::text("no"), // add another?
    ]);
    let raw = q.collect_interactive().unwrap();
    assert_eq!(raw.occurrence_count("inputs"), 2);
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.occurrence_count("inputs"), 2);
    assert_eq!(answers.get_text("inputs[0].name"), Some("alpha"));
    assert_eq!(answers.get_text("inputs[0].flag_name"), Some("alpha-flag"));
    assert_eq!(answers.get_text("inputs[1].name"), Some("beta"));
    assert_eq!(answers.get_bool("inputs[1].flag"), Some(false));
    assert_eq!(answers.get("inputs[1].flag_name"), None);
}

#[test]
#[serial(prompt_responder)]
fn interactive_collection_stops_at_the_maximum_without_asking() {
    let q = Questionnaire::new(
        "demo.capped",
        vec![Item::from(
            Group::new(
                "items",
                "Item?",
                vec![ScalarField::new("items.name", "Name?", ScalarKind::String)],
            )
            .repeatable(1)
            .max_occurrences(1),
        )],
    )
    .unwrap();
    // Exactly one prompt: the scripted responder would panic on an
    // unexpected add-another question.
    let _guard = ResponderGuard::install([PromptResponse::text("only")]);
    let raw = q.collect_interactive().unwrap();
    assert_eq!(raw.occurrence_count("items"), 1);
    assert_eq!(raw.get("items[0].name"), Some("only"));
}

#[test]
#[serial(prompt_responder)]
fn interactive_blank_add_another_means_no() {
    let q = repeatable_questionnaire();
    let _guard = ResponderGuard::install([
        PromptResponse::text("alpha"),
        PromptResponse::Skip, // flag: default "no"
        PromptResponse::Skip, // add another? blank = no
    ]);
    let raw = q.collect_interactive().unwrap();
    assert_eq!(raw.occurrence_count("inputs"), 1);
}

#[test]
#[serial(prompt_responder)]
fn interactive_add_another_retries_on_a_non_bool_answer() {
    let q = repeatable_questionnaire();
    let _guard = ResponderGuard::install([
        PromptResponse::text("alpha"),
        PromptResponse::Skip,          // flag: default "no"
        PromptResponse::text("maybe"), // add another: not yes/no -> re-ask
        PromptResponse::text("no"),
    ]);
    let raw = q.collect_interactive().unwrap();
    assert_eq!(raw.occurrence_count("inputs"), 1);
}

#[test]
#[serial(prompt_responder)]
fn interactive_cancellation_inside_an_occurrence_aborts() {
    let q = repeatable_questionnaire();
    let _guard = ResponderGuard::install([PromptResponse::text("alpha"), PromptResponse::Cancel]);
    let err = q.collect_interactive().unwrap_err();
    assert!(matches!(err, InputError::PromptCancelled));
}

// ============================================================================
// Cross-source equivalence
// ============================================================================

#[test]
#[serial(prompt_responder)]
fn nested_interactive_and_sheet_submissions_decode_identically() {
    let q = repeatable_questionnaire();

    let _guard = ResponderGuard::install([
        PromptResponse::text("alpha"),
        PromptResponse::text("yes"),
        PromptResponse::text("alpha-flag"),
        PromptResponse::text("yes"), // add another?
        PromptResponse::text("beta"),
        PromptResponse::Skip,       // flag: default "no"
        PromptResponse::text("no"), // add another?
    ]);
    let interactive = q.decode_answers(&q.collect_interactive().unwrap()).unwrap();

    // The same answers as a sheet: the rendered block answered, plus one
    // copied block for the second occurrence.
    let sheet = q.render_answer_sheet();
    let block_start = sheet.find("Describe an input.").unwrap();
    let block = sheet[block_start..].to_string();
    let first = sheet
        .replace("<id:inputs.name>\n", "<id:inputs.name>\nalpha\n")
        .replace("<id:inputs.flag>\nno\n", "<id:inputs.flag>\nyes\n")
        .replace(
            "<id:inputs.flag_name>\n",
            "<id:inputs.flag_name>\nalpha-flag\n",
        );
    let second = block.replace("<id:inputs.name>\n", "<id:inputs.name>\nbeta\n");
    let document = format!("{first}\n{second}");
    let batch = q
        .decode_answers(
            &q.read_answer_sheet_stdin_with(&MockStdin::piped(document))
                .unwrap(),
        )
        .unwrap();

    assert_eq!(interactive, batch);
}

#[test]
#[serial(prompt_responder)]
fn interactive_and_sheet_submissions_decode_identically() {
    let q = questionnaire();

    let _guard = ResponderGuard::install([
        PromptResponse::text("demo"),
        PromptResponse::text("yes"),
        PromptResponse::text("debian:stable"),
        PromptResponse::Skip,
    ]);
    let interactive = q.decode_answers(&q.collect_interactive().unwrap()).unwrap();

    let sheet = answer(
        &answer(&edited_sheet(&q), "project.docker", "yes"),
        "project.docker_image",
        "debian:stable",
    );
    let batch = q
        .decode_answers(
            &q.read_answer_sheet_stdin_with(&MockStdin::piped(sheet))
                .unwrap(),
        )
        .unwrap();

    assert_eq!(interactive, batch);
}
