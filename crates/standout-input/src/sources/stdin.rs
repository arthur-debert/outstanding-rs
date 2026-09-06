use std::sync::Arc;

use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::env::StdinReader;
use crate::InputError;
use crate::InputSources;

#[derive(Clone)]
pub struct StdinSource {
    reader: Option<Arc<dyn StdinReader>>,
    trim: bool,
}

impl StdinSource {
    pub fn new() -> Self {
        Self {
            reader: None,
            trim: true,
        }
    }

    pub fn with_reader(reader: impl StdinReader + 'static) -> Self {
        Self {
            reader: Some(Arc::new(reader)),
            trim: true,
        }
    }

    pub fn with_shared_reader(reader: Arc<dyn StdinReader>) -> Self {
        Self {
            reader: Some(reader),
            trim: true,
        }
    }

    pub fn trim(mut self, trim: bool) -> Self {
        self.trim = trim;
        self
    }

    fn bound_reader(&self) -> Result<&dyn StdinReader, InputError> {
        self.reader.as_deref().ok_or(InputError::StdinNotBound)
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
        match self.bound_reader() {
            Ok(reader) => !reader.is_terminal(),
            Err(_) => true,
        }
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        let reader = self.bound_reader()?;
        if reader.is_terminal() {
            return Ok(None);
        }

        let content = reader.read_to_string().map_err(InputError::StdinFailed)?;

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

pub fn read_if_piped() -> Result<Option<String>, InputError> {
    read_if_piped_from(&InputSources::from_process())
}

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
    fn unbound_stdin_collect_names_the_missing_binding() {
        let source = StdinSource::new();
        let err = source.collect(&empty_matches()).unwrap_err();
        assert!(matches!(err, InputError::StdinNotBound));
        let message = err.to_string();
        assert!(message.contains("bind_sources"));
        assert!(message.contains("with_shared_reader"));
    }

    #[test]
    fn unbound_stdin_stays_available_so_collect_reports_it() {
        let source = StdinSource::new();
        assert!(source.is_available(&empty_matches()));
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
