mod selection;

pub use selection::{InquireMultiSelect, InquireSelect};

use std::io::IsTerminal;
use std::ops::ControlFlow;
use std::sync::Arc;

use clap::ArgMatches;
use inquire::{
    ui::RenderConfig, Confirm, Editor, InquireError, Password, PasswordDisplayMode, Text,
};

use crate::collector::InputCollector;
use crate::responder::PromptResponder;
use crate::InputError;
use crate::InputSources;

fn map_inquire_error(e: InquireError) -> InputError {
    match e {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => {
            InputError::PromptCancelled
        }
        other => InputError::PromptFailed(other.to_string()),
    }
}

#[derive(Clone)]
pub struct InquireText {
    message: String,
    default: Option<String>,
    placeholder: Option<String>,
    help_message: Option<String>,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl InquireText {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: None,
            placeholder: None,
            help_message: None,
            responder: None,
        }
    }

    pub fn default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help_message = Some(help.into());
        self
    }

    pub fn prompt(&self) -> Result<String, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    pub fn prompt_from(&self, sources: &InputSources) -> Result<String, InputError> {
        crate::collector::prompt_value_from(self, sources)
    }
}

impl InputCollector<String> for InquireText {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.responder.is_some() || std::io::stdin().is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        if let ControlFlow::Break(value) =
            crate::responder::collect_intercept(crate::responder::intercept_text(
                crate::PromptKind::Text,
                &self.message,
                self.responder.as_deref(),
            ))?
        {
            return Ok(value);
        }

        let mut prompt = Text::new(&self.message);

        if let Some(default) = &self.default {
            prompt = prompt.with_default(default);
        }
        if let Some(placeholder) = &self.placeholder {
            prompt = prompt.with_placeholder(placeholder);
        }
        if let Some(help) = &self.help_message {
            prompt = prompt.with_help_message(help);
        }

        let result = prompt.prompt().map_err(map_inquire_error)?;

        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<String>>> {
        let mut bound = self.clone();
        bound.responder = Some(sources.responder_arc()?);
        Some(Box::new(bound))
    }

    fn can_retry(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct InquireConfirm {
    message: String,
    default: Option<bool>,
    help_message: Option<String>,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl InquireConfirm {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: None,
            help_message: None,
            responder: None,
        }
    }

    pub fn default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help_message = Some(help.into());
        self
    }

    pub fn prompt(&self) -> Result<bool, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    pub fn prompt_from(&self, sources: &InputSources) -> Result<bool, InputError> {
        crate::collector::prompt_value_from(self, sources)
    }
}

impl InputCollector<bool> for InquireConfirm {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.responder.is_some() || std::io::stdin().is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<bool>, InputError> {
        if let ControlFlow::Break(value) =
            crate::responder::collect_intercept(crate::responder::intercept_bool(
                crate::PromptKind::Confirm,
                &self.message,
                self.responder.as_deref(),
            ))?
        {
            return Ok(value);
        }

        let mut prompt = Confirm::new(&self.message);

        if let Some(default) = self.default {
            prompt = prompt.with_default(default);
        }
        if let Some(help) = &self.help_message {
            prompt = prompt.with_help_message(help);
        }

        let result = prompt.prompt().map_err(map_inquire_error)?;
        Ok(Some(result))
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<bool>>> {
        let mut bound = self.clone();
        bound.responder = Some(sources.responder_arc()?);
        Some(Box::new(bound))
    }

    fn can_retry(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct InquirePassword {
    message: String,
    help_message: Option<String>,
    display_mode: PasswordDisplayMode,
    confirmation: Option<String>,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl InquirePassword {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            help_message: None,
            display_mode: PasswordDisplayMode::Masked,
            confirmation: None,
            responder: None,
        }
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help_message = Some(help.into());
        self
    }

    pub fn hidden(mut self) -> Self {
        self.display_mode = PasswordDisplayMode::Hidden;
        self
    }

    pub fn masked(mut self) -> Self {
        self.display_mode = PasswordDisplayMode::Masked;
        self
    }

    pub fn full(mut self) -> Self {
        self.display_mode = PasswordDisplayMode::Full;
        self
    }

    pub fn with_confirmation(mut self, message: impl Into<String>) -> Self {
        self.confirmation = Some(message.into());
        self
    }

    pub fn prompt(&self) -> Result<String, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    pub fn prompt_from(&self, sources: &InputSources) -> Result<String, InputError> {
        crate::collector::prompt_value_from(self, sources)
    }
}

impl InputCollector<String> for InquirePassword {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.responder.is_some() || std::io::stdin().is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        if let ControlFlow::Break(value) =
            crate::responder::collect_intercept(crate::responder::intercept_text(
                crate::PromptKind::Password,
                &self.message,
                self.responder.as_deref(),
            ))?
        {
            return Ok(value);
        }

        let mut prompt = Password::new(&self.message).with_display_mode(self.display_mode);

        if let Some(help) = &self.help_message {
            prompt = prompt.with_help_message(help);
        }

        if let Some(confirmation) = &self.confirmation {
            prompt = prompt.with_display_toggle_enabled();
            prompt = prompt.with_custom_confirmation_message(confirmation);
        }

        let result = prompt.prompt().map_err(map_inquire_error)?;

        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<String>>> {
        let mut bound = self.clone();
        bound.responder = Some(sources.responder_arc()?);
        Some(Box::new(bound))
    }

    fn can_retry(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct InquireEditor {
    message: String,
    help_message: Option<String>,
    file_extension: String,
    predefined_text: Option<String>,
    render_config: Option<RenderConfig<'static>>,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl InquireEditor {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            help_message: None,
            file_extension: ".txt".to_string(),
            predefined_text: None,
            render_config: None,
            responder: None,
        }
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help_message = Some(help.into());
        self
    }

    pub fn extension(mut self, ext: impl Into<String>) -> Self {
        self.file_extension = ext.into();
        self
    }

    pub fn predefined_text(mut self, text: impl Into<String>) -> Self {
        self.predefined_text = Some(text.into());
        self
    }

    pub fn render_config(mut self, config: RenderConfig<'static>) -> Self {
        self.render_config = Some(config);
        self
    }

    pub fn prompt(&self) -> Result<String, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    pub fn prompt_from(&self, sources: &InputSources) -> Result<String, InputError> {
        crate::collector::prompt_value_from(self, sources)
    }
}

impl InputCollector<String> for InquireEditor {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.responder.is_some() || std::io::stdin().is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        if let ControlFlow::Break(value) =
            crate::responder::collect_intercept(crate::responder::intercept_text(
                crate::PromptKind::Editor,
                &self.message,
                self.responder.as_deref(),
            ))?
        {
            return Ok(value);
        }

        let mut prompt = Editor::new(&self.message).with_file_extension(&self.file_extension);

        if let Some(help) = &self.help_message {
            prompt = prompt.with_help_message(help);
        }
        if let Some(text) = &self.predefined_text {
            prompt = prompt.with_predefined_text(text);
        }
        if let Some(config) = &self.render_config {
            prompt = prompt.with_render_config(*config);
        }

        let result = prompt.prompt().map_err(map_inquire_error)?;

        let trimmed = result.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<String>>> {
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
    use super::*;

    pub(super) fn empty_matches() -> ArgMatches {
        clap::Command::new("test")
            .try_get_matches_from(["test"])
            .unwrap()
    }

    #[test]
    fn inquire_text_construction() {
        let source = InquireText::new("Name?")
            .default("Alice")
            .placeholder("Your name...")
            .help("Enter your full name");

        assert_eq!(source.name(), "prompt");
        assert!(source.can_retry());
    }

    #[test]
    fn inquire_confirm_construction() {
        let source = InquireConfirm::new("Proceed?")
            .default(true)
            .help("Are you sure?");

        assert_eq!(source.name(), "prompt");
        assert!(source.can_retry());
    }

    #[test]
    fn inquire_password_construction() {
        let source = InquirePassword::new("Password:")
            .help("Enter securely")
            .masked()
            .with_confirmation("Confirm:");

        assert_eq!(source.name(), "prompt");
        assert!(source.can_retry());
    }

    #[test]
    fn inquire_password_display_modes() {
        let _ = InquirePassword::new("P:").hidden();
        let _ = InquirePassword::new("P:").masked();
        let _ = InquirePassword::new("P:").full();
    }

    #[test]
    fn inquire_editor_construction() {
        let source = InquireEditor::new("Message:")
            .help("Enter in editor")
            .extension(".md")
            .predefined_text("# Title\n");

        assert_eq!(source.name(), "editor");
        assert!(source.can_retry());
    }

    use crate::{InputSources, PromptResponse, ScriptedResponder};
    use std::sync::Arc;

    pub(super) fn sources_with(responder: ScriptedResponder) -> InputSources {
        InputSources::from_process().with_responder(Arc::new(responder))
    }

    #[test]
    fn inquire_text_prompt_via_responder() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::text("Bob")]));
        let value = InquireText::new("Name?").prompt_from(&sources).unwrap();
        assert_eq!(value, "Bob");
    }

    #[test]
    fn inquire_text_prompt_cancel_via_responder() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::Cancel]));
        let err = InquireText::new("Name?").prompt_from(&sources).unwrap_err();
        assert!(matches!(err, InputError::PromptCancelled));
    }

    #[test]
    fn inquire_text_prompt_skip_via_responder() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::Skip]));
        let err = InquireText::new("Name?").prompt_from(&sources).unwrap_err();
        assert!(matches!(err, InputError::NoInput));
    }

    #[test]
    fn inquire_confirm_prompt_via_responder() {
        let sources = sources_with(ScriptedResponder::new([
            PromptResponse::Bool(true),
            PromptResponse::Bool(false),
        ]));
        assert!(InquireConfirm::new("Yes?").prompt_from(&sources).unwrap());
        assert!(!InquireConfirm::new("Yes?").prompt_from(&sources).unwrap());
    }

    #[test]
    fn inquire_password_prompt_via_responder() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::text("hunter2")]));
        let value = InquirePassword::new("Pwd:").prompt_from(&sources).unwrap();
        assert_eq!(value, "hunter2");
    }

    #[test]
    fn inquire_editor_prompt_via_responder() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::text(
            "edited content",
        )]));
        let value = InquireEditor::new("Notes:").prompt_from(&sources).unwrap();
        assert_eq!(value, "edited content");
    }

    #[test]
    fn responder_advances_through_multi_step_wizard() {
        let sources = sources_with(ScriptedResponder::new([
            PromptResponse::text("foo"),
            PromptResponse::Bool(true),
            PromptResponse::Choice(1),
        ]));
        assert_eq!(
            InquireText::new("Name:").prompt_from(&sources).unwrap(),
            "foo"
        );
        assert!(InquireConfirm::new("OK?").prompt_from(&sources).unwrap());
        let env: &'static str = InquireSelect::new("Env:", vec!["dev", "prod"])
            .prompt_from(&sources)
            .unwrap();
        assert_eq!(env, "prod");
    }

    #[test]
    fn inquire_text_chain_resolve_from_uses_responder() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::text("Bob")]));
        let chain = crate::InputChain::<String>::new().try_source(InquireText::new("Name?"));
        assert_eq!(
            chain.resolve_from(&empty_matches(), &sources).unwrap(),
            "Bob"
        );
    }
}
