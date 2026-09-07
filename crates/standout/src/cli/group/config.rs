use super::super::builder::TemplateAbsence;
use crate::cli::handler::CommandContext;
use crate::cli::hooks::{Hooks, RenderedOutput, TextOutput};
use crate::cli::questionnaire::{
    questionnaire_pre_dispatch, questionnaire_pre_dispatch_with,
    questionnaire_pre_dispatch_with_review, Confirmation, QuestionnaireCommand,
    QuestionnaireSettings,
};
use crate::StructuredOutputProjection;
use clap::ArgMatches;
use standout_input::questionnaire::{AnswerSheetFormat, FormError, QuestionnaireInput};
use standout_pipe::PipeTarget;
use std::cell::RefCell;
use std::rc::Rc;

pub struct CommandConfig<H> {
    pub(crate) handler: H,
    pub(crate) template_name: Option<String>,
    pub(crate) template_absence: Option<TemplateAbsence>,
    pub(crate) hooks: Option<Hooks>,
    /// Pre-dispatch only, kept apart from `hooks` so declaring a chain leaves
    /// the command's pre-dispatch registration free. Runs ahead of `hooks`.
    pub(crate) input_chains: Option<Hooks>,
    pub(crate) questionnaire: Option<QuestionnaireCommand>,
    pub(crate) questionnaire_resolution: Option<Hooks>,
    pub(crate) questionnaire_settings: Rc<RefCell<QuestionnaireSettings>>,
    pub(crate) structured_output_projection: Option<StructuredOutputProjection>,
    pub(crate) pageable: bool,
    pub(crate) without_config: bool,
}

impl<H> CommandConfig<H> {
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            template_name: None,
            template_absence: None,
            hooks: None,
            input_chains: None,
            questionnaire: None,
            questionnaire_resolution: None,
            questionnaire_settings: Rc::new(RefCell::new(QuestionnaireSettings::default())),
            structured_output_projection: None,
            pageable: false,
            without_config: false,
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

    fn resolve_questionnaire<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &mut CommandContext) -> Result<(), crate::cli::hooks::HookError>
            + 'static,
    {
        let resolution = self.questionnaire_resolution.take().unwrap_or_default();
        self.questionnaire_resolution = Some(resolution.pre_dispatch(f));
        self
    }

    pub fn questionnaire<T>(mut self) -> Self
    where
        T: QuestionnaireInput + Clone + Send + Sync + 'static,
    {
        self.questionnaire = Some(QuestionnaireCommand::new::<T>());
        let settings = Rc::clone(&self.questionnaire_settings);
        self.resolve_questionnaire(move |matches, ctx| {
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
        self.resolve_questionnaire(move |matches, ctx| {
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
        self.resolve_questionnaire(move |matches, ctx| {
            questionnaire_pre_dispatch_with_review::<T, _, _>(
                matches,
                ctx,
                &settings.borrow(),
                form.clone(),
                review.clone(),
            )
        })
    }

    pub fn without_config(mut self) -> Self {
        self.without_config = true;
        self
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

    /// Marks the command's complete human output as pageable. Eligibility only:
    /// the framework pages nothing unless the run is batch human output on a
    /// terminal the environment names a pager for.
    pub fn pageable(mut self) -> Self {
        self.pageable = true;
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
                standout_render::RenderData,
            ) -> Result<standout_render::RenderData, crate::cli::hooks::HookError>
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
        mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
        chain: standout_input::InputChain<T>,
    ) -> Self
    where
        T: Clone + Send + Sync + 'static,
    {
        let name = name.into();
        let chains = self.input_chains.take().unwrap_or_default();
        self.input_chains = Some(chains.pre_dispatch(move |matches, ctx| {
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
        }));
        self
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::handler::{FnHandler, Output as HandlerOutput};
    use serde_json::json;

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
}
