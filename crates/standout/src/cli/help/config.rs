use crate::setup::SetupError;
use crate::{OutputMode, Theme};
use clap::Command;
use console::Style;
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct CommandGroup {
    pub title: String,
    pub help: Option<String>,
    pub commands: Vec<Option<String>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HelpLength {
    #[default]
    Short,
    Long,
}

#[derive(Debug, Clone, Default)]
pub struct HelpConfig {
    pub template: Option<String>,
    pub theme: Option<Theme>,
    pub output_mode: Option<OutputMode>,
    pub command_groups: Option<Vec<CommandGroup>>,
    pub length: HelpLength,
}

pub fn default_help_theme() -> Theme {
    Theme::new()
        .add("header", Style::new().bold())
        .add("item", Style::new().bold())
        .add("metavar", Style::new().bold())
        .add("desc", Style::new())
        .add("default", Style::new().dim())
        .add("values", Style::new().dim())
        .add("usage", Style::new())
        .add("example", Style::new())
        .add("about", Style::new())
}

pub fn validate_command_groups(cmd: &Command, groups: &[CommandGroup]) -> Result<(), SetupError> {
    let known: HashSet<&str> = cmd
        .get_subcommands()
        .filter(|s| !s.is_hide_set())
        .map(|s| s.get_name())
        .collect();

    let mut phantoms = Vec::new();
    for group in groups {
        for name in group.commands.iter().flatten() {
            if !known.contains(name.as_str()) {
                phantoms.push(format!(
                    "group \"{}\": command \"{}\" does not exist",
                    group.title, name
                ));
            }
        }
    }

    if phantoms.is_empty() {
        Ok(())
    } else {
        Err(SetupError::Config(format!(
            "command group validation failed:\n  {}",
            phantoms.join("\n  ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ok() {
        let cmd = Command::new("root")
            .subcommand(Command::new("init"))
            .subcommand(Command::new("list"));

        let groups = vec![CommandGroup {
            title: "Main".into(),
            help: None,
            commands: vec![Some("init".into()), Some("list".into())],
        }];

        assert!(validate_command_groups(&cmd, &groups).is_ok());
    }

    #[test]
    fn test_validate_phantom_reference() {
        let cmd = Command::new("root").subcommand(Command::new("init"));

        let groups = vec![CommandGroup {
            title: "Main".into(),
            help: None,
            commands: vec![Some("init".into()), Some("nonexistent".into())],
        }];

        let err = validate_command_groups(&cmd, &groups).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nonexistent"));
        assert!(msg.contains("does not exist"));
    }

    #[test]
    fn test_validate_ungrouped_commands_ok() {
        let cmd = Command::new("root")
            .subcommand(Command::new("init"))
            .subcommand(Command::new("list"))
            .subcommand(Command::new("extra"));

        let groups = vec![CommandGroup {
            title: "Main".into(),
            help: None,
            commands: vec![Some("init".into())],
        }];

        assert!(validate_command_groups(&cmd, &groups).is_ok());
    }

    #[test]
    fn test_validate_with_separators() {
        let cmd = Command::new("root")
            .subcommand(Command::new("a"))
            .subcommand(Command::new("b"));

        let groups = vec![CommandGroup {
            title: "Main".into(),
            help: None,
            commands: vec![Some("a".into()), None, Some("b".into())],
        }];

        assert!(validate_command_groups(&cmd, &groups).is_ok());
    }

    #[test]
    fn test_validate_hidden_commands_not_checked() {
        let cmd = Command::new("root")
            .subcommand(Command::new("visible"))
            .subcommand(Command::new("hidden").hide(true));

        let groups = vec![CommandGroup {
            title: "Main".into(),
            help: None,
            commands: vec![Some("visible".into()), Some("hidden".into())],
        }];

        let err = validate_command_groups(&cmd, &groups).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("hidden"));
    }
}
