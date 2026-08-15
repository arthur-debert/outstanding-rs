//! Help rendering for clap commands.
//!
//! This module provides styled help output for clap commands using standout templates:
//!
//! - [`render_help`]: Render help for a command
//! - [`render_help_with_topics`]: Render help with a "Learn More" section listing topics
//! - [`HelpConfig`]: Configuration for help rendering
//! - [`HelpLength`]: Whether a render uses `about` (`-h`) or `long_about` (`--help`)
//! - [`CommandGroup`]: Define subcommand groups for organized help display
//! - [`validate_command_groups`]: Validate group config against a clap Command tree
//! - [`default_help_theme`]: Returns the default theme for help

mod config;
pub(crate) mod data;
mod render;

pub use config::{
    default_help_theme, validate_command_groups, CommandGroup, HelpConfig, HelpLength,
};
pub use render::{render_help, render_help_with_topics};
