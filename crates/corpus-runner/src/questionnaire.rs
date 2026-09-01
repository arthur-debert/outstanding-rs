// The exit questionnaire: rendered into the blind workspace at provision
// time, answered by the agent in place, decoded via `standout-input`.

use std::path::Path;

use standout_input::questionnaire::{Questionnaire, ScalarField, ScalarKind, ValidationDiagnostic};

use crate::report::QuestionnaireReport;

pub const SHEET_FILENAME: &str = "QUESTIONNAIRE.md";

// Kept next to `definition` so answer extraction and the definition cannot
// drift silently — a test pins them to each other.
pub const FIELD_IDS: &[&str] = &[
    "summary",
    "sources.docs",
    "sources.external",
    "friction",
    "docs.gaps",
    "workarounds",
    "confidence",
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
        // One field the agent answered in an unexpected shape is a
        // diagnostic about that field, not a reason to forget the rest: the
        // two sources answers are how ADR-0023 records blindness, and a run
        // whose self-report is discarded cannot say how blind it was. The
        // field that failed is dropped rather than published, so nothing
        // reading a single answer sees a value the questionnaire rejected,
        // and the report says `collected: false` either way.
        Err(diagnostics) => {
            let rejected: Vec<&str> = diagnostics
                .iter()
                .filter_map(|diagnostic| match diagnostic {
                    ValidationDiagnostic::Field { id, .. } => Some(id.as_str()),
                    ValidationDiagnostic::Form { .. } => None,
                })
                .collect();
            QuestionnaireReport {
                collected: false,
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
        ] {
            text = text.replace(&format!("<id:{id}>\n"), &format!("<id:{id}>\n{answer}\n"));
        }
        text
    }

    #[test]
    fn a_field_that_does_not_decode_keeps_the_answers_that_did() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SHEET_FILENAME), sheet("high")).unwrap();
        let collected = collect(dir.path());
        assert!(collected.collected, "{:?}", collected.diagnostics);
        assert_eq!(collected.answers.get("sources.external").unwrap(), "none");

        // The same sheet, with reasoning trailing the choice answer.
        std::fs::write(
            dir.path().join(SHEET_FILENAME),
            sheet("high\n\nEvery assertion passes."),
        )
        .unwrap();
        let collected = collect(dir.path());
        assert!(!collected.collected);
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
        // The value the questionnaire rejected is a diagnostic, not an answer.
        assert_eq!(collected.answers.get("confidence"), None, "{collected:?}");
    }
}
