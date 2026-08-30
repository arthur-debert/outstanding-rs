use std::io::{self, IsTerminal, Read};

use crate::InputError;

pub trait StdinReader: Send + Sync {
    fn is_terminal(&self) -> bool;

    fn read_to_string(&self) -> io::Result<String>;
}

pub trait EnvReader: Send + Sync {
    fn var(&self, name: &str) -> Option<String>;
}

pub trait ClipboardReader: Send + Sync {
    fn read(&self) -> Result<Option<String>, InputError>;
}

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

#[derive(Debug, Default, Clone, Copy)]
pub struct RealEnv;

impl EnvReader for RealEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

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

#[derive(Debug, Clone)]
pub struct MockStdin {
    is_terminal: bool,
    content: Option<String>,
}

impl MockStdin {
    pub fn terminal() -> Self {
        Self {
            is_terminal: true,
            content: None,
        }
    }

    pub fn piped(content: impl Into<String>) -> Self {
        Self {
            is_terminal: false,
            content: Some(content.into()),
        }
    }

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

#[derive(Debug, Clone, Default)]
pub struct MockEnv {
    vars: std::collections::HashMap<String, String>,
}

impl MockEnv {
    pub fn new() -> Self {
        Self::default()
    }

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

#[derive(Debug, Clone, Default)]
pub struct MockClipboard {
    content: Option<String>,
}

impl MockClipboard {
    pub fn empty() -> Self {
        Self { content: None }
    }

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
