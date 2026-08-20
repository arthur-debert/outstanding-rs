//! Editor-based input source.
//!
//! Opens the user's preferred text editor for multi-line input.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::SystemTime;

use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::InputError;
use crate::InputSources;

/// Abstraction over editor invocation for testability.
pub trait EditorRunner: Send + Sync {
    /// Detect the editor to use.
    ///
    /// Returns `None` if no editor is available.
    fn detect_editor(&self) -> Option<String>;

    /// Run the editor on the given file path.
    ///
    /// Returns `Ok(())` if the editor exited successfully, `Err` otherwise.
    fn run(&self, editor: &str, path: &Path) -> io::Result<()>;
}

/// Real editor runner using system commands.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealEditorRunner;

impl EditorRunner for RealEditorRunner {
    fn detect_editor(&self) -> Option<String> {
        // Try VISUAL first (supports GUI editors)
        if let Ok(editor) = std::env::var("VISUAL") {
            if !editor.is_empty() && editor_exists(&editor) {
                return Some(editor);
            }
        }

        // Fall back to EDITOR
        if let Ok(editor) = std::env::var("EDITOR") {
            if !editor.is_empty() && editor_exists(&editor) {
                return Some(editor);
            }
        }

        // Platform-specific fallbacks
        #[cfg(unix)]
        {
            for fallback in ["vim", "vi", "nano"] {
                if editor_exists(fallback) {
                    return Some(fallback.to_string());
                }
            }
        }

        #[cfg(windows)]
        {
            if editor_exists("notepad") {
                return Some("notepad".to_string());
            }
        }

        None
    }

    fn run(&self, editor: &str, path: &Path) -> io::Result<()> {
        // Parse the editor command to handle cases like "code --wait" or "vim -u NONE"
        let parts = shell_words::split(editor).map_err(|e| {
            io::Error::other(format!(
                "Failed to parse editor command '{}': {}",
                editor, e
            ))
        })?;

        if parts.is_empty() {
            return Err(io::Error::other("Editor command is empty"));
        }

        let (cmd, args) = parts.split_first().unwrap();
        let status = Command::new(cmd).args(args).arg(path).status()?;

        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "Editor exited with status: {}",
                status
            )))
        }
    }
}

/// Check if an editor command exists.
fn editor_exists(editor: &str) -> bool {
    // Extract the command name (first word) in case of "vim -u NONE" etc.
    let cmd = editor.split_whitespace().next().unwrap_or(editor);
    which::which(cmd).is_ok()
}

/// Collect input via an external text editor.
///
/// Opens the user's preferred editor (from `$VISUAL` or `$EDITOR`) with a
/// temporary file, waits for the user to save and close, then reads the result.
///
/// # Editor Detection
///
/// Editors are detected in this order:
/// 1. `$VISUAL` environment variable (supports GUI editors)
/// 2. `$EDITOR` environment variable
/// 3. Platform fallbacks: `vim`, `vi`, `nano` on Unix; `notepad` on Windows
///
/// # Example
///
/// ```ignore
/// use standout_input::{InputChain, ArgSource, EditorSource};
///
/// // Fall back to editor if no CLI argument
/// let chain = InputChain::<String>::new()
///     .try_source(ArgSource::new("message"))
///     .try_source(EditorSource::new());
///
/// let message = chain.resolve(&matches)?;
/// ```
///
/// # Configuration
///
/// ```ignore
/// let source = EditorSource::new()
///     .initial_content("# Enter your message\n\n")
///     .extension(".md")
///     .require_save(true);
/// ```
#[derive(Clone)]
pub struct EditorSource<R: EditorRunner = RealEditorRunner> {
    runner: Arc<R>,
    initial_content: Option<String>,
    extension: String,
    require_save: bool,
    trim: bool,
}

impl EditorSource<RealEditorRunner> {
    /// Create a new editor source using the system editor.
    pub fn new() -> Self {
        Self {
            runner: Arc::new(RealEditorRunner),
            initial_content: None,
            extension: ".txt".to_string(),
            require_save: false,
            trim: true,
        }
    }
}

impl Default for EditorSource<RealEditorRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: EditorRunner> EditorSource<R> {
    /// Create an editor source with a custom runner.
    ///
    /// Primarily used for testing to mock editor invocation.
    pub fn with_runner(runner: R) -> Self {
        Self {
            runner: Arc::new(runner),
            initial_content: None,
            extension: ".txt".to_string(),
            require_save: false,
            trim: true,
        }
    }

    /// Set initial content to populate the editor with.
    ///
    /// This can be used to provide a template or instructions.
    pub fn initial_content(mut self, content: impl Into<String>) -> Self {
        self.initial_content = Some(content.into());
        self
    }

    /// Set the file extension for the temporary file.
    ///
    /// This affects syntax highlighting in the editor.
    /// Default is `.txt`.
    pub fn extension(mut self, ext: impl Into<String>) -> Self {
        self.extension = ext.into();
        self
    }

    /// Require the user to actually save the file.
    ///
    /// If `true`, the source will return `None` if the file's modification
    /// time hasn't changed (i.e., the user closed without saving).
    /// Default is `false`.
    pub fn require_save(mut self, require: bool) -> Self {
        self.require_save = require;
        self
    }

    /// Control whether to trim whitespace from the result.
    ///
    /// Default is `true`.
    pub fn trim(mut self, trim: bool) -> Self {
        self.trim = trim;
        self
    }
}

impl<R: EditorRunner + 'static> EditorSource<R> {
    /// Open the editor and return the saved content.
    ///
    /// This is the standalone counterpart to [`InputCollector::collect`]:
    /// it skips the chain machinery (no `&ArgMatches` to plumb through) and
    /// is intended for wizard or REPL flows that drive standout themselves.
    ///
    /// Returns [`InputError::NoInput`] if stdin is not a TTY or no editor
    /// can be detected (the same conditions under which a chain would skip
    /// this source). User cancellation is reported as
    /// [`InputError::EditorCancelled`] when `require_save` is set and the
    /// user exits without saving.
    ///
    /// Routes through a [`PromptResponder`](crate::PromptResponder) on
    /// [`InputSources`] when one is supplied, so wizard tests can supply the
    /// editor's "saved" content directly without launching `$EDITOR`.
    pub fn prompt(&self) -> Result<String, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    /// [`prompt`](Self::prompt) against explicit [`InputSources`].
    pub fn prompt_from(&self, sources: &InputSources) -> Result<String, InputError> {
        if let Some(value) = crate::responder::intercept_text(
            crate::PromptKind::Editor,
            // EditorSource has no user-facing "message" — use the file
            // extension as the diagnostic hint so panic messages still
            // identify which editor source failed.
            &self.extension,
            sources.responder(),
        )? {
            return Ok(value);
        }
        let matches = crate::collector::empty_matches();
        if !self.is_available(matches) {
            return Err(InputError::NoInput);
        }
        self.collect(matches)?.ok_or(InputError::NoInput)
    }
}

impl<R: EditorRunner + 'static> InputCollector<String> for EditorSource<R> {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        // Editor is available if we can detect one and we have a TTY
        self.runner.detect_editor().is_some() && std::io::stdin().is_terminal()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        let editor = self.runner.detect_editor().ok_or(InputError::NoEditor)?;

        // Create a temporary file with the specified extension
        let mut builder = tempfile::Builder::new();
        builder.suffix(&self.extension);
        let temp_file = builder.tempfile().map_err(InputError::EditorFailed)?;

        let path = temp_file.path();

        // Write initial content if provided
        if let Some(content) = &self.initial_content {
            fs::write(path, content).map_err(InputError::EditorFailed)?;
        }

        // Record initial modification time if we need to check for save
        let initial_mtime = if self.require_save {
            get_mtime(path).ok()
        } else {
            None
        };

        // Run the editor
        self.runner
            .run(&editor, path)
            .map_err(InputError::EditorFailed)?;

        // Check if user actually saved (if required)
        if let Some(initial) = initial_mtime {
            if let Ok(final_mtime) = get_mtime(path) {
                if initial == final_mtime {
                    return Err(InputError::EditorCancelled);
                }
            }
        }

        // Read the result
        let content = fs::read_to_string(path).map_err(InputError::EditorFailed)?;

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

    fn can_retry(&self) -> bool {
        // Editor is interactive, so we can retry on validation failure
        true
    }
}

/// Get the modification time of a file.
fn get_mtime(path: &Path) -> io::Result<SystemTime> {
    fs::metadata(path)?.modified()
}

use std::io::IsTerminal;

/// Mock editor runner for testing.
///
/// Simulates editor behavior without actually launching an editor.
#[derive(Debug, Clone)]
pub struct MockEditorRunner {
    editor: Option<String>,
    result: MockEditorResult,
}

/// The result of a mock editor invocation.
#[derive(Debug, Clone)]
pub enum MockEditorResult {
    /// Editor writes this content and exits successfully.
    Success(String),
    /// Editor fails with an error.
    Failure(String),
    /// Editor exits without saving (for require_save tests).
    NoSave,
}

impl MockEditorRunner {
    /// Create a mock that simulates no editor available.
    pub fn no_editor() -> Self {
        Self {
            editor: None,
            result: MockEditorResult::Failure("no editor".to_string()),
        }
    }

    /// Create a mock that simulates successful editor input.
    pub fn with_result(content: impl Into<String>) -> Self {
        Self {
            editor: Some("mock-editor".to_string()),
            result: MockEditorResult::Success(content.into()),
        }
    }

    /// Create a mock that simulates editor failure.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            editor: Some("mock-editor".to_string()),
            result: MockEditorResult::Failure(message.into()),
        }
    }

    /// Create a mock that simulates closing without saving.
    pub fn no_save() -> Self {
        Self {
            editor: Some("mock-editor".to_string()),
            result: MockEditorResult::NoSave,
        }
    }
}

impl EditorRunner for MockEditorRunner {
    fn detect_editor(&self) -> Option<String> {
        self.editor.clone()
    }

    fn run(&self, _editor: &str, path: &Path) -> io::Result<()> {
        match &self.result {
            MockEditorResult::Success(content) => {
                // Write the mock content to the file
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(path)?;
                file.write_all(content.as_bytes())?;
                Ok(())
            }
            MockEditorResult::Failure(msg) => Err(io::Error::other(msg.clone())),
            MockEditorResult::NoSave => {
                // Don't modify the file at all
                Ok(())
            }
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

    #[test]
    fn editor_unavailable_when_no_editor() {
        let source = EditorSource::with_runner(MockEditorRunner::no_editor());
        assert!(!source.is_available(&empty_matches()));
    }

    #[test]
    fn editor_collects_input() {
        let source = EditorSource::with_runner(MockEditorRunner::with_result("hello from editor"));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("hello from editor".to_string()));
    }

    #[test]
    fn editor_trims_whitespace() {
        let source = EditorSource::with_runner(MockEditorRunner::with_result("  hello  \n\n"));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn editor_no_trim() {
        let source =
            EditorSource::with_runner(MockEditorRunner::with_result("  hello  \n")).trim(false);
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("  hello  \n".to_string()));
    }

    #[test]
    fn editor_returns_none_for_empty() {
        let source = EditorSource::with_runner(MockEditorRunner::with_result(""));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn editor_returns_none_for_whitespace_only() {
        let source = EditorSource::with_runner(MockEditorRunner::with_result("   \n\t  "));
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn editor_handles_failure() {
        let source = EditorSource::with_runner(MockEditorRunner::failure("editor crashed"));
        let result = source.collect(&empty_matches());
        assert!(matches!(result, Err(InputError::EditorFailed(_))));
    }

    #[test]
    fn editor_with_initial_content() {
        // The mock runner ignores initial content since it writes its own result
        let source = EditorSource::with_runner(MockEditorRunner::with_result("user input"))
            .initial_content("# Template\n\n");
        let result = source.collect(&empty_matches()).unwrap();
        assert_eq!(result, Some("user input".to_string()));
    }

    #[test]
    fn editor_can_retry() {
        let source = EditorSource::with_runner(MockEditorRunner::with_result("test"));
        assert!(source.can_retry());
    }

    #[test]
    fn editor_no_editor_error() {
        let source = EditorSource::with_runner(MockEditorRunner::no_editor());
        let result = source.collect(&empty_matches());
        assert!(matches!(result, Err(InputError::NoEditor)));
    }

    // === .prompt() shortcut ===
    //
    // EditorSource::is_available checks std::io::stdin().is_terminal() directly,
    // so under `cargo test` (no TTY) prompt() always short-circuits to NoInput.
    // The happy path with the mock runner is covered by the existing
    // editor_collects_content / editor_failure / editor_no_editor_error tests
    // on collect(), which prompt() delegates to once the TTY gate passes.

    use crate::{InputSources, PromptResponse, ScriptedResponder};
    use std::sync::Arc;

    fn sources_with(responder: ScriptedResponder) -> InputSources {
        InputSources::from_process().with_responder(Arc::new(responder))
    }

    #[test]
    fn editor_prompt_shortcut_returns_no_input_in_non_tty() {
        let source = EditorSource::with_runner(MockEditorRunner::with_result("hello"));
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::NoInput));
    }

    #[test]
    fn editor_prompt_shortcut_no_input_when_no_editor_detected() {
        // No TTY *and* no editor — both fail is_available, so NoInput either way.
        let source = EditorSource::with_runner(MockEditorRunner::no_editor());
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::NoInput));
    }

    #[test]
    fn editor_prompt_routes_through_responder_without_launching_editor() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::text(
            "edited body",
        )]));
        // Even with a no-editor mock runner, the responder gate runs first
        // and wins — the editor is never launched in tests.
        let source = EditorSource::with_runner(MockEditorRunner::no_editor());
        let value = source.prompt_from(&sources).unwrap();
        assert_eq!(value, "edited body");
    }

    #[test]
    fn editor_prompt_responder_cancel_propagates() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::Cancel]));
        let source = EditorSource::with_runner(MockEditorRunner::no_editor());
        let err = source.prompt_from(&sources).unwrap_err();
        assert!(matches!(err, InputError::PromptCancelled));
    }
}
