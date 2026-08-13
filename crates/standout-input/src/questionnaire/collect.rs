//! Collection adapters: named file, explicit stdin, and interactive prompts.
//!
//! Each adapter is a thin normalizer. The file and stdin adapters read one
//! complete answer-sheet document and hand it to the shared parser; the
//! interactive adapter walks the fields through the existing prompt
//! abstractions ([`TextPromptSource`] plus the process-global
//! [`PromptResponder`](crate::PromptResponder) test seam). All three end at
//! the same place — [`RawAnswers`] — and every answer they capture runs
//! through the same field decoders and validators in
//! [`decode`](super::decode). There is deliberately no adapter-specific
//! conversion, message wording, or answer-source merging: one submission
//! comes from exactly one source.
//!
//! Interactive collection keeps the immediate feedback loop: a decode or
//! field-validation failure on an *entered* answer re-prompts the current
//! question with the diagnostic, keeping every previously accepted answer.
//! Cancellation (EOF / Ctrl+D or a responder `Cancel`) aborts the whole
//! collection with [`InputError::PromptCancelled`]. A non-input outcome (a
//! responder `Skip`, or mid-collection terminal loss) is not an entry: it
//! follows the shared blank rule where blank resolves — default or omission
//! — but on a required field without a default it terminates the pass with
//! [`InputError::NoInput`] instead of re-prompting a source that will never
//! answer.

use std::path::Path;

use crate::env::StdinReader;

use super::definition::Questionnaire;
use super::parse::{AnswerSheetDiagnostic, RawAnswers};

#[cfg(feature = "simple-prompts")]
use std::collections::BTreeMap;
#[cfg(feature = "simple-prompts")]
use std::sync::Arc;

#[cfg(feature = "simple-prompts")]
use super::definition::{Constraint, Group, Item, ScalarField, ScalarKind};

#[cfg(feature = "simple-prompts")]
use crate::sources::{RealTerminal, TerminalIO, TextPromptSource};
#[cfg(feature = "simple-prompts")]
use crate::InputError;

#[cfg(feature = "simple-prompts")]
use super::decode::{decode_field, is_active, parse_bool, EarlierAnswers, FieldOutcome, ScopeCtx};

impl Questionnaire {
    /// Read one complete answer sheet from a named file.
    ///
    /// The whole document is read and parsed in one pass; the result is the
    /// same [`RawAnswers`] representation every collection path produces.
    /// Validate it with [`decode_answers`](Self::decode_answers) (or
    /// [`decode_answers_with`](Self::decode_answers_with)).
    ///
    /// # Errors
    ///
    /// [`AnswerSheetDiagnostic::UnreadableDocument`] when the file cannot be
    /// read, otherwise the parser's accumulated diagnostics.
    pub fn read_answer_sheet_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|error| {
            vec![AnswerSheetDiagnostic::UnreadableDocument {
                detail: format!("{}: {error}", path.display()),
            }]
        })?;
        self.parse_answer_sheet(&text)
    }

    /// Read one complete answer sheet from explicitly requested stdin
    /// (e.g. an `--answers -` style flag), against an explicit
    /// [`StdinReader`] — [`DefaultStdin`](crate::env::DefaultStdin) for the
    /// process's real stdin (which honors a test override installed via
    /// [`set_default_stdin_reader`](crate::env::set_default_stdin_reader)),
    /// or an injected reader in tests.
    ///
    /// Selecting stdin is an explicit caller decision — this adapter never
    /// merges stdin answers with any other source.
    ///
    /// # Errors
    ///
    /// [`AnswerSheetDiagnostic::UnreadableDocument`] when stdin is an
    /// interactive terminal (there is no piped document to read) or fails to
    /// read, otherwise the parser's accumulated diagnostics.
    pub fn read_answer_sheet_stdin_with(
        &self,
        reader: &dyn StdinReader,
    ) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        if reader.is_terminal() {
            return Err(vec![AnswerSheetDiagnostic::UnreadableDocument {
                detail: "stdin is an interactive terminal; pipe an answer sheet or pass a file"
                    .to_string(),
            }]);
        }
        let text = reader.read_to_string().map_err(|error| {
            vec![AnswerSheetDiagnostic::UnreadableDocument {
                detail: format!("stdin: {error}"),
            }]
        })?;
        self.parse_answer_sheet(&text)
    }

    /// Collect answers interactively, one prompt per applicable field
    /// occurrence.
    ///
    /// Prompts through [`TextPromptSource`] on the real terminal — and
    /// therefore through any installed
    /// [`PromptResponder`](crate::PromptResponder), which is how tests drive
    /// this without a TTY. Every entered answer runs through the same field
    /// decoders and validators as file and stdin answers; a failure on an
    /// entered answer is a *local, retryable* error — the question
    /// re-prompts with the diagnostic and all previously accepted answers
    /// are kept. A blank entry follows the shared blank rule (default —
    /// static or computed — first, then omission, then a required-answer
    /// re-prompt), a field with a
    /// [`DynamicDefault`](super::DynamicDefault) shows its computed default
    /// in the prompt message, and inactive conditional fields are skipped
    /// without prompting.
    ///
    /// Groups walk their children in place. A repeatable group collects its
    /// declared minimum number of occurrences, then asks a yes/no
    /// "add another?" question (blank means no) before each further
    /// occurrence, stopping unprompted at the declared maximum — so an
    /// interactive submission always satisfies the declared bounds, exactly
    /// like a well-formed answer sheet.
    ///
    /// The result is the same [`RawAnswers`] representation the document
    /// adapters produce (each entry already field-valid, keyed by occurrence
    /// path); run [`decode_answers`](Self::decode_answers) or
    /// [`decode_answers_with`](Self::decode_answers_with) on it for typed
    /// values and whole-form rules.
    ///
    /// # Errors
    ///
    /// - [`InputError::PromptCancelled`] when the user cancels (EOF/Ctrl+D).
    /// - [`InputError::NoInput`] when stdin is not a terminal and no
    ///   responder is installed (interactive collection needs one or the
    ///   other; it never silently reads a piped document), or when a
    ///   non-input outcome (a responder `Skip`, or mid-collection terminal
    ///   loss) lands on a required field without a default — the pass
    ///   terminates rather than re-prompting a source that produced no
    ///   input.
    /// - Any terminal I/O failure from the underlying prompt source.
    #[cfg(feature = "simple-prompts")]
    pub fn collect_interactive(&self) -> Result<RawAnswers, InputError> {
        self.collect_interactive_with_terminal(Arc::new(RealTerminal))
    }

    /// [`collect_interactive`](Self::collect_interactive) against an
    /// explicit shared terminal, for callers and tests that inject their own
    /// [`TerminalIO`] (e.g. [`MockTerminal`](crate::MockTerminal)).
    #[cfg(feature = "simple-prompts")]
    pub fn collect_interactive_with_terminal<T: TerminalIO + 'static>(
        &self,
        terminal: Arc<T>,
    ) -> Result<RawAnswers, InputError> {
        if crate::responder::current_prompt_responder().is_none() && !terminal.is_terminal() {
            return Err(InputError::NoInput);
        }

        let mut collector = Collector {
            questionnaire: self,
            terminal,
            raw: BTreeMap::new(),
            occurrences: BTreeMap::new(),
            outcomes: BTreeMap::new(),
        };
        collector.collect_items(self.items(), &mut vec![ScopeCtx::root()])?;
        Ok(RawAnswers::from_parts(collector.raw, collector.occurrences))
    }
}

/// One interactive collection pass: the walk state threaded through the
/// definition tree.
#[cfg(feature = "simple-prompts")]
struct Collector<'a, T: TerminalIO + 'static> {
    questionnaire: &'a Questionnaire,
    terminal: Arc<T>,
    /// Accepted raw answer text per occurrence path.
    raw: BTreeMap<String, String>,
    /// Collected occurrences per repeatable-group path base.
    occurrences: BTreeMap<String, usize>,
    /// Decode outcomes per occurrence path, for condition evaluation.
    outcomes: BTreeMap<String, FieldOutcome>,
}

#[cfg(feature = "simple-prompts")]
impl<T: TerminalIO + 'static> Collector<'_, T> {
    /// Walk one scope's items; `chain` is the open scope chain, innermost
    /// last (used to build occurrence paths and resolve controllers).
    fn collect_items(
        &mut self,
        items: &[Item],
        chain: &mut Vec<ScopeCtx>,
    ) -> Result<(), InputError> {
        for item in items {
            match item {
                Item::Field(field) => self.collect_field(field, chain)?,
                Item::Group(group) => match group.repeat() {
                    None => {
                        let base = chain
                            .last()
                            .expect("chain starts rooted")
                            .child_path(group.id());
                        chain.push(scope_for(group, base));
                        self.collect_items(group.children(), chain)?;
                        chain.pop();
                    }
                    Some(repeat) => {
                        let base = chain
                            .last()
                            .expect("chain starts rooted")
                            .child_path(group.id());
                        let mut count = 0;
                        loop {
                            if count >= repeat.min()
                                && (repeat.max() == Some(count) || !self.ask_add_another(group)?)
                            {
                                break;
                            }
                            chain.push(scope_for(group, format!("{base}[{count}]")));
                            self.collect_items(group.children(), chain)?;
                            chain.pop();
                            count += 1;
                        }
                        self.occurrences.insert(base, count);
                    }
                },
            }
        }
        Ok(())
    }

    /// Prompt for one field occurrence, retrying locally until an answer
    /// decodes (inactive occurrences are skipped without prompting). A
    /// blank *entry* follows the shared blank rule — default, then
    /// omission — and re-prompts when the rule cannot resolve it (a
    /// required field without a default). A non-input outcome — a
    /// responder `Skip`, or mid-collection terminal loss — resolves
    /// through the same blank rule, but when the rule cannot absorb it,
    /// collection terminates with [`InputError::NoInput`] instead of
    /// re-prompting a source that produced no input.
    fn collect_field(&mut self, field: &ScalarField, chain: &[ScopeCtx]) -> Result<(), InputError> {
        let path = chain
            .last()
            .expect("chain starts rooted")
            .child_path(field.id());
        // Interactive fields either decode or terminate the pass, so
        // controllers are never in an errored state and applicability is
        // always known.
        if is_active(self.questionnaire, field, chain, &self.outcomes) != Some(true) {
            self.outcomes.insert(path, FieldOutcome::Inactive);
            return Ok(());
        }

        // Earlier fields in the walk are final by now, so the dynamic
        // default is computed once and shown in the prompt message.
        let computed = field.dynamic_default().map(|dynamic| {
            dynamic.compute(&EarlierAnswers::new(
                self.questionnaire,
                chain,
                &self.outcomes,
            ))
        });
        let base = interactive_message(field, computed.as_deref());
        let mut message = base.clone();
        loop {
            // A blank entry and a non-input outcome both resolve to an
            // empty string: the shared blank rule then decides between
            // default and omission. Where it cannot (required, no
            // default), the decode error below re-prompts an entry but
            // ends the pass on non-input.
            let response = self.prompt(message.clone())?;
            let entered = response.clone().unwrap_or_default();
            match decode_field(field, &path, Some(&entered), computed.as_deref()) {
                Ok(outcome) => {
                    self.raw.insert(path.clone(), entered.trim().to_string());
                    self.outcomes.insert(
                        path,
                        match outcome {
                            Some(value) => FieldOutcome::Answered(value),
                            None => FieldOutcome::Omitted,
                        },
                    );
                    return Ok(());
                }
                Err(diagnostic) => {
                    // No input arrived at all (a responder `Skip` or a lost
                    // terminal — not an entry): re-prompting would spin
                    // forever against a source that will never answer, so
                    // the pass ends cleanly instead.
                    if response.is_none() {
                        return Err(InputError::NoInput);
                    }
                    message = format!("{diagnostic} Try again: {base}");
                }
            }
        }
    }

    /// Ask whether to collect one more occurrence of a repeatable group.
    /// Blank (an empty entry or a non-input outcome) means no; a non-yes/no
    /// entry re-asks with the diagnostic.
    fn ask_add_another(&mut self, group: &Group) -> Result<bool, InputError> {
        let base = format!("Add another? {} (yes/no) ", group.prompt());
        let mut message = base.clone();
        loop {
            match self.prompt(message.clone())? {
                None => return Ok(false),
                Some(entered) => match parse_bool(&entered) {
                    Some(answer) => return Ok(answer),
                    None if entered.trim().is_empty() => return Ok(false),
                    None => {
                        message = format!(
                            "Expected a yes/no answer (true, false, yes, no, y, or n). Try again: {base}"
                        );
                    }
                },
            }
        }
    }

    /// One prompt round trip: `Ok(Some(text))` is an entered answer (a
    /// blank line arrives as `Some("")`), `Ok(None)` is a non-input outcome
    /// (a responder `Skip`, or a lost terminal); cancellation and I/O
    /// failures propagate.
    fn prompt(&self, message: String) -> Result<Option<String>, InputError> {
        TextPromptSource::with_terminal(message, self.terminal.clone()).prompt_entry()
    }
}

/// The scope-chain entry for one group occurrence.
#[cfg(feature = "simple-prompts")]
fn scope_for(group: &Group, path_prefix: String) -> ScopeCtx {
    ScopeCtx {
        group_id: Some(group.id().to_string()),
        def_prefix: group.def_prefix(),
        path_prefix,
    }
}

/// The cosmetic prompt message for one field: its wording plus entry hints
/// (choices, the yes/no vocabulary, and the default — static, or the
/// `computed` dynamic default for this occurrence).
#[cfg(feature = "simple-prompts")]
fn interactive_message(field: &ScalarField, computed: Option<&str>) -> String {
    let mut message = field.prompt().to_string();
    if let Some(Constraint::OneOf(choices)) = field.constraint() {
        message.push_str(&format!(" ({})", choices.join(" / ")));
    } else if field.kind() == ScalarKind::Bool {
        message.push_str(" (yes/no)");
    }
    if let Some(default) = field.default().or(computed) {
        message.push_str(&format!(" [default: {default}]"));
    }
    message.push(' ');
    message
}
