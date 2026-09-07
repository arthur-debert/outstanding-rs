use std::fmt::Display;
use std::io::IsTerminal;
use std::ops::ControlFlow;
use std::sync::Arc;

use clap::ArgMatches;
use inquire::{MultiSelect, Select};

use super::map_inquire_error;
use crate::collector::InputCollector;
use crate::responder::PromptResponder;
use crate::{InputError, InputSources};

#[derive(Clone)]
pub struct InquireSelect<T> {
    message: String,
    options: Vec<T>,
    help_message: Option<String>,
    page_size: usize,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl<T: Display + Clone + Send + Sync + 'static> InquireSelect<T> {
    pub fn new(message: impl Into<String>, options: Vec<T>) -> Self {
        Self {
            message: message.into(),
            options,
            help_message: None,
            page_size: 10,
            responder: None,
        }
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help_message = Some(help.into());
        self
    }

    pub fn page_size(mut self, size: usize) -> Self {
        self.page_size = size;
        self
    }

    pub fn prompt(&self) -> Result<T, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    pub fn prompt_from(&self, sources: &InputSources) -> Result<T, InputError> {
        crate::collector::prompt_value_from(self, sources)
    }
}

impl<T: Display + Clone + Send + Sync + 'static> InputCollector<T> for InquireSelect<T> {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        !self.options.is_empty() && (self.responder.is_some() || std::io::stdin().is_terminal())
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<T>, InputError> {
        if let ControlFlow::Break(index) =
            crate::responder::collect_intercept(crate::responder::intercept_choice(
                &self.message,
                self.options.len(),
                self.responder.as_deref(),
            ))?
        {
            return Ok(index.map(|i| self.options[i].clone()));
        }

        if self.options.is_empty() {
            return Ok(None);
        }

        let mut prompt =
            Select::new(&self.message, self.options.clone()).with_page_size(self.page_size);

        if let Some(help) = &self.help_message {
            prompt = prompt.with_help_message(help);
        }

        let result = prompt.prompt().map_err(map_inquire_error)?;
        Ok(Some(result))
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<T>>> {
        let mut bound = self.clone();
        bound.responder = Some(sources.responder_arc()?);
        Some(Box::new(bound))
    }

    fn can_retry(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct InquireMultiSelect<T> {
    message: String,
    options: Vec<T>,
    help_message: Option<String>,
    page_size: usize,
    min_selections: Option<usize>,
    max_selections: Option<usize>,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl<T: Display + Clone + Send + Sync + 'static> InquireMultiSelect<T> {
    pub fn new(message: impl Into<String>, options: Vec<T>) -> Self {
        Self {
            message: message.into(),
            options,
            help_message: None,
            page_size: 10,
            min_selections: None,
            max_selections: None,
            responder: None,
        }
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help_message = Some(help.into());
        self
    }

    pub fn page_size(mut self, size: usize) -> Self {
        self.page_size = size;
        self
    }

    pub fn min_selections(mut self, min: usize) -> Self {
        self.min_selections = Some(min);
        self
    }

    pub fn max_selections(mut self, max: usize) -> Self {
        self.max_selections = Some(max);
        self
    }

    pub fn prompt(&self) -> Result<Vec<T>, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    pub fn prompt_from(&self, sources: &InputSources) -> Result<Vec<T>, InputError> {
        crate::collector::prompt_value_from(self, sources)
    }
}

impl<T: Display + Clone + Send + Sync + 'static> InputCollector<Vec<T>> for InquireMultiSelect<T> {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        !self.options.is_empty() && (self.responder.is_some() || std::io::stdin().is_terminal())
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<Vec<T>>, InputError> {
        let result = if let ControlFlow::Break(indices) =
            crate::responder::collect_intercept(crate::responder::intercept_choices(
                &self.message,
                self.options.len(),
                self.responder.as_deref(),
            ))? {
            match indices {
                Some(indices) => indices.iter().map(|&i| self.options[i].clone()).collect(),
                None => return Ok(None),
            }
        } else {
            if self.options.is_empty() {
                return Ok(None);
            }

            let mut prompt = MultiSelect::new(&self.message, self.options.clone())
                .with_page_size(self.page_size);

            if let Some(help) = &self.help_message {
                prompt = prompt.with_help_message(help);
            }

            prompt.prompt().map_err(map_inquire_error)?
        };

        if let Some(min) = self.min_selections {
            if result.len() < min {
                return Err(InputError::ValidationFailed(format!(
                    "At least {} selection(s) required",
                    min
                )));
            }
        }
        if let Some(max) = self.max_selections {
            if result.len() > max {
                return Err(InputError::ValidationFailed(format!(
                    "At most {} selection(s) allowed",
                    max
                )));
            }
        }

        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<Vec<T>>>> {
        let mut bound = self.clone();
        bound.responder = Some(sources.responder_arc()?);
        Some(Box::new(bound))
    }

    fn can_retry(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{empty_matches, sources_with};
    use super::*;
    use crate::{PromptResponse, ScriptedResponder};

    #[test]
    fn inquire_select_construction() {
        let source = InquireSelect::new("Choose:", vec!["a", "b", "c"])
            .help("Select one")
            .page_size(5);

        assert_eq!(source.name(), "prompt");
        assert!(source.can_retry());
    }

    #[test]
    fn inquire_select_empty_options_unavailable() {
        let source: InquireSelect<String> = InquireSelect::new("Choose:", vec![]);
        let _ = source.is_available(&empty_matches());
    }

    #[test]
    fn inquire_multiselect_construction() {
        let source = InquireMultiSelect::new("Select:", vec!["x", "y", "z"])
            .help("Select multiple")
            .page_size(10)
            .min_selections(1)
            .max_selections(2);

        assert_eq!(source.name(), "prompt");
        assert!(source.can_retry());
    }

    #[test]
    fn inquire_select_prompt_via_responder_returns_typed_value() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::Choice(2)]));
        let env: &'static str = InquireSelect::new("Env:", vec!["dev", "staging", "prod"])
            .prompt_from(&sources)
            .unwrap();
        assert_eq!(env, "prod");
    }

    #[test]
    fn inquire_select_prompt_cancel_via_responder() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::Cancel]));
        let err = InquireSelect::new("Env:", vec!["dev", "prod"])
            .prompt_from(&sources)
            .unwrap_err();
        assert!(matches!(err, InputError::PromptCancelled));
    }

    #[test]
    fn inquire_multiselect_prompt_via_responder_returns_typed_values() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::choices([0, 2])]));
        let picks: Vec<&'static str> = InquireMultiSelect::new("Pick:", vec!["a", "b", "c", "d"])
            .prompt_from(&sources)
            .unwrap();
        assert_eq!(picks, vec!["a", "c"]);
    }

    #[test]
    fn inquire_select_chain_resolve_from_uses_responder() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::Choice(2)]));
        let chain = crate::InputChain::<&'static str>::new()
            .try_source(InquireSelect::new("Env:", vec!["dev", "staging", "prod"]));
        assert_eq!(
            chain.resolve_from(&empty_matches(), &sources).unwrap(),
            "prod"
        );
    }

    #[test]
    fn inquire_multiselect_responder_rejects_under_minimum() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::choices([0])]));
        let source = InquireMultiSelect::new("Pick:", vec!["a", "b", "c"]).min_selections(2);
        let err = source.prompt_from(&sources).unwrap_err();
        assert!(matches!(err, InputError::ValidationFailed(msg) if msg.contains("At least 2")));
    }

    #[test]
    fn inquire_multiselect_responder_rejects_over_maximum() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::choices([0, 1, 2])]));
        let source = InquireMultiSelect::new("Pick:", vec!["a", "b", "c"]).max_selections(2);
        let err = source.prompt_from(&sources).unwrap_err();
        assert!(matches!(err, InputError::ValidationFailed(msg) if msg.contains("At most 2")));
    }
}
