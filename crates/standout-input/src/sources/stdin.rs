//! Stdin input source.

use std::sync::Arc;

use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::env::{RealStdin, StdinReader};
use crate::InputError;
use crate::InputSources;

/// Collect input from piped stdin.
///
/// This source reads from stdin only when it is piped (not a terminal).
/// If stdin is a TTY, the source returns `None` to allow the chain to
/// continue to the next source.
///
/// [`new`](Self::new) means "use this invocation's [`InputSources`]". An
/// explicit reader from [`with_reader`](Self::with_reader) is not rebound.
///
/// # Example
///
/// ```ignore
/// use standout_input::{InputChain, ArgSource, StdinSource};
///
/// // For: echo "hello" | myapp
/// let chain = InputChain::<String>::new()
///     .try_source(ArgSource::new("message"))
///     .try_source(StdinSource::new());
/// ```
///
/// # Testing
///
/// Use [`StdinSource::with_reader`] to inject a mock for testing, or pass
/// [`InputSources`] with a [`crate::MockStdin`] into
/// [`InputChain::resolve_from`](crate::InputChain::resolve_from):
///
/// ```ignore
/// use standout_input::{StdinSource, MockStdin};
///
/// let source = StdinSource::with_reader(MockStdin::piped("test input"));
/// ```
#[derive(Clone)]
pub struct StdinSource {
    /// `None` binds to [`InputSources`] at resolve time.
    reader: Option<Arc<dyn StdinReader>>,
    trim: bool,
}

impl StdinSource {
    /// Create a new stdin source.
    ///
    /// Reads the invocation's stdin when the chain is resolved against
    /// [`InputSources`]. Standalone [`InputChain::resolve`] uses
    /// [`InputSources::from_process`].
    pub fn new() -> Self {
        Self {
            reader: None,
            trim: true,
        }
    }

    /// Create a stdin source with a custom reader.
    ///
    /// This is primarily used for testing to inject mock stdin. The explicit
    /// reader is not replaced when the chain binds [`InputSources`].
    pub fn with_reader(reader: impl StdinReader + 'static) -> Self {
        Self {
            reader: Some(Arc::new(reader)),
            trim: true,
        }
    }

    /// Create a stdin source from a shared reader handle.
    pub fn with_shared_reader(reader: Arc<dyn StdinReader>) -> Self {
        Self {
            reader: Some(reader),
            trim: true,
        }
    }

    /// Control whether to trim whitespace from the input.
    ///
    /// Default is `true`.
    pub fn trim(mut self, trim: bool) -> Self {
        self.trim = trim;
        self
    }

    fn reader(&self) -> &dyn StdinReader {
        self.reader
            .as_deref()
            .unwrap_or(&RealStdin as &dyn StdinReader)
    }
}

impl Default for StdinSource {
    fn default() -> Self {
        Self::new()
    }
}

impl InputCollector<String> for StdinSource {
    fn name(&self) -> &'static str {
        "stdin"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        !self.reader().is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        if self.reader().is_terminal() {
            return Ok(None);
        }

        let content = self
            .reader()
            .read_to_string()
            .map_err(InputError::StdinFailed)?;

        if content.is_empty() {
            return Ok(None);
        }

        let result = if self.trim {
            content.trim().to_string()
        } else {
            content
        };

        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<String>>> {
        if self.reader.is_some() {
            return None;
        }
        Some(Box::new(Self {
            reader: Some(sources.stdin_arc()),
            trim: self.trim,
        }))
    }
}

/// Convenience function to read stdin if piped.
///
/// Returns `Ok(Some(content))` if stdin is piped and has content,
/// `Ok(None)` if stdin is a terminal or empty. Uses
/// [`InputSources::from_process`]. Prefer [`read_if_piped_from`] when the
/// caller already has invocation sources.
pub fn read_if_piped() -> Result<Option<String>, InputError> {
    read_if_piped_from(&InputSources::from_process())
}

/// Read piped stdin from an explicit [`InputSources`].
pub fn read_if_piped_from(sources: &InputSources) -> Result<Option<String>, InputError> {
    let reader = sources.stdin();
    if reader.is_terminal() {
        return Ok(None);
    }

    let content = reader.read_to_string().map_err(InputError::StdinFailed)?;

    if content.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(content.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockStdin;
    use clap::Command;

    fn empty_matches() -> ArgMatches {
        Command::new("test").try_get_matches_from(["test"]).unwrap()
    }

    #[test]
    fn stdin_available_when_piped() {
        let source = StdinSource::with_reader(MockStdin::piped("content"));
        assert!(source.is_available(&empty_matches()));
    }

    #[test]
    fn stdin_unavailable_when_terminal() {
        let source = StdinSource::with_reader(MockStdin::terminal());
        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn stdin_reads_piped_content() {
        let source = StdinSource::with_reader(MockStdin::piped("hello world"));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("hello world".to_string()));
    }

    #[test]
    fn stdin_trims_whitespace() {
        let source = StdinSource::with_reader(MockStdin::piped("  hello  \n"));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn stdin_no_trim() {
        let source = StdinSource::with_reader(MockStdin::piped("  hello  \n")).trim(false);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("  hello  \n".to_string()));
    }

    #[test]
    fn stdin_returns_none_for_empty() {
        let source = StdinSource::with_reader(MockStdin::piped_empty());
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn stdin_returns_none_for_whitespace_only() {
        let source = StdinSource::with_reader(MockStdin::piped("   \n\t  "));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn stdin_returns_none_when_terminal() {
        let source = StdinSource::with_reader(MockStdin::terminal());
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn bind_sources_uses_invocation_stdin() {
        let source = StdinSource::new();
        let sources = InputSources::from_process().with_stdin(MockStdin::piped("bound"));
        let bound = source.bind_sources(&sources).expect("unbound source binds");
        let result = bound.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("bound".to_string()));
    }

    #[test]
    fn bind_sources_keeps_explicit_reader() {
        let source = StdinSource::with_reader(MockStdin::piped("explicit"));
        let sources = InputSources::from_process().with_stdin(MockStdin::piped("ignored"));
        assert!(source.bind_sources(&sources).is_none());
        assert_eq!(
            source.collect(&empty_matches()).unwrap(),
            Some("explicit".to_string())
        );
    }
}
