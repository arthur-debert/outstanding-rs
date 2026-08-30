use std::fs;
use std::io::{self, Write};
use std::ops::ControlFlow;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::SystemTime;

use clap::ArgMatches;

use crate::collector::InputCollector;
use crate::responder::PromptResponder;
use crate::InputError;
use crate::InputSources;

pub trait EditorRunner: Send + Sync {
    fn detect_editor(&self) -> Option<String>;

    fn run(&self, editor: &str, path: &Path) -> io::Result<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RealEditorRunner;

impl EditorRunner for RealEditorRunner {
    fn detect_editor(&self) -> Option<String> {
        if let Ok(editor) = std::env::var("VISUAL") {
            if !editor.is_empty() && editor_exists(&editor) {
                return Some(editor);
            }
        }

        if let Ok(editor) = std::env::var("EDITOR") {
            if !editor.is_empty() && editor_exists(&editor) {
                return Some(editor);
            }
        }

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

fn editor_exists(editor: &str) -> bool {
    let cmd = editor.split_whitespace().next().unwrap_or(editor);
    which::which(cmd).is_ok()
}

#[derive(Clone)]
pub struct EditorSource<R: EditorRunner = RealEditorRunner> {
    runner: Arc<R>,
    initial_content: Option<String>,
    extension: String,
    require_save: bool,
    trim: bool,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl EditorSource<RealEditorRunner> {
    pub fn new() -> Self {
        Self {
            runner: Arc::new(RealEditorRunner),
            initial_content: None,
            extension: ".txt".to_string(),
            require_save: false,
            trim: true,
            responder: None,
        }
    }
}

impl Default for EditorSource<RealEditorRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: EditorRunner> EditorSource<R> {
    pub fn with_runner(runner: R) -> Self {
        Self {
            runner: Arc::new(runner),
            initial_content: None,
            extension: ".txt".to_string(),
            require_save: false,
            trim: true,
            responder: None,
        }
    }

    pub fn initial_content(mut self, content: impl Into<String>) -> Self {
        self.initial_content = Some(content.into());
        self
    }

    pub fn extension(mut self, ext: impl Into<String>) -> Self {
        self.extension = ext.into();
        self
    }

    pub fn require_save(mut self, require: bool) -> Self {
        self.require_save = require;
        self
    }

    pub fn trim(mut self, trim: bool) -> Self {
        self.trim = trim;
        self
    }
}

impl<R: EditorRunner + 'static> EditorSource<R> {
    pub fn prompt(&self) -> Result<String, InputError> {
        self.prompt_from(&InputSources::from_process())
    }

    pub fn prompt_from(&self, sources: &InputSources) -> Result<String, InputError> {
        crate::collector::prompt_value_from(self, sources)
    }
}

impl<R: EditorRunner + 'static> InputCollector<String> for EditorSource<R> {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.responder.is_some()
            || (self.runner.detect_editor().is_some() && std::io::stdin().is_terminal())
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        if let ControlFlow::Break(value) =
            crate::responder::collect_intercept(crate::responder::intercept_text(
                crate::PromptKind::Editor,
                &self.extension,
                self.responder.as_deref(),
            ))?
        {
            return Ok(value);
        }

        let editor = self.runner.detect_editor().ok_or(InputError::NoEditor)?;

        let mut builder = tempfile::Builder::new();
        builder.suffix(&self.extension);
        let temp_file = builder.tempfile().map_err(InputError::EditorFailed)?;

        let path = temp_file.path();

        if let Some(content) = &self.initial_content {
            fs::write(path, content).map_err(InputError::EditorFailed)?;
        }

        let initial_mtime = if self.require_save {
            get_mtime(path).ok()
        } else {
            None
        };

        self.runner
            .run(&editor, path)
            .map_err(InputError::EditorFailed)?;

        if let Some(initial) = initial_mtime {
            if let Ok(final_mtime) = get_mtime(path) {
                if initial == final_mtime {
                    return Err(InputError::EditorCancelled);
                }
            }
        }

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

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<String>>> {
        Some(Box::new(Self {
            runner: Arc::clone(&self.runner),
            initial_content: self.initial_content.clone(),
            extension: self.extension.clone(),
            require_save: self.require_save,
            trim: self.trim,
            responder: Some(sources.responder_arc()?),
        }))
    }

    fn can_retry(&self) -> bool {
        true
    }
}

fn get_mtime(path: &Path) -> io::Result<SystemTime> {
    fs::metadata(path)?.modified()
}

use std::io::IsTerminal;

#[derive(Debug, Clone)]
pub struct MockEditorRunner {
    editor: Option<String>,
    result: MockEditorResult,
}

#[derive(Debug, Clone)]
pub enum MockEditorResult {
    Success(String),
    Failure(String),
    NoSave,
}

impl MockEditorRunner {
    pub fn no_editor() -> Self {
        Self {
            editor: None,
            result: MockEditorResult::Failure("no editor".to_string()),
        }
    }

    pub fn with_result(content: impl Into<String>) -> Self {
        Self {
            editor: Some("mock-editor".to_string()),
            result: MockEditorResult::Success(content.into()),
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            editor: Some("mock-editor".to_string()),
            result: MockEditorResult::Failure(message.into()),
        }
    }

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
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(path)?;
                file.write_all(content.as_bytes())?;
                Ok(())
            }
            MockEditorResult::Failure(msg) => Err(io::Error::other(msg.clone())),
            MockEditorResult::NoSave => Ok(()),
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
        let source = EditorSource::with_runner(MockEditorRunner::no_editor());
        let err = source.prompt().unwrap_err();
        assert!(matches!(err, InputError::NoInput));
    }

    #[test]
    fn editor_prompt_routes_through_responder_without_launching_editor() {
        let sources = sources_with(ScriptedResponder::new([PromptResponse::text(
            "edited body",
        )]));
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
