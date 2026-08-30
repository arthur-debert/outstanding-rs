use std::sync::Arc;

use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::env::{ClipboardReader, RealClipboard};
use crate::InputError;
use crate::InputSources;

#[derive(Clone)]
pub struct ClipboardSource {
    reader: Option<Arc<dyn ClipboardReader>>,
    trim: bool,
}

impl ClipboardSource {
    pub fn new() -> Self {
        Self {
            reader: None,
            trim: true,
        }
    }

    pub fn with_reader(reader: impl ClipboardReader + 'static) -> Self {
        Self {
            reader: Some(Arc::new(reader)),
            trim: true,
        }
    }

    pub fn with_shared_reader(reader: Arc<dyn ClipboardReader>) -> Self {
        Self {
            reader: Some(reader),
            trim: true,
        }
    }

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
