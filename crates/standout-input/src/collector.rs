//! Core input collector trait.
//!
//! The [`InputCollector`] trait defines the interface for all input sources.
//! Implementations can be composed into chains with fallback behavior.

use clap::ArgMatches;

use crate::InputError;
use crate::InputSources;

/// A source that can collect input of type T.
///
/// Input collectors are the building blocks of input chains. Each collector
/// represents one way to obtain input: from CLI arguments, stdin, environment
/// variables, editors, or interactive prompts.
///
/// # Implementation Guidelines
///
/// - [`is_available`](Self::is_available) should return `false` if this source
///   cannot provide input in the current environment (e.g., no TTY for prompts,
///   stdin not piped for stdin source).
///
/// - [`collect`](Self::collect) should return `Ok(None)` to indicate "try the
///   next source" and `Ok(Some(value))` when input was successfully collected.
///   Return `Err` only for actual failures.
///
/// - Interactive collectors should implement [`can_retry`](Self::can_retry) to
///   return `true`, allowing validation failures to re-prompt the user.
///
/// # Example
///
/// ```ignore
/// use standout_input::{InputCollector, InputError};
/// use clap::ArgMatches;
///
/// struct FixedValue(String);
///
/// impl InputCollector<String> for FixedValue {
///     fn name(&self) -> &'static str { "fixed" }
///
///     fn is_available(&self, _: &ArgMatches) -> bool { true }
///
///     fn collect(&self, _: &ArgMatches) -> Result<Option<String>, InputError> {
///         Ok(Some(self.0.clone()))
///     }
/// }
/// ```
pub trait InputCollector<T>: Send + Sync {
    /// Human-readable name for this collector.
    ///
    /// Used in error messages and debugging. Examples: "argument", "stdin",
    /// "editor", "prompt".
    fn name(&self) -> &'static str;

    /// Check if this collector can provide input in the current environment.
    ///
    /// Returns `false` if:
    /// - Interactive collector but no TTY available
    /// - Stdin source but stdin is not piped
    /// - Argument source but argument was not provided
    ///
    /// The chain will skip unavailable collectors and try the next one.
    fn is_available(&self, matches: &ArgMatches) -> bool;

    /// Attempt to collect input from this source.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(value))` - Input was successfully collected
    /// - `Ok(None)` - This source has no input; try the next one in the chain
    /// - `Err(e)` - Collection failed; abort the chain with this error
    fn collect(&self, matches: &ArgMatches) -> Result<Option<T>, InputError>;

    /// Bind this collector to an invocation's [`InputSources`].
    ///
    /// Stdin and clipboard sources constructed with [`crate::StdinSource::new`]
    /// / [`crate::ClipboardSource::new`] return a collector that reads those
    /// invocation sources. Collectors built with an explicit reader, and
    /// sources that do not consult stdin/clipboard, return `None` and keep
    /// their existing behaviour.
    fn bind_sources(&self, _sources: &InputSources) -> Option<Box<dyn InputCollector<T>>> {
        None
    }

    /// Validate the collected value.
    ///
    /// Called after successful collection. Override to add source-specific
    /// validation that can trigger re-prompting for interactive sources.
    ///
    /// Default implementation accepts all values.
    fn validate(&self, _value: &T) -> Result<(), String> {
        Ok(())
    }

    /// Whether this collector supports retry on validation failure.
    ///
    /// Interactive collectors (prompts, editor) should return `true` to allow
    /// re-prompting when validation fails. Non-interactive sources (args,
    /// stdin) should return `false`.
    ///
    /// Default is `false`.
    fn can_retry(&self) -> bool {
        false
    }
}

/// Returns a process-wide static [`ArgMatches`] with no arguments.
///
/// Used by the `.prompt()` shortcuts on interactive sources, which need to
/// satisfy `InputCollector::collect`'s `&ArgMatches` parameter even when no
/// CLI parser is involved (the typical case for wizard/REPL flows that own
/// their own driver). Initialized lazily on first call and reused thereafter.
#[cfg(any(feature = "editor", feature = "simple-prompts", feature = "inquire"))]
pub(crate) fn empty_matches() -> &'static ArgMatches {
    use std::sync::OnceLock;
    static MATCHES: OnceLock<ArgMatches> = OnceLock::new();
    MATCHES.get_or_init(|| {
        clap::Command::new("__standout_input_prompt__")
            .no_binary_name(true)
            .try_get_matches_from(std::iter::empty::<&str>())
            .expect("empty command always parses with no args")
    })
}

/// Information about how input was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInput<T> {
    /// The resolved value.
    pub value: T,
    /// Which source provided the value.
    pub source: InputSourceKind,
}

/// The kind of source that provided input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSourceKind {
    /// From a CLI argument.
    Arg,
    /// From a CLI flag.
    Flag,
    /// From piped stdin.
    Stdin,
    /// From an environment variable.
    Env,
    /// From the system clipboard.
    Clipboard,
    /// From an external editor.
    Editor,
    /// From an interactive prompt.
    Prompt,
    /// From a default value.
    Default,
}

impl std::fmt::Display for InputSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arg => write!(f, "argument"),
            Self::Flag => write!(f, "flag"),
            Self::Stdin => write!(f, "stdin"),
            Self::Env => write!(f, "environment variable"),
            Self::Clipboard => write!(f, "clipboard"),
            Self::Editor => write!(f, "editor"),
            Self::Prompt => write!(f, "prompt"),
            Self::Default => write!(f, "default"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_kind_display() {
        assert_eq!(InputSourceKind::Arg.to_string(), "argument");
        assert_eq!(InputSourceKind::Stdin.to_string(), "stdin");
        assert_eq!(InputSourceKind::Editor.to_string(), "editor");
    }
}
