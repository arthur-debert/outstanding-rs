//! Parsing edited answer sheets back into raw answers.
//!
//! Parsing recognizes structure from a single rule: a line is a *question
//! line* if and only if it ends with a well-formed `<id:...>` tag — the tag
//! is the last non-whitespace content on the line. Any non-blank character
//! after the tag, even a period, demotes the whole line to ordinary prose.
//! An answer is all text between a question line and the next question line
//! (or end of file), outer whitespace trimmed, internal line breaks
//! preserved. A line whose terminal tag names a group opens one group
//! occurrence; occurrence counting is simply counting the group's tag
//! lines, exactly as copy-the-block editing implies. Everything before the
//! tag on a question line — display numbers, wording, indentation, type
//! hints — is cosmetic and freely editable, so a user may reword or
//! renumber a sheet without changing what it means.
//!
//! One limitation is accepted by design: an answer whose own line ends with
//! a schema-valid `<id:...>` tag is misparsed as a question line. There is
//! deliberately no escaping mechanism — the shape is rare in prose. As a
//! guard, accepted answer text containing `<id:` anywhere raises a
//! warning-level diagnostic ([`RawAnswers::warnings`]), which also catches
//! mangled or half-deleted tags.
//!
//! Repeated items are counted from occurrences of the stable group tag,
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
use super::render::{FINGERPRINT_PREFIX, FORMAT_LINE, QUESTIONNAIRE_PREFIX, TAG_OPEN};

/// The raw answers parsed from one answer sheet.
///
/// Values are keyed by *occurrence path* — the stable field ID, with a
/// zero-based index inserted for every enclosing repeatable-group occurrence
/// (`command.inputs[1].name`) — and hold the verbatim answer text with outer
/// whitespace trimmed and internal line breaks preserved. A field absent
/// from the document is absent here; a field whose answer area was left
/// blank is present with an empty string. Occurrence counts of repeatable
/// groups are carried alongside ([`occurrence_count`](Self::occurrence_count)),
/// as are any warning-level diagnostics ([`warnings`](Self::warnings)).
/// Decoding raw text into typed values (defaults, omission, validation) is a
/// later stage, shared with interactive collection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawAnswers {
    values: BTreeMap<String, String>,
    /// Occurrences per repeatable group, keyed by the group's own occurrence
    /// path base (`command.inputs`, or `command.inputs[0].flags` when
    /// nested). Groups with no submitted occurrence are absent.
    occurrences: BTreeMap<String, usize>,
    /// Warning-level diagnostics that do not fail the parse (currently:
    /// accepted answer text containing a `<id:` tag fragment).
    warnings: Vec<AnswerSheetDiagnostic>,
}

impl RawAnswers {
    /// Build raw answers directly, for collection paths (interactive
    /// prompting) that never see a document. Keys are occurrence paths;
    /// values are trimmed answer text; `occurrences` counts each repeatable
    /// group's collected occurrences by path base. Interactive answers are
    /// decoded as they are entered, so they carry no document warnings.
    #[cfg(feature = "simple-prompts")]
    pub(crate) fn from_parts(
        values: BTreeMap<String, String>,
        occurrences: BTreeMap<String, usize>,
    ) -> Self {
        Self {
            values,
            occurrences,
            warnings: Vec::new(),
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

    /// Warning-level diagnostics from parsing: problems worth showing the
    /// user that do not invalidate the submission. Currently
    /// [`AnswerSheetDiagnostic::SuspectedTagInAnswer`], raised when accepted
    /// answer text contains `<id:` anywhere — a mid-line tag mention, a
    /// mangled tag, or a half-deleted one. Empty for interactive collection.
    pub fn warnings(&self) -> &[AnswerSheetDiagnostic] {
        &self.warnings
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

    /// A question line's tag carries an ID the schema does not know.
    #[error("Line {line}: unknown question tag '<id:{id}>'. This questionnaire does not define that ID; if the line is prose, add any character after the tag, otherwise render a fresh answer sheet.")]
    UnknownTag {
        /// The unrecognized tag ID.
        id: String,
        /// 1-based line number of the question line.
        line: usize,
    },

    /// A field's question line appears more than once at the same
    /// occurrence path.
    #[error("Line {line}: duplicate question '<id:{path}>'. Each question may be answered once per occurrence; remove the extra question line or copy the complete group block instead.")]
    DuplicateField {
        /// The duplicated occurrence path.
        path: String,
        /// 1-based line number of the second occurrence.
        line: usize,
    },

    /// A non-repeatable group's tag line appears more than once.
    #[error("Line {line}: duplicate group '<id:{id}>'. This group is answered once; remove the extra block (only repeatable sections take copied blocks).")]
    DuplicateGroup {
        /// The duplicated stable group ID.
        id: String,
        /// 1-based line number of the second tag line.
        line: usize,
    },

    /// A known field or group tag appeared outside the scope its definition
    /// allows (e.g. a group's child without its group tag line, or another
    /// group's child inside this group's block).
    #[error("Line {line}: misplaced '<id:{id}>'. That ID is not valid at this point of the sheet; keep each question inside its own group block, or render a fresh answer sheet to restore the structure.")]
    MisplacedId {
        /// The known-but-misplaced stable ID.
        id: String,
        /// 1-based line number of the question line.
        line: usize,
    },

    /// Warning: accepted answer text contains `<id:` — a mid-line tag
    /// mention, a mangled tag, or a half-deleted one. The submission is
    /// still accepted; a tag only structures the sheet when it ends its
    /// line.
    #[error("Line {line}: warning: the answer for '{path}' contains '<id:'. A tag only marks a question when it ends its line; if this was meant to be a question line, remove everything after the tag — if it is ordinary prose, ignore this warning.")]
    SuspectedTagInAnswer {
        /// The occurrence path of the answer holding the fragment.
        path: String,
        /// 1-based line number of the answer line containing `<id:`.
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
    /// of an unknown, duplicate, or misplaced question line so it cannot
    /// leak into a neighbor.
    path: Option<String>,
    /// Accumulated answer lines with their 0-based document line indexes,
    /// so tag-fragment warnings can point at the exact line.
    lines: Vec<(usize, String)>,
}

impl OpenAnswer {
    /// Commit this answer: trim outer whitespace, keep internal line
    /// breaks, and raise a [`AnswerSheetDiagnostic::SuspectedTagInAnswer`]
    /// warning for every accepted line containing `<id:`.
    fn flush_into(
        self,
        values: &mut BTreeMap<String, String>,
        warnings: &mut Vec<AnswerSheetDiagnostic>,
    ) {
        let Some(path) = self.path else {
            return;
        };
        for (index, line) in &self.lines {
            if line.contains(TAG_OPEN) {
                warnings.push(AnswerSheetDiagnostic::SuspectedTagInAnswer {
                    path: path.clone(),
                    line: index + 1,
                });
            }
        }
        let text = self
            .lines
            .into_iter()
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
            .join("\n");
        values.insert(path, text.trim().to_string());
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

/// Returns the ID of the line-terminal `<id:...>` tag when `line` ends with
/// one: the tag must be the last non-whitespace content on the line, and its
/// ID must be shaped like a stable ID (non-empty, only `a-z`, `0-9`, `.`,
/// `_`, `-`). Any trailing non-blank character after the tag — or a
/// malformed ID — makes the line ordinary prose (`None`).
fn terminal_tag(line: &str) -> Option<&str> {
    let before_close = line.trim_end().strip_suffix('>')?;
    let open = before_close.rfind(TAG_OPEN)?;
    let id = &before_close[open + TAG_OPEN.len()..];
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    valid.then_some(id)
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
    /// The body parses in one linear pass under a single recognition rule:
    /// a line is a question line if and only if it ends with a `<id:...>`
    /// tag as its last non-whitespace content; any trailing non-blank
    /// character demotes the line to prose. A field's answer is everything
    /// between its question line and the next question line (or end of
    /// file), outer whitespace trimmed, internal line breaks preserved. A
    /// group tag line opens one group occurrence — repeated items come from
    /// repeated tag lines, so copying a complete rendered group block
    /// submits one more occurrence, whatever its display numbers say.
    /// Bracketed prose, `->` bullets, and mid-line tag mentions inside an
    /// answer are inert answer text (a mid-line `<id:` raises a
    /// [warning](RawAnswers::warnings)). The accepted trade-off: an answer
    /// line that itself *ends* with a schema-valid tag is read as a
    /// question line; there is no escaping mechanism.
    ///
    /// # Errors
    ///
    /// Returns every accumulated [`AnswerSheetDiagnostic`]: compatibility
    /// mismatches, malformed preambles, and unknown, duplicate, or
    /// misplaced tags on question lines. Occurrence counts *below* a
    /// repeatable group's minimum (or above its maximum) are not parse
    /// errors — they are structural validation, reported with the other
    /// value diagnostics by [`decode_answers`](Self::decode_answers).
    pub fn parse_answer_sheet(&self, text: &str) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        let lines: Vec<&str> = text.lines().collect();
        let body_start = self.check_preamble(&lines)?;

        let mut diagnostics: Vec<AnswerSheetDiagnostic> = Vec::new();
        let mut warnings: Vec<AnswerSheetDiagnostic> = Vec::new();
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        let mut occurrences: BTreeMap<String, usize> = BTreeMap::new();
        let mut seen_sections: HashSet<String> = HashSet::new();
        let mut stack: Vec<Scope> = Vec::new();
        let mut open: Option<OpenAnswer> = None;

        for (index, line) in lines.iter().enumerate().skip(body_start) {
            let Some(id) = terminal_tag(line) else {
                // Ordinary content: part of the open answer, or ignored
                // prose between a group tag line and its first question.
                if let Some(current) = open.as_mut() {
                    current.lines.push((index, line.to_string()));
                }
                continue;
            };

            if let Some(previous) = open.take() {
                previous.flush_into(&mut values, &mut warnings);
            }
            let is_group = self.node_meta(id).is_some_and(|meta| meta.group);
            if is_group {
                self.open_group(
                    id,
                    index,
                    &mut stack,
                    &mut occurrences,
                    &mut seen_sections,
                    &mut diagnostics,
                );
            } else {
                let path = self.open_field(id, index, &mut stack, &values, &mut diagnostics);
                open = Some(OpenAnswer {
                    path,
                    lines: Vec::new(),
                });
            }
        }
        if let Some(last) = open {
            last.flush_into(&mut values, &mut warnings);
        }

        if diagnostics.is_empty() {
            Ok(RawAnswers {
                values,
                occurrences,
                warnings,
            })
        } else {
            Err(diagnostics)
        }
    }

    /// Recognize one field question line: resolve its scope (popping closed
    /// groups), then return the occurrence path to accumulate its answer
    /// under — or `None` (a discard sink) with the appropriate diagnostic.
    /// Also handles unknown tags: the schema knows no node for the ID.
    fn open_field(
        &self,
        id: &str,
        line_index: usize,
        stack: &mut Vec<Scope>,
        values: &BTreeMap<String, String>,
        diagnostics: &mut Vec<AnswerSheetDiagnostic>,
    ) -> Option<String> {
        let line = line_index + 1;
        let Some(meta) = self.node_meta(id) else {
            diagnostics.push(AnswerSheetDiagnostic::UnknownTag {
                id: id.to_string(),
                line,
            });
            return None;
        };
        let Some(keep) = resolve_scope(stack, meta.parent.as_deref()) else {
            diagnostics.push(AnswerSheetDiagnostic::MisplacedId {
                id: id.to_string(),
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
        let path = path_join(path_prefix, child_segment(def_prefix, id));
        if values.contains_key(&path) {
            diagnostics.push(AnswerSheetDiagnostic::DuplicateField { path, line });
            return None;
        }
        Some(path)
    }

    /// Recognize one group tag line: resolve its scope, count the
    /// occurrence, and push the occurrence scope (a discard scope after a
    /// misplacement or duplicate, so nested content does not cascade
    /// diagnostics).
    fn open_group(
        &self,
        id: &str,
        line_index: usize,
        stack: &mut Vec<Scope>,
        occurrences: &mut BTreeMap<String, usize>,
        seen_sections: &mut HashSet<String>,
        diagnostics: &mut Vec<AnswerSheetDiagnostic>,
    ) {
        let line = line_index + 1;
        let group = self
            .group_def(id)
            .expect("caller verified the ID names a group");
        let parent = self
            .node_meta(id)
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
                id: id.to_string(),
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
        let base = path_join(parent_path, child_segment(parent_def, id));
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
                        id: id.to_string(),
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
