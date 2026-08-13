//! Parsing edited answer sheets back into raw answers.
//!
//! Parsing recognizes structure only from stable identities: a field opens
//! when a header-shaped line carries a schema-recognized bracketed ID, valid
//! in the current scope, and the following line begins with the `->` answer
//! marker. A group occurrence opens when a header-shaped line (never a
//! marker line) carries a group ID valid in the current scope *and* the next
//! header-shaped line is a child that group's definition permits — so
//! bracketed prose inside an answer stays answer content unless it satisfies
//! the full header contract. Everything cosmetic — display numbers, wording,
//! indentation, type hints — is ignored, so a user may freely reword or
//! renumber a sheet without changing what it means.
//!
//! Repeated items are counted from occurrences of the stable group header,
//! never from display numbers or wording. Each occurrence of a repeatable
//! group gives its answers an indexed *occurrence path* (`command.inputs`
//! occurrence 1 holds `command.inputs[1].name`); fields outside repeatable
//! groups keep their definition IDs as paths.
//!
//! Compatibility is exact-version: the preamble's answer-format version,
//! questionnaire ID, and fingerprint must all match the parsing definition,
//! or parsing stops with diagnostics that ask for a freshly rendered sheet.
//! No migration or fuzzy matching is attempted.

use std::collections::{BTreeMap, HashSet};

use super::definition::{child_segment, path_join, Questionnaire};
use super::render::{ANSWER_MARKER, FINGERPRINT_PREFIX, FORMAT_LINE, QUESTIONNAIRE_PREFIX};

/// The raw answers parsed from one answer sheet.
///
/// Values are keyed by *occurrence path* — the stable field ID, with a
/// zero-based index inserted for every enclosing repeatable-group occurrence
/// (`command.inputs[1].name`) — and hold the verbatim answer text with outer
/// whitespace trimmed and internal line breaks preserved. A field absent
/// from the document is absent here; a field whose marker was left blank is
/// present with an empty string. Occurrence counts of repeatable groups are
/// carried alongside ([`occurrence_count`](Self::occurrence_count)).
/// Decoding raw text into typed values (defaults, omission, validation) is a
/// later stage, shared with interactive collection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawAnswers {
    values: BTreeMap<String, String>,
    /// Occurrences per repeatable group, keyed by the group's own occurrence
    /// path base (`command.inputs`, or `command.inputs[0].flags` when
    /// nested). Groups with no submitted occurrence are absent.
    occurrences: BTreeMap<String, usize>,
}

impl RawAnswers {
    /// Build raw answers directly, for collection paths (interactive
    /// prompting) that never see a document. Keys are occurrence paths;
    /// values are trimmed answer text; `occurrences` counts each repeatable
    /// group's collected occurrences by path base.
    #[cfg(feature = "simple-prompts")]
    pub(crate) fn from_parts(
        values: BTreeMap<String, String>,
        occurrences: BTreeMap<String, usize>,
    ) -> Self {
        Self {
            values,
            occurrences,
        }
    }

    /// The raw answer text at an occurrence path (for fields outside
    /// repeatable groups: the stable field ID), if it appeared.
    pub fn get(&self, path: &str) -> Option<&str> {
        self.values.get(path).map(String::as_str)
    }

    /// How many occurrences of a repeatable group appeared, addressed by the
    /// group's occurrence path base — `command.inputs` at the root,
    /// `command.inputs[0].flags` for a group nested in another occurrence.
    pub fn occurrence_count(&self, group_path: &str) -> usize {
        self.occurrences.get(group_path).copied().unwrap_or(0)
    }

    /// Iterate over `(occurrence_path, raw_answer)` pairs, ordered by path.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Number of answered occurrence paths in the document.
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
/// Diagnostics identify locations by 1-based line number and fields by
/// stable ID or occurrence path; they never echo full answer values, since
/// answer sheets may contain sensitive content. Compatibility diagnostics
/// deliberately point at re-rendering: version 1 rejects incompatible sheets
/// instead of migrating them.
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

    /// A field header appears more than once at the same occurrence path.
    #[error("Line {line}: duplicate field '[{path}]'. Each field may be answered once per occurrence; remove the extra occurrence or copy the complete group block instead.")]
    DuplicateField {
        /// The duplicated occurrence path.
        path: String,
        /// 1-based line number of the second occurrence.
        line: usize,
    },

    /// A non-repeatable group's header appears more than once.
    #[error("Line {line}: duplicate group '[{id}]'. This group is answered once; remove the extra block (only repeatable sections take copied blocks).")]
    DuplicateGroup {
        /// The duplicated stable group ID.
        id: String,
        /// 1-based line number of the second header.
        line: usize,
    },

    /// A known field or group header appeared outside the scope its
    /// definition allows (e.g. a group's child without its group header, or
    /// another group's child inside this group's block).
    #[error("Line {line}: misplaced '[{id}]'. That ID is not valid at this point of the sheet; keep each field inside its own group block, or render a fresh answer sheet to restore the structure.")]
    MisplacedId {
        /// The known-but-misplaced stable ID.
        id: String,
        /// 1-based line number of the header-shaped line.
        line: usize,
    },

    /// A group header was written with a field-style `->` answer marker.
    #[error("Line {line}: group '[{id}]' does not take a '->' answer; groups introduce their nested questions. Remove the marker line.")]
    GroupAnswerMarker {
        /// The group ID carrying the marker.
        id: String,
        /// 1-based line number of the header-shaped line.
        line: usize,
    },

    /// The answer-sheet document could not be read at all (unreadable file,
    /// terminal stdin, or an I/O failure), so no content was parsed.
    #[error("Could not read the answer sheet: {detail}")]
    UnreadableDocument {
        /// What prevented reading, without any document content.
        detail: String,
    },
}

/// The field (or discard sink) currently accumulating answer lines.
struct OpenAnswer {
    /// `Some(path)` for a recognized field; `None` discards the answer text
    /// of an unknown, duplicate, or misplaced header so it cannot leak into
    /// a neighbor.
    path: Option<String>,
    lines: Vec<String>,
}

impl OpenAnswer {
    fn flush_into(self, values: &mut BTreeMap<String, String>) {
        if let Some(path) = self.path {
            values.insert(path, self.lines.join("\n").trim().to_string());
        }
    }
}

/// One open group occurrence on the parser's scope stack.
struct Scope {
    /// The group's stable ID.
    group_id: String,
    /// The definition-ID prefix its children extend (`<group_id>.`).
    def_prefix: String,
    /// The occurrence path of this occurrence (`command.inputs[1]`).
    path_prefix: String,
    /// A scope opened after a structural diagnostic: its content parses for
    /// boundary tracking but is discarded rather than piling on speculative
    /// diagnostics.
    discard: bool,
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

/// Whether a line is an answer-marker line (`->` after optional indent).
/// Marker lines carry answer text; they can never be group headers.
fn is_marker_line(line: &str) -> bool {
    line.trim_start().starts_with(ANSWER_MARKER)
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
    /// Within the body, a field is recognized only by a bracketed ID that is
    /// schema-valid where it appears and whose *next* line begins with the
    /// `->` answer marker; a group occurrence is recognized by a bracketed
    /// group ID (on a non-marker line) that is schema-valid where it appears
    /// and is followed by a child its definition permits. An answer is
    /// everything after its marker up to the next recognized header or end
    /// of file, outer whitespace trimmed, internal line breaks preserved.
    /// Bracketed prose inside an answer is answer text unless it satisfies
    /// one of those full header contracts. Repeated items come from repeated
    /// occurrences of the stable group header: copying a complete rendered
    /// group block submits one more occurrence, whatever its display
    /// numbers say.
    ///
    /// # Errors
    ///
    /// Returns every accumulated [`AnswerSheetDiagnostic`]: compatibility
    /// mismatches, malformed preambles, and unknown, duplicate, or misplaced
    /// IDs on header-shaped lines. Occurrence counts *below* a repeatable
    /// group's minimum (or above its maximum) are not parse errors — they
    /// are structural validation, reported with the other value diagnostics
    /// by [`decode_answers`](Self::decode_answers).
    pub fn parse_answer_sheet(&self, text: &str) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        let lines: Vec<&str> = text.lines().collect();
        let body_start = self.check_preamble(&lines)?;

        let mut diagnostics: Vec<AnswerSheetDiagnostic> = Vec::new();
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        let mut occurrences: BTreeMap<String, usize> = BTreeMap::new();
        let mut seen_sections: HashSet<String> = HashSet::new();
        let mut stack: Vec<Scope> = Vec::new();
        let mut open: Option<OpenAnswer> = None;

        let mut i = body_start;
        while i < lines.len() {
            let line = lines[i];
            let candidate = header_candidate(line);
            let marker_follows = lines.get(i + 1).is_some_and(|next| is_marker_line(next));

            // A field-header shape: bracketed ID followed by a marker line.
            if let (true, Some(candidate)) = (marker_follows, candidate) {
                if let Some(previous) = open.take() {
                    previous.flush_into(&mut values);
                }
                let path = self.open_field(candidate, i, &mut stack, &values, &mut diagnostics);
                let marker_line = lines[i + 1].trim_start();
                let first = marker_line[ANSWER_MARKER.len()..].to_string();
                open = Some(OpenAnswer {
                    path,
                    lines: vec![first],
                });
                i += 2;
                continue;
            }

            // A group-header shape: bracketed group ID on a non-marker line,
            // followed by a permitted child header.
            if let Some(candidate) = candidate {
                let is_group = self.node_meta(candidate).is_some_and(|meta| meta.group);
                if is_group
                    && !is_marker_line(line)
                    && self.group_contract_holds(candidate, &lines, i)
                {
                    if let Some(previous) = open.take() {
                        previous.flush_into(&mut values);
                    }
                    self.open_group(
                        candidate,
                        i,
                        &mut stack,
                        &mut occurrences,
                        &mut seen_sections,
                        &mut diagnostics,
                    );
                    i += 1;
                    continue;
                }
            }

            // Ordinary content: part of the open answer, or ignored prose.
            if let Some(current) = open.as_mut() {
                current.lines.push(line.to_string());
            }
            i += 1;
        }
        if let Some(last) = open {
            last.flush_into(&mut values);
        }

        if diagnostics.is_empty() {
            Ok(RawAnswers {
                values,
                occurrences,
            })
        } else {
            Err(diagnostics)
        }
    }

    /// Recognize one field header: resolve its scope (popping closed
    /// groups), then return the occurrence path to accumulate its answer
    /// under — or `None` (a discard sink) with the appropriate diagnostic.
    fn open_field(
        &self,
        candidate: &str,
        line_index: usize,
        stack: &mut Vec<Scope>,
        values: &BTreeMap<String, String>,
        diagnostics: &mut Vec<AnswerSheetDiagnostic>,
    ) -> Option<String> {
        let line = line_index + 1;
        let Some(meta) = self.node_meta(candidate) else {
            diagnostics.push(AnswerSheetDiagnostic::UnknownFieldId {
                id: candidate.to_string(),
                line,
            });
            return None;
        };
        if meta.group {
            diagnostics.push(AnswerSheetDiagnostic::GroupAnswerMarker {
                id: candidate.to_string(),
                line,
            });
            return None;
        }
        let Some(keep) = resolve_scope(stack, meta.parent.as_deref()) else {
            diagnostics.push(AnswerSheetDiagnostic::MisplacedId {
                id: candidate.to_string(),
                line,
            });
            return None;
        };
        stack.truncate(keep);
        let (def_prefix, path_prefix, discard) = match stack.last() {
            Some(scope) => (
                scope.def_prefix.as_str(),
                scope.path_prefix.as_str(),
                scope.discard,
            ),
            None => ("", "", false),
        };
        if discard {
            return None;
        }
        let path = path_join(path_prefix, child_segment(def_prefix, candidate));
        if values.contains_key(&path) {
            diagnostics.push(AnswerSheetDiagnostic::DuplicateField { path, line });
            return None;
        }
        Some(path)
    }

    /// Recognize one group header whose contract already held: resolve its
    /// scope, count the occurrence, and push the occurrence scope (a discard
    /// scope after a misplacement or duplicate, so nested content does not
    /// cascade diagnostics).
    fn open_group(
        &self,
        candidate: &str,
        line_index: usize,
        stack: &mut Vec<Scope>,
        occurrences: &mut BTreeMap<String, usize>,
        seen_sections: &mut HashSet<String>,
        diagnostics: &mut Vec<AnswerSheetDiagnostic>,
    ) {
        let line = line_index + 1;
        let group = self
            .group_def(candidate)
            .expect("caller verified the ID names a group");
        let parent = self
            .node_meta(candidate)
            .expect("known group has meta")
            .parent
            .clone();

        let discard_scope = |discard: bool| Scope {
            group_id: group.id().to_string(),
            def_prefix: group.def_prefix(),
            path_prefix: String::new(),
            discard,
        };

        let Some(keep) = resolve_scope(stack, parent.as_deref()) else {
            diagnostics.push(AnswerSheetDiagnostic::MisplacedId {
                id: candidate.to_string(),
                line,
            });
            stack.push(discard_scope(true));
            return;
        };
        stack.truncate(keep);
        let (parent_def, parent_path, parent_discard) = match stack.last() {
            Some(scope) => (
                scope.def_prefix.as_str(),
                scope.path_prefix.as_str(),
                scope.discard,
            ),
            None => ("", "", false),
        };
        if parent_discard {
            stack.push(discard_scope(true));
            return;
        }
        let base = path_join(parent_path, child_segment(parent_def, candidate));
        let path_prefix = match group.repeat() {
            Some(_) => {
                let count = occurrences.entry(base.clone()).or_insert(0);
                let index = *count;
                *count += 1;
                format!("{base}[{index}]")
            }
            None => {
                if !seen_sections.insert(base.clone()) {
                    diagnostics.push(AnswerSheetDiagnostic::DuplicateGroup {
                        id: candidate.to_string(),
                        line,
                    });
                    stack.push(discard_scope(true));
                    return;
                }
                base
            }
        };
        stack.push(Scope {
            group_id: group.id().to_string(),
            def_prefix: group.def_prefix(),
            path_prefix,
            discard: false,
        });
    }

    /// The group-header contract beyond the ID itself: the next
    /// header-shaped, non-marker line *whose ID the definition recognizes*
    /// must carry a *direct child* of this group — a child field followed
    /// by its own `->` marker, or a child group. Header-shaped lines with
    /// unknown IDs are ordinary prose (they cannot speak to the contract)
    /// and are skipped like any other non-header line. A recognized ID that
    /// is not a direct child fails the contract, leaving the group line as
    /// ordinary prose.
    fn group_contract_holds(&self, group_id: &str, lines: &[&str], at: usize) -> bool {
        for (offset, line) in lines[at + 1..].iter().enumerate() {
            if is_marker_line(line) {
                continue;
            }
            let Some(next) = header_candidate(line) else {
                continue;
            };
            let Some(meta) = self.node_meta(next) else {
                continue;
            };
            if meta.parent.as_deref() != Some(group_id) {
                return false;
            }
            if meta.group {
                return true;
            }
            let index = at + 1 + offset;
            return lines
                .get(index + 1)
                .is_some_and(|next| is_marker_line(next));
        }
        false
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

/// Find the stack depth to keep so the top of the stack is the scope for
/// `parent` (`None` = the questionnaire root): closed sibling groups pop,
/// while an ID whose parent is not on the stack at all is misplaced
/// (`None`).
fn resolve_scope(stack: &[Scope], parent: Option<&str>) -> Option<usize> {
    match parent {
        None => Some(0),
        Some(parent) => stack
            .iter()
            .rposition(|scope| scope.group_id == parent)
            .map(|found| found + 1),
    }
}
