//! Declarative input collection for CLI applications.
//!
//! `standout-input` provides a unified way to acquire user input from multiple
//! sources—CLI arguments, stdin, environment variables, editors, and interactive
//! prompts—with automatic fallback chains.
//!
//! # Quick Start
//!
//! ```ignore
//! use standout_input::{InputChain, ArgSource, StdinSource, DefaultSource};
//!
//! // Try argument first, then piped stdin, then default
//! let message = InputChain::<String>::new()
//!     .try_source(ArgSource::new("message"))
//!     .try_source(StdinSource::new())
//!     .default("default message".to_string())
//!     .resolve(&matches)?;
//! ```
//!
//! # Features
//!
//! - **`editor`** (default) - Enable [`EditorCollector`] for editor-based input
//! - **`simple-prompts`** (default) - Enable basic terminal prompts
//! - **`inquire`** - Enable rich TUI prompts via the inquire crate
//!
//! [`InputSources`] is the explicit stdin, clipboard, and prompt-responder for
//! one invocation. Production constructs it from the real process; tests put
//! mocks in the same type. Input collection takes it as an argument; there is
//! no process-global default-reader override.
//!
//! # Architecture
//!
//! The crate is built around the [`InputCollector`] trait, which all input
//! sources implement. Sources are composed into [`InputChain`]s that try each
//! source in order until one provides input.
//!
//! ```text
//! InputChain
//! ├── ArgSource      → None (not provided)
//! ├── StdinSource    → None (not piped)
//! ├── EditorSource   → Some("user input") ← returns this
//! └── DefaultSource  → (not reached)
//! ```
//!
//! # Questionnaire answer sheets
//!
//! The [`questionnaire`] module renders an application-defined questionnaire
//! — scalar fields plus nested and repeatable groups — as an editable prose
//! answer sheet, collects answers interactively or from a named file or
//! explicit stdin, and decodes every submission through one shared
//! validation pipeline keyed by stable identity (with indexed occurrence
//! paths for repeated items). See the module documentation for the format,
//! the application/library ownership boundary, and the exact-match
//! compatibility contract.
//!
//! # Testing
//!
//! All sources accept mock implementations for testing:
//!
//! ```
//! use standout_input::{StdinSource, env::MockStdin};
//!
//! // Test with simulated piped input
//! let source = StdinSource::with_reader(MockStdin::piped("test input"));
//! ```

mod chain;
mod collector;
pub mod env;
mod error;
mod input_sources;
mod inputs;
pub mod questionnaire;
mod responder;
pub mod sources;

// Re-export core types
pub use chain::InputChain;
pub use collector::{InputCollector, InputSourceKind, ResolvedInput};
pub use error::InputError;
pub use input_sources::InputSources;
pub use inputs::{Inputs, MissingInput};
pub use responder::{
    PromptContext, PromptKind, PromptResponder, PromptResponse, ScriptedResponder,
};

// Re-export sources at crate root for convenience
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

// Re-export mock types for testing
pub use env::{MockClipboard, MockEnv, MockStdin};

// Re-export real process readers for callers constructing [`InputSources`].
pub use env::{RealClipboard, RealStdin};
