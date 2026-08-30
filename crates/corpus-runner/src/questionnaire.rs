// The exit questionnaire: rendered into the blind workspace at provision
// time, answered by the agent in place, decoded via `standout-input`.

use std::path::Path;

use standout_input::questionnaire::{Questionnaire, ScalarField, ScalarKind};

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
        Err(diagnostics) => QuestionnaireReport {
            collected: false,
            diagnostics: warnings
                .into_iter()
                .chain(diagnostics.iter().map(ToString::to_string))
                .collect(),
            answers: Default::default(),
        },
    }
}
