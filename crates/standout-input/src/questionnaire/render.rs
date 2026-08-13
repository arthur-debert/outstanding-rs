//! Deterministic rendering of blank answer sheets.
//!
//! Rendering writes the prose document described in the
//! [module documentation](crate::questionnaire): a three-line `#!` metadata
//! preamble followed by one numbered question block per field, with declared
//! defaults pre-filled on their answer marker lines. The same definition
//! always renders byte-identical output.

use std::fmt::Write as _;

use super::definition::Questionnaire;

/// The exact first preamble line of a version-1 answer sheet.
pub(crate) const FORMAT_LINE: &str = "#! standout-answers 1";
/// Preamble key prefix for the questionnaire ID line.
pub(crate) const QUESTIONNAIRE_PREFIX: &str = "#! questionnaire:";
/// Preamble key prefix for the fingerprint line.
pub(crate) const FINGERPRINT_PREFIX: &str = "#! fingerprint:";
/// The marker introducing answer text under a field header.
pub(crate) const ANSWER_MARKER: &str = "->";

impl Questionnaire {
    /// Render a blank answer sheet for this questionnaire.
    ///
    /// The output is deterministic: rendering the same definition always
    /// produces the same document, including the fingerprint in the preamble.
    /// Each field renders as a header line — cosmetic display number and
    /// wording, the bracketed stable ID, and a parenthesized type hint —
    /// followed by the `->` answer marker. A field with a declared default
    /// renders the default pre-filled on the marker line; every other field
    /// renders a bare marker awaiting the answer.
    pub fn render_answer_sheet(&self) -> String {
        let mut out = String::new();
        out.push_str(FORMAT_LINE);
        out.push('\n');
        let _ = writeln!(out, "{QUESTIONNAIRE_PREFIX} {}", self.id());
        let _ = writeln!(out, "{FINGERPRINT_PREFIX} {}", self.fingerprint());
        for (index, field) in self.fields().iter().enumerate() {
            out.push('\n');
            let _ = writeln!(
                out,
                "{}. {} [{}] ({})",
                index + 1,
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
        out
    }
}
