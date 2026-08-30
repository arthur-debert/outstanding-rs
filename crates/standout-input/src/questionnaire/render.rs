use std::fmt::Write as _;

use super::definition::{Item, Questionnaire};

pub(crate) const FORMAT_LINE: &str = "#! standout-answers 1";
pub(crate) const QUESTIONNAIRE_PREFIX: &str = "#! questionnaire:";
pub(crate) const FINGERPRINT_PREFIX: &str = "#! fingerprint:";
pub(crate) const TAG_OPEN: &str = "<id:";

const REPEAT_GUIDANCE: &str =
    "(Add an item by copying one complete block - its heading line and its questions - below the last block, then answering the copy.)";

fn tag(id: &str) -> String {
    format!("{TAG_OPEN}{id}>")
}

impl Questionnaire {
    pub fn render_answer_sheet(&self) -> String {
        let mut out = String::new();
        out.push_str(FORMAT_LINE);
        out.push('\n');
        let _ = writeln!(out, "{QUESTIONNAIRE_PREFIX} {}", self.id());
        let _ = writeln!(out, "{FINGERPRINT_PREFIX} {}", self.fingerprint());
        render_items(self.items(), "", &mut out);
        out
    }
}

fn render_items(items: &[Item], number_prefix: &str, out: &mut String) {
    for (index, item) in items.iter().enumerate() {
        let number = display_number(number_prefix, index + 1);
        match item {
            Item::Field(field) => {
                out.push('\n');
                let _ = writeln!(
                    out,
                    "{number} {} ({}) {}",
                    field.prompt(),
                    field.type_hint(),
                    tag(field.id())
                );
                if let Some(default) = field.default() {
                    let _ = writeln!(out, "{default}");
                }
            }
            Item::Group(group) => {
                let occurrences = group.repeat().map_or(1, |repeat| repeat.min());
                for occurrence in 0..occurrences {
                    out.push('\n');
                    let _ = writeln!(
                        out,
                        "{number} {} ({}) {}",
                        group.prompt(),
                        group.type_hint(),
                        tag(group.id())
                    );
                    if group.repeat().is_some() && occurrence == 0 {
                        out.push_str(REPEAT_GUIDANCE);
                        out.push('\n');
                    }
                    render_items(group.children(), number.trim_end_matches('.'), out);
                }
            }
        }
    }
}

fn display_number(prefix: &str, ordinal: usize) -> String {
    if prefix.is_empty() {
        format!("{ordinal}.")
    } else {
        format!("{prefix}.{ordinal}")
    }
}
