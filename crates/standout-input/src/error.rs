use std::io;

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("No editor found. Set VISUAL or EDITOR environment variable.")]
    NoEditor,

    #[error("Editor cancelled without saving.")]
    EditorCancelled,

    #[error("Editor failed: {0}")]
    EditorFailed(#[source] io::Error),

    #[error("Failed to read stdin: {0}")]
    StdinFailed(#[source] io::Error),

    #[error("StdinSource has no reader bound. Return a copy bound to the run's stdin from InputCollector::bind_sources, built with StdinSource::with_shared_reader(sources.stdin_arc()); StdinSource::with_reader takes an owned reader such as a MockStdin.")]
    StdinNotBound,

    #[error("Failed to read {path}: {source}")]
    FileFailed {
        path: String,
        #[source]
        source: io::Error,
    },

    #[error("Failed to read clipboard: {0}")]
    ClipboardFailed(String),

    #[error("Prompt cancelled by user.")]
    PromptCancelled,

    #[error("Prompt failed: {0}")]
    PromptFailed(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("No input provided and no default available.")]
    NoInput,

    #[error("Required argument '{0}' not provided.")]
    MissingArgument(String),

    #[error("Failed to parse argument '{name}': {reason}")]
    ParseError { name: String, reason: String },
}

impl InputError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::ValidationFailed(msg.into())
    }

    pub fn file(path: impl Into<String>, source: io::Error) -> Self {
        Self::FileFailed {
            path: path.into(),
            source,
        }
    }

    pub fn parse(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ParseError {
            name: name.into(),
            reason: reason.into(),
        }
    }
}
