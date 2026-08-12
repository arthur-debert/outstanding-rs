//! Parsing edited answer sheets back into raw answers.
//!
//! Parsing recognizes structure only from stable identities: a field opens
//! when a header-shaped line carries a schema-recognized bracketed ID and the
//! following line begins with the `->` answer marker. Everything cosmetic —
//! display numbers, wording, indentation, type hints — is ignored, so a user
//! may freely reword or renumber a sheet without changing what it means.
//!
//! Compatibility is exact-version: the preamble's answer-format version,
//! questionnaire ID, and fingerprint must all match the parsing definition,
//! or parsing stops with diagnostics that ask for a freshly rendered sheet.
//! No migration or fuzzy matching is attempted.

use std::collections::BTreeMap;

use super::definition::Questionnaire;
use super::render::{ANSWER_MARKER, FINGERPRINT_PREFIX, FORMAT_LINE, QUESTIONNAIRE_PREFIX};

/// The raw answers parsed from one answer sheet.
///
/// Values are the verbatim answer text with outer whitespace trimmed and
/// internal line breaks preserved. A field absent from the document is absent
/// here; a field whose marker was left blank is present with an empty string.
/// Decoding raw text into typed values (defaults, omission, validation) is a
/// later stage, shared with interactive collection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawAnswers {
    values: BTreeMap<String, String>,
}

impl RawAnswers {
    /// The raw answer text for a stable field ID, if the field appeared.
    pub fn get(&self, field_id: &str) -> Option<&str> {
        self.values.get(field_id).map(String::as_str)
    }

    /// Iterate over `(field_id, raw_answer)` pairs, ordered by field ID.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Number of fields that appeared in the document.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether no fields appeared in the document.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// One problem found while parsing an answer sheet.
///
/// Diagnostics identify locations by 1-based line number and fields by stable
/// ID; they never echo full answer values, since answer sheets may contain
/// sensitive content. Compatibility diagnostics deliberately point at
/// re-rendering: version 1 rejects incompatible sheets instead of migrating
/// them.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum AnswerSheetDiagnostic {
    /// The `#!` metadata preamble is missing or malformed.
    #[error("Line {line}: malformed answer-sheet preamble: {detail}. Render a fresh answer sheet and copy your answers into it.")]
    MalformedPreamble {
        /// 1-based line number of the malformed or missing preamble line.
        line: usize,
        /// What was expected at that line.
        detail: String,
    },

    /// The sheet declares an answer-format version other than 1.
    #[error("Unsupported answer-format version '{found}' (this release reads only version 1). Render a fresh answer sheet; old sheets are not migrated.")]
    UnsupportedAnswerFormat {
        /// The version token found in the document.
        found: String,
    },

    /// The sheet was rendered for a different questionnaire.
    #[error("This answer sheet is for questionnaire '{found}', not '{expected}'. Render a fresh answer sheet for '{expected}'.")]
    QuestionnaireMismatch {
        /// The questionnaire ID this parser expects.
        expected: String,
        /// The questionnaire ID found in the document.
        found: String,
    },

    /// The sheet was rendered from a semantically different definition.
    #[error("This answer sheet was rendered from a different version of questionnaire semantics (fingerprint '{found}', expected '{expected}'). The questionnaire changed since this sheet was rendered; render a fresh answer sheet and copy your answers into it. Answers are not migrated.")]
    FingerprintMismatch {
        /// The fingerprint of the parsing definition.
        expected: String,
        /// The fingerprint found in the document.
        found: String,
    },

    /// A header-shaped line carries a bracketed ID the schema does not know.
    #[error("Line {line}: unknown field ID '[{id}]'. This questionnaire does not define that field; if the line is prose, remove the following '->' marker line, otherwise render a fresh answer sheet.")]
    UnknownFieldId {
        /// The unrecognized bracketed ID.
        id: String,
        /// 1-based line number of the header-shaped line.
        line: usize,
    },

    /// A field header appears more than once.
    #[error("Line {line}: duplicate field '[{id}]'. Each field may be answered once; remove the extra occurrence.")]
    DuplicateField {
        /// The duplicated stable field ID.
        id: String,
        /// 1-based line number of the second occurrence.
        line: usize,
    },
}

/// The field (or discard sink) currently accumulating answer lines.
struct OpenAnswer {
    /// `Some(id)` for a recognized field; `None` discards the answer text of
    /// an unknown or duplicate header so it cannot leak into a neighbor.
    id: Option<String>,
    lines: Vec<String>,
}

impl OpenAnswer {
    fn flush_into(self, values: &mut BTreeMap<String, String>) {
        if let Some(id) = self.id {
            values.insert(id, self.lines.join("\n").trim().to_string());
        }
    }
}

/// Returns the last bracketed token on `line` when it is shaped like a stable
/// ID (non-empty, only `a-z`, `0-9`, `.`, `_`, `-`).
///
/// Ordinary prose brackets (`[like this]`, `[Maybe?]`) do not qualify, so
/// they can never make a line header-shaped.
fn header_candidate(line: &str) -> Option<&str> {
    let close = line.rfind(']')?;
    let open = line[..close].rfind('[')?;
    let token = &line[open + 1..close];
    let valid = !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    valid.then_some(token)
}

impl Questionnaire {
    /// Parse an edited answer sheet back into [`RawAnswers`].
    ///
    /// The document must have been rendered by
    /// [`render_answer_sheet`](Self::render_answer_sheet) for this exact
    /// definition: the preamble's answer-format version, questionnaire ID,
    /// and fingerprint are checked exactly, and any mismatch returns
    /// diagnostics asking for a fresh sheet without reading the body.
    ///
    /// Within the body, a field is recognized only by a schema-recognized
    /// bracketed ID whose *next* line begins with the `->` answer marker.
    /// The answer is everything after the marker up to the next recognized
    /// header or end of file, outer whitespace trimmed, internal line breaks
    /// preserved. Bracketed prose inside an answer is answer text unless it
    /// satisfies that full header contract.
    ///
    /// # Errors
    ///
    /// Returns every accumulated [`AnswerSheetDiagnostic`]: compatibility
    /// mismatches, malformed preambles, and unknown or duplicate IDs on
    /// header-shaped lines.
    pub fn parse_answer_sheet(&self, text: &str) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        let lines: Vec<&str> = text.lines().collect();
        let body_start = self.check_preamble(&lines)?;

        let mut diagnostics = Vec::new();
        let mut values = BTreeMap::new();
        let mut open: Option<OpenAnswer> = None;
        let mut i = body_start;
        while i < lines.len() {
            let line = lines[i];
            let marker_follows = lines
                .get(i + 1)
                .is_some_and(|next| next.trim_start().starts_with(ANSWER_MARKER));
            if let (true, Some(candidate)) = (marker_follows, header_candidate(line)) {
                if let Some(previous) = open.take() {
                    previous.flush_into(&mut values);
                }
                let id = if self.field(candidate).is_none() {
                    diagnostics.push(AnswerSheetDiagnostic::UnknownFieldId {
                        id: candidate.to_string(),
                        line: i + 1,
                    });
                    None
                } else if values.contains_key(candidate) {
                    diagnostics.push(AnswerSheetDiagnostic::DuplicateField {
                        id: candidate.to_string(),
                        line: i + 1,
                    });
                    None
                } else {
                    Some(candidate.to_string())
                };
                let marker_line = lines[i + 1].trim_start();
                let first = marker_line[ANSWER_MARKER.len()..].to_string();
                open = Some(OpenAnswer {
                    id,
                    lines: vec![first],
                });
                i += 2;
            } else {
                if let Some(current) = open.as_mut() {
                    current.lines.push(line.to_string());
                }
                i += 1;
            }
        }
        if let Some(last) = open {
            last.flush_into(&mut values);
        }

        if diagnostics.is_empty() {
            Ok(RawAnswers { values })
        } else {
            Err(diagnostics)
        }
    }

    /// Validate the three-line `#!` preamble against this definition.
    ///
    /// Returns the index of the first body line, or every compatibility and
    /// shape diagnostic found. Blank lines before and between preamble lines
    /// are tolerated; the preamble content itself is matched exactly.
    fn check_preamble(&self, lines: &[&str]) -> Result<usize, Vec<AnswerSheetDiagnostic>> {
        let mut diagnostics = Vec::new();
        let mut i = 0;

        let next_content = |i: &mut usize| -> Option<usize> {
            while *i < lines.len() && lines[*i].trim().is_empty() {
                *i += 1;
            }
            (*i < lines.len()).then(|| {
                let at = *i;
                *i += 1;
                at
            })
        };

        match next_content(&mut i) {
            Some(at) => {
                let line = lines[at].trim();
                if line != FORMAT_LINE {
                    match line.strip_prefix("#! standout-answers ") {
                        Some(version) => {
                            diagnostics.push(AnswerSheetDiagnostic::UnsupportedAnswerFormat {
                                found: version.trim().to_string(),
                            })
                        }
                        None => diagnostics.push(AnswerSheetDiagnostic::MalformedPreamble {
                            line: at + 1,
                            detail: format!("expected '{FORMAT_LINE}'"),
                        }),
                    }
                }
            }
            None => diagnostics.push(AnswerSheetDiagnostic::MalformedPreamble {
                line: lines.len() + 1,
                detail: format!("expected '{FORMAT_LINE}'"),
            }),
        }

        let expect_keyed = |i: &mut usize,
                            prefix: &str,
                            diagnostics: &mut Vec<AnswerSheetDiagnostic>|
         -> Option<String> {
            match next_content(i) {
                Some(at) => match lines[at].trim().strip_prefix(prefix) {
                    Some(value) => Some(value.trim().to_string()),
                    None => {
                        diagnostics.push(AnswerSheetDiagnostic::MalformedPreamble {
                            line: at + 1,
                            detail: format!("expected '{prefix} ...'"),
                        });
                        None
                    }
                },
                None => {
                    diagnostics.push(AnswerSheetDiagnostic::MalformedPreamble {
                        line: lines.len() + 1,
                        detail: format!("expected '{prefix} ...'"),
                    });
                    None
                }
            }
        };

        if let Some(found) = expect_keyed(&mut i, QUESTIONNAIRE_PREFIX, &mut diagnostics) {
            if found != self.id() {
                diagnostics.push(AnswerSheetDiagnostic::QuestionnaireMismatch {
                    expected: self.id().to_string(),
                    found,
                });
            }
        }
        if let Some(found) = expect_keyed(&mut i, FINGERPRINT_PREFIX, &mut diagnostics) {
            if found != self.fingerprint() {
                diagnostics.push(AnswerSheetDiagnostic::FingerprintMismatch {
                    expected: self.fingerprint().to_string(),
                    found,
                });
            }
        }

        if diagnostics.is_empty() {
            Ok(i)
        } else {
            Err(diagnostics)
        }
    }
}
