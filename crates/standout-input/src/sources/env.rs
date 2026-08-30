use std::sync::Arc;

use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::env::{EnvReader, RealEnv};
use crate::InputError;

#[derive(Clone)]
pub struct EnvSource<R: EnvReader = RealEnv> {
    var_name: String,
    reader: Arc<R>,
}

impl EnvSource<RealEnv> {
    pub fn new(var_name: impl Into<String>) -> Self {
        Self {
            var_name: var_name.into(),
            reader: Arc::new(RealEnv),
        }
    }
}

impl<R: EnvReader> EnvSource<R> {
    pub fn with_reader(var_name: impl Into<String>, reader: R) -> Self {
        Self {
            var_name: var_name.into(),
            reader: Arc::new(reader),
        }
    }

    pub fn var_name(&self) -> &str {
        &self.var_name
    }
}

impl<R: EnvReader + 'static> InputCollector<String> for EnvSource<R> {
    fn name(&self) -> &'static str {
        "environment variable"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.reader
            .var(&self.var_name)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        match self.reader.var(&self.var_name) {
            Some(value) if !value.is_empty() => Ok(Some(value)),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockEnv;
    use clap::Command;

    fn empty_matches() -> ArgMatches {
        Command::new("test").try_get_matches_from(["test"]).unwrap()
    }

    #[test]
    fn env_available_when_set() {
        let env = MockEnv::new().with_var("MY_VAR", "value");
        let source = EnvSource::with_reader("MY_VAR", env);

        assert!(source.is_available(&empty_matches()));
    }

    #[test]
    fn env_unavailable_when_unset() {
        let env = MockEnv::new();
        let source = EnvSource::with_reader("MY_VAR", env);

        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn env_unavailable_when_empty() {
        let env = MockEnv::new().with_var("MY_VAR", "");
        let source = EnvSource::with_reader("MY_VAR", env);

        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn env_collects_value() {
        let env = MockEnv::new().with_var("MY_VAR", "hello");
        let source = EnvSource::with_reader("MY_VAR", env);

        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn env_returns_none_when_unset() {
        let env = MockEnv::new();
        let source = EnvSource::with_reader("MY_VAR", env);

        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn env_returns_none_when_empty() {
        let env = MockEnv::new().with_var("MY_VAR", "");
        let source = EnvSource::with_reader("MY_VAR", env);

        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }
}
