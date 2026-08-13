//! Pure answer-sheet engine tests: deterministic rendering, round trips,
//! multiline and whitespace behavior, cosmetic edits, malformed headers,
//! unknown or duplicate IDs, and compatibility failures.

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

/// Replace a field's bare marker with a marker carrying `answer`.
fn answer(sheet: &str, id: &str, answer: &str) -> String {
    let needle = format!("[{id}]");
    let mut out = String::new();
    let mut lines = sheet.lines().peekable();
    while let Some(line) = lines.next() {
        out.push_str(line);
        out.push('\n');
        if line.contains(&needle) {
            let marker = lines.next().expect("marker follows header");
            assert_eq!(marker, "->");
            out.push_str("-> ");
            out.push_str(answer);
            out.push('\n');
        }
    }
    out
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
         1. What is the project name? [project.name] (string)\n\
         ->\n\
         \n\
         2. Add any notes. [project.notes] (text, optional)\n\
         ->\n",
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
fn bracketed_prose_inside_answer_is_not_a_field() {
    let q = two_fields();
    // Contains a bracketed known ID and a bracketed unknown token, neither
    // followed by a marker line: both are ordinary answer text.
    let edited = answer(
        &q.render_answer_sheet(),
        "project.notes",
        "see [project.name] above\nand [some prose] too",
    );
    let answers = q.parse_answer_sheet(&edited).unwrap();
    assert_eq!(
        answers.get("project.notes"),
        Some("see [project.name] above\nand [some prose] too")
    );
}

#[test]
fn answer_on_marker_line_and_following_lines_belong_together() {
    let q = one_field();
    let sheet = q.render_answer_sheet();
    let edited = sheet.replace("->\n", "->   spaced out   \n");
    let answers = q.parse_answer_sheet(&edited).unwrap();
    assert_eq!(answers.get("project.name"), Some("spaced out"));
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
         99. Totally reworded question! [project.name] (whatever hint)\n\
         -> demo\n\
         \n\
         \t 1.1 Indented and renumbered. [project.notes]\n\
         \t -> noted\n",
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
         2. Add any notes. [project.notes] (text, optional)\n\
         -> noted\n\
         \n\
         1. What is the project name? [project.name] (string)\n\
         -> demo\n",
        q.fingerprint()
    );
    let answers = q.parse_answer_sheet(&reordered).unwrap();
    assert_eq!(answers.get("project.name"), Some("demo"));
    assert_eq!(answers.get("project.notes"), Some("noted"));
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
        .replace("[project.name]", "[unknown.field]");
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    // Only the compatibility diagnostic: the body is never interpreted.
    assert_eq!(
        diags,
        vec![AnswerSheetDiagnostic::UnsupportedAnswerFormat { found: "9".into() }]
    );
}

// ============================================================================
// Unknown and duplicate IDs
// ============================================================================

#[test]
fn unknown_id_in_header_shaped_line_is_a_diagnostic() {
    let q = one_field();
    let sheet = format!(
        "{}\n3. Bonus question? [project.bonus] (string)\n-> surprise\n",
        q.render_answer_sheet()
    );
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(
        diags,
        vec![AnswerSheetDiagnostic::UnknownFieldId {
            id: "project.bonus".into(),
            line: 8,
        }]
    );
}

#[test]
fn duplicate_field_header_is_a_diagnostic() {
    let q = one_field();
    let sheet = format!(
        "{}\n1. What is the project name? [project.name] (string)\n-> again\n",
        q.render_answer_sheet()
    );
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(
        diags,
        vec![AnswerSheetDiagnostic::DuplicateField {
            path: "project.name".into(),
            line: 8,
        }]
    );
}

#[test]
fn diagnostics_accumulate_across_the_body() {
    let q = two_fields();
    let sheet = format!(
        "{}\n\
         9. Mystery. [who.knows] (string)\n\
         -> x\n\
         \n\
         1. Again. [project.name] (string)\n\
         -> y\n",
        answer(&q.render_answer_sheet(), "project.name", "first")
    );
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(diags.len(), 2, "both problems reported: {diags:?}");
    assert!(matches!(
        diags[0],
        AnswerSheetDiagnostic::UnknownFieldId { .. }
    ));
    assert!(matches!(
        diags[1],
        AnswerSheetDiagnostic::DuplicateField { .. }
    ));
}

#[test]
fn header_without_marker_is_ordinary_text() {
    let q = two_fields();
    // A known-ID, header-shaped-looking line with no following marker line is
    // absorbed into the surrounding answer, not treated as a field.
    let edited = answer(
        &q.render_answer_sheet(),
        "project.notes",
        "quoting the header:\n1. What is the project name? [project.name] (string)\ndone",
    );
    let answers = q.parse_answer_sheet(&edited).unwrap();
    assert_eq!(
        answers.get("project.notes"),
        Some("quoting the header:\n1. What is the project name? [project.name] (string)\ndone")
    );
    assert_eq!(answers.get("project.name"), Some(""));
}
