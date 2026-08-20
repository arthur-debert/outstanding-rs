//! Help rendering for clap commands.
//!
//! This module provides styled help output for clap commands using standout templates:
//!
//! - [`render_help`]: Render help for a command (standalone; no `App`)
//! - [`render_help_with_topics`]: Render help with a "Learn More" section listing topics
//! - [`HelpConfig`]: Configuration for help rendering
//! - [`HelpLength`]: Whether a render uses `about` (`-h`) or `long_about` (`--help`)
//! - [`CommandGroup`]: Define subcommand groups for organized help display
//! - [`validate_command_groups`]: Validate group config against a clap Command tree
//! - [`default_help_theme`]: Returns the default theme for help
//!
//! Both standalone functions build a [`crate::RenderRequest`] and call
//! [`crate::render_request`]. Framework help on `App` uses the named
//! `standout/help` registry template registered at `build()`.

mod config;
pub(crate) mod data;
mod render;

pub use config::{
    default_help_theme, validate_command_groups, CommandGroup, HelpConfig, HelpLength,
};
pub(crate) use render::{
    human_help_format, inline_template_ref, named_or_inline_template, render_via_request,
    DEFAULT_HELP_TEMPLATE,
};
pub use render::{render_help, render_help_with_topics};
