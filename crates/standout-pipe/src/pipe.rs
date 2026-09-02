use crate::shell::{run_piped, ShellError};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum PipeError {
    #[error("Shell error: {0}")]
    Shell(#[from] ShellError),
}

pub trait PipeTarget: Send + Sync {
    fn pipe(&self, input: &str) -> Result<String, PipeError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeMode {
    Passthrough,
    Capture,
    Consume,
}

/// Shell-interpreted (`sh -c` / `cmd /C`): never build `command` from untrusted input.
pub struct SimplePipe {
    command: String,
    mode: PipeMode,
    timeout: Duration,
}

impl SimplePipe {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            mode: PipeMode::Passthrough,
            timeout: Duration::from_secs(30),
        }
    }

    pub fn capture(mut self) -> Self {
        self.mode = PipeMode::Capture;
        self
    }

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
