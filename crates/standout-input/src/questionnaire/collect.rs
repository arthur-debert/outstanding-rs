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
//! field-validation failure re-prompts the current question with the
//! diagnostic, keeping every previously accepted answer. Cancellation (EOF /
//! Ctrl+D or a responder `Cancel`) aborts the whole collection with
//! [`InputError::PromptCancelled`].

use std::path::Path;

use crate::env::{DefaultStdin, StdinReader};

use super::definition::Questionnaire;
use super::parse::{AnswerSheetDiagnostic, RawAnswers};

#[cfg(feature = "simple-prompts")]
use std::collections::BTreeMap;
#[cfg(feature = "simple-prompts")]
use std::sync::Arc;

#[cfg(feature = "simple-prompts")]
use super::definition::{Constraint, ScalarKind};

#[cfg(feature = "simple-prompts")]
use crate::sources::{RealTerminal, TerminalIO, TextPromptSource};
#[cfg(feature = "simple-prompts")]
use crate::InputError;

#[cfg(feature = "simple-prompts")]
use super::decode::{decode_field, is_active, FieldOutcome};

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
    /// (e.g. an `--answers -` style flag).
    ///
    /// Reads through the process-default stdin reader, which honors a test
    /// override installed via
    /// [`set_default_stdin_reader`](crate::env::set_default_stdin_reader).
    /// Selecting stdin is an explicit caller decision — this adapter never
    /// merges stdin answers with any other source.
    ///
    /// # Errors
    ///
    /// [`AnswerSheetDiagnostic::UnreadableDocument`] when stdin is an
    /// interactive terminal (there is no piped document to read) or fails to
    /// read, otherwise the parser's accumulated diagnostics.
    pub fn read_answer_sheet_stdin(&self) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        self.read_answer_sheet_stdin_with(&DefaultStdin)
    }

    /// [`read_answer_sheet_stdin`](Self::read_answer_sheet_stdin) against an
    /// explicit [`StdinReader`], for callers and tests that inject their own.
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

    /// Collect answers interactively, one prompt per applicable field.
    ///
    /// Prompts through [`TextPromptSource`] on the real terminal — and
    /// therefore through any installed
    /// [`PromptResponder`](crate::PromptResponder), which is how tests drive
    /// this without a TTY. Every entered answer runs through the same field
    /// decoders and validators as file and stdin answers; a failure is a
    /// *local, retryable* error — the question re-prompts with the
    /// diagnostic and all previously accepted answers are kept. A blank
    /// entry follows the shared blank rule (default first, then omission or
    /// a required-answer retry), and inactive conditional fields are skipped
    /// without prompting.
    ///
    /// The result is the same [`RawAnswers`] representation the document
    /// adapters produce (each entry already field-valid); run
    /// [`decode_answers`](Self::decode_answers) or
    /// [`decode_answers_with`](Self::decode_answers_with) on it for typed
    /// values and whole-form rules.
    ///
    /// # Errors
    ///
    /// - [`InputError::PromptCancelled`] when the user cancels (EOF/Ctrl+D).
    /// - [`InputError::NoInput`] when stdin is not a terminal and no
    ///   responder is installed (interactive collection needs one or the
    ///   other; it never silently reads a piped document).
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

        let mut raw: BTreeMap<String, String> = BTreeMap::new();
        let mut outcomes: BTreeMap<String, FieldOutcome> = BTreeMap::new();

        for field in self.fields() {
            // Interactive fields either decode or retry, so controllers are
            // never in an errored state and applicability is always known.
            if is_active(field, &outcomes) != Some(true) {
                outcomes.insert(field.id().to_string(), FieldOutcome::Inactive);
                continue;
            }

            let base = interactive_message(field);
            let mut message = base.clone();
            loop {
                let source = TextPromptSource::with_terminal(message.clone(), terminal.clone());
                let entered = match source.prompt() {
                    Ok(text) => text,
                    // Blank entry (or a responder `Skip`): the shared blank
                    // rule decides between default, omission, and retry.
                    Err(InputError::NoInput) => String::new(),
                    Err(error) => return Err(error),
                };
                match decode_field(field, Some(&entered)) {
                    Ok(outcome) => {
                        raw.insert(field.id().to_string(), entered.trim().to_string());
                        outcomes.insert(
                            field.id().to_string(),
                            match outcome {
                                Some(value) => FieldOutcome::Answered(value),
                                None => FieldOutcome::Omitted,
                            },
                        );
                        break;
                    }
                    Err(diagnostic) => {
                        message = format!("{diagnostic} Try again: {base}");
                    }
                }
            }
        }

        Ok(RawAnswers::from_values(raw))
    }
}

/// The cosmetic prompt message for one field: its wording plus entry hints
/// (choices, the yes/no vocabulary, the pre-filled default).
#[cfg(feature = "simple-prompts")]
fn interactive_message(field: &super::definition::ScalarField) -> String {
    let mut message = field.prompt().to_string();
    if let Some(Constraint::OneOf(choices)) = field.constraint() {
        message.push_str(&format!(" ({})", choices.join(" / ")));
    } else if field.kind() == ScalarKind::Bool {
        message.push_str(" (yes/no)");
    }
    if let Some(default) = field.default() {
        message.push_str(&format!(" [default: {default}]"));
    }
    message.push(' ');
    message
}
