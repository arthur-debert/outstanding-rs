use crate::shell::{run_piped, ShellError};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum PipeError {
    #[error("Shell error: {0}")]
    Shell(#[from] ShellError),
}

/// A target that can receive piped output
pub trait PipeTarget: Send + Sync {
    /// Pipe the input to the target and return the resulting output.
    ///
    /// If the target is configured to 'capture', the returned string is the command's stdout.
    /// If the target is 'passthrough', the returned string is the original input.
    /// If the target is 'consume', the returned string is empty (or filtered out by caller).
    fn pipe(&self, input: &str) -> Result<String, PipeError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeMode {
    /// Pipe to command, but ignore its output and return the original input.
    /// Used for side-effects like logging or clipboard where we still want to see the output.
    Passthrough,
    /// Pipe to command and use its output as the new result.
    /// Used for filters like `jq` or `sort`.
    Capture,
    /// Pipe to command and suppress further output.
    /// Used when the pipe destination is the final consumer (e.g. strict clipboard only).
    Consume,
}

/// A simple pipe that executes a shell command with input on stdin.
///
/// # Security Warning
///
/// The command string is passed directly to the shell (`sh -c` on Unix, `cmd /C` on Windows).
/// If you construct the command from untrusted input, you risk shell injection attacks.
///
/// ```ignore
/// // DANGEROUS if `user_input` is untrusted:
/// SimplePipe::new(format!("grep {}", user_input))
///
/// // SAFE alternatives:
/// // 1. Use a fixed command string
/// SimplePipe::new("grep pattern")
///
/// // 2. Validate/sanitize user input before interpolation
/// let sanitized = sanitize_for_shell(user_input);
/// SimplePipe::new(format!("grep {}", sanitized))
/// ```
pub struct SimplePipe {
    command: String,
    mode: PipeMode,
    timeout: Duration,
}

impl SimplePipe {
    /// Create a new pipe that executes the given shell command.
    ///
    /// The default mode is [`PipeMode::Passthrough`] with a 30-second timeout.
    ///
    /// # Security
    ///
    /// See the struct-level documentation for shell injection warnings.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            mode: PipeMode::Passthrough,
            timeout: Duration::from_secs(30),
        }
    }

    /// Use the command's stdout as the new output.
    pub fn capture(mut self) -> Self {
        self.mode = PipeMode::Capture;
        self
    }

    /// Don't print anything to the terminal after piping.
    pub fn consume(mut self) -> Self {
        self.mode = PipeMode::Consume;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl PipeTarget for SimplePipe {
    fn pipe(&self, input: &str) -> Result<String, PipeError> {
        let cmd_output = run_piped(&self.command, input, Some(self.timeout))?;

        match self.mode {
            PipeMode::Passthrough => Ok(input.to_string()),
            PipeMode::Capture => Ok(cmd_output),
            PipeMode::Consume => Ok(String::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_pipe_passthrough() {
        let pipe = SimplePipe::new(if cfg!(windows) {
            "findstr foo"
        } else {
            "grep foo"
        });
        // Passthrough should return ORIGINAL input, but the command is executed.
        let input = "foo\nbar";
        let output = pipe.pipe(input).unwrap();
        assert_eq!(output, "foo\nbar");
    }

    #[test]
    fn test_simple_pipe_capture() {
        let pipe = SimplePipe::new(if cfg!(windows) {
            "findstr foo"
        } else {
            "grep foo"
        })
        .capture();
        let input = "foo\nbar";
        let output = pipe.pipe(input).unwrap();
        assert_eq!(output.trim(), "foo");
    }

    #[test]
    fn test_simple_pipe_consume() {
        let pipe = SimplePipe::new(if cfg!(windows) {
            "findstr foo"
        } else {
            "grep foo"
        })
        .consume();
        let input = "foo\nbar";
        let output = pipe.pipe(input).unwrap();
        assert_eq!(output, "");
    }
}
