//! Clipboard input source.

use std::sync::Arc;

use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::env::{ClipboardReader, RealClipboard};
use crate::InputError;
use crate::InputSources;

/// Collect input from the system clipboard.
///
/// This source reads text from the system clipboard. It is available when
/// the clipboard contains non-empty text content.
///
/// [`new`](Self::new) means "use this invocation's [`InputSources`]". An
/// explicit reader from [`with_reader`](Self::with_reader) is not rebound.
///
/// # Platform Support
///
/// - **macOS**: Uses `pbpaste`
/// - **Linux**: Uses `xclip -selection clipboard -o`
/// - **Other**: Returns an error
///
/// # Example
///
/// ```ignore
/// use standout_input::{InputChain, ArgSource, ClipboardSource};
///
/// let chain = InputChain::<String>::new()
///     .try_source(ArgSource::new("content"))
///     .try_source(ClipboardSource::new());
/// ```
///
/// # Testing
///
/// Use [`ClipboardSource::with_reader`] to inject a mock for testing, or pass
/// [`InputSources`] with a [`crate::MockClipboard`] into
/// [`InputChain::resolve_from`](crate::InputChain::resolve_from):
///
/// ```ignore
/// use standout_input::{ClipboardSource, MockClipboard};
///
/// let source = ClipboardSource::with_reader(MockClipboard::with_content("clipboard text"));
/// ```
#[derive(Clone)]
pub struct ClipboardSource {
    /// `None` binds to [`InputSources`] at resolve time.
    reader: Option<Arc<dyn ClipboardReader>>,
    trim: bool,
}

impl ClipboardSource {
    /// Create a new clipboard source.
    ///
    /// Reads the invocation's clipboard when the chain is resolved against
    /// [`InputSources`]. Standalone [`InputChain::resolve`] uses
    /// [`InputSources::from_process`].
    pub fn new() -> Self {
        Self {
            reader: None,
            trim: true,
        }
    }

    /// Create a clipboard source with a custom reader.
    ///
    /// This is primarily used for testing to inject mock clipboard. The
    /// explicit reader is not replaced when the chain binds [`InputSources`].
    pub fn with_reader(reader: impl ClipboardReader + 'static) -> Self {
        Self {
            reader: Some(Arc::new(reader)),
            trim: true,
        }
    }

    /// Create a clipboard source from a shared reader handle.
    pub fn with_shared_reader(reader: Arc<dyn ClipboardReader>) -> Self {
        Self {
            reader: Some(reader),
            trim: true,
        }
    }

    /// Control whether to trim whitespace from the clipboard content.
    ///
    /// Default is `true`.
    pub fn trim(mut self, trim: bool) -> Self {
        self.trim = trim;
        self
    }

    fn reader(&self) -> &dyn ClipboardReader {
        self.reader
            .as_deref()
            .unwrap_or(&RealClipboard as &dyn ClipboardReader)
    }
}

impl Default for ClipboardSource {
    fn default() -> Self {
        Self::new()
    }
}

impl InputCollector<String> for ClipboardSource {
    fn name(&self) -> &'static str {
        "clipboard"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        match self.reader().read() {
            Ok(Some(content)) => !content.trim().is_empty(),
            Ok(None) => false,
            Err(e) => {
                eprintln!("Warning: clipboard unavailable: {}", e);
                false
            }
        }
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        match self.reader().read()? {
            Some(content) => {
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
            None => Ok(None),
        }
    }

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<String>>> {
        if self.reader.is_some() {
            return None;
        }
        Some(Box::new(Self {
            reader: Some(sources.clipboard_arc()),
            trim: self.trim,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockClipboard;
    use clap::Command;

    fn empty_matches() -> ArgMatches {
        Command::new("test").try_get_matches_from(["test"]).unwrap()
    }

    #[test]
    fn clipboard_available_when_has_content() {
        let source = ClipboardSource::with_reader(MockClipboard::with_content("content"));
        assert!(source.is_available(&empty_matches()));
    }

    #[test]
    fn clipboard_unavailable_when_empty() {
        let source = ClipboardSource::with_reader(MockClipboard::empty());
        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn clipboard_unavailable_when_whitespace_only() {
        let source = ClipboardSource::with_reader(MockClipboard::with_content("   \n\t  "));
        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn clipboard_collects_content() {
        let source = ClipboardSource::with_reader(MockClipboard::with_content("hello"));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn clipboard_trims_whitespace() {
        let source = ClipboardSource::with_reader(MockClipboard::with_content("  hello  \n"));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn clipboard_no_trim() {
        let source =
            ClipboardSource::with_reader(MockClipboard::with_content("  hello  ")).trim(false);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("  hello  ".to_string()));
    }

    #[test]
    fn clipboard_returns_none_when_empty() {
        let source = ClipboardSource::with_reader(MockClipboard::empty());
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn bind_sources_uses_invocation_clipboard() {
        let source = ClipboardSource::new();
        let sources =
            InputSources::from_process().with_clipboard(MockClipboard::with_content("bound"));
        let bound = source.bind_sources(&sources).expect("unbound source binds");
        let result = bound.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("bound".to_string()));
    }
}
