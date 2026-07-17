//! Environment abstractions for testability.
//!
//! This module provides traits that abstract over OS interactions,
//! allowing tests to run without depending on actual terminal state,
//! stdin piping, or clipboard contents.
//!
//! # Default readers and test overrides
//!
//! [`StdinSource::new`](crate::StdinSource::new) and
//! [`ClipboardSource::new`](crate::ClipboardSource::new) both use "default"
//! readers ([`DefaultStdin`], [`DefaultClipboard`]) that consult a
//! process-global override before falling back to the real OS-backed
//! implementation.
//!
//! Tests can swap in a mock without touching handler code by calling
//! [`set_default_stdin_reader`] / [`set_default_clipboard_reader`]. The
//! `TestHarness` in the `standout-test` crate wires these automatically.

use once_cell::sync::Lazy;
use std::io::{self, IsTerminal, Read};
use std::sync::{Arc, Mutex};

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

// === Process-global default reader overrides ===
//
// `StdinSource::new()` and `ClipboardSource::new()` resolve their reader
// through the `DefaultStdin` / `DefaultClipboard` shims below, which consult
// these slots before falling back to the real OS-backed readers. Tests use
// `set_default_*_reader` to install a mock for a serial scope; the
// `standout-test` `TestHarness` handles teardown via its `Drop` impl.

type SharedStdin = Arc<dyn StdinReader + Send + Sync>;
type SharedClipboard = Arc<dyn ClipboardReader + Send + Sync>;

static STDIN_OVERRIDE: Lazy<Mutex<Option<SharedStdin>>> = Lazy::new(|| Mutex::new(None));
static CLIPBOARD_OVERRIDE: Lazy<Mutex<Option<SharedClipboard>>> = Lazy::new(|| Mutex::new(None));

/// Installs a process-global stdin reader that [`DefaultStdin`] (and
/// therefore [`StdinSource::new`](crate::StdinSource::new)) will delegate
/// to until [`reset_default_stdin_reader`] is called.
///
/// Intended for test harnesses. Tests using this must run serially (e.g.
/// via `#[serial]`) because the override is process-global.
pub fn set_default_stdin_reader(reader: SharedStdin) {
    *STDIN_OVERRIDE.lock().unwrap() = Some(reader);
}

/// Clears the stdin override installed by [`set_default_stdin_reader`].
pub fn reset_default_stdin_reader() {
    *STDIN_OVERRIDE.lock().unwrap() = None;
}

/// Installs a process-global clipboard reader that [`DefaultClipboard`]
/// (and therefore [`ClipboardSource::new`](crate::ClipboardSource::new))
/// will delegate to until [`reset_default_clipboard_reader`] is called.
pub fn set_default_clipboard_reader(reader: SharedClipboard) {
    *CLIPBOARD_OVERRIDE.lock().unwrap() = Some(reader);
}

/// Clears the clipboard override installed by
/// [`set_default_clipboard_reader`].
pub fn reset_default_clipboard_reader() {
    *CLIPBOARD_OVERRIDE.lock().unwrap() = None;
}

fn current_stdin_override() -> Option<SharedStdin> {
    STDIN_OVERRIDE.lock().unwrap().clone()
}

fn current_clipboard_override() -> Option<SharedClipboard> {
    CLIPBOARD_OVERRIDE.lock().unwrap().clone()
}

/// Stdin reader used by [`StdinSource::new`](crate::StdinSource::new).
///
/// Delegates to a reader installed via [`set_default_stdin_reader`] if one
/// is present; otherwise falls back to [`RealStdin`]. The indirection lets
/// a test harness inject a [`MockStdin`] without reconstructing sources
/// inside handler code.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultStdin;

impl StdinReader for DefaultStdin {
    fn is_terminal(&self) -> bool {
        if let Some(r) = current_stdin_override() {
            return r.is_terminal();
        }
        RealStdin.is_terminal()
    }

    fn read_to_string(&self) -> io::Result<String> {
        if let Some(r) = current_stdin_override() {
            return r.read_to_string();
        }
        RealStdin.read_to_string()
    }
}

/// Clipboard reader used by
/// [`ClipboardSource::new`](crate::ClipboardSource::new).
///
/// Delegates to a reader installed via [`set_default_clipboard_reader`] if
/// one is present; otherwise falls back to [`RealClipboard`].
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultClipboard;

impl ClipboardReader for DefaultClipboard {
    fn read(&self) -> Result<Option<String>, InputError> {
        if let Some(r) = current_clipboard_override() {
            return r.read();
        }
        RealClipboard.read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    struct StdinOverrideGuard;

    impl Drop for StdinOverrideGuard {
        fn drop(&mut self) {
            reset_default_stdin_reader();
        }
    }

    struct ClipboardOverrideGuard;

    impl Drop for ClipboardOverrideGuard {
        fn drop(&mut self) {
            reset_default_clipboard_reader();
        }
    }

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

    #[test]
    #[serial]
    fn default_stdin_uses_override() {
        set_default_stdin_reader(Arc::new(MockStdin::piped("overridden")));
        let _guard = StdinOverrideGuard;
        let reader = DefaultStdin;
        assert!(!reader.is_terminal());
        assert_eq!(reader.read_to_string().unwrap(), "overridden");
    }

    #[test]
    #[serial]
    fn default_stdin_falls_back_without_override() {
        reset_default_stdin_reader();
        // Behaviour matches RealStdin; we can only assert it doesn't panic
        // and reports a terminal state consistent with the current process.
        let reader = DefaultStdin;
        let _ = reader.is_terminal();
    }

    #[test]
    #[serial]
    fn default_clipboard_uses_override() {
        set_default_clipboard_reader(Arc::new(MockClipboard::with_content("paste")));
        let _guard = ClipboardOverrideGuard;
        let reader = DefaultClipboard;
        assert_eq!(reader.read().unwrap(), Some("paste".to_string()));
    }
}
