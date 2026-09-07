use clap::ArgMatches;
use serde::Serialize;

use super::{AppBuilder, HookRegistrationSource, PendingCommand, TemplateAbsence, TemplateRef};
use crate::cli::group::{
    CommandConfig, ErasedConfigRecipe, GroupBuilder, GroupEntry, PassthroughRecipe, StructRecipe,
};
use crate::cli::handler::{CommandContext, Handler};
use crate::cli::hooks::Hooks;
use crate::setup::SetupError;

impl AppBuilder {
    pub fn commands<F>(mut self, configure: F) -> Result<Self, SetupError>
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());

        if let Some(ref default_cmd) = builder.default_command {
            self.default_command = Some(default_cmd.clone());
        }

        for (name, entry) in builder.entries {
            match entry {
                GroupEntry::Command { mut handler } => {
                    let template = if let Some(absence) = handler.template_absence() {
                        TemplateRef::Absent(absence)
                    } else if let Some(name) = handler.template_name() {
                        TemplateRef::Named(name.to_string())
                    } else {
                        TemplateRef::convention(&name)
                    };

                    if let Some(hooks) = handler.take_hooks() {
                        self.register_command_hooks(
                            &name,
                            hooks,
                            HookRegistrationSource::CommandConfig,
                        )?;
                    }
                    if let Some(chains) = handler.take_input_chains() {
                        self.register_command_input_chains(&name, chains);
                    }
                    if let Some(questionnaire) = handler.take_questionnaire() {
                        self.questionnaire_commands
                            .insert(name.clone(), questionnaire);
                    }
                    if let Some(resolution) = handler.take_questionnaire_resolution() {
                        self.register_command_questionnaire_resolution(&name, resolution);
                    }
                    if handler.without_config() {
                        self.config_exempt_commands.insert(name.clone());
                    }

                    let recipe = ErasedConfigRecipe::from_handler(handler);

                    if self.pending_commands.borrow().contains_key(&name) {
                        return Err(SetupError::DuplicateCommand(name));
                    }

                    self.pending_commands.borrow_mut().insert(
                        name,
                        PendingCommand {
                            recipe: Box::new(recipe),
                            template,
                        },
                    );
                }
                GroupEntry::Group { builder: nested } => {
                    self.register_group(&name, nested)?;
                }
            }
        }

        Ok(self)
    }

    pub(crate) fn register_group(
        &mut self,
        prefix: &str,
        builder: GroupBuilder,
    ) -> Result<(), SetupError> {
        for (name, entry) in builder.entries {
            let path = format!("{}.{}", prefix, name);

            match entry {
                GroupEntry::Command { mut handler } => {
                    let template = if let Some(absence) = handler.template_absence() {
                        TemplateRef::Absent(absence)
                    } else if let Some(name) = handler.template_name() {
                        TemplateRef::Named(name.to_string())
                    } else {
                        TemplateRef::convention(&path)
                    };

                    if let Some(hooks) = handler.take_hooks() {
                        self.register_command_hooks(
                            &path,
                            hooks,
                            HookRegistrationSource::CommandConfig,
                        )?;
                    }
                    if let Some(chains) = handler.take_input_chains() {
                        self.register_command_input_chains(&path, chains);
                    }
                    if let Some(questionnaire) = handler.take_questionnaire() {
                        self.questionnaire_commands
                            .insert(path.clone(), questionnaire);
                    }
                    if let Some(resolution) = handler.take_questionnaire_resolution() {
                        self.register_command_questionnaire_resolution(&path, resolution);
                    }
                    if handler.without_config() {
                        self.config_exempt_commands.insert(path.clone());
                    }

                    let recipe = ErasedConfigRecipe::from_handler(handler);

                    if self.pending_commands.borrow().contains_key(&path) {
                        return Err(SetupError::DuplicateCommand(path.clone()));
                    }

                    self.pending_commands.borrow_mut().insert(
                        path,
                        PendingCommand {
                            recipe: Box::new(recipe),
                            template,
                        },
                    );
                }
                GroupEntry::Group { builder: nested } => {
                    self.register_group(&path, nested)?;
                }
            }
        }
        Ok(())
    }

    pub fn command_with<H, T, C>(
        self,
        path: &str,
        handler: H,
        configure: C,
    ) -> Result<Self, SetupError>
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
        C: FnOnce(CommandConfig<H>) -> CommandConfig<H>,
    {
        self.register_struct_config(path, configure(CommandConfig::new(handler)))
    }

    fn register_struct_config<H, T>(
        mut self,
        path: &str,
        mut config: CommandConfig<H>,
    ) -> Result<Self, SetupError>
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
    {
        let template = if let Some(absence) = config.template_absence {
            TemplateRef::Absent(absence)
        } else if let Some(name) = config.template_name.take() {
            TemplateRef::Named(name)
        } else {
            TemplateRef::convention(path)
        };

        if let Some(hooks) = config.hooks.take() {
            self.register_command_hooks(path, hooks, HookRegistrationSource::CommandConfig)?;
        }
        if let Some(chains) = config.input_chains.take() {
            self.register_command_input_chains(path, chains);
        }
        if let Some(questionnaire) = config.questionnaire.take() {
            self.questionnaire_commands
                .insert(path.to_string(), questionnaire);
        }
        if let Some(resolution) = config.questionnaire_resolution.take() {
            self.register_command_questionnaire_resolution(path, resolution);
        }
        if config.without_config {
            self.config_exempt_commands.insert(path.to_string());
        }

        let mut recipe = StructRecipe::new(config.handler);
        if let Some(projection) = config.structured_output_projection {
            recipe = recipe.with_structured_output_projection(projection);
        }
        if config.pageable {
            recipe = recipe.pageable();
        }

        if self.pending_commands.borrow().contains_key(path) {
            return Err(SetupError::DuplicateCommand(path.to_string()));
        }

        self.pending_commands.borrow_mut().insert(
            path.to_string(),
            PendingCommand {
                recipe: Box::new(recipe),
                template,
            },
        );

        Ok(self)
    }

    pub fn command_passthrough<F>(self, path: &str, handler: F) -> Result<Self, SetupError>
    where
        F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
    {
        let recipe = PassthroughRecipe::new(handler);

        if self.pending_commands.borrow().contains_key(path) {
            return Err(SetupError::DuplicateCommand(path.to_string()));
        }

        self.pending_commands.borrow_mut().insert(
            path.to_string(),
            PendingCommand {
                recipe: Box::new(recipe),
                template: TemplateRef::Absent(TemplateAbsence::Silent),
            },
        );

        Ok(self)
    }

    pub fn hooks(mut self, path: &str, hooks: Hooks) -> Self {
        if let Err(error) =
            self.register_command_hooks(path, hooks, HookRegistrationSource::AppBuilderHooks)
        {
            self.setup_errors.push(error);
        }
        self
    }

    pub(super) fn register_command_hooks(
        &mut self,
        path: &str,
        hooks: Hooks,
        source: HookRegistrationSource,
    ) -> Result<(), SetupError> {
        let phases: Vec<_> = hooks.phases().collect();
        if phases.is_empty() {
            return Ok(());
        }

        for phase in &phases {
            let key = (path.to_string(), *phase);
            if let Some(existing_source) = self.hook_phase_sources.get(&key) {
                if *existing_source != source {
                    let first = existing_source.describe(path);
                    let second = source.describe(path);
                    return Err(SetupError::Config(format!(
                        "command `{path}` registers {phase} hooks twice: once from {first}, once from {second}; keep each (path, phase) in one registration path"
                    )));
                }
            }
        }

        for phase in phases {
            self.hook_phase_sources
                .insert((path.to_string(), phase), source);
        }

        let (key, hooks) = match self.command_hooks.remove_entry(path) {
            Some((key, existing)) => (key, existing.append(hooks)),
            None => (path.to_string(), hooks),
        };
        self.command_hooks.insert(key, hooks);
        Ok(())
    }

    /// No `HookRegistrationSource`: this registration is the framework's own and
    /// never collides with what the application registers for the same phase.
    pub(super) fn register_command_input_chains(&mut self, path: &str, chains: Hooks) {
        let (key, chains) = match self.command_input_chains.remove_entry(path) {
            Some((key, existing)) => (key, existing.append(chains)),
            None => (path.to_string(), chains),
        };
        self.command_input_chains.insert(key, chains);
    }

    pub(super) fn register_command_questionnaire_resolution(
        &mut self,
        path: &str,
        resolution: Hooks,
    ) {
        let (key, resolution) = match self.command_questionnaire_resolution.remove_entry(path) {
            Some((key, existing)) => (key, existing.append(resolution)),
            None => (path.to_string(), resolution),
        };
        self.command_questionnaire_resolution
            .insert(key, resolution);
    }
}

#[cfg(test)]
mod tests;
