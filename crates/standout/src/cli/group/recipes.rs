use crate::context::ContextRegistry;
use clap::ArgMatches;
use serde::Serialize;
use std::cell::RefCell;
use std::rc::Rc;

use super::super::builder::{SharedTemplateEngine, TemplateAbsence, TemplateRef};
use super::super::dispatch::{render_handler_output, DispatchFn};
use super::super::events::{event_template, EventContext, EventDestination};
use super::ErasedCommandConfig;
use crate::cli::handler::{
    emits_events, CommandContext, Handler, HandlerOutcome, Results, RunRecorder, StreamSink,
};
use crate::cli::hooks::Hooks;
use crate::cli::questionnaire::QuestionnaireCommand;
use crate::StructuredOutputProjection;
use standout_dispatch::verify::ExpectedArg;

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
    /// while it runs, so the build can require its `<name>.event` template.
    fn emits_events(&self) -> bool {
        false
    }

    /// True when the application marked the command's human output pageable.
    fn pageable(&self) -> bool {
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
pub(super) fn dispatch_from_handler<H>(
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
            let result = handler
                .borrow_mut()
                .handle(matches, ctx, &mut results)
                .map(HandlerOutcome::into_output);
            if let Some(failure) = destination.take_failure() {
                return Err(failure);
            }
            render_handler_output(
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
                emits_events::<H::Event>()
                    .then(|| destination.take_document_records())
                    .flatten(),
            )
        },
    ))
}

pub(super) fn dispatch_passthrough<F>(handler: Rc<RefCell<F>>) -> DispatchFn
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
                Ok(()) => Ok(crate::cli::dispatch::DispatchOutput::Silent {
                    status: crate::cli::handler::ExitStatus::SUCCESS,
                }),
                Err(e) => Err(crate::cli::dispatch::handler_run_error(e)),
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
    pageable: bool,
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
            pageable: false,
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

    pub fn pageable(mut self) -> Self {
        self.pageable = true;
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
        emits_events::<H::Event>()
    }

    fn pageable(&self) -> bool {
        self.pageable
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
    pageable: bool,
    #[allow(dead_code)]
    hooks: RefCell<Option<Hooks>>,
    structured_output_projection: Option<StructuredOutputProjection>,
}

impl ErasedConfigRecipe {
    pub fn from_handler(mut handler: Box<dyn ErasedCommandConfig>) -> Self {
        let template_name = handler.template_name().map(String::from);
        let template_absence = handler.template_absence();
        let emits_events = handler.emits_events();
        let pageable = handler.pageable();
        let hooks = handler.take_hooks();
        let structured_output_projection = handler.structured_output_projection().cloned();
        Self {
            config: RefCell::new(Some(handler)),
            template_name,
            template_absence,
            emits_events,
            pageable,
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

    fn pageable(&self) -> bool {
        self.pageable
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
