use super::CONFIG_OVERRIDE_ARG;
use crate::cli::builder::output_mode_flag_spelling;
use crate::cli::builder::App;
use crate::cli::builder::COLOR_ARG;
use crate::cli::builder::COLOR_FLAG_DEFAULT;
use crate::cli::builder::COLOR_FLAG_VALUES;
use crate::cli::builder::NO_PAGER_ARG;
use crate::cli::builder::OUTPUT_FILE_ARG;
use crate::cli::builder::OUTPUT_MODE_ARG;
use crate::cli::builder::OUTPUT_MODE_FLAG_VALUES;
use crate::cli::config::config_command_tree;
use crate::cli::questionnaire::augment_questionnaire_command;
use crate::cli::questionnaire::validate_questionnaire_surface;
use crate::SetupError;
use clap::Arg;
use clap::ArgAction;
use clap::Command;

impl App {
    pub(crate) fn built_clone(&self, cmd: &Command) -> Command {
        let mut built = cmd.clone();
        if let Some(version) = &self.version {
            built = built.version(version.clone());
        }
        built.build();
        built
    }

    pub(crate) fn config_override_flag_collision(&self, cmd: &Command) -> Result<(), SetupError> {
        let Some(flag) = self.config_override_flag.as_deref() else {
            return Ok(());
        };
        // Generated `--help`/`--version` only exist per command once clap builds the tree.
        let built = matches!(flag, "help" | "version").then(|| self.built_clone(cmd));
        if command_takes_flag(built.as_ref().unwrap_or(cmd), flag) {
            return Err(SetupError::Config(format!(
                "config_override_flag(\"{flag}\") is already taken by this application's clap Command"
            )));
        }
        Ok(())
    }

    /// Rejects a framework flag whose long name a command in the tree already
    /// declares. clap only catches the duplicate with a debug assertion, so a
    /// release build would ship a flag answering to one of the two definitions.
    pub(crate) fn framework_flag_collision(&self, cmd: &Command) -> Result<(), SetupError> {
        let installed = [
            (
                "output_flag",
                "no_output_flag()",
                self.output_flag.as_deref(),
            ),
            (
                "output_file_flag",
                "no_output_file_flag()",
                self.output_file_flag.as_deref(),
            ),
            ("color_flag", "no_color_flag()", self.color_flag.as_deref()),
            ("pager_flag", "no_pager_flag()", self.pager_flag.as_deref()),
        ];
        // Generated `--help`/`--version` only exist per command once clap builds
        // the tree, which is also what honors a command that turns one off.
        let built = installed
            .iter()
            .any(|(_, _, flag)| matches!(*flag, Some("help" | "version")))
            .then(|| self.built_clone(cmd));
        for (seam, removal, flag) in installed {
            let Some(flag) = flag else { continue };
            let searched = match flag {
                "help" | "version" => built.as_ref().unwrap_or(cmd),
                _ => cmd,
            };
            if let Some(owner) = command_declaring_long(searched, flag, &[]) {
                return Err(SetupError::Config(format!(
                    "{seam} installs `--{flag}`, which this application already declares on \
                     `{owner}`. Rename standout's with {seam}(Some(\"...\")), drop it with \
                     {removal}, or rename the application's own flag"
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn augment_framework_surface(&self, mut cmd: Command) -> Command {
        self.augment_questionnaire_commands(&mut cmd, &[]);

        if let Some(version) = &self.version {
            cmd = cmd.version(version.clone());
        }

        if let Some(ref flag_name) = self.output_flag {
            let mut arg = Arg::new(OUTPUT_MODE_ARG)
                .long(flag_name.clone())
                .value_name("MODE")
                .global(true)
                .value_parser(OUTPUT_MODE_FLAG_VALUES)
                .help("Structured output encoding");
            if let Some(spelling) = output_mode_flag_spelling(self.output_mode_fallback) {
                arg = arg.default_value(spelling);
            }
            cmd = cmd.arg(arg);
        }

        if let Some(ref flag_name) = self.color_flag {
            cmd = cmd.arg(
                Arg::new(COLOR_ARG)
                    .long(flag_name.clone())
                    .value_name("WHEN")
                    .global(true)
                    .value_parser(COLOR_FLAG_VALUES)
                    .default_value(COLOR_FLAG_DEFAULT)
                    .help("When to color human output"),
            );
        }

        if let Some(ref flag_name) = self.pager_flag {
            cmd = cmd.arg(
                Arg::new(NO_PAGER_ARG)
                    .long(flag_name.clone())
                    .global(true)
                    .action(ArgAction::SetTrue)
                    .help("Do not page the output"),
            );
        }

        if let Some(ref flag_name) = self.output_file_flag {
            cmd = cmd.arg(
                Arg::new(OUTPUT_FILE_ARG)
                    .long(flag_name.clone())
                    .value_name("PATH")
                    .global(true)
                    .action(ArgAction::Set)
                    .help("Write output to file instead of stdout"),
            );
        }

        if let Some(ref flag_name) = self.config_override_flag {
            cmd = cmd.arg(
                Arg::new(CONFIG_OVERRIDE_ARG)
                    .long(flag_name.clone())
                    .value_name("KEY=VALUE")
                    .global(true)
                    .action(ArgAction::Append)
                    .help("Override a configuration value"),
            );
        }

        if self.installs_config_command() {
            cmd = cmd.subcommand(config_command_tree());
        }

        cmd
    }

    fn augment_questionnaire_commands(&self, cmd: &mut Command, path: &[String]) {
        let path_str = path.join(".");
        if self.questionnaire_commands.contains_key(&path_str) {
            *cmd = augment_questionnaire_command(cmd.clone());
        }

        for subcommand in cmd.get_subcommands_mut() {
            let mut child_path = path.to_vec();
            child_path.push(subcommand.get_name().to_string());
            self.augment_questionnaire_commands(subcommand, &child_path);
        }
    }

    /// A blank name in a `.`-separated path can never match what dispatch joins from clap.
    pub(crate) fn malformed_registrations(&self) -> Result<(), SetupError> {
        let pending = self.pending_commands.borrow();
        let mut malformed: Vec<&str> = pending
            .keys()
            .chain(self.questionnaire_commands.keys())
            .map(String::as_str)
            .filter(|path| !path.is_empty() && path.split('.').any(str::is_empty))
            .collect();
        malformed.sort_unstable();
        malformed.dedup();

        let Some(path) = malformed.first() else {
            return Ok(());
        };

        Err(SetupError::Config(format!(
            "Registration path `{path}` has a blank command name: a path is \
             `.`-separated command names, and only the empty path names \
             something (the root command of a flat app). Drop the leading, \
             trailing or doubled `.`."
        )))
    }

    /// A registration with no clap subcommand behind it; canonical names only, never aliases.
    pub(crate) fn unreachable_registrations(&self, cmd: &Command) -> Result<(), SetupError> {
        let mut unreachable: Vec<String> = self
            .pending_commands
            .borrow()
            .keys()
            .filter(|path| {
                crate::cli::app::find_canonical_subcommand_recursive(cmd, &path_segments(path))
                    .is_none()
            })
            .cloned()
            .collect();
        unreachable.sort();

        let Some(path) = unreachable.first() else {
            return Ok(());
        };

        let hint = match declared_variant_in(cmd, path) {
            Some(DeclaredAs::SeparatorVariant(declared)) => format!(
                " The CLI declares `{}` — a registered name must match the CLI \
                 definition exactly (clap's derive names subcommands in kebab-case).",
                declared.replace('.', " "),
            ),
            Some(DeclaredAs::Alias(declared)) => format!(
                " The CLI declares `{}` and accepts `{}` as an alias for it — clap \
                 reports the declared name to dispatch, so register the handler under \
                 `{}`.",
                declared.replace('.', " "),
                path.replace('.', " "),
                declared.replace('.', " "),
            ),
            None => " Register the handler under a name the CLI declares, or drop the \
                     registration."
                .to_string(),
        };

        Err(SetupError::Config(format!(
            "No invocation reaches `{}`: this application registers a handler for it, \
             but its clap `Command` declares no such subcommand.{hint}",
            path.replace('.', " "),
        )))
    }

    pub(crate) fn validated_command_tree(&self, cmd: &Command) -> Result<Command, SetupError> {
        self.malformed_registrations()?;
        let propagated = self.validated_parse_surface(cmd)?;
        self.unreachable_registrations(cmd)?;
        Ok(propagated)
    }

    pub(crate) fn validated_parse_surface(&self, cmd: &Command) -> Result<Command, SetupError> {
        let propagated = crate::cli::app::with_globals_propagated(cmd);
        self.validate_questionnaire_surfaces(&propagated)?;
        self.config_override_flag_collision(cmd)?;
        self.framework_flag_collision(cmd)?;
        self.config_command_collision(cmd)?;
        Ok(propagated)
    }

    pub(crate) fn validate_questionnaire_surfaces(&self, cmd: &Command) -> Result<(), SetupError> {
        for path in self.questionnaire_commands.keys() {
            let parts = path.split('.').collect::<Vec<_>>();
            let Some(command) = crate::cli::app::find_canonical_subcommand_recursive(cmd, &parts)
            else {
                continue;
            };
            validate_questionnaire_surface(command, path)?;
        }
        Ok(())
    }
}

/// The empty path is the root command and yields no segments.
fn path_segments(path: &str) -> Vec<&str> {
    if path.is_empty() {
        return Vec::new();
    }
    path.split('.').collect()
}

/// The declared spelling an unreachable path matches modulo `-`/`_`, or by alias.
enum DeclaredAs {
    SeparatorVariant(String),
    Alias(String),
}

fn declared_variant_in(cmd: &Command, path: &str) -> Option<DeclaredAs> {
    let mut current = cmd;
    let mut declared = Vec::new();
    let mut through_alias = false;

    for segment in path_segments(path) {
        let wanted = segment.replace('-', "_");
        let sub = current
            .get_subcommands()
            .find(|sub| sub.get_name().replace('-', "_") == wanted)
            .or_else(|| {
                through_alias = true;
                current
                    .get_subcommands()
                    .find(|sub| sub.get_aliases().any(|alias| alias == segment))
            })?;
        declared.push(sub.get_name().to_string());
        current = sub;
    }

    let declared = declared.join(".");
    Some(if through_alias {
        DeclaredAs::Alias(declared)
    } else {
        DeclaredAs::SeparatorVariant(declared)
    })
}

const FRAMEWORK_ARG_IDS: [&str; 5] = [
    OUTPUT_MODE_ARG,
    OUTPUT_FILE_ARG,
    COLOR_ARG,
    NO_PAGER_ARG,
    CONFIG_OVERRIDE_ARG,
];

/// The path of the first command in the tree declaring `flag`, as its own long
/// invocation name or as one of its arguments' long names or aliases.
fn command_declaring_long(cmd: &Command, flag: &str, path: &[&str]) -> Option<String> {
    let mut here: Vec<&str> = path.to_vec();
    here.push(cmd.get_name());
    let declared = cmd.get_long_flag() == Some(flag)
        || cmd.get_all_long_flag_aliases().any(|alias| alias == flag)
        || cmd.get_arguments().any(|arg| {
            !FRAMEWORK_ARG_IDS.contains(&arg.get_id().as_str())
                && (arg.get_long() == Some(flag)
                    || arg
                        .get_all_aliases()
                        .is_some_and(|aliases| aliases.contains(&flag)))
        });
    if declared {
        return Some(here.join(" "));
    }
    cmd.get_subcommands()
        .find_map(|sub| command_declaring_long(sub, flag, &here))
}

pub(crate) fn command_takes_flag(cmd: &Command, flag: &str) -> bool {
    cmd.get_arguments().any(|arg| {
        arg.get_id() == CONFIG_OVERRIDE_ARG
            || arg.get_long() == Some(flag)
            || arg
                .get_all_aliases()
                .is_some_and(|aliases| aliases.contains(&flag))
    }) || cmd.get_subcommands().any(|sub| {
        sub.get_long_flag() == Some(flag)
            || sub.get_all_long_flag_aliases().any(|alias| alias == flag)
            || command_takes_flag(sub, flag)
    })
}

#[cfg(test)]
mod tests;
