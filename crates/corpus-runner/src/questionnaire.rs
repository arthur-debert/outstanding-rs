// The exit questionnaire: rendered into the blind workspace at provision
// time, answered by the agent in place, decoded via `standout-input`.

use std::path::Path;

use standout_input::questionnaire::{Questionnaire, ScalarField, ScalarKind, ValidationDiagnostic};

use crate::report::QuestionnaireReport;

pub const SHEET_FILENAME: &str = "QUESTIONNAIRE.md";

// A test pins these to `definition`.
pub const FIELD_IDS: &[&str] = &[
    "summary",
    "sources.docs",
    "sources.external",
    "friction",
    "docs.gaps",
    "workarounds",
    "confidence",
    "confidence_reason",
];

pub fn definition() -> Questionnaire {
    Questionnaire::new(
        "corpus.exit",
        vec![
            ScalarField::new(
                "summary",
                "Summarize what you built and what state it is in.",
                ScalarKind::Text,
            ),
            ScalarField::new(
                "sources.docs",
                "Which of the provided documentation files (under docs/) did you \
                 actually consult? List their paths.",
                ScalarKind::Text,
            ),
            ScalarField::new(
                "sources.external",
                "Did you rely on anything beyond the provided docs/ — web search, \
                 prior knowledge of standout internals, other repositories or \
                 source code? Say exactly what, or answer 'none'.",
                ScalarKind::Text,
            ),
            ScalarField::new(
                "friction",
                "Friction points, if any: for each, quote the exact command, error \
                 message, or doc passage that caused it.",
                ScalarKind::Text,
            )
            .optional(),
            ScalarField::new(
                "docs.gaps",
                "Places the documentation was wrong or missing, if any: quote what \
                 the doc says and what actually happened.",
                ScalarKind::Text,
            )
            .optional(),
            ScalarField::new(
                "workarounds",
                "Workarounds you left in the produced code, if any.",
                ScalarKind::Text,
            )
            .optional(),
            ScalarField::new(
                "confidence",
                "How confident are you that the acceptance criteria in SPEC.md pass?",
                ScalarKind::String,
            )
            .one_of(["low", "medium", "high"]),
            ScalarField::new(
                "confidence_reason",
                "Why? Name the gaps or edge cases, if any.",
                ScalarKind::Text,
            ),
        ],
    )
    .expect("static corpus.exit questionnaire definition is valid")
}

pub fn collect(workspace: &Path) -> QuestionnaireReport {
    let sheet_path = workspace.join(SHEET_FILENAME);
    let text = match std::fs::read_to_string(&sheet_path) {
        Ok(text) => text,
        Err(err) => {
            return QuestionnaireReport {
                collected: false,
                diagnostics: vec![format!("could not read {}: {err}", sheet_path.display())],
                answers: Default::default(),
            }
        }
    };

    let questionnaire = definition();
    let raw = match questionnaire.parse_answer_sheet(&text) {
        Ok(raw) => raw,
        Err(diagnostics) => {
            return QuestionnaireReport {
                collected: false,
                diagnostics: diagnostics.iter().map(ToString::to_string).collect(),
                answers: Default::default(),
            }
        }
    };
    let warnings: Vec<String> = raw.warnings().iter().map(ToString::to_string).collect();

    match questionnaire.decode_answers(&raw) {
        Ok(answers) => QuestionnaireReport {
            collected: true,
            diagnostics: warnings,
            answers: FIELD_IDS
                .iter()
                .filter_map(|id| {
                    answers
                        .get_text(id)
                        .map(|text| (id.to_string(), text.to_string()))
                })
                .collect(),
        },
        // A rejected field is dropped, the rest kept: a run whose self-report is
        // discarded cannot say how blind it was. The sheet was found and read, so
        // this is not the same fact as no sheet existing — `collected` says only
        // that.
        Err(diagnostics) => {
            let rejected: Vec<&str> = diagnostics
                .iter()
                .filter_map(|diagnostic| match diagnostic {
                    ValidationDiagnostic::Field { id, .. } => Some(id.as_str()),
                    ValidationDiagnostic::Form { .. } => None,
                })
                .collect();
            QuestionnaireReport {
                collected: true,
                diagnostics: warnings
                    .into_iter()
                    .chain(diagnostics.iter().map(ToString::to_string))
                    .collect(),
                answers: FIELD_IDS
                    .iter()
                    .filter(|id| !rejected.contains(*id))
                    .filter_map(|id| {
                        raw.get(id)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .map(|text| (id.to_string(), text.to_string()))
                    })
                    .collect(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(confidence: &str) -> String {
        let mut text = definition().render_answer_sheet();
        for (id, answer) in [
            ("summary", "Built the thing."),
            ("sources.docs", "docs/index.md"),
            ("sources.external", "none"),
            ("confidence", confidence),
            ("confidence_reason", "Every assertion passes."),
        ] {
            text = text.replace(&format!("<id:{id}>\n"), &format!("<id:{id}>\n{answer}\n"));
        }
        text
    }

    #[test]
    fn confidence_and_its_reason_are_two_fields() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SHEET_FILENAME), sheet("high")).unwrap();
        let collected = collect(dir.path());
        assert!(collected.collected, "{:?}", collected.diagnostics);
        assert_eq!(collected.answers.get("confidence").unwrap(), "high");
        assert_eq!(
            collected.answers.get("confidence_reason").unwrap(),
            "Every assertion passes."
        );
    }

    // A rejected field (confidence answered as more than one line, which
    // ADR-0016 forbids for a scalar) is a diagnostic on a sheet that was
    // found, not the absence of one: `collected` stays true, and every
    // other answer — including the field's own free-text sibling — is
    // kept.
    #[test]
    fn a_field_that_does_not_decode_keeps_the_answers_that_did() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(SHEET_FILENAME),
            sheet("high\n\nEvery assertion passes, mostly."),
        )
        .unwrap();
        let collected = collect(dir.path());
        assert!(collected.collected, "{:?}", collected.diagnostics);
        assert!(
            collected
                .diagnostics
                .iter()
                .any(|d| d.contains("confidence")),
            "{:?}",
            collected.diagnostics
        );
        assert_eq!(
            collected.answers.get("sources.docs").unwrap(),
            "docs/index.md"
        );
        assert_eq!(collected.answers.get("sources.external").unwrap(), "none");
        assert_eq!(
            collected.answers.get("confidence_reason").unwrap(),
            "Every assertion passes."
        );
        // The value the questionnaire rejected is a diagnostic, not an answer.
        assert_eq!(collected.answers.get("confidence"), None, "{collected:?}");
    }
}
