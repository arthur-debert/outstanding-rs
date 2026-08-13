//! Deterministic rendering of blank answer sheets.
//!
//! Rendering writes the prose document described in the
//! [module documentation](crate::questionnaire): a three-line `#!` metadata
//! preamble followed by one numbered question block per item, with declared
//! defaults pre-filled on their answer marker lines. Groups render a heading
//! line followed by their nested, dot-numbered children; a repeatable group
//! renders exactly its declared minimum number of occurrences plus one-line
//! guidance for copying a complete block. The same definition always renders
//! byte-identical output.
//!
//! All numbering is cosmetic. Each occurrence of a repeatable group renders
//! with the *same* display numbers — the parser counts occurrences of the
//! stable group header, never numbers — which also keeps every block an
//! exact copy of its siblings, so "copy the block" needs no renumbering.

use std::fmt::Write as _;

use super::definition::{Item, Questionnaire};

/// The exact first preamble line of a version-1 answer sheet.
pub(crate) const FORMAT_LINE: &str = "#! standout-answers 1";
/// Preamble key prefix for the questionnaire ID line.
pub(crate) const QUESTIONNAIRE_PREFIX: &str = "#! questionnaire:";
/// Preamble key prefix for the fingerprint line.
pub(crate) const FINGERPRINT_PREFIX: &str = "#! fingerprint:";
/// The marker introducing answer text under a field header.
pub(crate) const ANSWER_MARKER: &str = "->";

/// The copy-the-block guidance line rendered under a repeatable group's
/// first heading. Deliberately bracket-free so it can never look like a
/// header to the parser.
const REPEAT_GUIDANCE: &str =
    "(Add an item by copying one complete block - its heading line and its questions - below the last block, then answering the copy.)";

impl Questionnaire {
    /// Render a blank answer sheet for this questionnaire.
    ///
    /// The output is deterministic: rendering the same definition always
    /// produces the same document, including the fingerprint in the preamble.
    /// Each field renders as a header line — cosmetic display number and
    /// wording, the bracketed stable ID, and a parenthesized type hint —
    /// followed by the `->` answer marker. A field with a declared default
    /// renders the default pre-filled on the marker line; every other field
    /// renders a bare marker awaiting the answer. A group renders its
    /// heading line and its children with nested cosmetic numbering; a
    /// repeatable group renders exactly its declared minimum number of
    /// occurrence blocks and concise guidance to copy a complete block when
    /// adding an item.
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
                    "{number} {} [{}] ({})",
                    field.prompt(),
                    field.id(),
                    field.type_hint()
                );
                match field.default() {
                    Some(default) => {
                        let _ = writeln!(out, "{ANSWER_MARKER} {default}");
                    }
                    None => {
                        out.push_str(ANSWER_MARKER);
                        out.push('\n');
                    }
                }
            }
            Item::Group(group) => {
                let occurrences = group.repeat().map_or(1, |repeat| repeat.min());
                for occurrence in 0..occurrences {
                    out.push('\n');
                    let _ = writeln!(
                        out,
                        "{number} {} [{}] ({})",
                        group.prompt(),
                        group.id(),
                        group.type_hint()
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
