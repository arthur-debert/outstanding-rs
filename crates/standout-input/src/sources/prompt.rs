use std::io::{self, BufRead, IsTerminal, Write};
use std::ops::ControlFlow;
use std::sync::Arc;

use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::responder::PromptResponder;
use crate::InputError;
use crate::InputSources;

pub trait TerminalIO: Send + Sync {
    fn is_terminal(&self) -> bool;

    fn write_prompt(&self, prompt: &str) -> io::Result<()>;

    fn read_line(&self) -> io::Result<String>;
}

impl<T: TerminalIO + ?Sized> TerminalIO for Arc<T> {
    fn is_terminal(&self) -> bool {
        (**self).is_terminal()
    }

    fn write_prompt(&self, prompt: &str) -> io::Result<()> {
        (**self).write_prompt(prompt)
    }

    fn read_line(&self) -> io::Result<String> {
        (**self).read_line()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RealTerminal;

impl TerminalIO for RealTerminal {
    fn is_terminal(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn write_prompt(&self, prompt: &str) -> io::Result<()> {
        print!("{}", prompt);
        io::stdout().flush()
    }

    fn read_line(&self) -> io::Result<String> {
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        Ok(line)
    }
}

#[derive(Clone)]
pub struct TextPromptSource<T: TerminalIO = RealTerminal> {
    terminal: Arc<T>,
    prompt: String,
    trim: bool,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl TextPromptSource<RealTerminal> {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            terminal: Arc::new(RealTerminal),
            prompt: prompt.into(),
            trim: true,
            responder: None,
        }
    }
}

impl<T: TerminalIO> TextPromptSource<T> {
    pub fn with_terminal(prompt: impl Into<String>, terminal: T) -> Self {
        Self {
            terminal: Arc::new(terminal),
            prompt: prompt.into(),
            trim: true,
            responder: None,
        }
    }

    pub fn trim(mut self, trim: bool) -> Self {
        self.trim = trim;
        self
    }
}

impl<T: TerminalIO + 'static> TextPromptSource<T> {
    pub fn prompt(&self) -> Result<String, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    pub fn prompt_from(&self, sources: &InputSources) -> Result<String, InputError> {
        crate::collector::prompt_value_from(self, sources)
    }

    pub fn prompt_entry(&self) -> Result<Option<String>, InputError> {
        self.prompt_entry_from(&InputSources::from_process())
    }

    pub fn prompt_entry_from(&self, sources: &InputSources) -> Result<Option<String>, InputError> {
        match crate::responder::intercept_text(
            crate::PromptKind::Text,
            &self.prompt,
            sources.responder(),
        ) {
            Ok(Some(value)) => return Ok(Some(value)),
            Ok(None) => {}
            Err(InputError::NoInput) => return Ok(None),
            Err(error) => return Err(error),
        }
        let matches = crate::collector::empty_matches();
        if !self.is_available(matches) {
            return Ok(None);
        }
        Ok(Some(self.collect(matches)?.unwrap_or_default()))
    }
}

impl<T: TerminalIO + 'static> InputCollector<String> for TextPromptSource<T> {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.responder.is_some() || self.terminal.is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        if let ControlFlow::Break(value) =
            crate::responder::collect_intercept(crate::responder::intercept_text(
                crate::PromptKind::Text,
                &self.prompt,
                self.responder.as_deref(),
            ))?
        {
            return Ok(value);
        }

        if !self.terminal.is_terminal() {
            return Ok(None);
        }

        self.terminal
            .write_prompt(&self.prompt)
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        let line = self
            .terminal
            .read_line()
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        if line.is_empty() {
            return Err(InputError::PromptCancelled);
        }

        let result = if self.trim {
            line.trim().to_string()
        } else {
            line.trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string()
        };

        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<String>>> {
        Some(Box::new(Self {
            terminal: Arc::clone(&self.terminal),
            prompt: self.prompt.clone(),
            trim: self.trim,
            responder: Some(sources.responder_arc()?),
        }))
    }

    fn can_retry(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct ConfirmPromptSource<T: TerminalIO = RealTerminal> {
    terminal: Arc<T>,
    prompt: String,
    default: Option<bool>,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl ConfirmPromptSource<RealTerminal> {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            terminal: Arc::new(RealTerminal),
            prompt: prompt.into(),
            default: None,
            responder: None,
        }
    }
}

impl<T: TerminalIO> ConfirmPromptSource<T> {
    pub fn with_terminal(prompt: impl Into<String>, terminal: T) -> Self {
        Self {
            terminal: Arc::new(terminal),
            prompt: prompt.into(),
            default: None,
            responder: None,
        }
    }

    pub fn default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }
}

impl<T: TerminalIO + 'static> ConfirmPromptSource<T> {
    pub fn prompt(&self) -> Result<bool, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    pub fn prompt_from(&self, sources: &InputSources) -> Result<bool, InputError> {
        crate::collector::prompt_value_from(self, sources)
    }
}

impl<T: TerminalIO + 'static> InputCollector<bool> for ConfirmPromptSource<T> {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.responder.is_some() || self.terminal.is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<bool>, InputError> {
        if let ControlFlow::Break(value) =
            crate::responder::collect_intercept(crate::responder::intercept_bool(
                crate::PromptKind::Confirm,
                &self.prompt,
                self.responder.as_deref(),
            ))?
        {
            return Ok(value);
        }

        if !self.terminal.is_terminal() {
            return Ok(None);
        }

        let suffix = match self.default {
            None => "[y/n]",
            Some(true) => "[Y/n]",
            Some(false) => "[y/N]",
        };

        let full_prompt = format!("{} {} ", self.prompt, suffix);

        self.terminal
            .write_prompt(&full_prompt)
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        let line = self
            .terminal
            .read_line()
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        if line.is_empty() {
            return Err(InputError::PromptCancelled);
        }

        let input = line.trim().to_lowercase();

        if input.is_empty() {
            return Ok(self.default);
        }

        match input.as_str() {
            "y" | "yes" => Ok(Some(true)),
            "n" | "no" => Ok(Some(false)),
            _ => Err(InputError::ValidationFailed(
                "Please enter 'y' or 'n'".to_string(),
            )),
        }
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<bool>>> {
        Some(Box::new(Self {
            terminal: Arc::clone(&self.terminal),
            prompt: self.prompt.clone(),
            default: self.default,
            responder: Some(sources.responder_arc()?),
        }))
    }

    fn can_retry(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub struct MockTerminal {
    is_terminal: bool,
    responses: Vec<String>,
    response_index: std::sync::atomic::AtomicUsize,
}

impl Clone for MockTerminal {
    fn clone(&self) -> Self {
        Self {
            is_terminal: self.is_terminal,
            responses: self.responses.clone(),
            response_index: std::sync::atomic::AtomicUsize::new(
                self.response_index
                    .load(std::sync::atomic::Ordering::SeqCst),
            ),
        }
    }
}

impl MockTerminal {
    pub fn non_terminal() -> Self {
        Self {
            is_terminal: false,
            responses: vec![],
            response_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn with_response(response: impl Into<String>) -> Self {
        Self {
            is_terminal: true,
            responses: vec![response.into()],
            response_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn with_responses(responses: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            is_terminal: true,
            responses: responses.into_iter().map(Into::into).collect(),
            response_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn eof() -> Self {
        Self {
            is_terminal: true,
            responses: vec![],
            response_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl TerminalIO for MockTerminal {
    fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    fn write_prompt(&self, _prompt: &str) -> io::Result<()> {
        Ok(())
    }

    fn read_line(&self) -> io::Result<String> {
        let idx = self
            .response_index
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if idx < self.responses.len() {
            Ok(format!("{}\n", self.responses[idx]))
        } else {
            Ok(String::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Command;

    fn empty_matches() -> ArgMatches {
        Command::new("test").try_get_matches_from(["test"]).unwrap()
    }

    #[test]
    fn text_prompt_unavailable_when_not_terminal() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::non_terminal());
        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn text_prompt_available_when_terminal() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("test"));
        assert!(source.is_available(&empty_matches()));
    }

    #[test]
    fn text_prompt_collects_input() {
        let source =
            TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("Alice"));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("Alice".to_string()));
    }

    #[test]
    fn text_prompt_trims_whitespace() {
        let source =
            TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("  Bob  "));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("Bob".to_string()));
    }

    #[test]
    fn text_prompt_no_trim() {
        let source =
            TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("  Bob  "))
                .trim(false);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("  Bob  ".to_string()));
    }

    #[test]
    fn text_prompt_returns_none_for_empty() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response(""));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn text_prompt_returns_none_for_whitespace_only() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("   "));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn text_prompt_eof_cancels() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::eof());
        let result = source.collect(&empty_matches());
        assert!(matches!(result, Err(InputError::PromptCancelled)));
    }

    #[test]
    fn text_prompt_can_retry() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("test"));
        assert!(source.can_retry());
    }

    #[test]
    fn confirm_prompt_unavailable_when_not_terminal() {
        let source = ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::non_terminal());
        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn confirm_prompt_available_when_terminal() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response("y"));
        assert!(source.is_available(&empty_matches()));
    }

    #[test]
    fn confirm_prompt_yes() {
        for response in ["y", "Y", "yes", "YES", "Yes"] {
            let source = ConfirmPromptSource::with_terminal(
                "Proceed?",
                MockTerminal::with_response(response),
            );
            let result = source.collect(&empty_matches()).unwrap();
            assert_eq!(result, Some(true), "response '{}' should be true", response);
        }
    }

    #[test]
    fn confirm_prompt_no() {
        for response in ["n", "N", "no", "NO", "No"] {
            let source = ConfirmPromptSource::with_terminal(
                "Proceed?",
                MockTerminal::with_response(response),
            );
            let result = source.collect(&empty_matches()).unwrap();
            assert_eq!(
                result,
                Some(false),
                "response '{}' should be false",
                response
            );
        }
    }

    #[test]
    fn confirm_prompt_invalid_input() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response("maybe"));
        let result = source.collect(&empty_matches());
        assert!(matches!(result, Err(InputError::ValidationFailed(_))));
    }

    #[test]
    fn confirm_prompt_empty_with_default_true() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response(""))
                .default(true);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn confirm_prompt_empty_with_default_false() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response(""))
                .default(false);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn confirm_prompt_empty_without_default() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response(""));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn confirm_prompt_eof_cancels() {
        let source = ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::eof());
        let result = source.collect(&empty_matches());
        assert!(matches!(result, Err(InputError::PromptCancelled)));
    }

    #[test]
    fn confirm_prompt_can_retry() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response("y"));
        assert!(source.can_retry());
    }

    use crate::{InputSources, PromptResponse, ScriptedResponder};
    use std::sync::Arc;

    fn sources_with(responder: ScriptedResponder) -> InputSources {
        InputSources::from_process().with_responder(Arc::new(responder))
    }

    #[test]
    fn text_prompt_shortcut_returns_value() {
        let source =
            TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("Carol"));
        let value = source.prompt().unwrap();
        assert_eq!(value, "Carol");
    }

    #[test]
    fn text_prompt_shortcut_maps_empty_to_no_input() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("   "));
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::NoInput));
    }

    #[test]
    fn text_prompt_shortcut_propagates_cancel() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::eof());
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::PromptCancelled));
    }

    #[test]
    fn text_prompt_shortcut_skips_when_not_terminal() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::non_terminal());
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::NoInput));
    }

    #[test]
    fn confirm_prompt_shortcut_returns_value() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response("y"));
        let value = source.prompt().unwrap();
        assert!(value);
    }

    #[test]
    fn confirm_prompt_shortcut_propagates_cancel() {
        let source = ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::eof());
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::PromptCancelled));
    }

    #[test]
    fn confirm_prompt_shortcut_uses_default_on_empty() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response(""))
                .default(true);
        let value = source.prompt().unwrap();
        assert!(value);
    }

    #[test]
    fn text_prompt_routes_through_responder_even_without_tty() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::text("Ada")]));
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::non_terminal());
        let value = source.prompt_from(&sources).unwrap();
        assert_eq!(value, "Ada");
    }

    #[test]
    fn confirm_prompt_routes_through_responder() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::Bool(false)]));
        let source = ConfirmPromptSource::with_terminal("OK?", MockTerminal::non_terminal());
        let value = source.prompt_from(&sources).unwrap();
        assert!(!value);
    }

    #[test]
    fn bind_sources_makes_text_prompt_available_without_tty() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::non_terminal());
        let matches = empty_matches();
        assert!(!source.is_available(&matches));
        let sources = sources_with(ScriptedResponder::new([PromptResponse::text("Ada")]));
        let bound = source.bind_sources(&sources).expect("responder binds");
        assert!(bound.is_available(&matches));
        assert_eq!(bound.collect(&matches).unwrap(), Some("Ada".to_string()));
    }

    #[test]
    fn bind_sources_skip_does_not_read_terminal() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("tty"));
        let sources = sources_with(ScriptedResponder::new([PromptResponse::Skip]));
        let bound = source.bind_sources(&sources).expect("responder binds");
        assert_eq!(bound.collect(&empty_matches()).unwrap(), None);
    }
}
