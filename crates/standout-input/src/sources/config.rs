use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::InputError;
use crate::InputSources;

#[derive(Debug, Clone)]
pub struct ConfigSource<T: Clone + Send + Sync> {
    value: Option<T>,
}

impl<T: Clone + Send + Sync> ConfigSource<T> {
    pub fn new(value: Option<T>) -> Self {
        Self { value }
    }

    pub fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }
}

impl<T: Clone + Send + Sync + 'static> InputCollector<T> for ConfigSource<T> {
    fn name(&self) -> &'static str {
        "config"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.value.is_some()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<T>, InputError> {
        Ok(self.value.clone())
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
    fn config_available_when_some() {
        let source = ConfigSource::new(Some("remote".to_string()));
        assert!(source.is_available(&empty_matches()));
        assert_eq!(
            source.collect(&empty_matches()).unwrap(),
            Some("remote".to_string())
        );
    }

    #[test]
    fn config_unavailable_when_none() {
        let source = ConfigSource::<String>::new(None);
        assert!(!source.is_available(&empty_matches()));
        assert_eq!(source.collect(&empty_matches()).unwrap(), None);
    }
}
