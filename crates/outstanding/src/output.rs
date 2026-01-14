//! Output mode control for rendering.
//!
//! The [`OutputMode`] enum determines how output is rendered:
//! whether to include ANSI codes, render debug tags, or serialize as JSON.

use console::Term;
use std::io::Write;

/// Destination for rendered output.
///
/// Determines where the output should be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputDestination {
    /// Write to standard output
    Stdout,
    /// Write to a specific file
    File(std::path::PathBuf),
}

/// Validates that a file path is safe to write to.
///
/// Returns an error if the parent directory doesn't exist.
fn validate_path(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Parent directory does not exist: {}", parent.display()),
            ));
        }
    }
    Ok(())
}

/// Writes text content to the specified destination.
///
/// - `Stdout`: Writes to stdout with a newline
/// - `File`: Writes to the file (overwriting)
pub fn write_output(content: &str, dest: &OutputDestination) -> std::io::Result<()> {
    match dest {
        OutputDestination::Stdout => {
            // Use println! logic (writeln to stdout)
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            writeln!(handle, "{}", content)
        }
        OutputDestination::File(path) => {
            validate_path(path)?;
            std::fs::write(path, content)
        }
    }
}

/// Writes binary content to the specified destination.
///
/// - `Stdout`: Writes raw bytes to stdout
/// - `File`: Writes to the file (overwriting)
pub fn write_binary_output(content: &[u8], dest: &OutputDestination) -> std::io::Result<()> {
    match dest {
        OutputDestination::Stdout => {
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(content)
        }
        OutputDestination::File(path) => {
            validate_path(path)?;
            std::fs::write(path, content)
        }
    }
}

/// Controls how output is rendered.
///
/// This determines whether ANSI escape codes are included in the output,
/// or whether to output structured data formats like JSON.
///
/// # Variants
///
/// - `Auto` - Detect terminal capabilities automatically (default behavior)
/// - `Term` - Always include ANSI escape codes (for terminal output)
/// - `Text` - Never include ANSI escape codes (plain text)
/// - `TermDebug` - Render style names as bracket tags for debugging
/// - `Json` - Serialize data as JSON (skips template rendering)
///
/// # Example
///
/// ```rust
/// use outstanding::{render_with_output, Theme, OutputMode};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data { message: String }
///
/// let theme = Theme::new().add("ok", Style::new().green());
/// let data = Data { message: "Hello".into() };
///
/// // Auto-detect (default)
/// let auto = render_with_output(
///     r#"[ok]{{ message }}[/ok]"#,
///     &data,
///     &theme,
///     OutputMode::Auto,
/// ).unwrap();
///
/// // Force plain text
/// let plain = render_with_output(
///     r#"[ok]{{ message }}[/ok]"#,
///     &data,
///     &theme,
///     OutputMode::Text,
/// ).unwrap();
/// assert_eq!(plain, "Hello");
///
/// // Debug mode - renders bracket tags
/// let debug = render_with_output(
///     r#"[ok]{{ message }}[/ok]"#,
///     &data,
///     &theme,
///     OutputMode::TermDebug,
/// ).unwrap();
/// assert_eq!(debug, "[ok]Hello[/ok]");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// Auto-detect terminal capabilities
    #[default]
    Auto,
    /// Always use ANSI escape codes (terminal output)
    Term,
    /// Never use ANSI escape codes (plain text)
    Text,
    /// Debug mode: render style names as bracket tags `[name]text[/name]`
    TermDebug,
    /// Structured output: serialize data as JSON (skips template rendering)
    Json,
    /// Structured output: serialize data as YAML (skips template rendering)
    Yaml,
    /// Structured output: serialize data as XML (skips template rendering)
    Xml,
    /// Structured output: serialize flattened data as CSV (skips template rendering)
    Csv,
}

impl OutputMode {
    /// Resolves the output mode to a concrete decision about whether to use color.
    ///
    /// - `Auto` checks terminal capabilities
    /// - `Term` always returns `true`
    /// - `Text` always returns `false`
    /// - `TermDebug` returns `false` (handled specially by apply methods)
    /// - `Json` returns `false` (structured output, no ANSI codes)
    pub fn should_use_color(&self) -> bool {
        match self {
            OutputMode::Auto => Term::stdout().features().colors_supported(),
            OutputMode::Term => true,
            OutputMode::Text => false,
            OutputMode::TermDebug => false, // Handled specially
            OutputMode::Json => false,      // Structured output
            OutputMode::Yaml => false,      // Structured output
            OutputMode::Xml => false,       // Structured output
            OutputMode::Csv => false,       // Structured output
        }
    }

    /// Returns true if this is debug mode (bracket tags instead of ANSI).
    pub fn is_debug(&self) -> bool {
        matches!(self, OutputMode::TermDebug)
    }

    /// Returns true if this is a structured output mode (JSON, etc.).
    ///
    /// Structured modes serialize data directly instead of rendering templates.
    pub fn is_structured(&self) -> bool {
        matches!(
            self,
            OutputMode::Json | OutputMode::Yaml | OutputMode::Xml | OutputMode::Csv
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_mode_term_should_use_color() {
        assert!(OutputMode::Term.should_use_color());
    }

    #[test]
    fn test_output_mode_text_should_not_use_color() {
        assert!(!OutputMode::Text.should_use_color());
    }

    #[test]
    fn test_output_mode_default_is_auto() {
        assert_eq!(OutputMode::default(), OutputMode::Auto);
    }

    #[test]
    fn test_output_mode_term_debug_is_debug() {
        assert!(OutputMode::TermDebug.is_debug());
        assert!(!OutputMode::Auto.is_debug());
        assert!(!OutputMode::Term.is_debug());
        assert!(!OutputMode::Text.is_debug());
        assert!(!OutputMode::Json.is_debug());
    }

    #[test]
    fn test_output_mode_term_debug_should_not_use_color() {
        assert!(!OutputMode::TermDebug.should_use_color());
    }

    #[test]
    fn test_output_mode_json_should_not_use_color() {
        assert!(!OutputMode::Json.should_use_color());
    }

    #[test]
    fn test_output_mode_json_is_structured() {
        assert!(OutputMode::Json.is_structured());
    }

    #[test]
    fn test_output_mode_non_json_not_structured() {
        assert!(!OutputMode::Auto.is_structured());
        assert!(!OutputMode::Term.is_structured());
        assert!(!OutputMode::Text.is_structured());
        assert!(!OutputMode::TermDebug.is_structured());
    }

    #[test]
    fn test_output_mode_json_not_debug() {
        assert!(!OutputMode::Json.is_debug());
    }

    #[test]
    fn test_write_output_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        let dest = OutputDestination::File(file_path.clone());

        write_output("hello", &dest).unwrap();

        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_write_output_file_overwrite() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        std::fs::write(&file_path, "initial").unwrap();

        let dest = OutputDestination::File(file_path.clone());
        write_output("new", &dest).unwrap();

        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "new");
    }

    #[test]
    fn test_write_output_binary_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.bin");
        let dest = OutputDestination::File(file_path.clone());

        write_binary_output(&[1, 2, 3], &dest).unwrap();

        let content = std::fs::read(&file_path).unwrap();
        assert_eq!(content, vec![1, 2, 3]);
    }

    #[test]
    fn test_write_output_invalid_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("missing").join("output.txt");
        let dest = OutputDestination::File(file_path);

        let result = write_output("hello", &dest);
        assert!(result.is_err());
    }
}
