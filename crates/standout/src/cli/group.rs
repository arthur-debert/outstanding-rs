use crate::context::ContextRegistry;
use clap::ArgMatches;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::builder::{SharedTemplateEngine, TemplateAbsence, TemplateRef};
use super::dispatch::DispatchFn;
use crate::cli::handler::{CommandContext, FnHandler, Handler, HandlerResult};
use crate::cli::hooks::Hooks;
use crate::cli::questionnaire::QuestionnaireCommand;
use crate::StructuredOutputProjection;
use standout_dispatch::verify::ExpectedArg;

mod config;
mod recipes;

pub use config::CommandConfig;
use recipes::{dispatch_from_handler, dispatch_passthrough};
pub(crate) use recipes::{CommandRecipe, ErasedConfigRecipe, PassthroughRecipe, StructRecipe};

pub(crate) enum GroupEntry {
    Command {
        handler: Box<dyn ErasedCommandConfig>,
    },
    Group {
        builder: GroupBuilder,
    },
}

pub(crate) trait ErasedCommandConfig {
    fn template_name(&self) -> Option<&str>;
    fn template_absence(&self) -> Option<TemplateAbsence>;
    #[allow(dead_code)]
    fn hooks(&self) -> Option<&Hooks>;
    fn take_hooks(&mut self) -> Option<Hooks>;
    fn take_input_chains(&mut self) -> Option<Hooks>;
    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand>;
    fn take_questionnaire_resolution(&mut self) -> Option<Hooks>;
    fn without_config(&self) -> bool;
    fn emits_events(&self) -> bool {
        false
    }
    fn pageable(&self) -> bool {
        false
    }
    #[allow(clippy::too_many_arguments)]
    fn register(
        self: Box<Self>,
        path: &str,
        template: TemplateRef,
        context_registry: ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
        strict_style_tags: bool,
    ) -> DispatchFn;

    fn expected_args(&self) -> Vec<ExpectedArg>;

    fn structured_output_projection(&self) -> Option<&StructuredOutputProjection> {
        None
    }
}

#[derive(Default)]
pub struct GroupBuilder {
    pub(crate) entries: HashMap<String, GroupEntry>,
    pub(crate) default_command: Option<String>,
}

impl GroupBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    pub fn get_default_command(&self) -> Option<&str> {
        self.default_command.as_deref()
    }

    pub fn command<F, T>(self, name: &str, handler: F) -> Self
    where
        F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
        T: Serialize + 'static,
    {
        self.command_with(name, handler, |cfg| cfg)
    }

    pub fn command_with<F, T, C>(mut self, name: &str, handler: F, configure: C) -> Self
    where
        F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
        T: Serialize + 'static,
        C: FnOnce(CommandConfig<FnHandler<F, T>>) -> CommandConfig<FnHandler<F, T>>,
    {
        let config = CommandConfig::new(FnHandler::new(handler));
        let config = configure(config);
        self.entries.insert(
            name.to_string(),
            GroupEntry::Command {
                handler: Box::new(ClosureCommandConfig {
                    handler: Rc::new(RefCell::new(config.handler)),
                    template_name: config.template_name,
                    template_absence: config.template_absence,
                    hooks: config.hooks,
                    input_chains: config.input_chains,
                    questionnaire: config.questionnaire,
                    questionnaire_resolution: config.questionnaire_resolution,
                    structured_output_projection: config.structured_output_projection,
                    pageable: config.pageable,
                    without_config: config.without_config,
                }),
            },
        );
        self
    }

    pub fn passthrough<F>(mut self, name: &str, handler: F) -> Self
    where
        F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
    {
        self.entries.insert(
            name.to_string(),
            GroupEntry::Command {
                handler: Box::new(PassthroughCommandConfig {
                    handler: Rc::new(RefCell::new(handler)),
                }),
            },
        );
        self
    }

    pub fn group<F>(mut self, name: &str, configure: F) -> Self
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());
        self.entries
            .insert(name.to_string(), GroupEntry::Group { builder });
        self
    }

    pub fn default_command(mut self, name: &str) -> Self {
        if let Some(existing) = &self.default_command {
            panic!(
                "Only one default command can be defined. '{}' is already set as default.",
                existing
            );
        }
        self.default_command = Some(name.to_string());
        self
    }
}

struct ClosureCommandConfig<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
    T: Serialize + 'static,
{
    handler: Rc<RefCell<FnHandler<F, T>>>,
    template_name: Option<String>,
    template_absence: Option<TemplateAbsence>,
    hooks: Option<Hooks>,
    input_chains: Option<Hooks>,
    questionnaire: Option<QuestionnaireCommand>,
    questionnaire_resolution: Option<Hooks>,
    structured_output_projection: Option<StructuredOutputProjection>,
    pageable: bool,
    without_config: bool,
}

impl<F, T> ErasedCommandConfig for ClosureCommandConfig<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
    T: Serialize + 'static,
{
    fn template_name(&self) -> Option<&str> {
        self.template_name.as_deref()
    }

    fn template_absence(&self) -> Option<TemplateAbsence> {
        self.template_absence
    }

    fn hooks(&self) -> Option<&Hooks> {
        self.hooks.as_ref()
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.take()
    }

    fn take_input_chains(&mut self) -> Option<Hooks> {
        self.input_chains.take()
    }

    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand> {
        self.questionnaire.take()
    }

    fn take_questionnaire_resolution(&mut self) -> Option<Hooks> {
        self.questionnaire_resolution.take()
    }

    fn without_config(&self) -> bool {
        self.without_config
    }

    fn pageable(&self) -> bool {
        self.pageable
    }

    fn register(
        self: Box<Self>,
        _path: &str,
        template: TemplateRef,
        context_registry: ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
        strict_style_tags: bool,
    ) -> DispatchFn {
        dispatch_from_handler(
            self.handler,
            template,
            context_registry,
            template_engine,
            template_registry,
            self.structured_output_projection,
            strict_style_tags,
        )
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        self.handler.borrow().expected_args()
    }

    fn structured_output_projection(&self) -> Option<&StructuredOutputProjection> {
        self.structured_output_projection.as_ref()
    }
}

struct PassthroughCommandConfig<F>
where
    F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
{
    handler: Rc<RefCell<F>>,
}

impl<F> ErasedCommandConfig for PassthroughCommandConfig<F>
where
    F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
{
    fn template_name(&self) -> Option<&str> {
        None
    }

    fn template_absence(&self) -> Option<TemplateAbsence> {
        Some(TemplateAbsence::Silent)
    }

    fn hooks(&self) -> Option<&Hooks> {
        None
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        None
    }

    fn take_input_chains(&mut self) -> Option<Hooks> {
        None
    }

    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand> {
        None
    }

    fn take_questionnaire_resolution(&mut self) -> Option<Hooks> {
        None
    }

    fn without_config(&self) -> bool {
        false
    }

    fn register(
        self: Box<Self>,
        _path: &str,
        _template: TemplateRef,
        _context_registry: ContextRegistry,
        _template_engine: SharedTemplateEngine,
        _template_registry: Option<Rc<crate::TemplateRegistry>>,
        _strict_style_tags: bool,
    ) -> DispatchFn {
        dispatch_passthrough(self.handler)
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::cli::handler::Output as HandlerOutput;
    use serde_json::json;

    #[test]
    fn test_group_builder_creation() {
        let group = GroupBuilder::new();
        assert!(group.entries.is_empty());
    }

    #[test]
    fn test_group_builder_command() {
        let group = GroupBuilder::new().command("test", |_m, _ctx| {
            Ok(HandlerOutput::Render(json!({"ok": true})))
        });

        assert!(group.entries.contains_key("test"));
    }

    #[test]
    fn test_group_builder_nested() {
        let group = GroupBuilder::new()
            .command("top", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .group("nested", |g| {
                g.command("inner", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            });

        assert!(group.entries.contains_key("top"));
        assert!(group.entries.contains_key("nested"));
    }

    #[test]
    fn test_group_builder_default_command() {
        let group = GroupBuilder::new()
            .command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .command("add", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .default_command("list");

        assert_eq!(group.default_command, Some("list".to_string()));
    }

    #[test]
    #[should_panic(expected = "Only one default command can be defined")]
    fn test_group_builder_duplicate_default_command_panics() {
        let _ = GroupBuilder::new()
            .command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .command("add", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .default_command("list")
            .default_command("add");
    }
}
