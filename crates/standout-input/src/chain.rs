//! Input chain builder for composing multiple sources.
//!
//! The [`InputChain`] allows chaining multiple input sources with fallback
//! behavior. Sources are tried in order until one provides input.

use std::fmt;

use clap::ArgMatches;

use crate::collector::{InputCollector, InputSourceKind, ResolvedInput};
use crate::InputError;
use crate::InputSources;

/// Validator function type.
type ValidatorFn<T> = Box<dyn Fn(&T) -> Result<(), String> + Send + Sync>;

/// Chain multiple input sources with fallback behavior.
///
/// Sources are tried in the order they were added. The first source that
/// returns `Some(value)` wins. If all sources return `None`, the chain
/// uses the default value or returns [`InputError::NoInput`].
///
/// # Example
///
/// ```ignore
/// use standout_input::{InputChain, ArgSource, StdinSource, DefaultSource};
///
/// // Try argument first, then stdin, then use default
/// let chain = InputChain::<String>::new()
///     .try_source(ArgSource::new("message"))
///     .try_source(StdinSource::new())
///     .try_source(DefaultSource::new("default message".to_string()));
///
/// let value = chain.resolve(&matches)?;
/// ```
///
/// # Validation
///
/// Add validators to check the resolved value:
///
/// ```ignore
/// let chain = InputChain::<String>::new()
///     .try_source(ArgSource::new("email"))
///     .validate(|s| s.contains('@'), "Must be a valid email");
/// ```
///
/// Interactive sources (prompts, editor) can retry on validation failure.
pub struct InputChain<T> {
    sources: Vec<(Box<dyn InputCollector<T>>, InputSourceKind)>,
    validators: Vec<(ValidatorFn<T>, String)>,
    default: Option<T>,
}

impl<T: Clone + Send + Sync + 'static> InputChain<T> {
    /// Create a new empty input chain.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            validators: Vec::new(),
            default: None,
        }
    }

    /// Add a source to the chain.
    ///
    /// Sources are tried in the order they are added.
    pub fn try_source<C: InputCollector<T> + 'static>(mut self, source: C) -> Self {
        let kind = source_kind_from_name(source.name());
        self.sources.push((Box::new(source), kind));
        self
    }

    /// Add a source with an explicit kind.
    ///
    /// Use this when the source name doesn't map to a standard kind.
    pub fn try_source_with_kind<C: InputCollector<T> + 'static>(
        mut self,
        source: C,
        kind: InputSourceKind,
    ) -> Self {
        self.sources.push((Box::new(source), kind));
        self
    }

    /// Add a validation rule.
    ///
    /// The validator is called after a source successfully provides input.
    /// If validation fails:
    /// - Interactive sources (where `can_retry()` is true) will re-prompt
    /// - Non-interactive sources will return a validation error
    ///
    /// Multiple validators are checked in order; all must pass.
    pub fn validate<F>(mut self, f: F, error_msg: impl Into<String>) -> Self
    where
        F: Fn(&T) -> bool + Send + Sync + 'static,
    {
        let msg = error_msg.into();
        let msg_for_closure = msg.clone();
        self.validators.push((
            Box::new(move |value| {
                if f(value) {
                    Ok(())
                } else {
                    Err(msg_for_closure.clone())
                }
            }),
            msg,
        ));
        self
    }

    /// Add a validation rule that returns a Result.
    ///
    /// Unlike [`validate`](Self::validate), this allows custom error messages
    /// per validation failure.
    pub fn validate_with<F>(mut self, f: F) -> Self
    where
        F: Fn(&T) -> Result<(), String> + Send + Sync + 'static,
    {
        self.validators
            .push((Box::new(f), "validation failed".to_string()));
        self
    }

    /// Set a default value to use when no source provides input.
    ///
    /// This is equivalent to adding a [`DefaultSource`](crate::DefaultSource)
    /// at the end of the chain.
    pub fn default(mut self, value: T) -> Self {
        self.default = Some(value);
        self
    }

    /// Resolve the chain and return the input value.
    ///
    /// Tries each source in order until one provides input, then runs
    /// validation. Returns the value or an error. Uses
    /// [`InputSources::from_process`]; prefer [`resolve_from`] when the
    /// caller already has invocation sources.
    pub fn resolve(&self, matches: &ArgMatches) -> Result<T, InputError> {
        self.resolve_from(matches, &InputSources::from_process())
    }

    /// Resolve against explicit [`InputSources`].
    ///
    /// Stdin, clipboard, and (for interactive collection) the prompt
    /// responder come from `sources` rather than process-global defaults.
    pub fn resolve_from(
        &self,
        matches: &ArgMatches,
        sources: &InputSources,
    ) -> Result<T, InputError> {
        self.resolve_from_with_source(matches, sources)
            .map(|r| r.value)
    }

    /// Resolve the chain and return the input with source metadata.
    ///
    /// Like [`resolve`](Self::resolve), but also returns which source
    /// provided the value.
    pub fn resolve_with_source(
        &self,
        matches: &ArgMatches,
    ) -> Result<ResolvedInput<T>, InputError> {
        self.resolve_from_with_source(matches, &InputSources::from_process())
    }

    /// Resolve against explicit [`InputSources`] and return source metadata.
    pub fn resolve_from_with_source(
        &self,
        matches: &ArgMatches,
        sources: &InputSources,
    ) -> Result<ResolvedInput<T>, InputError> {
        for (source, kind) in &self.sources {
            let bound = source.bind_sources(sources);
            let source: &dyn InputCollector<T> = match bound.as_ref() {
                Some(bound) => bound.as_ref(),
                None => source.as_ref(),
            };
            if !source.is_available(matches) {
                continue;
            }

            // This loop is intentional: interactive sources (where can_retry() is true)
            // will re-prompt on validation failure. The `break` on None moves to the
            // next source in the chain.
            #[allow(clippy::while_let_loop)]
            loop {
                match source.collect(matches)? {
                    Some(value) => {
                        // Run source-level validation
                        if let Err(msg) = source.validate(&value) {
                            if source.can_retry() {
                                eprintln!("Invalid: {}", msg);
                                continue;
                            }
                            return Err(InputError::ValidationFailed(msg));
                        }

                        // Run chain-level validators
                        for (validator, _) in &self.validators {
                            if let Err(msg) = validator(&value) {
                                if source.can_retry() {
                                    eprintln!("Invalid: {}", msg);
                                    continue;
                                }
                                return Err(InputError::ValidationFailed(msg));
                            }
                        }

                        return Ok(ResolvedInput {
                            value,
                            source: *kind,
                        });
                    }
                    None => break, // Try next source
                }
            }
        }

        // No source provided input; try default
        if let Some(value) = &self.default {
            return Ok(ResolvedInput {
                value: value.clone(),
                source: InputSourceKind::Default,
            });
        }

        Err(InputError::NoInput)
    }

    /// Check if any source is available to provide input.
    pub fn has_available_source(&self, matches: &ArgMatches) -> bool {
        self.sources.iter().any(|(s, _)| s.is_available(matches)) || self.default.is_some()
    }

    /// Get the number of sources in the chain.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
}

impl<T: Clone + Send + Sync + 'static> Default for InputChain<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Debug for InputChain<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputChain")
            .field(
                "sources",
                &self.sources.iter().map(|(_, k)| k).collect::<Vec<_>>(),
            )
            .field("validators", &self.validators.len())
            .field("has_default", &self.default.is_some())
            .finish()
    }
}

/// Map source name to InputSourceKind.
fn source_kind_from_name(name: &str) -> InputSourceKind {
    match name {
        "argument" => InputSourceKind::Arg,
        "flag" => InputSourceKind::Flag,
        "stdin" => InputSourceKind::Stdin,
        "environment variable" => InputSourceKind::Env,
        "clipboard" => InputSourceKind::Clipboard,
        "editor" => InputSourceKind::Editor,
        "prompt" => InputSourceKind::Prompt,
        "default" => InputSourceKind::Default,
        _ => InputSourceKind::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::{MockClipboard, MockEnv, MockStdin};
    use crate::sources::{ArgSource, ClipboardSource, DefaultSource, EnvSource, StdinSource};
    use clap::{Arg, Command};

    fn make_matches(args: &[&str]) -> ArgMatches {
        Command::new("test")
            .arg(Arg::new("message").long("message").short('m'))
            .try_get_matches_from(args)
            .unwrap()
    }

    #[test]
    fn chain_resolves_first_available() {
        let matches = make_matches(&["test", "--message", "from arg"]);

        let chain = InputChain::<String>::new()
            .try_source(ArgSource::new("message"))
            .try_source(DefaultSource::new("default".to_string()));

        let result = chain.resolve_with_source(&matches).unwrap();
        assert_eq!(result.value, "from arg");
        assert_eq!(result.source, InputSourceKind::Arg);
    }

    #[test]
    fn chain_falls_back_to_next_source() {
        let matches = make_matches(&["test"]); // No --message

        let chain = InputChain::<String>::new()
            .try_source(ArgSource::new("message"))
            .try_source(StdinSource::with_reader(MockStdin::piped("from stdin")));

        let result = chain.resolve_with_source(&matches).unwrap();
        assert_eq!(result.value, "from stdin");
        assert_eq!(result.source, InputSourceKind::Stdin);
    }

    #[test]
    fn chain_falls_back_to_default() {
        let matches = make_matches(&["test"]);

        let chain = InputChain::<String>::new()
            .try_source(ArgSource::new("message"))
            .try_source(StdinSource::with_reader(MockStdin::terminal()))
            .default("default value".to_string());

        let result = chain.resolve_with_source(&matches).unwrap();
        assert_eq!(result.value, "default value");
        assert_eq!(result.source, InputSourceKind::Default);
    }

    #[test]
    fn chain_error_when_no_input() {
        let matches = make_matches(&["test"]);

        let chain = InputChain::<String>::new()
            .try_source(ArgSource::new("message"))
            .try_source(StdinSource::with_reader(MockStdin::terminal()));

        let result = chain.resolve(&matches);
        assert!(matches!(result, Err(InputError::NoInput)));
    }

    #[test]
    fn chain_validation_passes() {
        let matches = make_matches(&["test", "--message", "valid@email.com"]);

        let chain = InputChain::<String>::new()
            .try_source(ArgSource::new("message"))
            .validate(|s| s.contains('@'), "Must contain @");

        let result = chain.resolve(&matches).unwrap();
        assert_eq!(result, "valid@email.com");
    }

    #[test]
    fn chain_validation_fails() {
        let matches = make_matches(&["test", "--message", "invalid"]);

        let chain = InputChain::<String>::new()
            .try_source(ArgSource::new("message"))
            .validate(|s| s.contains('@'), "Must contain @");

        let result = chain.resolve(&matches);
        assert!(matches!(result, Err(InputError::ValidationFailed(_))));
    }

    #[test]
    fn chain_multiple_validators() {
        let matches = make_matches(&["test", "--message", "ab"]);

        let chain = InputChain::<String>::new()
            .try_source(ArgSource::new("message"))
            .validate(|s| !s.is_empty(), "Cannot be empty")
            .validate(|s| s.len() >= 3, "Must be at least 3 characters");

        let result = chain.resolve(&matches);
        assert!(matches!(result, Err(InputError::ValidationFailed(_))));
    }

    #[test]
    fn chain_complex_fallback() {
        let matches = make_matches(&["test"]);

        // arg → stdin → env → clipboard → default
        let chain = InputChain::<String>::new()
            .try_source(ArgSource::new("message"))
            .try_source(StdinSource::with_reader(MockStdin::terminal()))
            .try_source(EnvSource::with_reader("MY_MSG", MockEnv::new()))
            .try_source(ClipboardSource::with_reader(MockClipboard::with_content(
                "from clipboard",
            )));

        let result = chain.resolve_with_source(&matches).unwrap();
        assert_eq!(result.value, "from clipboard");
        assert_eq!(result.source, InputSourceKind::Clipboard);
    }

    #[test]
    fn chain_has_available_source() {
        let matches = make_matches(&["test"]);

        let chain_with_default = InputChain::<String>::new()
            .try_source(ArgSource::new("message"))
            .default("default".to_string());

        assert!(chain_with_default.has_available_source(&matches));

        let chain_without = InputChain::<String>::new().try_source(ArgSource::new("message"));

        assert!(!chain_without.has_available_source(&matches));
    }

    #[test]
    fn chain_source_count() {
        let chain = InputChain::<String>::new()
            .try_source(ArgSource::new("a"))
            .try_source(ArgSource::new("b"))
            .try_source(ArgSource::new("c"));

        assert_eq!(chain.source_count(), 3);
    }
}
