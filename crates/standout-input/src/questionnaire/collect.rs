use std::path::Path;

use crate::env::StdinReader;

use super::definition::Questionnaire;
use super::parse::{AnswerSheetDiagnostic, AnswerSheetFormat, RawAnswers};

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
    pub fn read_answer_sheet_file(
        &self,
        path: impl AsRef<Path>,
        format: &dyn AnswerSheetFormat,
    ) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|error| {
            vec![AnswerSheetDiagnostic::UnreadableDocument {
                detail: format!("{}: {error}", path.display()),
            }]
        })?;
        format.parse(self, &text)
    }

    pub fn read_answer_sheet_stdin(
        &self,
        reader: &dyn StdinReader,
        format: &dyn AnswerSheetFormat,
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
        format.parse(self, &text)
    }

    #[cfg(feature = "simple-prompts")]
    pub fn collect_interactive(&self) -> Result<RawAnswers, InputError> {
        self.collect_interactive_from(&crate::InputSources::from_process())
    }

    #[cfg(feature = "simple-prompts")]
    pub fn collect_interactive_from(
        &self,
        sources: &crate::InputSources,
    ) -> Result<RawAnswers, InputError> {
        self.collect_interactive_with_terminal_from(Arc::new(RealTerminal), sources)
    }

    #[cfg(feature = "simple-prompts")]
    pub fn collect_interactive_with_terminal<T: TerminalIO + 'static>(
        &self,
        terminal: Arc<T>,
    ) -> Result<RawAnswers, InputError> {
        self.collect_interactive_with_terminal_from(terminal, &crate::InputSources::from_process())
    }

    #[cfg(feature = "simple-prompts")]
    pub fn collect_interactive_with_terminal_from<T: TerminalIO + 'static>(
        &self,
        terminal: Arc<T>,
        sources: &crate::InputSources,
    ) -> Result<RawAnswers, InputError> {
        if sources.responder().is_none() && !terminal.is_terminal() {
            return Err(InputError::NoInput);
        }

        let mut collector = Collector {
            questionnaire: self,
            terminal,
            responder: sources.responder_arc(),
            raw: BTreeMap::new(),
            occurrences: BTreeMap::new(),
            outcomes: BTreeMap::new(),
        };
        collector.collect_items(self.items(), &mut vec![ScopeCtx::root()])?;
        Ok(RawAnswers::from_parts(collector.raw, collector.occurrences))
    }
}

#[cfg(feature = "simple-prompts")]
struct Collector<'a, T: TerminalIO + 'static> {
    questionnaire: &'a Questionnaire,
    terminal: Arc<T>,
    responder: Option<std::sync::Arc<dyn crate::PromptResponder>>,
    raw: BTreeMap<String, String>,
    occurrences: BTreeMap<String, usize>,
    outcomes: BTreeMap<String, FieldOutcome>,
}

#[cfg(feature = "simple-prompts")]
impl<T: TerminalIO + 'static> Collector<'_, T> {
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

    fn collect_field(&mut self, field: &ScalarField, chain: &[ScopeCtx]) -> Result<(), InputError> {
        let path = chain
            .last()
            .expect("chain starts rooted")
            .child_path(field.id());
        if is_active(self.questionnaire, field, chain, &self.outcomes) != Some(true) {
            self.outcomes.insert(path, FieldOutcome::Inactive);
            return Ok(());
        }

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
                    if response.is_none() {
                        return Err(InputError::NoInput);
                    }
                    message = format!("{diagnostic} Try again: {base}");
                }
            }
        }
    }

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

    fn prompt(&self, message: String) -> Result<Option<String>, InputError> {
        let source = TextPromptSource::with_terminal(message, self.terminal.clone());
        match &self.responder {
            Some(responder) => {
                let sources = crate::InputSources::from_process()
                    .with_responder(std::sync::Arc::clone(responder));
                source.prompt_entry_from(&sources)
            }
            None => source.prompt_entry(),
        }
    }
}

#[cfg(feature = "simple-prompts")]
fn scope_for(group: &Group, path_prefix: String) -> ScopeCtx {
    ScopeCtx {
        group_id: Some(group.id().to_string()),
        def_prefix: group.def_prefix(),
        path_prefix,
    }
}

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
