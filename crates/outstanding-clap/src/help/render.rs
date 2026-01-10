//! Help rendering functions.

use clap::Command;
use outstanding::topics::TopicRegistry;
use outstanding::{render_with_output, OutputMode, ThemeChoice};

use super::config::{default_help_theme, HelpConfig};
use super::data::{extract_help_data, extract_help_data_with_topics};

/// Renders the help for a clap command using outstanding.
pub fn render_help(
    cmd: &Command,
    config: Option<HelpConfig>,
) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("template.txt"));

    let theme = config.theme.unwrap_or_else(default_help_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let data = extract_help_data(cmd);

    render_with_output(template, &data, ThemeChoice::from(&theme), mode)
}

/// Renders the help for a clap command with topics in a "Learn More" section.
pub fn render_help_with_topics(
    cmd: &Command,
    registry: &TopicRegistry,
    config: Option<HelpConfig>,
) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("template.txt"));

    let theme = config.theme.unwrap_or_else(default_help_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let data = extract_help_data_with_topics(cmd, registry);

    render_with_output(template, &data, ThemeChoice::from(&theme), mode)
}
