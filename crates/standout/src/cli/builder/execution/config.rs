use super::CONFIG_OVERRIDE_ARG;
use crate::cli::builder::App;
use crate::cli::config::config_command;
use crate::cli::config::config_result_output;
use crate::cli::config::config_run_error;
use crate::cli::config::parse_override_pair;
use crate::cli::config::ResolvedConfig;
use crate::cli::config::CONFIG_COMMAND;
use crate::cli::dispatch::extract_command_path;
use crate::cli::dispatch::get_deepest_matches;
use crate::cli::dispatch::render_handler_output;
use crate::cli::handler::DispatchResult;
use crate::cli::handler::RunError;
use crate::cli::handler::RunErrorKind;
use crate::cli::handler::RunRecorder;
use crate::cli::handler::StreamSink;
use crate::ColorPolicy;
use crate::InputSources;
use crate::Representation;
use crate::TargetProperties;
use clap::ArgMatches;
use standout_render::warnings::WarningBuffer;
use std::sync::Arc;

impl App {
    pub(super) fn config_command_action(
        &self,
        matches: &ArgMatches,
    ) -> Option<Result<clapfig::ConfigAction, clapfig::ClapfigError>> {
        if !self.installs_config_command() {
            return None;
        }
        let (name, sub_matches) = matches.subcommand()?;
        (name == CONFIG_COMMAND).then(|| config_command().parse(sub_matches))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_config_command(
        &self,
        action: Result<clapfig::ConfigAction, clapfig::ClapfigError>,
        path: Vec<String>,
        matches: &ArgMatches,
        output_mode: Representation,
        color_policy: ColorPolicy,
        target: TargetProperties,
        sources: InputSources,
        sink: &StreamSink,
        recorder: &RunRecorder,
        warnings: &WarningBuffer,
    ) -> DispatchResult {
        let seam = self
            .config
            .as_ref()
            .expect("the config command is installed only beside a config seam");
        let overrides = match self.config_overrides(matches) {
            Ok(overrides) => overrides,
            Err(error) => return DispatchResult::Error(error),
        };
        let result = match action.and_then(|action| seam.handle(&action, &overrides)) {
            Ok(result) => result,
            Err(error) => return DispatchResult::Error(config_run_error(error)),
        };
        let override_path = self.output_file_override(matches);
        let mut ctx = match self.command_context(
            path,
            output_mode,
            color_policy,
            override_path.as_deref(),
            sink,
            recorder,
            warnings,
        ) {
            Ok(ctx) => ctx,
            Err(error) => return DispatchResult::Error(error),
        };
        ctx.extensions.insert(sources);
        let sub_matches = get_deepest_matches(matches);
        let (output, template) = config_result_output(result, output_mode);
        let dispatch_output = match render_handler_output(
            Ok(output),
            sub_matches,
            &ctx,
            recorder,
            None,
            &template,
            &self.theme,
            &self.context_registry,
            &self.template_engine,
            self.template_registry.as_ref(),
            output_mode,
            color_policy,
            None,
            target,
            None,
        ) {
            Ok(output) => output,
            Err(error) => return DispatchResult::Error(error),
        };
        self.present_dispatch_output(
            dispatch_output,
            None,
            sub_matches,
            &ctx,
            output_mode,
            false,
            override_path,
            sink,
            warnings,
        )
    }

    pub(super) fn resolve_config_for(
        &self,
        matches: &ArgMatches,
    ) -> Result<Option<ResolvedConfig>, RunError> {
        let path = extract_command_path(matches).join(".");
        if !self.get_commands().contains_key(&path) || self.config_exempt_commands.contains(&path) {
            return Ok(None);
        }
        self.resolve_config(matches)
    }

    pub(crate) fn resolve_config(
        &self,
        matches: &ArgMatches,
    ) -> Result<Option<ResolvedConfig>, RunError> {
        let Some(seam) = self.config.as_ref() else {
            return Ok(None);
        };
        let overrides = self.config_overrides(matches)?;
        let dir = std::env::current_dir()
            .map_err(|error| RunError::config(error.to_string(), Arc::new(error)))?;
        seam.resolve_at(&overrides, &dir)
            .map(Some)
            .map_err(config_run_error)
    }

    fn config_overrides(&self, matches: &ArgMatches) -> Result<Vec<(String, String)>, RunError> {
        match matches.try_get_many::<String>(CONFIG_OVERRIDE_ARG) {
            Ok(Some(pairs)) => pairs
                .map(|pair| parse_override_pair(pair))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|message| RunError::new(message, RunErrorKind::ClapUsage)),
            _ => Ok(Vec::new()),
        }
    }
}
