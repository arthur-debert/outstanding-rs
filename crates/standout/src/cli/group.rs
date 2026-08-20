//! Nested command group builder for declarative dispatch.
//!
//! This module provides [`GroupBuilder`] for creating nested command hierarchies
//! with a fluent API, and [`CommandConfig`] for inline command configuration.

use crate::context::ContextRegistry;

use clap::ArgMatches;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::builder::{SharedTemplateEngine, TemplateAbsence, TemplateRef};
use super::dispatch::{render_handler_output, DispatchFn};
use crate::cli::handler::{CommandContext, FnHandler, Handler, HandlerResult};
use crate::cli::hooks::{Hooks, RenderedOutput, TextOutput};
use crate::cli::questionnaire::{
    questionnaire_pre_dispatch, questionnaire_pre_dispatch_with,
    questionnaire_pre_dispatch_with_review, QuestionnaireCommand,
};
use crate::StructuredOutputProjection;
use standout_dispatch::verify::ExpectedArg;
use standout_input::questionnaire::{FormError, QuestionnaireInput};
use standout_pipe::PipeTarget;

// ============================================================================
// CommandRecipe - Deferred dispatch closure creation
// ============================================================================

/// A recipe for creating a dispatch closure.
///
/// Unlike `ErasedCommandConfig::register` which consumes self, this trait
/// takes `&self` to allow deferred closure creation. This enables late binding
/// where the theme is passed at dispatch time rather than captured at registration.
///
/// # Implementation Notes
///
/// Most implementations (`ClosureRecipe`, `StructRecipe`) can be called multiple
/// times since they clone their Rc-wrapped handlers. However, `ErasedConfigRecipe`
/// is single-use due to type erasure constraints - it will panic if called twice.
/// This is acceptable because `ensure_commands_finalized()` is guarded to run
/// only once per built app.
pub(crate) trait CommandRecipe {
    /// Returns the template for this command, if explicitly set.
    #[allow(dead_code)]
    fn template(&self) -> Option<&str>;

    /// Returns the explicit template registry name, if set.
    #[allow(dead_code)]
    fn template_name(&self) -> Option<&str> {
        None
    }

    /// Returns the declared reason this command has no template, if any.
    #[allow(dead_code)]
    fn template_absence(&self) -> Option<TemplateAbsence> {
        None
    }

    /// Returns hooks for this command, if set.
    #[allow(dead_code)]
    fn hooks(&self) -> Option<&Hooks>;

    /// Takes ownership of questionnaire metadata, if set.
    #[allow(dead_code)]
    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand>;

    /// Takes ownership of hooks (for registration with AppBuilder).
    #[allow(dead_code)]
    fn take_hooks(&mut self) -> Option<Hooks>;

    /// Creates a dispatch closure with the given configuration.
    ///
    /// Recipe and config registration share [`dispatch_from_handler`];
    /// passthrough commands share [`dispatch_passthrough`]. There is no
    /// public [`GroupBuilder`] change.
    ///
    /// # Panics
    ///
    /// `ErasedConfigRecipe` will panic if called more than once (see trait docs).
    fn create_dispatch(
        &self,
        template: &TemplateRef,
        context_registry: &ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
    ) -> DispatchFn;

    /// Returns the arguments expected by this command handler.
    fn expected_args(&self) -> Vec<ExpectedArg>;
}

/// One dispatch closure for recipe and config handler registration.
///
/// `CommandRecipe::create_dispatch` and `ErasedCommandConfig::register` both
/// call this so the two surfaces cannot drift. The public [`GroupBuilder`]
/// API is unchanged.
fn dispatch_from_handler<H>(
    handler: Rc<RefCell<H>>,
    template: TemplateRef,
    context_registry: ContextRegistry,
    template_engine: SharedTemplateEngine,
    template_registry: Option<Rc<crate::TemplateRegistry>>,
    structured_output_projection: Option<StructuredOutputProjection>,
) -> DispatchFn
where
    H: Handler + 'static,
    H::Output: Serialize,
{
    Rc::new(RefCell::new(
        move |matches: &ArgMatches,
              ctx: &CommandContext,
              hooks: Option<&Hooks>,
              output_mode: crate::OutputMode,
              theme: &crate::Theme,
              ambiguous_width: crate::AmbiguousWidth,
              target: Option<crate::TargetProperties>| {
            let result = handler.borrow_mut().handle(matches, ctx);
            render_handler_output(
                result,
                matches,
                ctx,
                hooks,
                &template,
                theme,
                &context_registry,
                &template_engine,
                template_registry.as_ref(),
                output_mode,
                structured_output_projection.as_ref(),
                ambiguous_width,
                target,
            )
        },
    ))
}

/// One passthrough dispatch closure for recipe and config registration.
fn dispatch_passthrough<F>(handler: Rc<RefCell<F>>) -> DispatchFn
where
    F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
{
    Rc::new(RefCell::new(
        move |matches: &ArgMatches,
              ctx: &CommandContext,
              _hooks: Option<&Hooks>,
              _output_mode: crate::OutputMode,
              _theme: &crate::Theme,
              _ambiguous_width: crate::AmbiguousWidth,
              _target: Option<crate::TargetProperties>| {
            match (handler.borrow_mut())(matches, ctx) {
                Ok(()) => Ok(super::dispatch::DispatchOutput::Silent),
                Err(e) => Err(super::dispatch::handler_run_error(e)),
            }
        },
    ))
}

/// Recipe for closure-based command handlers.
pub(crate) struct ClosureRecipe<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
    T: Serialize + 'static,
{
    handler: Rc<RefCell<FnHandler<F, T>>>,
    template: Option<String>,
    hooks: Option<Hooks>,
    questionnaire: Option<QuestionnaireCommand>,
    structured_output_projection: Option<StructuredOutputProjection>,
}

impl<F, T> ClosureRecipe<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
    T: Serialize + 'static,
{
    pub fn new(handler: FnHandler<F, T>) -> Self {
        Self {
            handler: Rc::new(RefCell::new(handler)),
            template: None,
            hooks: None,
            questionnaire: None,
            structured_output_projection: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_template(mut self, template: String) -> Self {
        self.template = Some(template);
        self
    }

    #[allow(dead_code)]
    pub fn with_hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = Some(hooks);
        self
    }

    pub fn with_structured_output_projection(
        mut self,
        projection: StructuredOutputProjection,
    ) -> Self {
        self.structured_output_projection = Some(projection);
        self
    }
}

impl<F, T> CommandRecipe for ClosureRecipe<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
    T: Serialize + 'static,
{
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
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

    fn create_dispatch(
        &self,
        template: &TemplateRef,
        context_registry: &ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
    ) -> DispatchFn {
        dispatch_from_handler(
            self.handler.clone(),
            template.clone(),
            context_registry.clone(),
            template_engine,
            template_registry,
            self.structured_output_projection.clone(),
        )
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        self.handler.borrow().expected_args()
    }
}

/// Recipe for struct-based command handlers.
pub(crate) struct StructRecipe<H, T>
where
    H: Handler<Output = T> + 'static,
    T: Serialize + 'static,
{
    handler: Rc<RefCell<H>>,
    #[allow(dead_code)]
    template: Option<String>,
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
            template: None,
            hooks: None,
            questionnaire: None,
            structured_output_projection: None,
            _phantom: std::marker::PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn with_template(mut self, template: String) -> Self {
        self.template = Some(template);
        self
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
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
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

    fn create_dispatch(
        &self,
        template: &TemplateRef,
        context_registry: &ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
    ) -> DispatchFn {
        dispatch_from_handler(
            self.handler.clone(),
            template.clone(),
            context_registry.clone(),
            template_engine,
            template_registry,
            self.structured_output_projection.clone(),
        )
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        self.handler.borrow().expected_args()
    }
}

/// Wrapper around ErasedCommandConfig that implements CommandRecipe.
///
/// This allows group-registered commands to use the deferred closure pattern.
/// The inner config is wrapped in RefCell to allow interior mutability.
///
/// # Single-Use Constraint
///
/// Unlike `ClosureRecipe` and `StructRecipe`, this implementation can only
/// have `create_dispatch` called once. This is because `ErasedCommandConfig::register`
/// consumes `Box<Self>`, so we must use `.take()` to extract it from the RefCell.
///
/// This constraint is safe because `ensure_commands_finalized()` in `App`
/// is guarded to run only once, so each recipe's `create_dispatch` is called
/// exactly once during the built app's lifecycle.
pub(crate) struct ErasedConfigRecipe {
    config: RefCell<Option<Box<dyn ErasedCommandConfig>>>,
    #[allow(dead_code)]
    template: Option<String>,
    template_name: Option<String>,
    template_absence: Option<TemplateAbsence>,
    #[allow(dead_code)]
    hooks: RefCell<Option<Hooks>>,
}

impl ErasedConfigRecipe {
    /// Creates a new recipe from an existing boxed handler (for group registration).
    pub fn from_handler(mut handler: Box<dyn ErasedCommandConfig>) -> Self {
        let template = handler.template().map(String::from);
        let template_name = handler.template_name().map(String::from);
        let template_absence = handler.template_absence();
        let hooks = handler.take_hooks();
        Self {
            config: RefCell::new(Some(handler)),
            template,
            template_name,
            template_absence,
            hooks: RefCell::new(hooks),
        }
    }
}

impl CommandRecipe for ErasedConfigRecipe {
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    fn template_name(&self) -> Option<&str> {
        self.template_name.as_deref()
    }

    fn template_absence(&self) -> Option<TemplateAbsence> {
        self.template_absence
    }

    fn hooks(&self) -> Option<&Hooks> {
        // Can't return reference through RefCell, but hooks are extracted during construction
        None
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.borrow_mut().take()
    }

    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand> {
        None
    }

    fn create_dispatch(
        &self,
        template: &TemplateRef,
        context_registry: &ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
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
        )
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        // See implementation note in prev step thought: we check if config is present
        if let Some(config) = self.config.borrow().as_ref() {
            config.expected_args()
        } else {
            Vec::new()
        }
    }
}

/// Recipe for passthrough commands that bypass the rendering pipeline.
///
/// The handler receives `&ArgMatches` and `&CommandContext`, writes directly to
/// stdout (or does whatever it needs), and the framework marks the command as
/// handled with no output.
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
    fn template(&self) -> Option<&str> {
        None
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

    fn create_dispatch(
        &self,
        _template: &TemplateRef,
        _context_registry: &ContextRegistry,
        _template_engine: SharedTemplateEngine,
        _template_registry: Option<Rc<crate::TemplateRegistry>>,
    ) -> DispatchFn {
        dispatch_passthrough(self.handler.clone())
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        Vec::new()
    }
}

/// Configuration for a single command.
///
/// Used internally to collect handler, template, and hooks before
/// registering with the builder.
pub struct CommandConfig<H> {
    pub(crate) handler: H,
    pub(crate) template: Option<String>,
    pub(crate) template_name: Option<String>,
    pub(crate) template_absence: Option<TemplateAbsence>,
    pub(crate) hooks: Option<Hooks>,
    pub(crate) questionnaire: Option<QuestionnaireCommand>,
    pub(crate) structured_output_projection: Option<StructuredOutputProjection>,
}

impl<H> CommandConfig<H> {
    /// Creates a new command config with the given handler.
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            template: None,
            template_name: None,
            template_absence: None,
            hooks: None,
            questionnaire: None,
            structured_output_projection: None,
        }
    }

    /// Sets explicit inline template source for this command.
    ///
    /// This value is always rendered as MiniJinja source text. Use
    /// [`template_name`](Self::template_name) to reference a registry entry.
    pub fn template(mut self, template: impl Into<String>) -> Self {
        self.template = Some(template.into());
        self.template_name = None;
        self.template_absence = None;
        self
    }

    /// Sets an explicit registry template name for this command.
    ///
    /// The name resolves through templates registered with `.templates(...)` or
    /// `.templates_dir(...)`, including extension fallback.
    pub fn template_name(mut self, name: impl Into<String>) -> Self {
        self.template = None;
        self.template_name = Some(name.into());
        self.template_absence = None;
        self
    }

    /// Declares that returned render data is only valid for structured modes.
    ///
    /// `auto`, omitted `--output`, and explicit `json` serialize as JSON;
    /// `yaml`, `xml`, and `csv` use their serializers; `term`, `text`, and
    /// `term-debug` fail because this command has no human presentation.
    pub fn structured_only(mut self) -> Self {
        self.template = None;
        self.template_name = None;
        self.template_absence = Some(TemplateAbsence::StructuredOnly);
        self
    }

    /// Declares that this command intentionally emits no presentation text.
    pub fn silent(mut self) -> Self {
        self.template = None;
        self.template_name = None;
        self.template_absence = Some(TemplateAbsence::Silent);
        self
    }

    /// Declares that this command's successful output is binary.
    pub fn binary(mut self) -> Self {
        self.template = None;
        self.template_name = None;
        self.template_absence = Some(TemplateAbsence::Binary);
        self
    }

    /// Sets hooks for this command.
    ///
    /// If the parent `AppBuilder` also registers hooks for this command path
    /// through `.hooks()`, the same hook phase can appear in only one place.
    /// `build()` returns a configuration error naming the path and phase when
    /// both APIs configure the same phase.
    pub fn hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Attaches a derived questionnaire input to this command.
    ///
    /// Standout injects the reserved questionnaire CLI surface into this
    /// command at parse time: `--answers FILE|-`, `--yes`, and the
    /// side-effect-free `questions` subcommand. Before the handler runs, the
    /// framework resolves exactly one answer source (answer sheet file,
    /// explicit stdin, or interactive prompts), decodes it through the
    /// shared questionnaire pipeline, optionally renders an application
    /// review configured with
    /// [`questionnaire_with_form_and_review`](Self::questionnaire_with_form_and_review),
    /// runs the attended confirmation gate unless `--yes` was supplied, and
    /// stores the typed value in the command context. Handlers read it with
    /// [`CommandContextInput::questionnaire`](crate::cli::CommandContextInput::questionnaire).
    pub fn questionnaire<T>(mut self) -> Self
    where
        T: QuestionnaireInput + Clone + Send + Sync + 'static,
    {
        self.questionnaire = Some(QuestionnaireCommand::new::<T>());
        self.pre_dispatch(questionnaire_pre_dispatch::<T>)
    }

    /// Attaches a derived questionnaire input with typed whole-form rules.
    ///
    /// This is the same injected CLI surface as [`questionnaire`](Self::questionnaire),
    /// but after field decoding fills the derived struct, `form` can reject
    /// combinations that only make sense across multiple answers. The returned
    /// [`FormError`] values join the shared questionnaire validation diagnostics.
    pub fn questionnaire_with_form<T, F>(mut self, form: F) -> Self
    where
        T: QuestionnaireInput + Clone + Send + Sync + 'static,
        F: Fn(&T) -> Vec<FormError> + Clone + 'static,
    {
        self.questionnaire = Some(QuestionnaireCommand::new::<T>());
        self.pre_dispatch(move |matches, ctx| {
            questionnaire_pre_dispatch_with::<T, _>(matches, ctx, form.clone())
        })
    }

    /// Attaches a derived questionnaire with whole-form rules and a
    /// pre-confirmation application review.
    ///
    /// The framework first resolves and decodes the questionnaire, then calls
    /// `review` with the typed answers and stdout before it asks for attended
    /// confirmation. A declined or missing confirmation prevents the handler
    /// from running, so applications can show the exact operation that would
    /// happen while keeping side effects behind the confirmation gate.
    pub fn questionnaire_with_form_and_review<T, F, R>(mut self, form: F, review: R) -> Self
    where
        T: QuestionnaireInput + Clone + Send + Sync + 'static,
        F: Fn(&T) -> Vec<FormError> + Clone + 'static,
        R: Fn(&T, &mut dyn std::io::Write) -> anyhow::Result<()> + Clone + 'static,
    {
        self.questionnaire = Some(QuestionnaireCommand::new::<T>());
        self.pre_dispatch(move |matches, ctx| {
            questionnaire_pre_dispatch_with_review::<T, _, _>(
                matches,
                ctx,
                form.clone(),
                review.clone(),
            )
        })
    }

    /// Attaches a presentation-layer projection for structured output.
    ///
    /// The projection consumes the serialized response after post-dispatch
    /// hooks. It does not change handler data or human-rendered output.
    pub fn structured_output_projection(mut self, projection: StructuredOutputProjection) -> Self {
        self.structured_output_projection = Some(projection);
        self
    }

    /// Adds a pre-dispatch hook for this command.
    ///
    /// Pre-dispatch hooks receive mutable access to [`CommandContext`], allowing
    /// state injection via `ctx.extensions`. Handlers can then retrieve this state.
    pub fn pre_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &mut CommandContext) -> Result<(), crate::cli::hooks::HookError>
            + 'static,
    {
        let hooks = self.hooks.take().unwrap_or_default();
        self.hooks = Some(hooks.pre_dispatch(f));
        self
    }

    /// Adds a post-dispatch hook for this command.
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

    /// Adds a post-output hook for this command.
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

    /// Registers a declarative input chain for this command.
    ///
    /// The chain is resolved during pre-dispatch — before the handler runs —
    /// and the resolved value is stored in an [`Inputs`](standout_input::Inputs)
    /// bag on `ctx.extensions` under `name`. The handler retrieves it with
    /// [`CommandContextInput::input`](crate::cli::CommandContextInput::input):
    ///
    /// ```ignore
    /// use standout::cli::{App, CommandContextInput, Output};
    /// use standout::input::{ArgSource, EditorSource, InputChain, StdinSource};
    ///
    /// App::builder()
    ///     .command_with("create", |_m, ctx| {
    ///         let body: &String = ctx.input("body")?;
    ///         Ok(Output::Render(serde_json::json!({ "body": body })))
    ///     }, |cfg| {
    ///         cfg.template_name("create")
    ///            .input("body", InputChain::<String>::new()
    ///                .try_source(ArgSource::new("body"))
    ///                .try_source(StdinSource::new())
    ///                .try_source(EditorSource::new()))
    ///     })?
    ///     .build()?;
    /// ```
    ///
    /// Multiple `.input(...)` calls on the same command accumulate under unique
    /// names; duplicate names fail during pre-dispatch instead of overwriting
    /// an earlier resolved value. Each
    /// registers a pre-dispatch hook that writes into the shared bag, so
    /// commands can declare several named inputs of any types.
    ///
    /// `name` accepts anything convertible into `Cow<'static, str>` — string
    /// literals, owned `String`s built at runtime (e.g. from config), and
    /// explicit `Cow`s all work.
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
            // Pre-dispatch hooks receive the top-level ArgMatches, but the
            // chain's sources reference args defined on the deepest subcommand
            // (the same matches the handler sees). Resolve against those.
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

    /// Pipes the output to a shell command in passthrough mode.
    ///
    /// The output is sent to the command's stdin, but the original output
    /// is preserved and returned. Useful for side effects like `tee` or `pbcopy`
    /// where you still want to see the output.
    ///
    /// Uses a default timeout of 30 seconds. For custom timeouts, use
    /// [`pipe_to_with_timeout`](Self::pipe_to_with_timeout).
    ///
    /// # Note
    ///
    /// Only [`RenderedOutput::Text`] is piped. Binary and silent outputs pass through unchanged.
    pub fn pipe_to(self, command: impl Into<String>) -> Self {
        self.pipe_to_with_timeout(command, std::time::Duration::from_secs(30))
    }

    /// Pipes the output to a shell command in passthrough mode with a custom timeout.
    ///
    /// See [`pipe_to`](Self::pipe_to) for details on passthrough mode.
    ///
    /// # Note
    ///
    /// Only [`RenderedOutput::Text`] is piped. Binary and silent outputs pass through unchanged.
    /// The raw output (without ANSI codes) is piped to the command, matching shell semantics.
    /// The terminal still displays the formatted output with colors.
    pub fn pipe_to_with_timeout(
        self,
        command: impl Into<String>,
        timeout: std::time::Duration,
    ) -> Self {
        let command = command.into();
        self.post_output(move |_matches, _ctx, output| {
            if let RenderedOutput::Text(ref text_output) = output {
                let pipe = standout_pipe::SimplePipe::new(command.clone()).with_timeout(timeout);
                // Pipe the raw output (no ANSI codes) - matches shell semantics
                pipe.pipe(&text_output.raw)
                    .map_err(|e| crate::cli::hooks::HookError::post_output(e.to_string()))?;
                // Passthrough: return original output (formatted for terminal)
                Ok(output)
            } else {
                Ok(output)
            }
        })
    }

    /// Pipes the output to a shell command in capture mode.
    ///
    /// The output is sent to the command's stdin, and the command's stdout
    /// becomes the new output. Useful for filters like `jq` or `sort`.
    ///
    /// Uses a default timeout of 30 seconds. For custom timeouts, use
    /// [`pipe_through_with_timeout`](Self::pipe_through_with_timeout).
    ///
    /// # Note
    ///
    /// Only [`RenderedOutput::Text`] is piped. Binary and silent outputs pass through unchanged.
    pub fn pipe_through(self, command: impl Into<String>) -> Self {
        self.pipe_through_with_timeout(command, std::time::Duration::from_secs(30))
    }

    /// Pipes the output to a shell command in capture mode with a custom timeout.
    ///
    /// See [`pipe_through`](Self::pipe_through) for details on capture mode.
    ///
    /// # Note
    ///
    /// Only [`RenderedOutput::Text`] is piped. Binary and silent outputs pass through unchanged.
    /// The raw output (without ANSI codes) is piped to the command, matching shell semantics.
    /// The command's stdout becomes the new output.
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
                // Pipe the raw output (no ANSI codes) - matches shell semantics
                let result = pipe
                    .pipe(&text_output.raw)
                    .map_err(|e| crate::cli::hooks::HookError::post_output(e.to_string()))?;
                // Capture: command's output becomes the new output (plain text)
                Ok(RenderedOutput::Text(TextOutput::plain(result)))
            } else {
                Ok(output)
            }
        })
    }

    /// Pipes the output to the system clipboard.
    ///
    /// This uses a platform-specific clipboard command:
    /// - macOS: `pbcopy`
    /// - Linux: `xclip -selection clipboard`
    ///
    /// This consumes the output (nothing is printed to terminal).
    /// The raw output (without ANSI codes) is copied to the clipboard,
    /// so you get clean text without escape sequences.
    ///
    /// # Errors
    ///
    /// Returns an error if the platform is not supported (neither macOS nor Linux).
    /// Use [`pipe_to`](Self::pipe_to) with a custom clipboard command for other platforms.
    ///
    /// # Note
    ///
    /// Only [`RenderedOutput::Text`] is piped. Binary and silent outputs pass through unchanged.
    pub fn pipe_to_clipboard(self) -> Self {
        self.post_output(move |_matches, _ctx, output| {
            if let RenderedOutput::Text(ref text_output) = output {
                if let Some(pipe) = standout_pipe::clipboard() {
                    // Pipe the raw output (no ANSI codes) to clipboard
                    let result = pipe
                        .pipe(&text_output.raw)
                        .map_err(|e| crate::cli::hooks::HookError::post_output(e.to_string()))?;
                    // Consume mode: return empty (clipboard() uses .consume())
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

    /// Pipes the output using a custom [`PipeTarget`](standout_pipe::PipeTarget).
    ///
    /// This is the most flexible piping option, allowing custom implementations
    /// beyond shell commands.
    ///
    /// # Note
    ///
    /// Only [`RenderedOutput::Text`] is piped. Binary and silent outputs pass through unchanged.
    /// The raw output (without ANSI codes) is piped to the target.
    pub fn pipe_with<P>(self, target: P) -> Self
    where
        P: standout_pipe::PipeTarget + 'static,
    {
        let target = Rc::new(target);
        self.post_output(move |_matches, _ctx, output| {
            if let RenderedOutput::Text(ref text_output) = output {
                // Pipe the raw output (no ANSI codes)
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

/// Entry in the group builder - either a command or a nested group.
pub(crate) enum GroupEntry {
    /// A leaf command with handler, optional template, and optional hooks
    Command {
        handler: Box<dyn ErasedCommandConfig>,
    },
    /// A nested group
    Group { builder: GroupBuilder },
}

/// Type-erased command configuration for storage.
pub(crate) trait ErasedCommandConfig {
    fn template(&self) -> Option<&str>;
    fn template_name(&self) -> Option<&str>;
    fn template_absence(&self) -> Option<TemplateAbsence>;
    #[allow(dead_code)]
    fn hooks(&self) -> Option<&Hooks>;
    fn take_hooks(&mut self) -> Option<Hooks>;
    fn take_questionnaire(&mut self) -> Option<QuestionnaireCommand>;
    fn register(
        self: Box<Self>,
        path: &str,
        template: TemplateRef,
        context_registry: ContextRegistry,
        template_engine: SharedTemplateEngine,
        template_registry: Option<Rc<crate::TemplateRegistry>>,
    ) -> DispatchFn;

    fn expected_args(&self) -> Vec<ExpectedArg>;
}

/// Builder for a group of related commands.
///
/// Groups allow organizing commands hierarchically:
///
/// ```rust,ignore
/// App::builder()
///     .group("db", |g| g
///         .command("migrate", db::migrate)
///         .command("backup", db::backup))
///     .group("app", |g| g
///         .command("start", app::start)
///         .group("config", |g| g
///             .command("get", app::config_get)
///             .command("set", app::config_set)))
///     .build()
/// ```
#[derive(Default)]
pub struct GroupBuilder {
    pub(crate) entries: HashMap<String, GroupEntry>,
    /// The default command to use when no subcommand is specified
    pub(crate) default_command: Option<String>,
}

impl GroupBuilder {
    /// Creates a new empty group builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if a command or group with the given name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Returns the number of registered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if no entries are registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the default command name, if one is set.
    pub fn get_default_command(&self) -> Option<&str> {
        self.default_command.as_deref()
    }

    /// Registers a command handler (closure) in this group.
    ///
    /// The template will be derived from the command path if not explicitly set.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// .group("db", |g| g
    ///     .command("migrate", |_m, _ctx| {
    ///         Ok(HandlerOutput::Render(json!({"status": "done"})))
    ///     }))
    /// ```
    pub fn command<F, T>(self, name: &str, handler: F) -> Self
    where
        F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
        T: Serialize + 'static,
    {
        self.command_with(name, handler, |cfg| cfg)
    }

    /// Registers a command handler with configuration.
    ///
    /// Use this to set explicit template or hooks inline:
    ///
    /// ```rust,ignore
    /// .group("db", |g| g
    ///     .command_with("migrate", handler, |cfg| cfg
    ///         .template_name("custom/migrate")
    ///         .pre_dispatch(validate_db)))
    /// ```
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
                    template: config.template,
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

    /// Registers a struct handler in this group.
    pub fn handler<H, T>(self, name: &str, handler: H) -> Self
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
    {
        self.handler_with(name, handler, |cfg| cfg)
    }

    /// Registers a struct handler with configuration.
    pub fn handler_with<H, T, C>(mut self, name: &str, handler: H, configure: C) -> Self
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
        C: FnOnce(CommandConfig<H>) -> CommandConfig<H>,
    {
        let config = CommandConfig::new(handler);
        let config = configure(config);
        self.entries.insert(
            name.to_string(),
            GroupEntry::Command {
                handler: Box::new(StructCommandConfig {
                    handler: Rc::new(RefCell::new(config.handler)),
                    template: config.template,
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

    /// Registers a passthrough command that bypasses the rendering pipeline.
    ///
    /// The handler receives `&ArgMatches` and `&CommandContext`, writes directly to
    /// stdout (or does whatever it needs), and the framework marks the command as
    /// handled with no rendered output. The command still participates in
    /// help/completions.
    ///
    /// Use this for commands that manage their own output (e.g., shell init scripts
    /// that output `eval`-able code, or commands that delegate to another tool).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// .group("app", |g| g
    ///     .passthrough("init-sh", |_m, _ctx| {
    ///         print!("export PATH=\"$HOME/.myapp/bin:$PATH\"");
    ///         Ok(())
    ///     }))
    /// ```
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

    /// Creates a nested group within this group.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// .group("app", |g| g
    ///     .group("config", |g| g
    ///         .command("get", get_handler)
    ///         .command("set", set_handler)))
    /// ```
    pub fn group<F>(mut self, name: &str, configure: F) -> Self
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());
        self.entries
            .insert(name.to_string(), GroupEntry::Group { builder });
        self
    }

    /// Sets a command as the default command for this group.
    ///
    /// When the CLI is invoked without a subcommand (a "naked" invocation),
    /// the default command is automatically used.
    ///
    /// # Panics
    ///
    /// Panics if a default command has already been set, as only one
    /// default command can be defined.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// .group("app", |g| g
    ///     .command("list", list_handler)
    ///     .command("add", add_handler)
    ///     .default_command("list"))  // "list" is used when no command specified
    /// ```
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

/// Internal: closure-based command config that implements ErasedCommandConfig
struct ClosureCommandConfig<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
    T: Serialize + 'static,
{
    handler: Rc<RefCell<FnHandler<F, T>>>,
    template: Option<String>,
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
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

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
    ) -> DispatchFn {
        dispatch_from_handler(
            self.handler,
            template,
            context_registry,
            template_engine,
            template_registry,
            self.structured_output_projection,
        )
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        self.handler.borrow().expected_args()
    }
}

/// Internal: struct-based command config that implements ErasedCommandConfig
struct StructCommandConfig<H, T>
where
    H: Handler<Output = T> + 'static,
    T: Serialize + 'static,
{
    handler: Rc<RefCell<H>>,
    template: Option<String>,
    template_name: Option<String>,
    template_absence: Option<TemplateAbsence>,
    hooks: Option<Hooks>,
    questionnaire: Option<QuestionnaireCommand>,
    structured_output_projection: Option<StructuredOutputProjection>,
}

impl<H, T> ErasedCommandConfig for StructCommandConfig<H, T>
where
    H: Handler<Output = T> + 'static,
    T: Serialize + 'static,
{
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

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
    ) -> DispatchFn {
        dispatch_from_handler(
            self.handler,
            template,
            context_registry,
            template_engine,
            template_registry,
            self.structured_output_projection,
        )
    }

    fn expected_args(&self) -> Vec<ExpectedArg> {
        self.handler.borrow().expected_args()
    }
}

/// Internal: passthrough command config that bypasses rendering.
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
    fn template(&self) -> Option<&str> {
        None
    }

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
    fn test_command_config_template() {
        let config =
            CommandConfig::new(FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(HandlerOutput::Render(json!({})))
            }))
            .template("custom {{ value }}");

        assert_eq!(config.template, Some("custom {{ value }}".to_string()));
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
