use crate::context::ContextRegistry;

use clap::ArgMatches;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::builder::{SharedTemplateEngine, TemplateAbsence, TemplateRef};
use super::dispatch::{render_handler_output, DispatchFn, DispatchOutput};
use super::events::{event_template, EventContext, EventDestination};
use crate::cli::handler::{
    CommandContext, FnHandler, Handler, HandlerResult, Results, RunRecorder, StreamSink,
};
use crate::cli::hooks::{Hooks, RenderedOutput, TextOutput};
use crate::cli::questionnaire::{
    questionnaire_pre_dispatch, questionnaire_pre_dispatch_with,
    questionnaire_pre_dispatch_with_review, Confirmation, QuestionnaireCommand,
    QuestionnaireSettings,
};
use crate::StructuredOutputProjection;
use standout_dispatch::verify::ExpectedArg;
use standout_input::questionnaire::{AnswerSheetFormat, FormError, QuestionnaireInput};
use standout_pipe::PipeTarget;

pub(crate) trait CommandRecipe {
    #[allow(dead_code)]
    fn template_name(&self) -> Option<&str> {
        None
    }

    #[allow(dead_code)]
    fn template_absence(&self) -> Option<TemplateAbsence> {
        None
    }

    #[allow(dead_code)]
    fn hooks(&self) -> Option<&Hooks>;

    #[allow(dead_code)]
    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand>;

    #[allow(dead_code)]
    fn take_hooks(&mut self) -> Option<Hooks>;

    /// True when the command's handler declares that it produces its result
    /// while it runs, so build-time template validation can require the
    /// `<name>.event` template it will render each event from.
    fn emits_events(&self) -> bool {
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn create_dispatch(
        &self,
        template: &TemplateRef,
        context_registry: &ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
        strict_style_tags: bool,
    ) -> DispatchFn;

    fn expected_args(&self) -> Vec<ExpectedArg>;

    fn structured_output_projection(&self) -> Option<&StructuredOutputProjection> {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_from_handler<H>(
    handler: Rc<RefCell<H>>,
    template: TemplateRef,
    context_registry: ContextRegistry,
    template_engine: SharedTemplateEngine,
    template_registry: Option<Rc<crate::TemplateRegistry>>,
    structured_output_projection: Option<StructuredOutputProjection>,
    strict_style_tags: bool,
) -> DispatchFn
where
    H: Handler + 'static,
    H::Output: Serialize,
{
    Rc::new(RefCell::new(
        move |matches: &ArgMatches,
              ctx: &CommandContext,
              recorder: &RunRecorder,
              sink: &StreamSink,
              hooks: Option<&Hooks>,
              output_mode: crate::Representation,
              color_policy: crate::ColorPolicy,
              theme: &crate::Theme,
              target: crate::TargetProperties| {
            let command_path = ctx.command_path.join(".");
            super::dispatch::reject_events_under_a_document_encoding(
                H::EMITS_EVENTS,
                &command_path,
                output_mode,
            )?;
            let destination = Rc::new(EventDestination::new(
                sink.clone(),
                EventContext {
                    command_path,
                    template: event_template(&template),
                    theme: theme.clone(),
                    context_registry: context_registry.clone(),
                    template_engine: template_engine.clone(),
                    template_registry: template_registry.clone(),
                    representation: output_mode,
                    color_policy,
                    target,
                    warnings: ctx
                        .extensions
                        .get::<standout_render::warnings::WarningBuffer>()
                        .cloned(),
                    strict_style_tags,
                },
            ));
            let mut results =
                Results::<H::Event>::for_run(Some(recorder.clone()), destination.clone());
            let result = handler.borrow_mut().handle(matches, ctx, &mut results);
            // An event the framework could not carry outranks whatever the
            // handler went on to return, including a swallowed `emit` error.
            if let Some(failure) = destination.take_failure() {
                return Err(failure);
            }
            let output = render_handler_output(
                result,
                matches,
                ctx,
                recorder,
                hooks,
                &template,
                theme,
                &context_registry,
                &template_engine,
                template_registry.as_ref(),
                output_mode,
                color_policy,
                structured_output_projection.as_ref(),
                target,
            )?;
            super::dispatch::reject_payload_from_an_emitting_command(
                H::EMITS_EVENTS,
                matches!(output, DispatchOutput::Binary(_, _)),
                matches!(output, DispatchOutput::Artifact { .. }),
            )?;
            Ok(output)
        },
    ))
}

fn dispatch_passthrough<F>(handler: Rc<RefCell<F>>) -> DispatchFn
where
    F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
{
    Rc::new(RefCell::new(
        move |matches: &ArgMatches,
              ctx: &CommandContext,
              _recorder: &RunRecorder,
              _sink: &StreamSink,
              _hooks: Option<&Hooks>,
              _output_mode: crate::Representation,
              _color_policy: crate::ColorPolicy,
              _theme: &crate::Theme,
              _target: crate::TargetProperties| {
            match (handler.borrow_mut())(matches, ctx) {
                Ok(()) => Ok(super::dispatch::DispatchOutput::Silent {
                    status: super::handler::ExitStatus::SUCCESS,
                }),
                Err(e) => Err(super::dispatch::handler_run_error(e)),
            }
        },
    ))
}

pub(crate) struct StructRecipe<H, T>
where
    H: Handler<Output = T> + 'static,
    T: Serialize + 'static,
{
    handler: Rc<RefCell<H>>,
    hooks: Option<Hooks>,
    questionnaire: Option<QuestionnaireCommand>,
    structured_output_projection: Option<StructuredOutputProjection>,
    _phantom: std::marker::PhantomData<T>,
}

impl<H, T> StructRecipe<H, T>
where
    H: Handler<Output = T> + 'static,
    T: Serialize + 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler: Rc::new(RefCell::new(handler)),
            hooks: None,
            questionnaire: None,
            structured_output_projection: None,
            _phantom: std::marker::PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn with_hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = Some(hooks);
        self
    }

    #[allow(dead_code)]
    pub fn with_structured_output_projection(
        mut self,
        projection: StructuredOutputProjection,
    ) -> Self {
        self.structured_output_projection = Some(projection);
        self
    }
}

impl<H, T> CommandRecipe for StructRecipe<H, T>
where
    H: Handler<Output = T> + 'static,
    T: Serialize + 'static,
{
    fn hooks(&self) -> Option<&Hooks> {
        self.hooks.as_ref()
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.take()
    }

    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand> {
        self.questionnaire.take()
    }

    fn emits_events(&self) -> bool {
        H::EMITS_EVENTS
    }

    fn create_dispatch(
        &self,
        template: &TemplateRef,
        context_registry: &ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
        strict_style_tags: bool,
    ) -> DispatchFn {
        dispatch_from_handler(
            self.handler.clone(),
            template.clone(),
            context_registry.clone(),
            template_engine,
            template_registry,
            self.structured_output_projection.clone(),
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

pub(crate) struct ErasedConfigRecipe {
    config: RefCell<Option<Box<dyn ErasedCommandConfig>>>,
    template_name: Option<String>,
    template_absence: Option<TemplateAbsence>,
    emits_events: bool,
    #[allow(dead_code)]
    hooks: RefCell<Option<Hooks>>,
    structured_output_projection: Option<StructuredOutputProjection>,
}

impl ErasedConfigRecipe {
    pub fn from_handler(mut handler: Box<dyn ErasedCommandConfig>) -> Self {
        let template_name = handler.template_name().map(String::from);
        let template_absence = handler.template_absence();
        let emits_events = handler.emits_events();
        let hooks = handler.take_hooks();
        let structured_output_projection = handler.structured_output_projection().cloned();
        Self {
            config: RefCell::new(Some(handler)),
            template_name,
            template_absence,
            emits_events,
            hooks: RefCell::new(hooks),
            structured_output_projection,
        }
    }
}

impl CommandRecipe for ErasedConfigRecipe {
    fn template_name(&self) -> Option<&str> {
        self.template_name.as_deref()
    }

    fn template_absence(&self) -> Option<TemplateAbsence> {
        self.template_absence
    }

    fn hooks(&self) -> Option<&Hooks> {
        None
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.borrow_mut().take()
    }

    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand> {
        None
    }

    fn emits_events(&self) -> bool {
        self.emits_events
    }

    fn create_dispatch(
        &self,
        template: &TemplateRef,
        context_registry: &ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
        strict_style_tags: bool,
    ) -> DispatchFn {
        let config = self
            .config
            .borrow_mut()
            .take()
            .expect("ErasedConfigRecipe::create_dispatch called more than once");
        config.register(
            "",
            template.clone(),
            context_registry.clone(),
            template_engine,
            template_registry,
            strict_style_tags,
        )
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        if let Some(config) = self.config.borrow().as_ref() {
            config.expected_args()
        } else {
            Vec::new()
        }
    }

    fn structured_output_projection(&self) -> Option<&StructuredOutputProjection> {
        self.structured_output_projection.as_ref()
    }
}

pub(crate) struct PassthroughRecipe<F>
where
    F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
{
    handler: Rc<RefCell<F>>,
}

impl<F> PassthroughRecipe<F>
where
    F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
{
    pub fn new(handler: F) -> Self {
        Self {
            handler: Rc::new(RefCell::new(handler)),
        }
    }
}

impl<F> CommandRecipe for PassthroughRecipe<F>
where
    F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
{
    fn hooks(&self) -> Option<&Hooks> {
        None
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        None
    }

    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand> {
        None
    }

    fn create_dispatch(
        &self,
        _template: &TemplateRef,
        _context_registry: &ContextRegistry,
        _template_engine: SharedTemplateEngine,
        _template_registry: Option<Rc<crate::TemplateRegistry>>,
        _strict_style_tags: bool,
    ) -> DispatchFn {
        dispatch_passthrough(self.handler.clone())
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        Vec::new()
    }
}

pub struct CommandConfig<H> {
    pub(crate) handler: H,
    pub(crate) template_name: Option<String>,
    pub(crate) template_absence: Option<TemplateAbsence>,
    pub(crate) hooks: Option<Hooks>,
    pub(crate) questionnaire: Option<QuestionnaireCommand>,
    pub(crate) questionnaire_settings: Rc<RefCell<QuestionnaireSettings>>,
    pub(crate) structured_output_projection: Option<StructuredOutputProjection>,
}

impl<H> CommandConfig<H> {
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            template_name: None,
            template_absence: None,
            hooks: None,
            questionnaire: None,
            questionnaire_settings: Rc::new(RefCell::new(QuestionnaireSettings::default())),
            structured_output_projection: None,
        }
    }

    pub fn template_name(mut self, name: impl Into<String>) -> Self {
        self.template_name = Some(name.into());
        self.template_absence = None;
        self
    }

    pub fn structured_only(mut self) -> Self {
        self.template_name = None;
        self.template_absence = Some(TemplateAbsence::StructuredOnly);
        self
    }

    pub fn silent(mut self) -> Self {
        self.template_name = None;
        self.template_absence = Some(TemplateAbsence::Silent);
        self
    }

    pub fn binary(mut self) -> Self {
        self.template_name = None;
        self.template_absence = Some(TemplateAbsence::Binary);
        self
    }

    pub fn hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = Some(hooks);
        self
    }

    pub fn questionnaire<T>(mut self) -> Self
    where
        T: QuestionnaireInput + Clone + Send + Sync + 'static,
    {
        self.questionnaire = Some(QuestionnaireCommand::new::<T>());
        let settings = Rc::clone(&self.questionnaire_settings);
        self.pre_dispatch(move |matches, ctx| {
            questionnaire_pre_dispatch::<T>(matches, ctx, &settings.borrow())
        })
    }

    pub fn questionnaire_with_form<T, F>(mut self, form: F) -> Self
    where
        T: QuestionnaireInput + Clone + Send + Sync + 'static,
        F: Fn(&T) -> Vec<FormError> + Clone + 'static,
    {
        self.questionnaire = Some(QuestionnaireCommand::new::<T>());
        let settings = Rc::clone(&self.questionnaire_settings);
        self.pre_dispatch(move |matches, ctx| {
            questionnaire_pre_dispatch_with::<T, _>(matches, ctx, &settings.borrow(), form.clone())
        })
    }

    pub fn questionnaire_with_form_and_review<T, F, R>(mut self, form: F, review: R) -> Self
    where
        T: QuestionnaireInput + Clone + Send + Sync + 'static,
        F: Fn(&T) -> Vec<FormError> + Clone + 'static,
        R: Fn(&T, &mut dyn std::io::Write) -> anyhow::Result<()> + Clone + 'static,
    {
        self.questionnaire = Some(QuestionnaireCommand::new::<T>());
        let settings = Rc::clone(&self.questionnaire_settings);
        self.pre_dispatch(move |matches, ctx| {
            questionnaire_pre_dispatch_with_review::<T, _, _>(
                matches,
                ctx,
                &settings.borrow(),
                form.clone(),
                review.clone(),
            )
        })
    }

    /// Order against the `questionnaire*` calls does not matter.
    pub fn confirmation(self, confirmation: Confirmation) -> Self {
        self.questionnaire_settings.borrow_mut().confirmation = confirmation;
        self
    }

    /// Replaces the framework's preamble/fingerprint sheet for `--answers`.
    pub fn answer_sheet_format(self, format: impl AnswerSheetFormat + 'static) -> Self {
        self.questionnaire_settings.borrow_mut().format = Rc::new(format);
        self
    }

    pub fn structured_output_projection(mut self, projection: StructuredOutputProjection) -> Self {
        self.structured_output_projection = Some(projection);
        self
    }

    pub fn pre_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &mut CommandContext) -> Result<(), crate::cli::hooks::HookError>
            + 'static,
    {
        let hooks = self.hooks.take().unwrap_or_default();
        self.hooks = Some(hooks.pre_dispatch(f));
        self
    }

    pub fn post_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(
                &ArgMatches,
                &CommandContext,
                serde_json::Value,
            ) -> Result<serde_json::Value, crate::cli::hooks::HookError>
            + 'static,
    {
        let hooks = self.hooks.take().unwrap_or_default();
        self.hooks = Some(hooks.post_dispatch(f));
        self
    }

    pub fn post_output<F>(mut self, f: F) -> Self
    where
        F: Fn(
                &ArgMatches,
                &CommandContext,
                crate::cli::hooks::RenderedOutput,
            )
                -> Result<crate::cli::hooks::RenderedOutput, crate::cli::hooks::HookError>
            + 'static,
    {
        let hooks = self.hooks.take().unwrap_or_default();
        self.hooks = Some(hooks.post_output(f));
        self
    }

    pub fn input<T>(
        self,
        name: impl Into<std::borrow::Cow<'static, str>>,
        chain: standout_input::InputChain<T>,
    ) -> Self
    where
        T: Clone + Send + Sync + 'static,
    {
        let name = name.into();
        self.pre_dispatch(move |matches, ctx| {
            use crate::cli::CommandContextInput;
            let sub_matches = crate::cli::dispatch::get_deepest_matches(matches);
            let sources = ctx.input_sources();
            let resolved = chain
                .resolve_from_with_source(sub_matches, sources)
                .map_err(|e| {
                    crate::cli::hooks::HookError::pre_dispatch(format!("input `{}`: {}", name, e))
                })?;
            if !ctx.extensions.contains::<standout_input::Inputs>() {
                ctx.extensions.insert(standout_input::Inputs::new());
            }
            let bag = ctx
                .extensions
                .get_mut::<standout_input::Inputs>()
                .expect("Inputs just inserted");
            if let Some(source) = bag.source_of(name.as_ref()) {
                return Err(crate::cli::hooks::HookError::pre_dispatch(format!(
                    "input `{}` is already resolved from {}; duplicate input names are not supported",
                    name, source
                )));
            }
            bag.insert(name.clone(), resolved);
            Ok(())
        })
    }

    pub fn pipe_to(self, command: impl Into<String>) -> Self {
        self.pipe_to_with_timeout(command, std::time::Duration::from_secs(30))
    }

    pub fn pipe_to_with_timeout(
        self,
        command: impl Into<String>,
        timeout: std::time::Duration,
    ) -> Self {
        let command = command.into();
        self.post_output(move |_matches, _ctx, output| {
            if let RenderedOutput::Text(ref text_output) = output {
                let pipe = standout_pipe::SimplePipe::new(command.clone()).with_timeout(timeout);
                pipe.pipe(&text_output.raw)
                    .map_err(|e| crate::cli::hooks::HookError::post_output(e.to_string()))?;
                Ok(output)
            } else {
                Ok(output)
            }
        })
    }

    pub fn pipe_through(self, command: impl Into<String>) -> Self {
        self.pipe_through_with_timeout(command, std::time::Duration::from_secs(30))
    }

    pub fn pipe_through_with_timeout(
        self,
        command: impl Into<String>,
        timeout: std::time::Duration,
    ) -> Self {
        let command = command.into();
        self.post_output(move |_matches, _ctx, output| {
            if let RenderedOutput::Text(ref text_output) = output {
                let pipe = standout_pipe::SimplePipe::new(command.clone())
                    .capture()
                    .with_timeout(timeout);
                let result = pipe
                    .pipe(&text_output.raw)
                    .map_err(|e| crate::cli::hooks::HookError::post_output(e.to_string()))?;
                Ok(RenderedOutput::Text(TextOutput::plain(result)))
            } else {
                Ok(output)
            }
        })
    }

    pub fn pipe_to_clipboard(self) -> Self {
        self.post_output(move |_matches, _ctx, output| {
            if let RenderedOutput::Text(ref text_output) = output {
                if let Some(pipe) = standout_pipe::clipboard() {
                    let result = pipe
                        .pipe(&text_output.raw)
                        .map_err(|e| crate::cli::hooks::HookError::post_output(e.to_string()))?;
                    Ok(RenderedOutput::Text(TextOutput::plain(result)))
                } else {
                    Err(crate::cli::hooks::HookError::post_output(
                        "Clipboard not supported on this platform. \
                         Use pipe_to() with a platform-specific clipboard command.",
                    ))
                }
            } else {
                Ok(output)
            }
        })
    }

    pub fn pipe_with<P>(self, target: P) -> Self
    where
        P: standout_pipe::PipeTarget + 'static,
    {
        let target = Rc::new(target);
        self.post_output(move |_matches, _ctx, output| {
            if let RenderedOutput::Text(ref text_output) = output {
                let result = target
                    .pipe(&text_output.raw)
                    .map_err(|e| crate::cli::hooks::HookError::post_output(e.to_string()))?;
                Ok(RenderedOutput::Text(TextOutput::plain(result)))
            } else {
                Ok(output)
            }
        })
    }
}

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
    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand>;
    /// True when the handler declares that it produces its result while it runs.
    fn emits_events(&self) -> bool {
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
                    questionnaire: config.questionnaire,
                    structured_output_projection: config.structured_output_projection,
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
    questionnaire: Option<QuestionnaireCommand>,
    structured_output_projection: Option<StructuredOutputProjection>,
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

    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand> {
        self.questionnaire.take()
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

    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand> {
        None
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
    fn test_command_config_template_name() {
        let config =
            CommandConfig::new(FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(HandlerOutput::Render(json!({})))
            }))
            .template_name("inner");

        assert_eq!(config.template_name, Some("inner".to_string()));
    }

    #[test]
    fn test_command_config_hooks() {
        let config =
            CommandConfig::new(FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(HandlerOutput::Render(json!({})))
            }))
            .pre_dispatch(|_, _| Ok(()));

        assert!(config.hooks.is_some());
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
