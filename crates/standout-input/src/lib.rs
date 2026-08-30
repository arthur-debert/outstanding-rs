//! Declarative input collection for CLI applications.
//!
//! Sources implement [`InputCollector`] and compose into an [`InputChain`]
//! that tries each in order until one resolves. [`InputSources`] is the
//! explicit stdin/clipboard/prompt-responder for one invocation — production
//! constructs it from the real process, tests put mocks in the same type;
//! there is no process-global default-reader override.
//!
//! Feature flags: `editor` (default) enables [`EditorCollector`];
//! `simple-prompts` (default) enables basic terminal prompts; `inquire`
//! enables rich TUI prompts.
//!
//! The [`questionnaire`] module renders an application-defined questionnaire
//! as an editable prose answer sheet, collects answers from prompts, a file,
//! or stdin, and decodes every submission through one validation pipeline
//! keyed by stable identity. See its module doc for the format and
//! compatibility contract.

mod chain;
mod collector;
pub mod env;
mod error;
mod input_sources;
mod inputs;
pub mod questionnaire;
mod responder;
pub mod sources;

pub use chain::InputChain;
pub use collector::{InputCollector, InputSourceKind, ResolvedInput};
pub use error::InputError;
pub use input_sources::InputSources;
pub use inputs::{Inputs, MissingInput};
pub use responder::{
    PromptContext, PromptKind, PromptResponder, PromptResponse, ScriptedResponder,
};

pub use sources::{
    read_if_piped, read_if_piped_from, ArgSource, ClipboardSource, DefaultSource, EnvSource,
    FlagSource, StdinSource,
};

#[cfg(feature = "editor")]
pub use sources::{EditorRunner, EditorSource, MockEditorResult, MockEditorRunner};

#[cfg(feature = "simple-prompts")]
pub use sources::{ConfirmPromptSource, MockTerminal, TerminalIO, TextPromptSource};

#[cfg(feature = "inquire")]
pub use sources::{
    InquireConfirm, InquireEditor, InquireMultiSelect, InquirePassword, InquireSelect, InquireText,
};

pub use env::{MockClipboard, MockEnv, MockStdin};

pub use env::{RealClipboard, RealStdin};
