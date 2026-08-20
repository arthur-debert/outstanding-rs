//! Input source implementations.
//!
//! This module contains the built-in input sources:
//!
//! - [`ArgSource`] - Read from CLI arguments
//! - [`FlagSource`] - Read from CLI flags
//! - [`StdinSource`] - Read from piped stdin
//! - [`EnvSource`] - Read from environment variables
//! - [`ClipboardSource`] - Read from system clipboard
//! - [`DefaultSource`] - Provide a fallback value
//! - [`EditorSource`] - Read from external text editor (requires `editor` feature)

mod arg;
mod clipboard;
mod default;
mod env;
mod stdin;

#[cfg(feature = "editor")]
mod editor;

#[cfg(feature = "simple-prompts")]
mod prompt;

#[cfg(feature = "inquire")]
mod inquire_adapters;

pub use arg::{ArgSource, FlagSource};
pub use clipboard::ClipboardSource;
pub use default::DefaultSource;
pub use env::EnvSource;
pub use stdin::{read_if_piped, read_if_piped_from, StdinSource};

#[cfg(feature = "editor")]
pub use editor::{EditorRunner, EditorSource, MockEditorResult, MockEditorRunner};

#[cfg(feature = "simple-prompts")]
pub use prompt::{ConfirmPromptSource, MockTerminal, RealTerminal, TerminalIO, TextPromptSource};

#[cfg(feature = "inquire")]
pub use inquire_adapters::{
    InquireConfirm, InquireEditor, InquireMultiSelect, InquirePassword, InquireSelect, InquireText,
};
