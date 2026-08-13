//! Shared decode/validation engine tests: extended scalar definitions
//! (kinds, defaults, constraints, conditions, validator revisions),
//! fingerprint coverage of every semantic property, default pre-filling,
//! blank resolution, conditional applicability, and accumulated batch
//! diagnostics.

use standout_input::questionnaire::{
    AnswerValue, FieldValidator, FormError, Questionnaire, QuestionnaireError, ScalarField,
    ScalarKind, ValidationDiagnostic,
};

/// A questionnaire exercising every scalar property: kinds, optionality,
/// defaults, choices, a condition, and an application validator.
fn full() -> Questionnaire {
    Questionnaire::new(
        "demo.full",
        vec![
            ScalarField::new("project.name", "Project name?", ScalarKind::String).with_validator(
                FieldValidator::new("name-rules-1", |value| {
                    let name = value.as_text().unwrap_or_default();
                    if name.contains(' ') {
                        Err("the name must not contain spaces".to_string())
                    } else {
                        Ok(())
                    }
                }),
            ),
            ScalarField::new("project.license", "License?", ScalarKind::String)
                .one_of(["mit", "bsd", "gpl"])
                .with_default("mit"),
            ScalarField::new("project.docker", "Use Docker?", ScalarKind::Bool).with_default("no"),
            ScalarField::new("project.docker_image", "Base image?", ScalarKind::Path)
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

fn decode(
    q: &Questionnaire,
    sheet: &str,
) -> Result<standout_input::questionnaire::Answers, Vec<ValidationDiagnostic>> {
    let raw = q.parse_answer_sheet(sheet).expect("sheet parses");
    q.decode_answers(&raw)
}

// ============================================================================
// Definition validation for the extended properties
// ============================================================================

#[test]
fn choices_on_bool_field_are_rejected() {
    let err = Questionnaire::new(
        "demo.q",
        vec![ScalarField::new("a", "A?", ScalarKind::Bool).one_of(["x", "y"])],
    )
    .unwrap_err();
    assert!(matches!(err, QuestionnaireError::InvalidConstraint { id, .. } if id == "a"));
}

#[test]
fn empty_and_duplicate_choices_are_rejected() {
    // Answers are trimmed before matching, so a choice with outer whitespace
    // is unsatisfiable (and would shadow its trimmed twin in dup detection).
    for choices in [vec![], vec!["x", "x"], vec![" x", "y"], vec!["x", "x "]] {
        let err = Questionnaire::new(
            "demo.q",
            vec![ScalarField::new("a", "A?", ScalarKind::String).one_of(choices)],
        )
        .unwrap_err();
        assert!(matches!(err, QuestionnaireError::InvalidConstraint { .. }));
    }
}

#[test]
fn default_must_decode_cleanly() {
    // A bool default outside the yes/no vocabulary.
    let err = Questionnaire::new(
        "demo.q",
        vec![ScalarField::new("a", "A?", ScalarKind::Bool).with_default("maybe")],
    )
    .unwrap_err();
    assert!(matches!(err, QuestionnaireError::InvalidDefault { id, .. } if id == "a"));

    // A default outside the declared choices.
    let err = Questionnaire::new(
        "demo.q",
        vec![ScalarField::new("a", "A?", ScalarKind::String)
            .one_of(["x", "y"])
            .with_default("z")],
    )
    .unwrap_err();
    assert!(matches!(err, QuestionnaireError::InvalidDefault { .. }));

    // A multiline default cannot render as a single pre-filled line.
    let err = Questionnaire::new(
        "demo.q",
        vec![ScalarField::new("a", "A?", ScalarKind::Text).with_default("one\ntwo")],
    )
    .unwrap_err();
    assert!(matches!(err, QuestionnaireError::InvalidDefault { .. }));

    // A default with outer whitespace could never survive the render/parse
    // round trip (parsed answers are trimmed).
    let err = Questionnaire::new(
        "demo.q",
        vec![ScalarField::new("a", "A?", ScalarKind::String).with_default(" x ")],
    )
    .unwrap_err();
    assert!(matches!(err, QuestionnaireError::InvalidDefault { .. }));
}

#[test]
fn condition_must_reference_an_earlier_known_field() {
    let err = Questionnaire::new(
        "demo.q",
        vec![ScalarField::new("a", "A?", ScalarKind::String).active_when("ghost", "x")],
    )
    .unwrap_err();
    assert!(
        matches!(err, QuestionnaireError::UnknownConditionController { controller, .. } if controller == "ghost")
    );

    let err = Questionnaire::new(
        "demo.q",
        vec![
            ScalarField::new("a", "A?", ScalarKind::String).active_when("b", "x"),
            ScalarField::new("b", "B?", ScalarKind::String),
        ],
    )
    .unwrap_err();
    assert!(
        matches!(err, QuestionnaireError::ConditionOrder { controller, .. } if controller == "b")
    );
}

#[test]
fn condition_expected_value_must_be_reachable() {
    // A bool controller with a non-boolean expected value.
    let err = Questionnaire::new(
        "demo.q",
        vec![
            ScalarField::new("a", "A?", ScalarKind::Bool),
            ScalarField::new("b", "B?", ScalarKind::String).active_when("a", "sometimes"),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, QuestionnaireError::InvalidCondition { .. }));

    // A constrained controller whose choices never include the expected value.
    let err = Questionnaire::new(
        "demo.q",
        vec![
            ScalarField::new("a", "A?", ScalarKind::String).one_of(["x", "y"]),
            ScalarField::new("b", "B?", ScalarKind::String).active_when("a", "z"),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, QuestionnaireError::InvalidCondition { .. }));

    // An unconstrained controller never decodes to a blank value or one with
    // outer whitespace (answers are trimmed, blanks resolve to omission).
    for expected in ["", " x "] {
        let err = Questionnaire::new(
            "demo.q",
            vec![
                ScalarField::new("a", "A?", ScalarKind::String),
                ScalarField::new("b", "B?", ScalarKind::String).active_when("a", expected),
            ],
        )
        .unwrap_err();
        assert!(matches!(err, QuestionnaireError::InvalidCondition { .. }));
    }
}

#[test]
fn validator_revision_must_be_non_empty() {
    let err = Questionnaire::new(
        "demo.q",
        vec![ScalarField::new("a", "A?", ScalarKind::String)
            .with_validator(FieldValidator::new("", |_| Ok(())))],
    )
    .unwrap_err();
    assert!(matches!(err, QuestionnaireError::EmptyValidatorRevision { id } if id == "a"));
}

// ============================================================================
// Fingerprint: every semantic property participates; presentation does not
// ============================================================================

fn fp(fields: Vec<ScalarField>) -> String {
    Questionnaire::new("demo.q", fields)
        .unwrap()
        .fingerprint()
        .to_string()
}

#[test]
fn default_constraint_condition_and_revision_all_enter_the_fingerprint() {
    let base = || ScalarField::new("a", "A?", ScalarKind::String);
    let plain = fp(vec![base()]);

    assert_ne!(plain, fp(vec![base().with_default("x")]));
    assert_ne!(
        fp(vec![base().with_default("x")]),
        fp(vec![base().with_default("y")])
    );
    assert_ne!(plain, fp(vec![base().one_of(["x", "y"])]));
    assert_ne!(
        plain,
        fp(vec![
            base().with_validator(FieldValidator::new("v1", |_| Ok(())))
        ])
    );
    assert_ne!(
        fp(vec![
            base().with_validator(FieldValidator::new("v1", |_| Ok(())))
        ]),
        fp(vec![
            base().with_validator(FieldValidator::new("v2", |_| Ok(())))
        ])
    );

    let with_condition = |expected: &str| {
        fp(vec![
            ScalarField::new("c", "C?", ScalarKind::String),
            ScalarField::new("a", "A?", ScalarKind::String).active_when("c", expected),
        ])
    };
    let without_condition = fp(vec![
        ScalarField::new("c", "C?", ScalarKind::String),
        ScalarField::new("a", "A?", ScalarKind::String),
    ]);
    assert_ne!(without_condition, with_condition("x"));
    assert_ne!(with_condition("x"), with_condition("y"));
}

#[test]
fn presentation_only_properties_stay_out_of_the_fingerprint() {
    // Choice order is presentation; the choice set is semantic.
    assert_eq!(
        fp(vec![
            ScalarField::new("a", "A?", ScalarKind::String).one_of(["x", "y"])
        ]),
        fp(vec![
            ScalarField::new("a", "Reworded?", ScalarKind::String).one_of(["y", "x"])
        ])
    );
    // A validator closure's behavior is invisible; only the revision counts.
    assert_eq!(
        fp(vec![ScalarField::new("a", "A?", ScalarKind::String)
            .with_validator(FieldValidator::new("v1", |_| Ok(())))]),
        fp(vec![ScalarField::new("a", "A?", ScalarKind::String)
            .with_validator(FieldValidator::new("v1", |_| Err(
                "no".to_string()
            )))])
    );
}

#[test]
fn bool_condition_values_are_canonicalized_for_identity() {
    let declare = |expected: &str| {
        fp(vec![
            ScalarField::new("c", "C?", ScalarKind::Bool),
            ScalarField::new("a", "A?", ScalarKind::String).active_when("c", expected),
        ])
    };
    assert_eq!(declare("yes"), declare("true"));
    assert_eq!(declare("yes"), declare("Y"));
    assert_ne!(declare("yes"), declare("no"));
}

// ============================================================================
// Rendering: defaults pre-filled, hints stay cosmetic
// ============================================================================

#[test]
fn defaults_render_pre_filled_and_round_trip() {
    let q = full();
    let sheet = q.render_answer_sheet();
    assert!(sheet.contains("(mit, bsd, or gpl) <id:project.license>\nmit\n"));
    assert!(sheet.contains("<id:project.docker>\nno\n"));

    // An untouched sheet decodes: name is missing (required, no default),
    // everything else resolves through defaults and omission.
    let filled = answer(&sheet, "project.name", "demo");
    let answers = decode(&q, &filled).unwrap();
    assert_eq!(answers.get_text("project.name"), Some("demo"));
    assert_eq!(answers.get_text("project.license"), Some("mit"));
    assert_eq!(answers.get_bool("project.docker"), Some(false));
    assert_eq!(answers.get("project.docker_image"), None);
    assert_eq!(answers.get("project.notes"), None);
}

// ============================================================================
// Blank resolution: default first, then optionality
// ============================================================================

#[test]
fn blank_resolves_default_before_optionality() {
    let q = full();
    let sheet = q.render_answer_sheet();
    // Blank out the pre-filled license default entirely.
    let blanked = answer(&sheet, "project.license", "");
    let filled = answer(&blanked, "project.name", "demo");
    let answers = decode(&q, &filled).unwrap();
    assert_eq!(answers.get_text("project.license"), Some("mit"));
}

#[test]
fn required_blank_without_default_is_missing() {
    let q = full();
    let sheet = q.render_answer_sheet();
    let diagnostics = decode(&q, &sheet).unwrap_err();
    assert_eq!(
        diagnostics,
        vec![ValidationDiagnostic::MissingAnswer {
            id: "project.name".to_string()
        }]
    );
}

// ============================================================================
// Kind conversion and constraints
// ============================================================================

#[test]
fn bool_vocabulary_is_shared_and_case_insensitive() {
    let q = full();
    let sheet = answer(&q.render_answer_sheet(), "project.name", "demo");
    for (text, expected) in [("TRUE", true), ("Yes", true), ("y", true), ("n", false)] {
        let answers = decode(&q, &answer(&sheet, "project.docker", text));
        // Answering docker=true activates the image question, which is then
        // missing — so only check the docker decode for the true cases.
        match answers {
            Ok(a) => assert_eq!(a.get_bool("project.docker"), Some(expected)),
            Err(d) => {
                assert!(expected, "false must not activate the image question");
                assert_eq!(
                    d,
                    vec![ValidationDiagnostic::MissingAnswer {
                        id: "project.docker_image".to_string()
                    }]
                );
            }
        }
    }
}

#[test]
fn invalid_bool_and_multiline_string_are_conversion_errors() {
    let q = full();
    let sheet = answer(&q.render_answer_sheet(), "project.name", "demo");
    let diagnostics = decode(&q, &answer(&sheet, "project.docker", "maybe")).unwrap_err();
    assert!(matches!(
        &diagnostics[..],
        [ValidationDiagnostic::InvalidValue { id, .. }] if id == "project.docker"
    ));

    let diagnostics = decode(&q, &answer(&sheet, "project.name", "two\nlines")).unwrap_err();
    assert!(matches!(
        &diagnostics[..],
        [ValidationDiagnostic::InvalidValue { id, .. }] if id == "project.name"
    ));
}

#[test]
fn constraint_violations_report_the_choices_not_the_value() {
    let q = full();
    let sheet = answer(&q.render_answer_sheet(), "project.name", "demo");
    let diagnostics = decode(&q, &answer(&sheet, "project.license", "wtfpl")).unwrap_err();
    assert_eq!(
        diagnostics,
        vec![ValidationDiagnostic::ConstraintViolation {
            id: "project.license".to_string(),
            allowed: vec!["mit".to_string(), "bsd".to_string(), "gpl".to_string()],
        }]
    );
    assert!(!diagnostics[0].to_string().contains("wtfpl"));
}

#[test]
fn application_validator_runs_in_the_shared_pipeline() {
    let q = full();
    let sheet = answer(&q.render_answer_sheet(), "project.name", "has spaces");
    let diagnostics = decode(&q, &sheet).unwrap_err();
    assert_eq!(
        diagnostics,
        vec![ValidationDiagnostic::FieldValidation {
            id: "project.name".to_string(),
            message: "the name must not contain spaces".to_string(),
        }]
    );
}

// ============================================================================
// Conditional applicability
// ============================================================================

#[test]
fn active_required_conditional_field_must_be_answered() {
    let q = full();
    let sheet = answer(&q.render_answer_sheet(), "project.name", "demo");
    let diagnostics = decode(&q, &answer(&sheet, "project.docker", "yes")).unwrap_err();
    assert_eq!(
        diagnostics,
        vec![ValidationDiagnostic::MissingAnswer {
            id: "project.docker_image".to_string()
        }]
    );

    let filled = answer(
        &answer(&sheet, "project.docker", "yes"),
        "project.docker_image",
        "debian:stable",
    );
    let answers = decode(&q, &filled).unwrap();
    assert_eq!(
        answers.get_text("project.docker_image"),
        Some("debian:stable")
    );
}

#[test]
fn populated_inactive_field_is_an_error() {
    let q = full();
    let sheet = answer(&q.render_answer_sheet(), "project.name", "demo");
    // docker stays "no" (default): the image question is inactive.
    let stale = answer(&sheet, "project.docker_image", "debian:stable");
    let diagnostics = decode(&q, &stale).unwrap_err();
    assert_eq!(
        diagnostics,
        vec![ValidationDiagnostic::InactiveAnswered {
            id: "project.docker_image".to_string(),
            controller: "project.docker".to_string(),
            expected: "true".to_string(),
        }]
    );
}

#[test]
fn inactive_field_keeps_untouched_pre_filled_default() {
    let q = Questionnaire::new(
        "demo.q",
        vec![
            ScalarField::new("a", "A?", ScalarKind::Bool).with_default("no"),
            ScalarField::new("b", "B?", ScalarKind::String)
                .with_default("fallback")
                .active_when("a", "yes"),
        ],
    )
    .unwrap();
    // Untouched sheet: b's pre-filled default remains while b is inactive.
    let answers = decode(&q, &q.render_answer_sheet()).unwrap();
    assert_eq!(answers.get("b"), None);
}

#[test]
fn errored_controller_skips_dependents_without_piling_on() {
    let q = full();
    let sheet = answer(&q.render_answer_sheet(), "project.name", "demo");
    // The controller fails to decode; its dependent must not add noise.
    let diagnostics = decode(&q, &answer(&sheet, "project.docker", "maybe")).unwrap_err();
    assert_eq!(diagnostics.len(), 1);
}

// ============================================================================
// Accumulation and whole-form validation
// ============================================================================

#[test]
fn independent_diagnostics_accumulate_in_one_pass() {
    let q = full();
    let sheet = q.render_answer_sheet(); // name left blank: missing
    let bad = answer(&sheet, "project.license", "wtfpl"); // constraint violation
    let diagnostics = decode(&q, &bad).unwrap_err();
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics
        .iter()
        .any(|d| matches!(d, ValidationDiagnostic::MissingAnswer { id } if id == "project.name")));
    assert!(diagnostics.iter().any(
        |d| matches!(d, ValidationDiagnostic::ConstraintViolation { id, .. } if id == "project.license")
    ));
}

#[test]
fn form_errors_accumulate_after_a_clean_field_stage() {
    let q = full();
    let sheet = answer(&q.render_answer_sheet(), "project.name", "demo");
    let raw = q.parse_answer_sheet(&sheet).unwrap();

    let diagnostics = q
        .decode_answers_with(&raw, |_| {
            vec![
                FormError::new(["project.name"], "the name is already taken"),
                FormError::new(Vec::<String>::new(), "the project limit is reached"),
            ]
        })
        .unwrap_err();
    assert_eq!(
        diagnostics,
        vec![
            ValidationDiagnostic::Form {
                fields: vec!["project.name".to_string()],
                message: "the name is already taken".to_string(),
            },
            ValidationDiagnostic::Form {
                fields: vec![],
                message: "the project limit is reached".to_string(),
            },
        ]
    );
    assert_eq!(
        diagnostics[0].to_string(),
        "the name is already taken (fields: project.name)"
    );
}

#[test]
fn form_validation_does_not_run_over_broken_fields() {
    let q = full();
    let raw = q.parse_answer_sheet(&q.render_answer_sheet()).unwrap();
    let diagnostics = q
        .decode_answers_with(&raw, |_| panic!("form must not run on field failures"))
        .unwrap_err();
    assert!(matches!(
        &diagnostics[..],
        [ValidationDiagnostic::MissingAnswer { .. }]
    ));
}

#[test]
fn form_success_passes_the_answers_through() {
    let q = full();
    let sheet = answer(&q.render_answer_sheet(), "project.name", "demo");
    let raw = q.parse_answer_sheet(&sheet).unwrap();
    let answers = q.decode_answers_with(&raw, |_| Vec::new()).unwrap();
    assert_eq!(answers.get_text("project.name"), Some("demo"));
}

// ============================================================================
// Decoded value surface
// ============================================================================

#[test]
fn answer_values_expose_typed_accessors() {
    let text = AnswerValue::Text("x".to_string());
    let boolean = AnswerValue::Bool(true);
    assert_eq!(text.as_text(), Some("x"));
    assert_eq!(text.as_bool(), None);
    assert_eq!(boolean.as_bool(), Some(true));
    assert_eq!(boolean.as_text(), None);
}
