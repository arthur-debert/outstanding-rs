//! Deterministic rendering of blank answer sheets.
//!
//! Rendering writes the prose document described in the
//! [module documentation](crate::questionnaire): a three-line `#!` metadata
//! preamble followed by one numbered question block per item. Every question
//! line ends with its stable identity tag (`<id:project.name>`) as the last
//! non-whitespace content on the line; a declared default renders pre-filled
//! as the answer text on the line below its question. Groups render a
//! heading line (ending with the group's tag) followed by their nested,
//! dot-numbered children; a repeatable group renders exactly its declared
//! minimum number of occurrences plus one-line guidance for copying a
//! complete block. The same definition always renders byte-identical output.
//!
//! All numbering is cosmetic. Each occurrence of a repeatable group renders
//! with the *same* display numbers — the parser counts occurrences of the
//! stable group tag, never numbers — which also keeps every block an exact
//! copy of its siblings, so "copy the block" needs no renumbering.

use std::fmt::Write as _;

use super::definition::{Item, Questionnaire};

/// The exact first preamble line of a version-1 answer sheet.
pub(crate) const FORMAT_LINE: &str = "#! standout-answers 1";
/// Preamble key prefix for the questionnaire ID line.
pub(crate) const QUESTIONNAIRE_PREFIX: &str = "#! questionnaire:";
/// Preamble key prefix for the fingerprint line.
pub(crate) const FINGERPRINT_PREFIX: &str = "#! fingerprint:";
/// The opening delimiter of a question tag (`<id:project.name>`).
pub(crate) const TAG_OPEN: &str = "<id:";

/// The copy-the-block guidance line rendered under a repeatable group's
/// first heading. Deliberately free of `<id:` and not ending in `>`, so it
/// can never read as a question tag to the parser or trip the tag-fragment
/// warning.
const REPEAT_GUIDANCE: &str =
    "(Add an item by copying one complete block - its heading line and its questions - below the last block, then answering the copy.)";

/// The rendered question tag for one stable ID.
fn tag(id: &str) -> String {
    format!("{TAG_OPEN}{id}>")
}

impl Questionnaire {
    /// Render a blank answer sheet for this questionnaire.
    ///
    /// The output is deterministic: rendering the same definition always
    /// produces the same document, including the fingerprint in the preamble.
    /// Each field renders as one question line — cosmetic display number and
    /// wording, a parenthesized type hint, and the line-terminal
    /// `<id:...>` tag — with the answer expected on the following lines. A
    /// field with a declared default renders the default pre-filled as its
    /// answer text; every other field leaves the answer area blank. A group
    /// renders its heading line (ending with the group's tag) and its
    /// children with nested cosmetic numbering; a repeatable group renders
    /// exactly its declared minimum number of occurrence blocks and concise
    /// guidance to copy a complete block when adding an item.
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

/// Render one scope's items, numbering them `1.`/`2.`… at the root and
/// `<prefix>.1`/`<prefix>.2`… when nested.
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

/// The cosmetic display number for one item: `3.` at the root, `3.1` when
/// nested (matching the rendered examples in the module documentation).
fn display_number(prefix: &str, ordinal: usize) -> String {
    if prefix.is_empty() {
        format!("{ordinal}.")
    } else {
        format!("{prefix}.{ordinal}")
    }
}
