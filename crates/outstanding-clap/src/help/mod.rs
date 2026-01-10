//! Help rendering for clap commands.
//!
//! This module provides styled help output for clap commands using outstanding templates:
//!
//! - [`render_help`]: Render help for a command
//! - [`render_help_with_topics`]: Render help with a "Learn More" section listing topics
//! - [`HelpConfig`]: Configuration for help rendering
//! - [`default_help_theme`]: Returns the default theme for help

mod config;
pub(crate) mod data;
mod render;

pub use config::{default_help_theme, HelpConfig};
pub use render::{render_help, render_help_with_topics};
