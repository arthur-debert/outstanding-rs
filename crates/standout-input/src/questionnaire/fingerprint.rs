//! Semantic fingerprinting for questionnaire definitions.
//!
//! The fingerprint answers one question at parse time: "was this sheet
//! rendered from a semantically identical definition?" Version 1 accepts only
//! an exact match; a mismatch asks the user for a fresh sheet rather than
//! guessing how old answers map onto new semantics.
//!
//! The canonical form hashed here includes exactly the semantic surface of a
//! definition — questionnaire ID; group structure (each item's enclosing
//! group) and repeat bounds; and each field's stable ID, kind, optionality,
//! default, constraint choices, condition, and application-validator
//! revision — everything that changes which answers are accepted. Entries
//! are sorted by stable ID and constraint choices are sorted, so
//! presentation order stays cosmetic. Wording, numbering, and styling never
//! appear in the canonical form, so copy edits cannot invalidate existing
//! sheets.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::definition::{Constraint, Item, ScalarField};

/// Version tag for the canonical form itself, distinct from the rendered
/// answer-format version: changing how the fingerprint is computed must
/// change every fingerprint.
const CANONICAL_FORM_VERSION: &str = "3";

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
/// Deterministic for a given semantic definition and independent of item
/// presentation order and constraint-choice order. Optional semantic
/// properties (parent group, repeat bounds, default, constraint, condition,
/// validator revision) appear only when declared, so absence and emptiness
/// cannot collide.
pub(crate) fn compute_fingerprint(questionnaire_id: &str, items: &[Item]) -> String {
    let mut entries: Vec<(String, String)> = Vec::new();
    collect_entries(items, None, &mut entries);
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut canonical = format!(
        "standout-answers-canonical {CANONICAL_FORM_VERSION}\nquestionnaire={questionnaire_id}\n"
    );
    for (_, entry) in entries {
        canonical.push_str(&entry);
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

/// Collect one canonical `(id, entry)` line per item, depth first. The
/// parent group appears in every nested entry, so group structure — not just
/// the flat set of IDs — is part of the hash.
fn collect_entries(items: &[Item], parent: Option<&str>, entries: &mut Vec<(String, String)>) {
    for item in items {
        match item {
            Item::Field(field) => {
                entries.push((field.id().to_string(), field_entry(field, parent)));
            }
            Item::Group(group) => {
                let mut entry = format!("group={}", group.id());
                if let Some(parent) = parent {
                    let _ = write!(entry, "|parent={parent}");
                }
                if let Some(repeat) = group.repeat() {
                    let _ = write!(entry, "|repeat_min={}", repeat.min());
                    if let Some(max) = repeat.max() {
                        let _ = write!(entry, "|repeat_max={max}");
                    }
                }
                entries.push((group.id().to_string(), entry));
                collect_entries(group.children(), Some(group.id()), entries);
            }
        }
    }
}

/// The canonical entry for one scalar field.
fn field_entry(field: &ScalarField, parent: Option<&str>) -> String {
    let mut entry = format!(
        "field={}|kind={}|optional={}",
        field.id(),
        field.kind().name(),
        field.is_optional()
    );
    if let Some(parent) = parent {
        let _ = write!(entry, "|parent={parent}");
    }
    if let Some(default) = field.default() {
        let _ = write!(entry, "|default={}", esc(default));
    }
    if let Some(Constraint::OneOf(choices)) = field.constraint() {
        let mut sorted_choices: Vec<&String> = choices.iter().collect();
        sorted_choices.sort();
        let joined: Vec<String> = sorted_choices.iter().map(|c| esc(c)).collect();
        let _ = write!(entry, "|one_of={}", joined.join(","));
    }
    if let Some(condition) = field.condition() {
        let _ = write!(
            entry,
            "|active_when={}={}",
            condition.controller(),
            esc(condition.expected())
        );
    }
    if let Some(validator) = field.validator() {
        let _ = write!(entry, "|validator={}", esc(validator.revision()));
    }
    entry
}
