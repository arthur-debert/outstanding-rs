use std::collections::{BTreeMap, HashSet};

use super::definition::{child_segment, path_join, Questionnaire};
use super::render::{FINGERPRINT_PREFIX, FORMAT_LINE, QUESTIONNAIRE_PREFIX, TAG_OPEN};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawAnswers {
    values: BTreeMap<String, String>,
    occurrences: BTreeMap<String, usize>,
    warnings: Vec<AnswerSheetDiagnostic>,
}

impl RawAnswers {
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

    pub fn get(&self, path: &str) -> Option<&str> {
        self.values.get(path).map(String::as_str)
    }

    pub fn occurrence_count(&self, group_path: &str) -> usize {
        self.occurrences.get(group_path).copied().unwrap_or(0)
    }

    pub fn warnings(&self) -> &[AnswerSheetDiagnostic] {
        &self.warnings
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum AnswerSheetDiagnostic {
    #[error("{message}")]
    Incompatible { message: String },

    #[error("Line {line}: {message}")]
    Tag { line: usize, message: String },

    #[error("Line {line}: warning: the answer for '{path}' contains '<id:'. A tag only marks a question when it ends its line; if this was meant to be a question line, remove everything after the tag — if it is ordinary prose, ignore this warning.")]
    SuspectedTagInAnswer { path: String, line: usize },

    #[error("Could not read the answer sheet: {detail}")]
    UnreadableDocument { detail: String },
}

impl AnswerSheetDiagnostic {
    fn incompatible(message: impl Into<String>) -> Self {
        Self::Incompatible {
            message: message.into(),
        }
    }

    fn tag(line: usize, message: impl Into<String>) -> Self {
        Self::Tag {
            line,
            message: message.into(),
        }
    }

    fn malformed_preamble(line: usize, detail: impl std::fmt::Display) -> Self {
        Self::incompatible(format!(
            "Line {line}: malformed answer-sheet preamble: {detail}. Render a fresh answer sheet and copy your answers into it."
        ))
    }
}

struct OpenAnswer {
    path: Option<String>,
    lines: Vec<(usize, String)>,
}

impl OpenAnswer {
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

struct Scope {
    group_id: String,
    def_prefix: String,
    path_prefix: String,
    discard: bool,
}

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
            diagnostics.push(AnswerSheetDiagnostic::tag(
                line,
                format!("unknown question tag '<id:{id}>'. This questionnaire does not define that ID; if the line is prose, add any character after the tag, otherwise render a fresh answer sheet."),
            ));
            return None;
        };
        let Some(keep) = resolve_scope(stack, meta.parent.as_deref()) else {
            diagnostics.push(AnswerSheetDiagnostic::tag(
                line,
                format!("misplaced '<id:{id}>'. That ID is not valid at this point of the sheet; keep each question inside its own group block, or render a fresh answer sheet to restore the structure."),
            ));
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
            diagnostics.push(AnswerSheetDiagnostic::tag(
                line,
                format!("duplicate question '<id:{path}>'. Each question may be answered once per occurrence; remove the extra question line or copy the complete group block instead."),
            ));
            return None;
        }
        Some(path)
    }

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
            diagnostics.push(AnswerSheetDiagnostic::tag(
                line,
                format!("misplaced '<id:{id}>'. That ID is not valid at this point of the sheet; keep each question inside its own group block, or render a fresh answer sheet to restore the structure."),
            ));
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
                    diagnostics.push(AnswerSheetDiagnostic::tag(
                        line,
                        format!("duplicate group '<id:{id}>'. This group is answered once; remove the extra block (only repeatable sections take copied blocks)."),
                    ));
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
                            diagnostics.push(AnswerSheetDiagnostic::incompatible(format!(
                                "Unsupported answer-format version '{}' (this release reads only version 1). Render a fresh answer sheet; old sheets are not migrated.",
                                version.trim()
                            )))
                        }
                        None => diagnostics.push(AnswerSheetDiagnostic::malformed_preamble(
                            at + 1,
                            format!("expected '{FORMAT_LINE}'"),
                        )),
                    }
                }
            }
            None => diagnostics.push(AnswerSheetDiagnostic::malformed_preamble(
                lines.len() + 1,
                format!("expected '{FORMAT_LINE}'"),
            )),
        }

        let expect_keyed = |i: &mut usize,
                            prefix: &str,
                            diagnostics: &mut Vec<AnswerSheetDiagnostic>|
         -> Option<String> {
            match next_content(i) {
                Some(at) => match lines[at].trim().strip_prefix(prefix) {
                    Some(value) => Some(value.trim().to_string()),
                    None => {
                        diagnostics.push(AnswerSheetDiagnostic::malformed_preamble(
                            at + 1,
                            format!("expected '{prefix} ...'"),
                        ));
                        None
                    }
                },
                None => {
                    diagnostics.push(AnswerSheetDiagnostic::malformed_preamble(
                        lines.len() + 1,
                        format!("expected '{prefix} ...'"),
                    ));
                    None
                }
            }
        };

        if let Some(found) = expect_keyed(&mut i, QUESTIONNAIRE_PREFIX, &mut diagnostics) {
            if found != self.id() {
                diagnostics.push(AnswerSheetDiagnostic::incompatible(format!(
                    "This answer sheet is for questionnaire '{found}', not '{expected}'. Render a fresh answer sheet for '{expected}'.",
                    expected = self.id()
                )));
            }
        }
        if let Some(found) = expect_keyed(&mut i, FINGERPRINT_PREFIX, &mut diagnostics) {
            if found != self.fingerprint() {
                diagnostics.push(AnswerSheetDiagnostic::incompatible(format!(
                    "This answer sheet was rendered from a different version of questionnaire semantics (fingerprint '{found}', expected '{expected}'). The questionnaire changed since this sheet was rendered; render a fresh answer sheet and copy your answers into it. Answers are not migrated.",
                    expected = self.fingerprint()
                )));
            }
        }

        if diagnostics.is_empty() {
            Ok(i)
        } else {
            Err(diagnostics)
        }
    }
}

fn resolve_scope(stack: &[Scope], parent: Option<&str>) -> Option<usize> {
    match parent {
        None => Some(0),
        Some(parent) => stack
            .iter()
            .rposition(|scope| scope.group_id == parent)
            .map(|found| found + 1),
    }
}
