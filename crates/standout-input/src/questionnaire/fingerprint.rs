//! Semantic fingerprinting for questionnaire definitions.
//!
//! The fingerprint answers one question at parse time: "was this sheet
//! rendered from a semantically identical definition?" Version 1 accepts only
//! an exact match; a mismatch asks the user for a fresh sheet rather than
//! guessing how old answers map onto new semantics.
//!
//! The canonical form hashed here includes exactly the semantic surface of a
//! definition — questionnaire ID, and each field's stable ID, kind, and
//! optionality — with fields sorted by stable ID so presentation order stays
//! cosmetic. Wording, numbering, and styling never appear in the canonical
//! form, so copy edits cannot invalidate existing sheets.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::definition::ScalarField;

/// Version tag for the canonical form itself, distinct from the rendered
/// answer-format version: changing how the fingerprint is computed must
/// change every fingerprint.
const CANONICAL_FORM_VERSION: &str = "1";

/// Compute the `sha256:<hex>` fingerprint of a definition's semantic surface.
///
/// Deterministic for a given semantic definition and independent of field
/// presentation order.
pub(crate) fn compute_fingerprint(questionnaire_id: &str, fields: &[ScalarField]) -> String {
    let mut canonical = format!(
        "standout-answers-canonical {CANONICAL_FORM_VERSION}\nquestionnaire={questionnaire_id}\n"
    );
    let mut sorted: Vec<&ScalarField> = fields.iter().collect();
    sorted.sort_by(|a, b| a.id().cmp(b.id()));
    for field in sorted {
        let _ = writeln!(
            canonical,
            "field={}|kind={}|optional={}",
            field.id(),
            field.kind().name(),
            field.is_optional()
        );
    }
    let digest = Sha256::digest(canonical.as_bytes());
    let mut out = String::with_capacity(7 + digest.len() * 2);
    out.push_str("sha256:");
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
