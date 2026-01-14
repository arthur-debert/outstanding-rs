//! Help rendering configuration.

use crate::{OutputMode, Theme};
use console::Style;

/// Configuration for clap help rendering.
#[derive(Debug, Clone, Default)]
pub struct HelpConfig {
    /// Custom template string. If None, uses the default template.
    pub template: Option<String>,
    /// Custom theme. If None, uses the default theme.
    pub theme: Option<Theme>,
    /// Output mode. If None, uses Auto (auto-detects).
    pub output_mode: Option<OutputMode>,
}

/// Returns the default theme for help rendering.
pub fn default_help_theme() -> Theme {
    Theme::new()
        .add("header", Style::new().bold())
        .add("item", Style::new().bold())
        .add("desc", Style::new())
        .add("usage", Style::new())
        .add("example", Style::new())
        .add("about", Style::new())
}
