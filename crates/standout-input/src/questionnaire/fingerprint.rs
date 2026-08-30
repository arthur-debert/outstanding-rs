use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::definition::{Constraint, Group, Item, ScalarField};

const CANONICAL_FORM_VERSION: &str = "3";

fn esc(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

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

fn collect_entries(items: &[Item], parent: Option<&str>, entries: &mut Vec<(String, String)>) {
    for item in items {
        match item {
            Item::Field(field) => {
                entries.push((field.id().to_string(), field_entry(field, parent)));
            }
            Item::Group(group) => {
                let Group {
                    id,
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

fn field_entry(field: &ScalarField, parent: Option<&str>) -> String {
    let ScalarField {
        id,
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
