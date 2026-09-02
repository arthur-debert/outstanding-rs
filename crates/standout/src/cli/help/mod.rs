mod config;
pub(crate) mod data;
mod document;
mod render;

pub use config::{
    default_help_theme, validate_command_groups, CommandGroup, HelpConfig, HelpLength,
};
pub use document::{HelpArg, HelpDocument, HelpSubcommand};
pub(crate) use render::{
    help_is_a_document, human_help_format, inline_template_ref, named_or_inline_template,
    render_help_document, render_via_request, DEFAULT_HELP_TEMPLATE,
};
pub use render::{render_help, render_help_with_topics};
