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
//! default (a static value, or a dynamic default's declared revision —
//! closures cannot be hashed), constraint choices, condition, and
//! application-validator revision — everything that changes which answers
//! are accepted. Entries are sorted by stable ID and constraint choices are
//! sorted, so presentation order stays cosmetic. Wording, numbering, and
//! styling never appear in the canonical form, so copy edits cannot
//! invalidate existing sheets.
//!
//! [`field_entry`] and [`collect_entries`] destructure [`ScalarField`] and
//! [`Group`] exhaustively, with no rest pattern: a future struct field is a
//! compile error at the fingerprint site until its author decides
//! semantic-or-cosmetic, so no semantic property can silently escape the
//! hash.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::definition::{Constraint, Group, Item, ScalarField};

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
///
/// [`Group`] is destructured exhaustively (no rest pattern) as a drift
/// guard: adding a struct field fails compilation here until its author
/// decides whether it is semantic (enters the entry) or cosmetic (discarded
/// explicitly by name). [`field_entry`] applies the same guard to
/// [`ScalarField`].
fn collect_entries(items: &[Item], parent: Option<&str>, entries: &mut Vec<(String, String)>) {
    for item in items {
        match item {
            Item::Field(field) => {
                entries.push((field.id().to_string(), field_entry(field, parent)));
            }
            Item::Group(group) => {
                let Group {
                    id,
                    // Cosmetic: wording never enters the hash.
                    prompt: _,
                    children,
                    repeat,
                } = group;
                let mut entry = format!("group={id}");
                if let Some(parent) = parent {
                    let _ = write!(entry, "|parent={parent}");
                }
                if let Some(repeat) = repeat {
                    let _ = write!(entry, "|repeat_min={}", repeat.min());
                    if let Some(max) = repeat.max() {
                        let _ = write!(entry, "|repeat_max={max}");
                    }
                }
                entries.push((id.clone(), entry));
                collect_entries(children, Some(id), entries);
            }
        }
    }
}

/// The canonical entry for one scalar field.
///
/// [`ScalarField`] is destructured exhaustively (no rest pattern) as a
/// drift guard: adding a struct field fails compilation here until its
/// author decides whether it is semantic (enters the entry) or cosmetic
/// (discarded explicitly by name).
fn field_entry(field: &ScalarField, parent: Option<&str>) -> String {
    let ScalarField {
        id,
        // Cosmetic: wording never enters the hash.
        prompt: _,
        kind,
        optional,
        default,
        dynamic_default,
        constraint,
        condition,
        validator,
    } = field;
    let mut entry = format!("field={id}|kind={}|optional={optional}", kind.name());
    if let Some(parent) = parent {
        let _ = write!(entry, "|parent={parent}");
    }
    if let Some(default) = default {
        let _ = write!(entry, "|default={}", esc(default));
    }
    // A dynamic default's declared revision is its semantic identity; the
    // closure cannot be hashed (mutually exclusive with `default` by
    // construction, and keyed distinctly so the two can never collide).
    if let Some(dynamic) = dynamic_default {
        let _ = write!(entry, "|default_revision={}", esc(dynamic.revision()));
    }
    if let Some(Constraint::OneOf(choices)) = constraint {
        let mut sorted_choices: Vec<&String> = choices.iter().collect();
        sorted_choices.sort();
        let joined: Vec<String> = sorted_choices.iter().map(|c| esc(c)).collect();
        let _ = write!(entry, "|one_of={}", joined.join(","));
    }
    if let Some(condition) = condition {
        let _ = write!(
            entry,
            "|active_when={}={}",
            condition.controller(),
            esc(condition.expected())
        );
    }
    if let Some(validator) = validator {
        let _ = write!(entry, "|validator={}", esc(validator.revision()));
    }
    entry
}
