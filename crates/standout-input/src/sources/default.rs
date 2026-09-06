use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::InputError;
use crate::InputSources;

#[derive(Debug, Clone)]
pub struct DefaultSource<T: Clone + Send + Sync> {
    value: T,
}

impl<T: Clone + Send + Sync> DefaultSource<T> {
    pub fn new(value: T) -> Self {
        Self { value }
    }

    pub fn value(&self) -> &T {
        &self.value
    }
}

impl<T: Clone + Send + Sync + 'static> InputCollector<T> for DefaultSource<T> {
    fn name(&self) -> &'static str {
        "default"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        true
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<T>, InputError> {
        Ok(Some(self.value.clone()))
    }

    fn bind_sources(&self, _sources: &InputSources) -> Option<Box<dyn InputCollector<T>>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Command;

    fn empty_matches() -> ArgMatches {
        Command::new("test").try_get_matches_from(["test"]).unwrap()
    }

    #[test]
    fn default_always_available() {
        let source = DefaultSource::new("default");
        assert!(source.is_available(&empty_matches()));
    }

    #[test]
    fn default_returns_value() {
        let source = DefaultSource::new("default value".to_string());
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("default value".to_string()));
    }

    #[test]
    fn default_with_bool() {
        let source = DefaultSource::new(false);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn default_with_number() {
        let source = DefaultSource::new(42i32);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some(42));
    }
}
