use clap::ArgMatches;

use crate::collector::{InputCollector, InputSourceKind, ResolvedInput};
use crate::InputError;

#[derive(Debug, Clone)]
pub struct ArgSource {
    name: String,
}

impl ArgSource {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    pub fn arg_name(&self) -> &str {
        &self.name
    }
}

impl InputCollector<String> for ArgSource {
    fn name(&self) -> &'static str {
        "argument"
    }

    fn is_available(&self, matches: &ArgMatches) -> bool {
        matches.contains_id(&self.name) && matches.get_one::<String>(&self.name).is_some()
    }

    fn collect(&self, matches: &ArgMatches) -> Result<Option<String>, InputError> {
        Ok(matches.get_one::<String>(&self.name).cloned())
    }
}

#[derive(Debug, Clone)]
pub struct FlagSource {
    name: String,
    invert: bool,
}

impl FlagSource {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            invert: false,
        }
    }

    pub fn inverted(mut self) -> Self {
        self.invert = true;
        self
    }

    pub fn flag_name(&self) -> &str {
        &self.name
    }
}

impl InputCollector<bool> for FlagSource {
    fn name(&self) -> &'static str {
        "flag"
    }

    fn is_available(&self, matches: &ArgMatches) -> bool {
        matches.contains_id(&self.name)
    }

    fn collect(&self, matches: &ArgMatches) -> Result<Option<bool>, InputError> {
        let value = matches.get_flag(&self.name);
        let result = if self.invert { !value } else { value };

        if matches.get_flag(&self.name) {
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }
}

impl FlagSource {
    pub fn resolve(&self, matches: &ArgMatches) -> Result<ResolvedInput<bool>, InputError> {
        let value = matches.get_flag(&self.name);
        let result = if self.invert { !value } else { value };

        Ok(ResolvedInput {
            value: result,
            source: InputSourceKind::Flag,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, Command};

    fn make_matches(args: &[&str]) -> ArgMatches {
        Command::new("test")
            .arg(Arg::new("message").long("message").short('m'))
            .arg(
                Arg::new("verbose")
                    .long("verbose")
                    .short('v')
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new("no-editor")
                    .long("no-editor")
                    .action(clap::ArgAction::SetTrue),
            )
            .try_get_matches_from(args)
            .unwrap()
    }

    #[test]
    fn arg_source_available_when_provided() {
        let matches = make_matches(&["test", "--message", "hello"]);
        let source = ArgSource::new("message");

        assert!(source.is_available(&matches));
        assert_eq!(source.collect(&matches).unwrap(), Some("hello".to_string()));
    }

    #[test]
    fn arg_source_unavailable_when_missing() {
        let matches = make_matches(&["test"]);
        let source = ArgSource::new("message");

        assert!(!source.is_available(&matches));
        assert_eq!(source.collect(&matches).unwrap(), None);
    }

    #[test]
    fn flag_source_returns_some_when_set() {
        let matches = make_matches(&["test", "--verbose"]);
        let source = FlagSource::new("verbose");

        assert!(source.is_available(&matches));
        assert_eq!(source.collect(&matches).unwrap(), Some(true));
    }

    #[test]
    fn flag_source_returns_none_when_not_set() {
        let matches = make_matches(&["test"]);
        let source = FlagSource::new("verbose");

        assert!(source.is_available(&matches));
        assert_eq!(source.collect(&matches).unwrap(), None);
    }

    #[test]
    fn flag_source_inverted() {
        let matches = make_matches(&["test", "--no-editor"]);
        let source = FlagSource::new("no-editor").inverted();

        assert_eq!(source.collect(&matches).unwrap(), Some(false));
    }

    #[test]
    fn flag_source_inverted_not_set() {
        let matches = make_matches(&["test"]);
        let source = FlagSource::new("no-editor").inverted();

        assert_eq!(source.collect(&matches).unwrap(), None);
    }
}
