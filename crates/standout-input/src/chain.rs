use std::fmt;

use clap::ArgMatches;

use crate::collector::{InputCollector, InputSourceKind, ResolvedInput};
use crate::InputError;
use crate::InputSources;

type ValidatorFn<T> = Box<dyn Fn(&T) -> Result<(), String> + Send + Sync>;

pub struct InputChain<T> {
    sources: Vec<(Box<dyn InputCollector<T>>, InputSourceKind)>,
    validators: Vec<(ValidatorFn<T>, String)>,
    default: Option<T>,
}

impl<T: Clone + Send + Sync + 'static> InputChain<T> {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            validators: Vec::new(),
            default: None,
        }
    }

    pub fn try_source<C: InputCollector<T> + 'static>(mut self, source: C) -> Self {
        let kind = source_kind_from_name(source.name());
        self.sources.push((Box::new(source), kind));
        self
    }

    pub fn try_source_with_kind<C: InputCollector<T> + 'static>(
        mut self,
        source: C,
        kind: InputSourceKind,
    ) -> Self {
        self.sources.push((Box::new(source), kind));
        self
    }

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

    pub fn validate_with<F>(mut self, f: F) -> Self
    where
        F: Fn(&T) -> Result<(), String> + Send + Sync + 'static,
    {
        self.validators
            .push((Box::new(f), "validation failed".to_string()));
        self
    }

    pub fn default(mut self, value: T) -> Self {
        self.default = Some(value);
        self
    }

    pub fn resolve(&self, matches: &ArgMatches) -> Result<T, InputError> {
        self.resolve_from(matches, &InputSources::from_process())
    }

    pub fn resolve_from(
        &self,
        matches: &ArgMatches,
        sources: &InputSources,
    ) -> Result<T, InputError> {
        self.resolve_from_with_source(matches, sources)
            .map(|r| r.value)
    }

    pub fn resolve_with_source(
        &self,
        matches: &ArgMatches,
    ) -> Result<ResolvedInput<T>, InputError> {
        self.resolve_from_with_source(matches, &InputSources::from_process())
    }

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

            #[allow(clippy::while_let_loop)]
            'collect: loop {
                match source.collect(matches)? {
                    Some(value) => {
                        if let Err(msg) = source.validate(&value) {
                            if source.can_retry() {
                                eprintln!("Invalid: {}", msg);
                                continue 'collect;
                            }
                            return Err(InputError::ValidationFailed(msg));
                        }

                        for (validator, _) in &self.validators {
                            if let Err(msg) = validator(&value) {
                                if source.can_retry() {
                                    eprintln!("Invalid: {}", msg);
                                    continue 'collect;
                                }
                                return Err(InputError::ValidationFailed(msg));
                            }
                        }

                        return Ok(ResolvedInput {
                            value,
                            source: *kind,
                        });
                    }
                    None => break,
                }
            }
        }

        if let Some(value) = &self.default {
            return Ok(ResolvedInput {
                value: value.clone(),
                source: InputSourceKind::Default,
            });
        }

        Err(InputError::NoInput)
    }

    pub fn has_available_source(&self, matches: &ArgMatches) -> bool {
        self.has_available_source_from(matches, &InputSources::from_process())
    }

    pub fn has_available_source_from(&self, matches: &ArgMatches, sources: &InputSources) -> bool {
        self.sources.iter().any(|(source, _)| {
            let bound = source.bind_sources(sources);
            let source: &dyn InputCollector<T> = match bound.as_ref() {
                Some(bound) => bound.as_ref(),
                None => source.as_ref(),
            };
            source.is_available(matches)
        }) || self.default.is_some()
    }

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

fn source_kind_from_name(name: &str) -> InputSourceKind {
    match name {
        "argument" => InputSourceKind::Arg,
        "flag" => InputSourceKind::Flag,
        "file" => InputSourceKind::File,
        "stdin" => InputSourceKind::Stdin,
        "environment variable" => InputSourceKind::Env,
        "config" => InputSourceKind::Config,
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
        let matches = make_matches(&["test"]);

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

    #[cfg(feature = "simple-prompts")]
    #[test]
    fn chain_resolve_from_uses_scripted_responder_without_tty() {
        use crate::sources::{MockTerminal, TextPromptSource};
        use crate::{PromptResponse, ScriptedResponder};
        use std::sync::Arc;

        let matches = make_matches(&["test"]);
        let chain = InputChain::<String>::new().try_source(TextPromptSource::with_terminal(
            "Name: ",
            MockTerminal::non_terminal(),
        ));
        let sources =
            InputSources::from_process().with_responder(Arc::new(ScriptedResponder::new([
                PromptResponse::text("Ada"),
            ])));

        assert_eq!(chain.resolve_from(&matches, &sources).unwrap(), "Ada");
    }

    #[cfg(feature = "simple-prompts")]
    #[test]
    fn chain_validation_retries_interactive_source_via_scripted_responder() {
        use crate::sources::{MockTerminal, TextPromptSource};
        use crate::{PromptResponse, ScriptedResponder};
        use std::sync::Arc;

        let matches = make_matches(&["test"]);
        let chain = InputChain::<String>::new()
            .try_source(TextPromptSource::with_terminal(
                "Name: ",
                MockTerminal::non_terminal(),
            ))
            .validate(|s| s.len() >= 3, "too short");
        let sources =
            InputSources::from_process().with_responder(Arc::new(ScriptedResponder::new([
                PromptResponse::text("ab"),
                PromptResponse::text("Ada"),
            ])));

        assert_eq!(chain.resolve_from(&matches, &sources).unwrap(), "Ada");
    }

    #[cfg(feature = "simple-prompts")]
    #[test]
    fn chain_skips_interactive_source_without_responder_or_tty() {
        use crate::sources::{MockTerminal, TextPromptSource};

        let matches = make_matches(&["test"]);
        let chain = InputChain::<String>::new()
            .try_source(TextPromptSource::with_terminal(
                "Name: ",
                MockTerminal::non_terminal(),
            ))
            .default("fallback".to_string());
        let sources = InputSources::from_process();

        assert_eq!(chain.resolve_from(&matches, &sources).unwrap(), "fallback");
        assert!(chain.has_available_source_from(&matches, &sources));
    }

    #[cfg(feature = "editor")]
    #[test]
    fn chain_resolve_from_uses_scripted_responder_for_editor() {
        use crate::sources::{EditorSource, MockEditorRunner};
        use crate::{PromptResponse, ScriptedResponder};
        use std::sync::Arc;

        let matches = make_matches(&["test"]);
        let chain = InputChain::<String>::new()
            .try_source(EditorSource::with_runner(MockEditorRunner::no_editor()));
        let sources =
            InputSources::from_process().with_responder(Arc::new(ScriptedResponder::new([
                PromptResponse::text("edited body"),
            ])));

        assert_eq!(
            chain.resolve_from(&matches, &sources).unwrap(),
            "edited body"
        );
    }
}
