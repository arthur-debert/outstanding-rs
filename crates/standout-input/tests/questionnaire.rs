//! Pure answer-sheet engine tests: deterministic rendering, round trips,
//! multiline and whitespace behavior, cosmetic edits, demoted question
//! lines, unknown or duplicate tags, tag-fragment warnings, and
//! compatibility failures.

use standout_input::questionnaire::{
    AnswerSheetDiagnostic, Questionnaire, QuestionnaireError, ScalarField, ScalarKind,
};

fn one_field() -> Questionnaire {
    Questionnaire::new(
        "demo.profile",
        vec![ScalarField::new(
            "project.name",
            "What is the project name?",
            ScalarKind::String,
        )],
    )
    .unwrap()
}

fn two_fields() -> Questionnaire {
    Questionnaire::new(
        "demo.profile",
        vec![
            ScalarField::new(
                "project.name",
                "What is the project name?",
                ScalarKind::String,
            ),
            ScalarField::new("project.notes", "Add any notes.", ScalarKind::Text).optional(),
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

// ============================================================================
// Definition validation
// ============================================================================

#[test]
fn definition_rejects_invalid_questionnaire_id() {
    let err = Questionnaire::new(
        "Has Spaces",
        vec![ScalarField::new("a", "A?", ScalarKind::String)],
    )
    .unwrap_err();
    assert_eq!(
        err,
        QuestionnaireError::InvalidQuestionnaireId("Has Spaces".into())
    );

    let err =
        Questionnaire::new("", vec![ScalarField::new("a", "A?", ScalarKind::String)]).unwrap_err();
    assert_eq!(err, QuestionnaireError::InvalidQuestionnaireId("".into()));
}

#[test]
fn definition_rejects_invalid_field_id() {
    let err = Questionnaire::new(
        "demo",
        vec![ScalarField::new("Project.Name", "A?", ScalarKind::String)],
    )
    .unwrap_err();
    assert_eq!(err, QuestionnaireError::InvalidId("Project.Name".into()));
}

#[test]
fn definition_rejects_duplicate_field_ids() {
    let err = Questionnaire::new(
        "demo",
        vec![
            ScalarField::new("a", "First?", ScalarKind::String),
            ScalarField::new("a", "Second?", ScalarKind::Text),
        ],
    )
    .unwrap_err();
    assert_eq!(err, QuestionnaireError::DuplicateId("a".into()));
}

#[test]
fn definition_rejects_empty_field_list() {
    let err = Questionnaire::new("demo", Vec::<ScalarField>::new()).unwrap_err();
    assert_eq!(err, QuestionnaireError::NoFields);
}

// ============================================================================
// Deterministic rendering
// ============================================================================

#[test]
fn rendering_is_deterministic_and_carries_all_declared_parts() {
    let q = two_fields();
    let sheet = q.render_answer_sheet();
    assert_eq!(
        sheet,
        q.render_answer_sheet(),
        "same definition, same bytes"
    );

    let expected = format!(
        "#! standout-answers 1\n\
         #! questionnaire: demo.profile\n\
         #! fingerprint: {}\n\
         \n\
         1. What is the project name? (string) <id:project.name>\n\
         \n\
         2. Add any notes. (text, optional) <id:project.notes>\n",
        q.fingerprint()
    );
    assert_eq!(sheet, expected);
    assert!(q.fingerprint().starts_with("sha256:"));
}

// ============================================================================
// Round trips
// ============================================================================

#[test]
fn blank_sheet_round_trips_to_empty_answers() {
    let q = two_fields();
    let answers = q.parse_answer_sheet(&q.render_answer_sheet()).unwrap();
    assert_eq!(answers.get("project.name"), Some(""));
    assert_eq!(answers.get("project.notes"), Some(""));
    assert_eq!(answers.len(), 2);
    assert!(answers.warnings().is_empty());
}

#[test]
fn scalar_answer_round_trips() {
    let q = one_field();
    let edited = answer(&q.render_answer_sheet(), "project.name", "wizard");
    let answers = q.parse_answer_sheet(&edited).unwrap();
    assert_eq!(answers.get("project.name"), Some("wizard"));
}

#[test]
fn multiline_answer_preserves_internal_breaks_and_trims_outer_whitespace() {
    let q = two_fields();
    let edited = answer(
        &q.render_answer_sheet(),
        "project.notes",
        "\nFirst line.\n\nThird line, after a kept blank.\n\n",
    );
    let answers = q.parse_answer_sheet(&edited).unwrap();
    assert_eq!(
        answers.get("project.notes"),
        Some("First line.\n\nThird line, after a kept blank.")
    );
}

#[test]
fn a_blank_line_between_question_and_answer_still_binds_the_answer() {
    // Regression: the old format's header/marker adjacency rule silently
    // swallowed a header separated from its answer by a blank line. The
    // answer is simply everything up to the next question line.
    let q = one_field();
    let edited = q
        .render_answer_sheet()
        .replace("<id:project.name>\n", "<id:project.name>\n\nwizard\n");
    let answers = q.parse_answer_sheet(&edited).unwrap();
    assert_eq!(answers.get("project.name"), Some("wizard"));
}

#[test]
fn bracketed_prose_and_marker_bullets_inside_an_answer_are_inert() {
    // Regression: under the old format, bracketed IDs and `->` lines were
    // structural. Both are now ordinary answer text.
    let q = two_fields();
    let edited = answer(
        &q.render_answer_sheet(),
        "project.notes",
        "see [project.name] above\n-> a bullet, not a marker\nand [some prose] too",
    );
    let answers = q.parse_answer_sheet(&edited).unwrap();
    assert_eq!(
        answers.get("project.notes"),
        Some("see [project.name] above\n-> a bullet, not a marker\nand [some prose] too")
    );
    assert!(answers.warnings().is_empty());
}

// ============================================================================
// Cosmetic freedom: numbers, wording, indentation, hints
// ============================================================================

#[test]
fn display_edits_do_not_change_parsing() {
    let q = two_fields();
    let cosmetic = format!(
        "#! standout-answers 1\n\
         #! questionnaire: demo.profile\n\
         #! fingerprint: {}\n\
         \n\
         Some guidance prose the renderer never wrote.\n\
         \n\
         99. Totally reworded question! (whatever [hint], with -> and brackets) <id:project.name>\n\
         demo\n\
         \n\
         \t 1.1 Indented and renumbered. <id:project.notes>\n\
         \t noted\n",
        q.fingerprint()
    );
    let answers = q.parse_answer_sheet(&cosmetic).unwrap();
    assert_eq!(answers.get("project.name"), Some("demo"));
    assert_eq!(answers.get("project.notes"), Some("noted"));
}

#[test]
fn field_order_in_document_is_cosmetic() {
    let q = two_fields();
    let reordered = format!(
        "#! standout-answers 1\n\
         #! questionnaire: demo.profile\n\
         #! fingerprint: {}\n\
         \n\
         2. Add any notes. (text, optional) <id:project.notes>\n\
         noted\n\
         \n\
         1. What is the project name? (string) <id:project.name>\n\
         demo\n",
        q.fingerprint()
    );
    let answers = q.parse_answer_sheet(&reordered).unwrap();
    assert_eq!(answers.get("project.name"), Some("demo"));
    assert_eq!(answers.get("project.notes"), Some("noted"));
}

// ============================================================================
// Demotion and the accepted misparse limitation
// ============================================================================

#[test]
fn trailing_text_after_the_tag_demotes_the_line_to_prose() {
    // Regression: a trailing annotation after the ID must not
    // half-recognize the line; any non-blank character after the tag makes
    // the whole line ordinary prose (which then trips the tag-fragment
    // warning as a hint that a tag may have been mangled).
    let q = two_fields();
    let edited = answer(
        &q.render_answer_sheet(),
        "project.notes",
        "quoting a question line:\n1. What is the project name? <id:project.name> (see above)\ndone",
    );
    let answers = q.parse_answer_sheet(&edited).unwrap();
    assert_eq!(
        answers.get("project.notes"),
        Some("quoting a question line:\n1. What is the project name? <id:project.name> (see above)\ndone")
    );
    assert_eq!(answers.get("project.name"), Some(""));
    assert_eq!(answers.warnings().len(), 1);
}

#[test]
fn an_answer_line_ending_with_a_valid_tag_is_misparsed_by_design() {
    // The documented limitation: there is no escaping, so an answer line
    // that itself ends with a schema-valid tag reads as a question line.
    let q = two_fields();
    let sheet = format!(
        "#! standout-answers 1\n\
         #! questionnaire: demo.profile\n\
         #! fingerprint: {}\n\
         \n\
         1. What is the project name? (string) <id:project.name>\n\
         this prose line ends with <id:project.notes>\n",
        q.fingerprint()
    );
    let answers = q.parse_answer_sheet(&sheet).unwrap();
    assert_eq!(answers.get("project.name"), Some(""));
    assert_eq!(answers.get("project.notes"), Some(""));
}

// ============================================================================
// Tag-fragment warnings
// ============================================================================

#[test]
fn a_tag_fragment_inside_an_answer_raises_a_warning() {
    let q = two_fields();
    let edited = answer(
        &q.render_answer_sheet(),
        "project.notes",
        "mentions <id:project.name> mid-line\nand a mangled <id:project.na",
    );
    let answers = q.parse_answer_sheet(&edited).unwrap();
    assert_eq!(
        answers.get("project.notes"),
        Some("mentions <id:project.name> mid-line\nand a mangled <id:project.na")
    );
    assert_eq!(answers.warnings().len(), 2, "{:?}", answers.warnings());
    assert!(answers.warnings().iter().all(|w| matches!(
        w,
        AnswerSheetDiagnostic::SuspectedTagInAnswer { path, .. } if path == "project.notes"
    )));
    let message = answers.warnings()[0].to_string();
    assert!(message.contains("warning"), "{message}");
    assert!(
        !message.contains("mid-line"),
        "warnings never echo answer text: {message}"
    );
}

// ============================================================================
// Fingerprint semantics
// ============================================================================

#[test]
fn fingerprint_ignores_wording_and_field_order() {
    let base = two_fields();
    let reworded = Questionnaire::new(
        "demo.profile",
        vec![
            ScalarField::new("project.name", "Reworded entirely?", ScalarKind::String),
            ScalarField::new("project.notes", "Different help text.", ScalarKind::Text).optional(),
        ],
    )
    .unwrap();
    assert_eq!(base.fingerprint(), reworded.fingerprint());

    let reordered = Questionnaire::new(
        "demo.profile",
        vec![
            ScalarField::new("project.notes", "Add any notes.", ScalarKind::Text).optional(),
            ScalarField::new(
                "project.name",
                "What is the project name?",
                ScalarKind::String,
            ),
        ],
    )
    .unwrap();
    assert_eq!(base.fingerprint(), reordered.fingerprint());
}

#[test]
fn fingerprint_changes_on_semantic_edits() {
    let base = two_fields();

    let renamed = Questionnaire::new(
        "demo.profile",
        vec![
            ScalarField::new(
                "project.title",
                "What is the project name?",
                ScalarKind::String,
            ),
            ScalarField::new("project.notes", "Add any notes.", ScalarKind::Text).optional(),
        ],
    )
    .unwrap();
    assert_ne!(base.fingerprint(), renamed.fingerprint());

    let rekinded = Questionnaire::new(
        "demo.profile",
        vec![
            ScalarField::new(
                "project.name",
                "What is the project name?",
                ScalarKind::Text,
            ),
            ScalarField::new("project.notes", "Add any notes.", ScalarKind::Text).optional(),
        ],
    )
    .unwrap();
    assert_ne!(base.fingerprint(), rekinded.fingerprint());

    let required = Questionnaire::new(
        "demo.profile",
        vec![
            ScalarField::new(
                "project.name",
                "What is the project name?",
                ScalarKind::String,
            ),
            ScalarField::new("project.notes", "Add any notes.", ScalarKind::Text),
        ],
    )
    .unwrap();
    assert_ne!(base.fingerprint(), required.fingerprint());

    let other_questionnaire = Questionnaire::new(
        "demo.other",
        vec![
            ScalarField::new(
                "project.name",
                "What is the project name?",
                ScalarKind::String,
            ),
            ScalarField::new("project.notes", "Add any notes.", ScalarKind::Text).optional(),
        ],
    )
    .unwrap();
    assert_ne!(base.fingerprint(), other_questionnaire.fingerprint());
}

// ============================================================================
// Compatibility failures
// ============================================================================

#[test]
fn wrong_answer_format_version_is_rejected() {
    let q = one_field();
    let sheet = q
        .render_answer_sheet()
        .replace("#! standout-answers 1", "#! standout-answers 2");
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(
        diags,
        vec![AnswerSheetDiagnostic::UnsupportedAnswerFormat { found: "2".into() }]
    );
    assert!(diags[0].to_string().contains("Render a fresh answer sheet"));
}

#[test]
fn wrong_questionnaire_id_is_rejected() {
    let q = one_field();
    let sheet = q
        .render_answer_sheet()
        .replace("questionnaire: demo.profile", "questionnaire: other.app");
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(
        diags,
        vec![AnswerSheetDiagnostic::QuestionnaireMismatch {
            expected: "demo.profile".into(),
            found: "other.app".into(),
        }]
    );
    assert!(diags[0].to_string().contains("Render a fresh answer sheet"));
}

#[test]
fn stale_fingerprint_is_rejected_not_migrated() {
    let old = one_field();
    let sheet = answer(&old.render_answer_sheet(), "project.name", "kept");
    // The application later renames the field: a semantic change.
    let new = Questionnaire::new(
        "demo.profile",
        vec![ScalarField::new(
            "project.title",
            "What is the project name?",
            ScalarKind::String,
        )],
    )
    .unwrap();
    let diags = new.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(
        diags,
        vec![AnswerSheetDiagnostic::FingerprintMismatch {
            expected: new.fingerprint().to_string(),
            found: old.fingerprint().to_string(),
        }]
    );
    let message = diags[0].to_string();
    assert!(message.contains("render a fresh answer sheet"));
    assert!(message.contains("not migrated"));
}

#[test]
fn malformed_preamble_is_diagnosed_per_line() {
    let q = one_field();

    let diags = q.parse_answer_sheet("").unwrap_err();
    assert!(matches!(
        diags[0],
        AnswerSheetDiagnostic::MalformedPreamble { line: 1, .. }
    ));

    let diags = q
        .parse_answer_sheet("not a preamble\nstill not\nnope\n")
        .unwrap_err();
    assert_eq!(diags.len(), 3, "each preamble line diagnosed: {diags:?}");
    assert!(diags
        .iter()
        .all(|d| matches!(d, AnswerSheetDiagnostic::MalformedPreamble { .. })));
}

#[test]
fn compatibility_failure_skips_body_parsing() {
    let q = one_field();
    let sheet = q
        .render_answer_sheet()
        .replace("#! standout-answers 1", "#! standout-answers 9")
        .replace("<id:project.name>", "<id:unknown.field>");
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    // Only the compatibility diagnostic: the body is never interpreted.
    assert_eq!(
        diags,
        vec![AnswerSheetDiagnostic::UnsupportedAnswerFormat { found: "9".into() }]
    );
}

// ============================================================================
// Unknown and duplicate tags
// ============================================================================

#[test]
fn unknown_tag_on_a_question_line_is_a_diagnostic() {
    let q = one_field();
    let sheet = format!(
        "{}\n3. Bonus question? (string) <id:project.bonus>\nsurprise\n",
        q.render_answer_sheet()
    );
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(
        diags,
        vec![AnswerSheetDiagnostic::UnknownTag {
            id: "project.bonus".into(),
            line: 7,
        }]
    );
}

#[test]
fn duplicate_question_line_is_a_diagnostic() {
    let q = one_field();
    let sheet = format!(
        "{}\n1. What is the project name? (string) <id:project.name>\nagain\n",
        q.render_answer_sheet()
    );
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(
        diags,
        vec![AnswerSheetDiagnostic::DuplicateField {
            path: "project.name".into(),
            line: 7,
        }]
    );
}

#[test]
fn diagnostics_accumulate_across_the_body() {
    let q = two_fields();
    let sheet = format!(
        "{}\n\
         9. Mystery. (string) <id:who.knows>\n\
         x\n\
         \n\
         1. Again. (string) <id:project.name>\n\
         y\n",
        answer(&q.render_answer_sheet(), "project.name", "first")
    );
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(diags.len(), 2, "both problems reported: {diags:?}");
    assert!(matches!(diags[0], AnswerSheetDiagnostic::UnknownTag { .. }));
    assert!(matches!(
        diags[1],
        AnswerSheetDiagnostic::DuplicateField { .. }
    ));
}
