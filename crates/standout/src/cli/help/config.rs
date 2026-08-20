//! Help rendering configuration.

use crate::setup::SetupError;
use crate::{OutputMode, Theme};
use clap::Command;
use console::Style;
use std::collections::HashSet;

/// Defines a group of subcommands for help display.
///
/// When provided via [`HelpConfig::command_groups`], subcommands are organized
/// into named sections instead of appearing in a single "Commands" group.
///
/// Use `None` entries in [`commands`](CommandGroup::commands) to insert blank
/// line separators for visual sub-grouping within a section.
#[derive(Debug, Clone, Default)]
pub struct CommandGroup {
    /// Section header (e.g., "Commands", "Per Pad(s)").
    pub title: String,
    /// Optional help text displayed below the title, before the command list.
    pub help: Option<String>,
    /// Command names in display order.
    /// Use `None` to insert a blank line separator between commands.
    pub commands: Vec<Option<String>>,
}

/// Which of clap's two descriptions a help render uses.
///
/// Clap gives a command a terse `about` and an optional full `long_about`, and
/// its convention is that `-h` shows the first and `--help` the second. Help
/// interception has to carry that distinction itself, because the request
/// reaches standout as a `DisplayHelp` error that does not say which spelling
/// raised it — so the invocation is classified from the raw arguments and the
/// answer travels here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HelpLength {
    /// `-h`: the command's `about`.
    ///
    /// The default, so a render with no invocation to classify — a direct
    /// [`render_help`](super::render_help) call — stays terse.
    #[default]
    Short,
    /// `--help` and the `help` word: the command's `long_about`, falling back
    /// to `about` when it declares none.
    Long,
}

/// Configuration for clap help rendering.
#[derive(Debug, Clone, Default)]
pub struct HelpConfig {
    /// Custom template string. If None, uses the default template.
    ///
    /// Standalone [`super::render_help`] carries this as
    /// [`crate::TemplateRef::Inline`] and validates literal style tags against
    /// the resolved theme at request construction (the equivalent of the
    /// ADR-0020 check `build()` runs on named registry templates). Framework
    /// help uses the named `standout/help` registry entry instead.
    pub template: Option<String>,
    /// Theme overlaid on [`default_help_theme`]: per style name an entry
    /// here wins, and tags it leaves undefined keep their default styling.
    /// If None, the default help theme alone is used.
    ///
    /// Framework help on `App` ignores this field and uses the one theme
    /// `build()` merged (ADR-0020).
    pub theme: Option<Theme>,
    /// Output mode. If None, uses Auto (auto-detects).
    ///
    /// Structured modes (`json` / `yaml` / `csv` / `xml`) still print human
    /// help: glue maps them to [`OutputMode::Auto`] on the request
    /// (ADR-0029).
    pub output_mode: Option<OutputMode>,
    /// Subcommand grouping for help display. If None, all subcommands
    /// appear in a single "Commands" group (default behavior).
    pub command_groups: Option<Vec<CommandGroup>>,
    /// Which description to render — see [`HelpLength`]. Defaults to
    /// [`HelpLength::Short`].
    pub length: HelpLength,
}

/// Returns the default theme for help rendering.
///
/// Every render starts from this theme; a configured [`HelpConfig::theme`]
/// — the application theme, on the builder paths — overlays it rather than
/// replacing it, so every tag the template emits always resolves even when
/// the configured theme defines none of them.
///
/// Every surface the template can emit has an entry, including the ones that
/// carry information clap spells with punctuation: standout renders a default
/// and a possible-value set as `[default]`/`[values]` text and leaves the
/// emphasis to the theme, rather than baking `[…]` brackets into the template.
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

/// Validates command groups against the actual clap Command tree.
///
/// Checks for phantom references: command names in groups that don't exist
/// as subcommands in the Command. Ungrouped commands are OK — they will be
/// auto-appended to an "Other" group at render time.
///
/// Call this from a `#[test]` to catch misconfigurations in CI.
///
/// # Example
///
/// ```rust,ignore
/// #[test]
/// fn test_help_groups_match_commands() {
///     let cmd = Cli::command();
///     let groups = my_command_groups();
///     validate_command_groups(&cmd, &groups).unwrap();
/// }
/// ```
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
