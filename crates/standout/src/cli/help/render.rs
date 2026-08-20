//! Help rendering functions.

use crate::topics::TopicRegistry;
use crate::{render_with_output, OutputMode, RenderError, Theme};
use clap::Command;

use super::config::{default_help_theme, HelpConfig};
use super::data::{extract_help_data, extract_help_data_with_topics};

/// Resolves the theme a help render styles with.
///
/// [`default_help_theme`] is the base and the configured theme overlays it —
/// per style name, a configured entry wins. The builder populates the config
/// with the *application* theme, which carries the app's output vocabulary,
/// not help's; replacing the default with it would leave every help tag
/// unresolved and render as literal `[tag?]` markup in terminal output.
/// Overlaying keeps deliberate restyling working while the tags an app
/// leaves alone still resolve.
fn resolve_help_theme(configured: Option<Theme>) -> Theme {
    match configured {
        Some(theme) => default_help_theme().merge(theme),
        None => default_help_theme(),
    }
}

/// Renders the help for a clap command using standout.
pub fn render_help(cmd: &Command, config: Option<HelpConfig>) -> Result<String, RenderError> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("template.txt"));

    let theme = resolve_help_theme(config.theme);
    let mode = human_help_mode(config.output_mode.unwrap_or(OutputMode::Auto));

    let data = extract_help_data(cmd, config.command_groups.as_deref(), config.length);

    render_with_output(template, &data, &theme, mode)
}

/// ADR-0029: structured `--output` still prints human help.
fn human_help_mode(mode: OutputMode) -> OutputMode {
    if mode.is_structured() {
        OutputMode::Auto
    } else {
        mode
    }
}

/// Renders the help for a clap command with topics in a "Learn More" section.
pub fn render_help_with_topics(
    cmd: &Command,
    registry: &TopicRegistry,
    config: Option<HelpConfig>,
) -> Result<String, RenderError> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("template.txt"));

    let theme = resolve_help_theme(config.theme);
    let mode = human_help_mode(config.output_mode.unwrap_or(OutputMode::Auto));

    let data = extract_help_data_with_topics(
        cmd,
        registry,
        config.command_groups.as_deref(),
        config.length,
    );

    render_with_output(template, &data, &theme, mode)
}
