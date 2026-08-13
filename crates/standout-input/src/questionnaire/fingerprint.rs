//! Semantic fingerprinting for questionnaire definitions.
//!
//! The fingerprint answers one question at parse time: "was this sheet
//! rendered from a semantically identical definition?" Version 1 accepts only
//! an exact match; a mismatch asks the user for a fresh sheet rather than
//! guessing how old answers map onto new semantics.
//!
//! The canonical form hashed here includes exactly the semantic surface of a
//! definition — questionnaire ID, and each field's stable ID, kind,
//! optionality, default, constraint choices, condition, and application-
//! validator revision — everything that changes which answers are accepted.
//! Fields are sorted by stable ID and constraint choices are sorted, so
//! presentation order stays cosmetic. Wording, numbering, and styling never
//! appear in the canonical form, so copy edits cannot invalidate existing
//! sheets.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::definition::{Constraint, ScalarField};

/// Version tag for the canonical form itself, distinct from the rendered
/// answer-format version: changing how the fingerprint is computed must
/// change every fingerprint.
const CANONICAL_FORM_VERSION: &str = "2";

/// Escape free-form text for embedding in the canonical form, so values
/// containing separators can never collide with the form's structure.
fn esc(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

/// Compute the `sha256:<hex>` fingerprint of a definition's semantic surface.
///
/// Deterministic for a given semantic definition and independent of field
/// presentation order and constraint-choice order. Optional semantic
/// properties (default, constraint, condition, validator revision) appear
/// only when declared, so absence and emptiness cannot collide.
pub(crate) fn compute_fingerprint(questionnaire_id: &str, fields: &[ScalarField]) -> String {
    let mut canonical = format!(
        "standout-answers-canonical {CANONICAL_FORM_VERSION}\nquestionnaire={questionnaire_id}\n"
    );
    let mut sorted: Vec<&ScalarField> = fields.iter().collect();
    sorted.sort_by(|a, b| a.id().cmp(b.id()));
    for field in sorted {
        let _ = write!(
            canonical,
            "field={}|kind={}|optional={}",
            field.id(),
            field.kind().name(),
            field.is_optional()
        );
        if let Some(default) = field.default() {
            let _ = write!(canonical, "|default={}", esc(default));
        }
        if let Some(Constraint::OneOf(choices)) = field.constraint() {
            let mut sorted_choices: Vec<&String> = choices.iter().collect();
            sorted_choices.sort();
            let joined: Vec<String> = sorted_choices.iter().map(|c| esc(c)).collect();
            let _ = write!(canonical, "|one_of={}", joined.join(","));
        }
        if let Some(condition) = field.condition() {
            let _ = write!(
                canonical,
                "|active_when={}={}",
                condition.controller(),
                esc(condition.expected())
            );
        }
        if let Some(validator) = field.validator() {
            let _ = write!(canonical, "|validator={}", esc(validator.revision()));
        }
        canonical.push('\n');
    }
    let digest = Sha256::digest(canonical.as_bytes());
    let mut out = String::with_capacity(7 + digest.len() * 2);
    out.push_str("sha256:");
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
