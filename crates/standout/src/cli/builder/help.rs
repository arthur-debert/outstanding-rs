use super::App;
use crate::cli::default_command::ParseFailure;
use crate::cli::help::data::extract_help_data;
use crate::cli::help::data::extract_help_data_with_topics;
use crate::cli::help::help_is_a_document;
use crate::cli::help::human_help_format;
use crate::cli::help::named_or_inline_template;
use crate::cli::help::render_help_document;
use crate::cli::help::render_via_request;
use crate::cli::help::HelpConfig;
use crate::cli::help::HelpLength;
use crate::cli::help::DEFAULT_HELP_TEMPLATE;
use crate::cli::result::HelpDisplay;
use crate::cli::result::HelpResult;
use crate::setup::SetupError;
use crate::topics::topic_data;
use crate::topics::topics_list_data;
use crate::topics::DEFAULT_TOPICS_LIST_TEMPLATE;
use crate::topics::DEFAULT_TOPIC_TEMPLATE;
use crate::ColorPolicy;
use crate::RenderError;
use crate::Representation;
use crate::Theme;
use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;
use clap::Command;
use serde::Serialize;

impl App {
    pub fn get_matches_from<I, T>(
        &self,
        cmd: Command,
        itr: I,
        sources: &crate::InputSources,
    ) -> HelpResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        if let Err(error) = self.validated_parse_surface(&cmd) {
            return HelpResult::Error(clap::Error::raw(
                clap::error::ErrorKind::ArgumentConflict,
                format!("{error}\n"),
            ));
        }

        let mut cmd = self.augment_command_with_help(cmd);

        if let Some(error) = self.help_word_collision(&cmd) {
            return HelpResult::Error(clap::Error::raw(
                clap::error::ErrorKind::InvalidSubcommand,
                format!("{error}\n"),
            ));
        }

        let args: Vec<std::ffi::OsString> = itr.into_iter().map(Into::into).collect();

        let matches = match self.parse_with_default_command(&cmd, &args, sources.stdin()) {
            Ok(matches) => matches,
            Err(ParseFailure::UnknownDefault(e)) => {
                return HelpResult::Error(
                    cmd.clone()
                        .error(clap::error::ErrorKind::InvalidSubcommand, e.to_string()),
                )
            }
            Err(ParseFailure::Clap(e)) => {
                let color_policy = self.resolve_color_policy(
                    self.typed_color_from_unparsed(&args),
                    ColorPolicy::Auto,
                    None,
                );
                return match self.intercept_display_help(
                    &mut cmd,
                    &args,
                    &e,
                    None,
                    color_policy,
                    None,
                ) {
                    Some(display) => display.into(),
                    None => HelpResult::Error(e),
                };
            }
        };

        let color_policy =
            self.resolve_color_policy(self.typed_color_policy(&matches), ColorPolicy::Auto, None);
        match self.intercept_help_word(&mut cmd, &matches, None, color_policy, None) {
            Some(display) => display.into(),
            None => HelpResult::Matches(matches),
        }
    }

    pub(crate) fn intercept_help_word(
        &self,
        cmd: &mut Command,
        matches: &ArgMatches,
        target: Option<crate::TargetProperties>,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> Option<HelpDisplay> {
        if !self.help_handling {
            return None;
        }
        let (name, sub_matches) = matches.subcommand()?;
        (name == "help").then(|| {
            self.render_help_word(cmd, matches, sub_matches, target, color_policy, warnings)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn intercept_display_help(
        &self,
        cmd: &mut Command,
        args: &[std::ffi::OsString],
        error: &clap::Error,
        target: Option<crate::TargetProperties>,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> Option<HelpDisplay> {
        (self.help_handling && error.kind() == clap::error::ErrorKind::DisplayHelp).then(|| {
            self.render_help_for_display_help_error(cmd, args, target, color_policy, warnings)
        })
    }

    fn help_target_properties(
        &self,
        target: Option<crate::TargetProperties>,
    ) -> crate::TargetProperties {
        let mut target = target.unwrap_or_else(crate::TargetProperties::detect);
        target.ambiguous_width = self.ambiguous_width;
        target
    }

    fn help_theme(&self) -> Theme {
        self.theme.clone()
    }

    fn help_template(
        &self,
        override_source: Option<&str>,
        named: &str,
        default_source: &str,
    ) -> Result<crate::TemplateRef, RenderError> {
        let theme = self.help_theme();
        if let Some(source) = override_source {
            return crate::cli::help::inline_template_ref(source, &theme, named);
        }
        named_or_inline_template(
            self.template_registry.as_deref(),
            named,
            default_source,
            &theme,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_help_surface<T: Serialize>(
        &self,
        data: &T,
        template: crate::TemplateRef,
        format: Representation,
        target: crate::TargetProperties,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> Result<String, RenderError> {
        render_via_request(
            data,
            template,
            self.help_theme(),
            format,
            color_policy,
            target,
            self.template_engine.clone(),
            self.template_registry.clone(),
            Some(self.context_registry.clone()),
            warnings,
        )
    }

    fn help_display(&self, cmd: &Command, rendered: Result<String, RenderError>) -> HelpDisplay {
        match rendered {
            Ok(text) => HelpDisplay::Rendered { text },
            Err(e) => Self::render_failure(cmd, e),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_help_word(
        &self,
        cmd: &mut Command,
        matches: &ArgMatches,
        sub_matches: &ArgMatches,
        target: Option<crate::TargetProperties>,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let format = self.extract_output_mode(matches);
        let target = self.help_target_properties(target);
        let config = HelpConfig {
            command_groups: self.help_command_groups.clone(),
            length: HelpLength::Long,
            ..Default::default()
        };
        if let Some(topic_args) = sub_matches.get_many::<String>("topic") {
            let keywords: Vec<_> = topic_args.map(|s| s.as_str()).collect();
            if !keywords.is_empty() {
                return self.handle_help_request(
                    cmd,
                    &keywords,
                    config,
                    format,
                    target,
                    color_policy,
                    warnings,
                );
            }
        }

        self.render_root_help(cmd, config, format, target, color_policy, warnings)
    }

    fn render_failure(cmd: &Command, error: impl std::fmt::Display) -> HelpDisplay {
        HelpDisplay::RenderFailed(cmd.clone().error(
            clap::error::ErrorKind::Io,
            format!("failed to render help: {error}"),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_root_help(
        &self,
        cmd: &Command,
        config: HelpConfig,
        format: Representation,
        target: crate::TargetProperties,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        if help_is_a_document(format) {
            return self.help_document(cmd, &[], config.length, format);
        }
        let template = match self.help_template(
            config.template.as_deref(),
            crate::assets::HELP_TEMPLATE_NAME,
            DEFAULT_HELP_TEMPLATE,
        ) {
            Ok(template) => template,
            Err(e) => return Self::render_failure(cmd, e),
        };
        let data = extract_help_data_with_topics(
            cmd,
            &[],
            &self.registry,
            config.command_groups.as_deref(),
            config.length,
            &target,
        )
        .expect("the root is always at the empty path");
        self.help_display(
            cmd,
            self.render_help_surface(
                &data,
                template,
                human_help_format(format),
                target,
                color_policy,
                warnings,
            ),
        )
    }

    fn help_document(
        &self,
        cmd: &Command,
        path: &[&str],
        length: HelpLength,
        format: Representation,
    ) -> HelpDisplay {
        match render_help_document(cmd, path, length, format) {
            Ok(Some(text)) => HelpDisplay::Rendered { text },
            Ok(None) => HelpDisplay::Clap(cmd.clone().error(
                clap::error::ErrorKind::InvalidSubcommand,
                format!("The subcommand '{}' wasn't recognized", path.join(" ")),
            )),
            Err(e) => Self::render_failure(cmd, e),
        }
    }

    fn render_help_for_display_help_error(
        &self,
        cmd: &mut Command,
        args: &[std::ffi::OsString],
        target: Option<crate::TargetProperties>,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let request = Self::help_request(cmd, args);
        let format = self.extract_output_mode_from_unparsed(args);
        let target = self.help_target_properties(target);
        let config = HelpConfig {
            command_groups: self.help_command_groups.clone(),
            length: request.length,
            ..Default::default()
        };

        if request.target.is_empty() {
            return self.render_root_help(cmd, config, format, target, color_policy, warnings);
        }

        let keywords: Vec<&str> = request.target.iter().map(|s| s.as_str()).collect();
        self.handle_help_request(
            cmd,
            &keywords,
            config,
            format,
            target,
            color_policy,
            warnings,
        )
    }

    fn help_request(cmd: &Command, args: &[std::ffi::OsString]) -> HelpRequest {
        HelpRequest {
            target: Self::help_target(cmd, args),
            length: Self::help_length(cmd, args),
        }
    }

    fn help_length(cmd: &Command, args: &[std::ffi::OsString]) -> HelpLength {
        let probe = cmd
            .clone()
            .disable_help_flag(true)
            .ignore_errors(true)
            .arg(
                Arg::new(HELP_PROBE_SHORT)
                    .short('h')
                    .action(ArgAction::SetTrue)
                    .global(true)
                    .hide(true),
            )
            .arg(
                Arg::new(HELP_PROBE_LONG)
                    .long("help")
                    .action(ArgAction::SetTrue)
                    .global(true)
                    .hide(true),
            );

        match probe.try_get_matches_from(args) {
            Ok(matches) if matches.get_flag(HELP_PROBE_LONG) => HelpLength::Long,
            _ => HelpLength::Short,
        }
    }

    fn help_target(cmd: &Command, args: &[std::ffi::OsString]) -> Vec<String> {
        let Ok(matches) = cmd
            .clone()
            .disable_help_flag(true)
            .ignore_errors(true)
            .try_get_matches_from(args)
        else {
            return Vec::new();
        };

        let mut chain = Vec::new();
        let mut current = &matches;
        while let Some((name, sub)) = current.subcommand() {
            chain.push(name.to_string());
            current = sub;
        }
        chain
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_help_request(
        &self,
        cmd: &mut Command,
        keywords: &[&str],
        config: HelpConfig,
        format: Representation,
        target: crate::TargetProperties,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let sub_name = keywords[0];
        let page_format = human_help_format(format);

        if sub_name == "topics" {
            let template = match self.help_template(
                None,
                crate::assets::TOPICS_LIST_TEMPLATE_NAME,
                DEFAULT_TOPICS_LIST_TEMPLATE,
            ) {
                Ok(template) => template,
                Err(e) => return Self::render_failure(cmd, e),
            };
            let data =
                topics_list_data(&self.registry, &format!("{} help", cmd.get_name()), &target);
            return self.help_display(
                cmd,
                self.render_help_surface(
                    &data,
                    template,
                    page_format,
                    target,
                    color_policy,
                    warnings,
                ),
            );
        }

        if crate::cli::app::find_subcommand_recursive(cmd, keywords).is_some() {
            if help_is_a_document(format) {
                return self.help_document(cmd, keywords, config.length, format);
            }
            let template = match self.help_template(
                config.template.as_deref(),
                crate::assets::HELP_TEMPLATE_NAME,
                DEFAULT_HELP_TEMPLATE,
            ) {
                Ok(template) => template,
                Err(e) => return Self::render_failure(cmd, e),
            };
            if let Some(data) = extract_help_data(
                cmd,
                keywords,
                config.command_groups.as_deref(),
                config.length,
                &target,
            ) {
                return self.help_display(
                    cmd,
                    self.render_help_surface(
                        &data,
                        template,
                        page_format,
                        target,
                        color_policy,
                        warnings,
                    ),
                );
            }
        }

        if let Some(topic) = self.registry.get_topic(sub_name) {
            let template = match self.help_template(
                None,
                crate::assets::TOPIC_TEMPLATE_NAME,
                DEFAULT_TOPIC_TEMPLATE,
            ) {
                Ok(template) => template,
                Err(e) => return Self::render_failure(cmd, e),
            };
            return self.help_display(
                cmd,
                self.render_help_surface(
                    &topic_data(topic),
                    template,
                    page_format,
                    target,
                    color_policy,
                    warnings,
                ),
            );
        }

        let err = cmd.error(
            clap::error::ErrorKind::InvalidSubcommand,
            format!("The subcommand or topic '{}' wasn't recognized", sub_name),
        );
        HelpDisplay::Clap(err)
    }

    pub fn augment_command_with_help(&self, cmd: Command) -> Command {
        let cmd = self.augment_framework_surface(cmd);

        if !self.help_handling {
            return cmd;
        }

        let cmd = cmd.disable_help_subcommand(true);
        if self.installs_help_word(&cmd) {
            let has_subcommands = cmd.get_subcommands().next().is_some();
            cmd.subcommand(help_word_command(has_subcommands))
                .subcommand_negates_reqs(true)
        } else {
            cmd
        }
    }

    pub(crate) fn help_word_collision(&self, augmented: &Command) -> Option<SetupError> {
        if !self.help_handling {
            return None;
        }
        let claims = augmented
            .get_subcommands()
            .filter(|sub| claims_help(sub))
            .count();
        (claims > 1).then(|| duplicate_help_word(DECLARED_CLAIM))
    }

    pub(crate) fn installs_help_word(&self, cmd: &Command) -> bool {
        self.help_word
            || cmd.get_subcommands().next().is_some()
            || cmd.get_positionals().next().is_none()
    }
}

fn claims_help(cmd: &Command) -> bool {
    cmd.get_name() == "help" || cmd.get_all_aliases().any(|alias| alias == "help")
}

pub(super) fn claims_root_help(path: &str) -> bool {
    path == "help" || path.starts_with("help.")
}

const DECLARED_CLAIM: &str =
    "this application's clap `Command` declares `help` (as a subcommand name or alias)";

pub(super) fn registered_claim(path: &str) -> String {
    if path == "help" {
        "this application registers a `help` command".to_string()
    } else {
        format!("this application registers `{path}`, hanging a command off the same root word")
    }
}

pub(super) fn duplicate_help_word(claim: &str) -> SetupError {
    SetupError::DuplicateCommand(format!(
        "help — {claim}, and standout installs a `help` word of its own, since help \
         handling is on by default. Rename the application's command, or call \
         .help_handling(false) to keep the name (help is then clap's own, and \
         command_groups and topics become unavailable)"
    ))
}

const HELP_PROBE_SHORT: &str = "__standout_help_short";

const HELP_PROBE_LONG: &str = "__standout_help_long";

#[derive(Debug, Default, PartialEq, Eq)]
struct HelpRequest {
    target: Vec<String>,
    length: HelpLength,
}

fn help_word_command(has_subcommands: bool) -> Command {
    let (about, topic_help) = if has_subcommands {
        (
            "Print this message or the help of the given subcommand(s)",
            "The subcommand or topic to print help for",
        )
    } else {
        ("Print this message", "The topic to print help for")
    };

    Command::new("help").about(about).arg(
        Arg::new("topic")
            .action(ArgAction::Set)
            .num_args(1..)
            .help(topic_help),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn probe_command() -> Command {
        Command::new("app")
            .arg(Arg::new("out").short('o').long("out"))
            .arg(Arg::new("verbose").short('v').action(ArgAction::SetTrue))
            .arg(Arg::new("range"))
            .subcommand(Command::new("build").arg(Arg::new("target")))
    }

    fn request(args: &[&str]) -> HelpRequest {
        let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
        App::help_request(&probe_command(), &args)
    }

    #[test]
    fn test_help_request_reads_the_spelling() {
        assert_eq!(request(&["app", "--help"]).length, HelpLength::Long);
        assert_eq!(request(&["app", "-h"]).length, HelpLength::Short);
    }

    #[test]
    fn test_help_request_reads_the_target_command() {
        let deep = request(&["app", "build", "--help"]);
        assert_eq!(deep.target, vec!["build".to_string()]);
        assert_eq!(deep.length, HelpLength::Long);

        assert!(request(&["app", "--help"]).target.is_empty());
    }

    #[test]
    fn test_help_request_separates_the_spelling_from_the_target() {
        let early = request(&["app", "--help", "build"]);
        assert!(
            early.target.is_empty(),
            "the walk must stop at the flag, got {:?}",
            early.target
        );
        assert_eq!(early.length, HelpLength::Long);

        let short = request(&["app", "-h", "build"]);
        assert!(short.target.is_empty());
        assert_eq!(short.length, HelpLength::Short);
    }

    #[test]
    fn test_help_request_reads_short_flag_clusters() {
        assert_eq!(request(&["app", "-vh"]).length, HelpLength::Short);
    }

    #[test]
    fn test_help_request_reads_inline_values() {
        assert_eq!(
            request(&["app", "--out=x", "--help"]).length,
            HelpLength::Long
        );
    }

    #[test]
    fn test_help_request_does_not_mistake_an_option_value_for_a_flag() {
        assert_eq!(request(&["app", "-o", "h"]).length, HelpLength::Short);
        assert!(request(&["app", "-o", "h"]).target.is_empty());
    }

    #[test]
    fn test_help_request_respects_the_terminator() {
        assert_eq!(request(&["app", "--", "--help"]).length, HelpLength::Short);
    }

    #[test]
    fn test_help_request_defaults_to_the_root_and_short() {
        assert_eq!(request(&["app"]), HelpRequest::default());
    }
}
