//! Environment abstractions for testability.
//!
//! This module provides traits that abstract over OS interactions,
//! allowing tests to run without depending on actual terminal state,
//! stdin piping, or clipboard contents.
//!
//! Production collection receives readers through [`crate::InputSources`].
//! Tests construct mocks ([`MockStdin`], [`MockClipboard`]) and put them on
//! that same type; there is no process-global default-reader override.

use std::io::{self, IsTerminal, Read};

use crate::InputError;

/// Abstraction over stdin reading.
///
/// This trait allows tests to mock stdin without actually piping data.
pub trait StdinReader: Send + Sync {
    /// Check if stdin is a terminal (TTY).
    ///
    /// Returns `true` if stdin is interactive, `false` if piped.
    fn is_terminal(&self) -> bool;

    /// Read all content from stdin.
    ///
    /// This should only be called if `is_terminal()` returns `false`.
    fn read_to_string(&self) -> io::Result<String>;
}

/// Abstraction over environment variables.
pub trait EnvReader: Send + Sync {
    /// Get an environment variable value.
    fn var(&self, name: &str) -> Option<String>;
}

/// Abstraction over clipboard access.
pub trait ClipboardReader: Send + Sync {
    /// Read text from the system clipboard.
    fn read(&self) -> Result<Option<String>, InputError>;
}

// === Real implementations ===

/// Real stdin reader using std::io.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealStdin;

impl StdinReader for RealStdin {
    fn is_terminal(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn read_to_string(&self) -> io::Result<String> {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        Ok(buffer)
    }
}

/// Real environment variable reader.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealEnv;

impl EnvReader for RealEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

/// Real clipboard reader using platform commands.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealClipboard;

impl ClipboardReader for RealClipboard {
    fn read(&self) -> Result<Option<String>, InputError> {
        read_clipboard_impl()
    }
}

#[cfg(target_os = "macos")]
fn read_clipboard_impl() -> Result<Option<String>, InputError> {
    let output = std::process::Command::new("pbpaste")
        .output()
        .map_err(|e| InputError::ClipboardFailed(e.to_string()))?;

    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout).to_string();
        if content.is_empty() {
            Ok(None)
        } else {
            Ok(Some(content))
        }
    } else {
        Ok(None)
    }
}

#[cfg(target_os = "linux")]
fn read_clipboard_impl() -> Result<Option<String>, InputError> {
    let output = std::process::Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
        .map_err(|e| InputError::ClipboardFailed(e.to_string()))?;

    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout).to_string();
        if content.is_empty() {
            Ok(None)
        } else {
            Ok(Some(content))
        }
    } else {
        Ok(None)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn read_clipboard_impl() -> Result<Option<String>, InputError> {
    Err(InputError::ClipboardFailed(
        "Clipboard not supported on this platform".to_string(),
    ))
}

// === Mock implementations for testing ===

/// Mock stdin reader for testing.
///
/// Allows tests to simulate both terminal and piped stdin.
#[derive(Debug, Clone)]
pub struct MockStdin {
    is_terminal: bool,
    content: Option<String>,
}

impl MockStdin {
    /// Create a mock that simulates a terminal (no piped input).
    pub fn terminal() -> Self {
        Self {
            is_terminal: true,
            content: None,
        }
    }

    /// Create a mock that simulates piped input.
    pub fn piped(content: impl Into<String>) -> Self {
        Self {
            is_terminal: false,
            content: Some(content.into()),
        }
    }

    /// Create a mock that simulates empty piped input.
    pub fn piped_empty() -> Self {
        Self {
            is_terminal: false,
            content: Some(String::new()),
        }
    }
}

impl StdinReader for MockStdin {
    fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    fn read_to_string(&self) -> io::Result<String> {
        Ok(self.content.clone().unwrap_or_default())
    }
}

/// Mock environment variable reader for testing.
#[derive(Debug, Clone, Default)]
pub struct MockEnv {
    vars: std::collections::HashMap<String, String>,
}

impl MockEnv {
    /// Create an empty mock environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an environment variable.
    pub fn with_var(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(name.into(), value.into());
        self
    }
}

impl EnvReader for MockEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }
}

/// Mock clipboard reader for testing.
#[derive(Debug, Clone, Default)]
pub struct MockClipboard {
    content: Option<String>,
}

impl MockClipboard {
    /// Create an empty clipboard mock.
    pub fn empty() -> Self {
        Self { content: None }
    }

    /// Create a clipboard mock with content.
    pub fn with_content(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
        }
    }
}

impl ClipboardReader for MockClipboard {
    fn read(&self) -> Result<Option<String>, InputError> {
        Ok(self.content.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_stdin_terminal() {
        let stdin = MockStdin::terminal();
        assert!(stdin.is_terminal());
    }

    #[test]
    fn mock_stdin_piped() {
        let stdin = MockStdin::piped("hello world");
        assert!(!stdin.is_terminal());
        assert_eq!(stdin.read_to_string().unwrap(), "hello world");
    }

    #[test]
    fn mock_stdin_piped_empty() {
        let stdin = MockStdin::piped_empty();
        assert!(!stdin.is_terminal());
        assert_eq!(stdin.read_to_string().unwrap(), "");
    }

    #[test]
    fn mock_env_empty() {
        let env = MockEnv::new();
        assert_eq!(env.var("MISSING"), None);
    }

    #[test]
    fn mock_env_with_vars() {
        let env = MockEnv::new()
            .with_var("EDITOR", "vim")
            .with_var("HOME", "/home/user");

        assert_eq!(env.var("EDITOR"), Some("vim".to_string()));
        assert_eq!(env.var("HOME"), Some("/home/user".to_string()));
        assert_eq!(env.var("MISSING"), None);
    }

    #[test]
    fn mock_clipboard_empty() {
        let clipboard = MockClipboard::empty();
        assert_eq!(clipboard.read().unwrap(), None);
    }

    #[test]
    fn mock_clipboard_with_content() {
        let clipboard = MockClipboard::with_content("clipboard text");
        assert_eq!(
            clipboard.read().unwrap(),
            Some("clipboard text".to_string())
        );
    }
}
