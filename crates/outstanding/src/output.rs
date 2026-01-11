//! Output mode control for rendering.
//!
//! The [`OutputMode`] enum determines how output is rendered:
//! whether to include ANSI codes, render debug tags, or serialize as JSON.

use console::Term;

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
///     r#"{{ message | style("ok") }}"#,
///     &data,
///     &theme,
///     OutputMode::Auto,
/// ).unwrap();
///
/// // Force plain text
/// let plain = render_with_output(
///     r#"{{ message | style("ok") }}"#,
///     &data,
///     &theme,
///     OutputMode::Text,
/// ).unwrap();
/// assert_eq!(plain, "Hello");
///
/// // Debug mode - renders bracket tags
/// let debug = render_with_output(
///     r#"{{ message | style("ok") }}"#,
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
        matches!(self, OutputMode::Json)
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
}
