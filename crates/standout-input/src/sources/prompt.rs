//! Simple terminal prompts.
//!
//! Basic interactive prompts that work without external dependencies.
//! For richer TUI prompts, use the `inquire` feature instead.

use std::io::{self, BufRead, IsTerminal, Write};
use std::sync::Arc;

use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::InputError;

/// Abstraction over terminal I/O for testability.
pub trait TerminalIO: Send + Sync {
    /// Check if stdin is a terminal.
    fn is_terminal(&self) -> bool;

    /// Write a prompt to stdout.
    fn write_prompt(&self, prompt: &str) -> io::Result<()>;

    /// Read a line from stdin.
    fn read_line(&self) -> io::Result<String>;
}

/// Sharing one terminal across several prompt sources: an `Arc<T>` delegates
/// to the wrapped terminal, so a single (possibly stateful mock) terminal can
/// back a sequence of [`TextPromptSource`] / [`ConfirmPromptSource`] values —
/// e.g. one prompt per questionnaire field.
impl<T: TerminalIO + ?Sized> TerminalIO for Arc<T> {
    fn is_terminal(&self) -> bool {
        (**self).is_terminal()
    }

    fn write_prompt(&self, prompt: &str) -> io::Result<()> {
        (**self).write_prompt(prompt)
    }

    fn read_line(&self) -> io::Result<String> {
        (**self).read_line()
    }
}

/// Real terminal I/O.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealTerminal;

impl TerminalIO for RealTerminal {
    fn is_terminal(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn write_prompt(&self, prompt: &str) -> io::Result<()> {
        print!("{}", prompt);
        io::stdout().flush()
    }

    fn read_line(&self) -> io::Result<String> {
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        Ok(line)
    }
}

/// Simple text input prompt.
///
/// Prompts the user for text input in the terminal. Only available when
/// stdin is a TTY (not piped).
///
/// # Example
///
/// ```ignore
/// use standout_input::{InputChain, ArgSource, TextPromptSource};
///
/// let chain = InputChain::<String>::new()
///     .try_source(ArgSource::new("name"))
///     .try_source(TextPromptSource::new("Enter your name: "));
///
/// let name = chain.resolve(&matches)?;
/// ```
#[derive(Clone)]
pub struct TextPromptSource<T: TerminalIO = RealTerminal> {
    terminal: Arc<T>,
    prompt: String,
    trim: bool,
}

impl TextPromptSource<RealTerminal> {
    /// Create a new text prompt source.
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            terminal: Arc::new(RealTerminal),
            prompt: prompt.into(),
            trim: true,
        }
    }
}

impl<T: TerminalIO> TextPromptSource<T> {
    /// Create a text prompt with a custom terminal for testing.
    pub fn with_terminal(prompt: impl Into<String>, terminal: T) -> Self {
        Self {
            terminal: Arc::new(terminal),
            prompt: prompt.into(),
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
}

impl<T: TerminalIO + 'static> TextPromptSource<T> {
    /// Prompt the user for text and return the entered value.
    ///
    /// Standalone counterpart to [`InputCollector::collect`] for wizard /
    /// REPL flows that drive standout themselves and have no `&ArgMatches`
    /// to plumb through. Returns the entered text on success. Routes
    /// through any installed
    /// [`PromptResponder`](crate::PromptResponder).
    ///
    /// Errors:
    /// - [`InputError::PromptCancelled`] on EOF (Ctrl+D)
    /// - [`InputError::NoInput`] if stdin is not a TTY *or* the user
    ///   submits empty input
    /// - [`InputError::PromptFailed`] on terminal I/O failure
    pub fn prompt(&self) -> Result<String, InputError> {
        if let Some(value) =
            crate::responder::intercept_text(crate::PromptKind::Text, &self.prompt)?
        {
            return Ok(value);
        }
        let matches = crate::collector::empty_matches();
        if !self.is_available(matches) {
            return Err(InputError::NoInput);
        }
        self.collect(matches)?.ok_or(InputError::NoInput)
    }
}

impl<T: TerminalIO + 'static> InputCollector<String> for TextPromptSource<T> {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.terminal.is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        if !self.terminal.is_terminal() {
            return Ok(None);
        }

        self.terminal
            .write_prompt(&self.prompt)
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        let line = self
            .terminal
            .read_line()
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        // Check for EOF (user pressed Ctrl+D)
        if line.is_empty() {
            return Err(InputError::PromptCancelled);
        }

        let result = if self.trim {
            line.trim().to_string()
        } else {
            // Still need to remove trailing newline from read_line
            line.trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string()
        };

        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    fn can_retry(&self) -> bool {
        true
    }
}

/// Simple yes/no confirmation prompt.
///
/// Prompts the user for a yes/no response. Accepts y/yes/n/no (case-insensitive).
///
/// # Example
///
/// ```ignore
/// use standout_input::{InputChain, FlagSource, ConfirmPromptSource};
///
/// let chain = InputChain::<bool>::new()
///     .try_source(FlagSource::new("yes"))
///     .try_source(ConfirmPromptSource::new("Proceed?"));
///
/// let confirmed = chain.resolve(&matches)?;
/// ```
#[derive(Clone)]
pub struct ConfirmPromptSource<T: TerminalIO = RealTerminal> {
    terminal: Arc<T>,
    prompt: String,
    default: Option<bool>,
}

impl ConfirmPromptSource<RealTerminal> {
    /// Create a new confirmation prompt.
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            terminal: Arc::new(RealTerminal),
            prompt: prompt.into(),
            default: None,
        }
    }
}

impl<T: TerminalIO> ConfirmPromptSource<T> {
    /// Create a confirm prompt with a custom terminal for testing.
    pub fn with_terminal(prompt: impl Into<String>, terminal: T) -> Self {
        Self {
            terminal: Arc::new(terminal),
            prompt: prompt.into(),
            default: None,
        }
    }

    /// Set a default value for when the user presses Enter without input.
    ///
    /// The prompt suffix will change to indicate the default:
    /// - `None`: `[y/n]`
    /// - `Some(true)`: `[Y/n]`
    /// - `Some(false)`: `[y/N]`
    pub fn default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }
}

impl<T: TerminalIO + 'static> ConfirmPromptSource<T> {
    /// Prompt the user for a yes/no answer and return the resolved boolean.
    ///
    /// Standalone counterpart to [`InputCollector::collect`] for wizard /
    /// REPL flows that drive standout themselves and have no `&ArgMatches`
    /// to plumb through. Routes through any installed
    /// [`PromptResponder`](crate::PromptResponder).
    ///
    /// Errors:
    /// - [`InputError::PromptCancelled`] on EOF (Ctrl+D)
    /// - [`InputError::NoInput`] if stdin is not a TTY, *or* if the user
    ///   submits an empty line and no [`default`](Self::default) was set
    /// - [`InputError::ValidationFailed`] if the user enters something
    ///   that isn't a y/yes/n/no variant
    /// - [`InputError::PromptFailed`] on terminal I/O failure
    pub fn prompt(&self) -> Result<bool, InputError> {
        if let Some(value) =
            crate::responder::intercept_bool(crate::PromptKind::Confirm, &self.prompt)?
        {
            return Ok(value);
        }
        let matches = crate::collector::empty_matches();
        if !self.is_available(matches) {
            return Err(InputError::NoInput);
        }
        self.collect(matches)?.ok_or(InputError::NoInput)
    }
}

impl<T: TerminalIO + 'static> InputCollector<bool> for ConfirmPromptSource<T> {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.terminal.is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<bool>, InputError> {
        if !self.terminal.is_terminal() {
            return Ok(None);
        }

        let suffix = match self.default {
            None => "[y/n]",
            Some(true) => "[Y/n]",
            Some(false) => "[y/N]",
        };

        let full_prompt = format!("{} {} ", self.prompt, suffix);

        self.terminal
            .write_prompt(&full_prompt)
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        let line = self
            .terminal
            .read_line()
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        // Check for EOF
        if line.is_empty() {
            return Err(InputError::PromptCancelled);
        }

        let input = line.trim().to_lowercase();

        if input.is_empty() {
            // Use default if available, otherwise return None to continue chain
            return Ok(self.default);
        }

        match input.as_str() {
            "y" | "yes" => Ok(Some(true)),
            "n" | "no" => Ok(Some(false)),
            _ => {
                // Invalid input - for non-interactive we'd fail, but prompt can retry
                Err(InputError::ValidationFailed(
                    "Please enter 'y' or 'n'".to_string(),
                ))
            }
        }
    }

    fn can_retry(&self) -> bool {
        true
    }
}

/// Mock terminal for testing prompts.
#[derive(Debug)]
pub struct MockTerminal {
    is_terminal: bool,
    responses: Vec<String>,
    /// Index of the next response to return.
    response_index: std::sync::atomic::AtomicUsize,
}

impl Clone for MockTerminal {
    fn clone(&self) -> Self {
        Self {
            is_terminal: self.is_terminal,
            responses: self.responses.clone(),
            response_index: std::sync::atomic::AtomicUsize::new(
                self.response_index
                    .load(std::sync::atomic::Ordering::SeqCst),
            ),
        }
    }
}

impl MockTerminal {
    /// Create a mock that simulates a non-terminal.
    pub fn non_terminal() -> Self {
        Self {
            is_terminal: false,
            responses: vec![],
            response_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Create a mock terminal that returns the given response.
    pub fn with_response(response: impl Into<String>) -> Self {
        Self {
            is_terminal: true,
            responses: vec![response.into()],
            response_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Create a mock terminal that returns multiple responses in sequence.
    ///
    /// Useful for testing retry scenarios.
    pub fn with_responses(responses: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            is_terminal: true,
            responses: responses.into_iter().map(Into::into).collect(),
            response_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Create a mock that simulates EOF (Ctrl+D).
    pub fn eof() -> Self {
        Self {
            is_terminal: true,
            responses: vec![], // Empty vec means EOF
            response_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl TerminalIO for MockTerminal {
    fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    fn write_prompt(&self, _prompt: &str) -> io::Result<()> {
        // Mock doesn't actually write
        Ok(())
    }

    fn read_line(&self) -> io::Result<String> {
        let idx = self
            .response_index
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if idx < self.responses.len() {
            // Add newline like real read_line does
            Ok(format!("{}\n", self.responses[idx]))
        } else {
            // EOF
            Ok(String::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Command;

    fn empty_matches() -> ArgMatches {
        Command::new("test").try_get_matches_from(["test"]).unwrap()
    }

    // === TextPromptSource tests ===

    #[test]
    fn text_prompt_unavailable_when_not_terminal() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::non_terminal());
        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn text_prompt_available_when_terminal() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("test"));
        assert!(source.is_available(&empty_matches()));
    }

    #[test]
    fn text_prompt_collects_input() {
        let source =
            TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("Alice"));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("Alice".to_string()));
    }

    #[test]
    fn text_prompt_trims_whitespace() {
        let source =
            TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("  Bob  "));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("Bob".to_string()));
    }

    #[test]
    fn text_prompt_no_trim() {
        let source =
            TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("  Bob  "))
                .trim(false);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("  Bob  ".to_string()));
    }

    #[test]
    fn text_prompt_returns_none_for_empty() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response(""));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn text_prompt_returns_none_for_whitespace_only() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("   "));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn text_prompt_eof_cancels() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::eof());
        let result = source.collect(&empty_matches());
        assert!(matches!(result, Err(InputError::PromptCancelled)));
    }

    #[test]
    fn text_prompt_can_retry() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("test"));
        assert!(source.can_retry());
    }

    // === ConfirmPromptSource tests ===

    #[test]
    fn confirm_prompt_unavailable_when_not_terminal() {
        let source = ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::non_terminal());
        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn confirm_prompt_available_when_terminal() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response("y"));
        assert!(source.is_available(&empty_matches()));
    }

    #[test]
    fn confirm_prompt_yes() {
        for response in ["y", "Y", "yes", "YES", "Yes"] {
            let source = ConfirmPromptSource::with_terminal(
                "Proceed?",
                MockTerminal::with_response(response),
            );
            let result = source.collect(&empty_matches()).unwrap();
            assert_eq!(result, Some(true), "response '{}' should be true", response);
        }
    }

    #[test]
    fn confirm_prompt_no() {
        for response in ["n", "N", "no", "NO", "No"] {
            let source = ConfirmPromptSource::with_terminal(
                "Proceed?",
                MockTerminal::with_response(response),
            );
            let result = source.collect(&empty_matches()).unwrap();
            assert_eq!(
                result,
                Some(false),
                "response '{}' should be false",
                response
            );
        }
    }

    #[test]
    fn confirm_prompt_invalid_input() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response("maybe"));
        let result = source.collect(&empty_matches());
        assert!(matches!(result, Err(InputError::ValidationFailed(_))));
    }

    #[test]
    fn confirm_prompt_empty_with_default_true() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response(""))
                .default(true);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn confirm_prompt_empty_with_default_false() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response(""))
                .default(false);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn confirm_prompt_empty_without_default() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response(""));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn confirm_prompt_eof_cancels() {
        let source = ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::eof());
        let result = source.collect(&empty_matches());
        assert!(matches!(result, Err(InputError::PromptCancelled)));
    }

    #[test]
    fn confirm_prompt_can_retry() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response("y"));
        assert!(source.can_retry());
    }

    // === .prompt() shortcut ===
    //
    // Every test that calls .prompt() shares one #[serial] axis
    // (`prompt_responder`) because the global responder override is
    // process-wide; without serialization a responder installed by a
    // parallel responder-using test would leak into these vanilla
    // shortcut tests.

    use crate::{
        reset_default_prompt_responder, set_default_prompt_responder, PromptResponse,
        ScriptedResponder,
    };
    use serial_test::serial;
    use std::sync::Arc;

    #[test]
    #[serial(prompt_responder)]
    fn text_prompt_shortcut_returns_value() {
        let source =
            TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("Carol"));
        let value = source.prompt().unwrap();
        assert_eq!(value, "Carol");
    }

    #[test]
    #[serial(prompt_responder)]
    fn text_prompt_shortcut_maps_empty_to_no_input() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("   "));
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::NoInput));
    }

    #[test]
    #[serial(prompt_responder)]
    fn text_prompt_shortcut_propagates_cancel() {
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::eof());
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::PromptCancelled));
    }

    #[test]
    #[serial(prompt_responder)]
    fn text_prompt_shortcut_skips_when_not_terminal() {
        // .prompt() should still surface NoInput when the underlying source
        // declines (e.g. no TTY) — the wizard caller can decide what to do.
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::non_terminal());
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::NoInput));
    }

    #[test]
    #[serial(prompt_responder)]
    fn confirm_prompt_shortcut_returns_value() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response("y"));
        let value = source.prompt().unwrap();
        assert!(value);
    }

    #[test]
    #[serial(prompt_responder)]
    fn confirm_prompt_shortcut_propagates_cancel() {
        let source = ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::eof());
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::PromptCancelled));
    }

    #[test]
    #[serial(prompt_responder)]
    fn confirm_prompt_shortcut_uses_default_on_empty() {
        let source =
            ConfirmPromptSource::with_terminal("Proceed?", MockTerminal::with_response(""))
                .default(true);
        let value = source.prompt().unwrap();
        assert!(value);
    }

    // === .prompt() via PromptResponder ===

    struct ResponderGuard;
    impl ResponderGuard {
        fn install(responder: ScriptedResponder) -> Self {
            set_default_prompt_responder(Arc::new(responder));
            Self
        }
    }
    impl Drop for ResponderGuard {
        fn drop(&mut self) {
            reset_default_prompt_responder();
        }
    }

    #[test]
    #[serial(prompt_responder)]
    fn text_prompt_routes_through_responder_even_without_tty() {
        // The non-terminal MockTerminal would normally return NoInput from
        // prompt(); the responder gate runs *first*, so the responder wins.
        let _g = ResponderGuard::install(ScriptedResponder::new([PromptResponse::text("Ada")]));
        let source = TextPromptSource::with_terminal("Name: ", MockTerminal::non_terminal());
        let value = source.prompt().unwrap();
        assert_eq!(value, "Ada");
    }

    #[test]
    #[serial(prompt_responder)]
    fn confirm_prompt_routes_through_responder() {
        let _g = ResponderGuard::install(ScriptedResponder::new([PromptResponse::Bool(false)]));
        let source = ConfirmPromptSource::with_terminal("OK?", MockTerminal::non_terminal());
        let value = source.prompt().unwrap();
        assert!(!value);
    }
}
