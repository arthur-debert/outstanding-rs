use clap::ArgMatches;

use crate::InputError;
use crate::InputSources;

pub trait InputCollector<T>: Send + Sync {
    fn name(&self) -> &'static str;

    fn is_available(&self, matches: &ArgMatches) -> bool;

    fn collect(&self, matches: &ArgMatches) -> Result<Option<T>, InputError>;

    fn bind_sources(&self, _sources: &InputSources) -> Option<Box<dyn InputCollector<T>>> {
        None
    }

    fn validate(&self, _value: &T) -> Result<(), String> {
        Ok(())
    }

    fn can_retry(&self) -> bool {
        false
    }
}

#[cfg(any(feature = "editor", feature = "simple-prompts", feature = "inquire"))]
pub(crate) fn prompt_value_from<T>(
    collector: &dyn InputCollector<T>,
    sources: &InputSources,
) -> Result<T, InputError> {
    let bound = collector.bind_sources(sources);
    let source: &dyn InputCollector<T> = match bound.as_ref() {
        Some(bound) => bound.as_ref(),
        None => collector,
    };
    let matches = empty_matches();
    if !source.is_available(matches) {
        return Err(InputError::NoInput);
    }
    source.collect(matches)?.ok_or(InputError::NoInput)
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInput<T> {
    pub value: T,
    pub source: InputSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSourceKind {
    Arg,
    Flag,
    File,
    Stdin,
    Env,
    Config,
    Clipboard,
    Editor,
    Prompt,
    Default,
}

impl std::fmt::Display for InputSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arg => write!(f, "argument"),
            Self::Flag => write!(f, "flag"),
            Self::File => write!(f, "file"),
            Self::Stdin => write!(f, "stdin"),
            Self::Env => write!(f, "environment variable"),
            Self::Config => write!(f, "config"),
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
        assert_eq!(InputSourceKind::File.to_string(), "file");
        assert_eq!(InputSourceKind::Stdin.to_string(), "stdin");
        assert_eq!(InputSourceKind::Editor.to_string(), "editor");
    }
}
