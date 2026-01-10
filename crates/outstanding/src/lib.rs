//! # Outstanding - Non-Interactive CLI Framework
//!
//! Outstanding is a CLI output framework that decouples your application logic from
//! terminal presentation. It provides:
//!
//! - **Template rendering** with MiniJinja + styled output
//! - **Themes** for named style definitions (colors, bold, etc.)
//! - **Automatic terminal capability detection** (TTY, CLICOLOR, etc.)
//! - **Output mode control** (Auto/Term/Text/TermDebug)
//! - **Help topics system** for extended documentation
//! - **Pager support** for long content
//!
//! This crate is **CLI-agnostic** - it doesn't care how you parse arguments.
//! For easy integration with clap, see the `outstanding-clap` crate.
//!
//! ## Core Concepts
//!
//! - [`Theme`]: Named collection of `console::Style` values (e.g., `"header"` → bold cyan)
//! - [`AdaptiveTheme`]: Light/dark theme pair with OS detection
//! - [`OutputMode`]: Control output formatting (Auto/Term/Text/TermDebug)
//! - [`topics`]: Help topics system for extended documentation
//! - `style` filter: `{{ value | style("name") }}` applies registered styles in templates
//! - [`Renderer`]: Pre-compile templates for repeated rendering
//!
//! ## Quick Start
//!
//! ```rust
//! use outstanding::{render, Theme, ThemeChoice};
//! use console::Style;
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct Summary {
//!     title: String,
//!     total: usize,
//! }
//!
//! let theme = Theme::new()
//!     .add("title", Style::new().bold())
//!     .add("count", Style::new().cyan());
//!
//! let template = r#"
//! {{ title | style("title") }}
//! ---------------------------
//! Total items: {{ total | style("count") }}
//! "#;
//!
//! let output = render(
//!     template,
//!     &Summary { title: "Report".into(), total: 3 },
//!     ThemeChoice::from(&theme),
//! ).unwrap();
//! println!("{}", output);
//! ```
//!
//! ## Adaptive Themes (Light & Dark)
//!
//! ```rust
//! use outstanding::{AdaptiveTheme, Theme, ThemeChoice, OutputMode};
//! use console::Style;
//!
//! let light = Theme::new().add("tone", Style::new().green());
//! let dark  = Theme::new().add("tone", Style::new().yellow().italic());
//! let adaptive = AdaptiveTheme::new(light, dark);
//!
//! // Automatically renders with the user's OS theme (via the `dark-light` crate)
//! let banner = outstanding::render_with_output(
//!     r#"Mode: {{ "active" | style("tone") }}"#,
//!     &serde_json::json!({}),
//!     ThemeChoice::Adaptive(&adaptive),
//!     OutputMode::Term,
//! ).unwrap();
//! ```
//!
//! ## Rendering Strategy
//!
//! 1. Build a [`Theme`] (or [`AdaptiveTheme`]) using the fluent `console::Style` API.
//! 2. Load/define templates using regular MiniJinja syntax (`{{ value }}`, `{% for %}`, etc.).
//! 3. Call [`render`] for ad-hoc rendering or create a [`Renderer`] if you have many templates.
//! 4. Outstanding injects the `style` filter, auto-detects colors, and returns the final string.
//!
//! Everything from the theme inward is pure Rust data: no code outside Outstanding needs
//! to touch stdout/stderr or ANSI escape sequences directly.
//!
//! ## More Examples
//!
//! ```rust
//! use outstanding::{Renderer, Theme};
//! use console::Style;
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct Entry { label: String, value: i32 }
//!
//! let theme = Theme::new()
//!     .add("label", Style::new().bold())
//!     .add("value", Style::new().green());
//!
//! let mut renderer = Renderer::new(theme).unwrap();
//! renderer.add_template("row", "{{ label | style(\"label\") }}: {{ value | style(\"value\") }}").unwrap();
//! let rendered = renderer.render("row", &Entry { label: "Count".into(), value: 42 }).unwrap();
//! assert_eq!(rendered, "Count: 42");
//! ```
//!
//! ## Help Topics System
//!
//! The [`topics`] module provides a help topics system for extended documentation:
//!
//! ```rust
//! use outstanding::topics::{Topic, TopicRegistry, TopicType, render_topic};
//!
//! // Create and populate a registry
//! let mut registry = TopicRegistry::new();
//! registry.add_topic(Topic::new(
//!     "Storage",
//!     "Notes are stored in ~/.notes/\n\nEach note is a separate file.",
//!     TopicType::Text,
//!     Some("storage".to_string()),
//! ));
//!
//! // Render a topic
//! if let Some(topic) = registry.get_topic("storage") {
//!     let output = render_topic(topic, None).unwrap();
//!     println!("{}", output);
//! }
//!
//! // Load topics from a directory
//! registry.add_from_directory_if_exists("docs/topics").ok();
//! ```
//!
//! ## Integration with Clap
//!
//! For clap-based CLIs, use the `outstanding-clap` crate which handles:
//! - Help command interception (`help`, `help <topic>`, `help topics`)
//! - Output flag injection (`--output=auto|term|text`)
//! - Styled help rendering
//!
//! ```rust,ignore
//! use clap::Command;
//! use outstanding_clap::Outstanding;
//!
//! // Simplest usage - all features enabled by default
//! let matches = Outstanding::run(Command::new("my-app"));
//! ```

pub mod table;
pub mod topics;

use console::{Style, Term};
use dark_light::{detect as detect_os_theme, Mode as OsThemeMode};
use minijinja::{Environment, Value};
pub use minijinja::Error;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

/// Default prefix shown when a style name is not found.
pub const DEFAULT_MISSING_STYLE_INDICATOR: &str = "(!?)";

/// A style value that can be either a concrete style or an alias to another style.
///
/// This enables layered styling where semantic styles can reference presentation
/// styles, which in turn reference visual styles with concrete formatting.
///
/// # Example
///
/// ```rust
/// use outstanding::{Theme, StyleValue};
/// use console::Style;
///
/// let theme = Theme::new()
///     // Visual layer - concrete styles
///     .add("muted", Style::new().dim())
///     .add("accent", Style::new().cyan().bold())
///     // Presentation layer - aliases to visual
///     .add("disabled", "muted")
///     // Semantic layer - aliases to presentation
///     .add("timestamp", "disabled");
/// ```
#[derive(Debug, Clone)]
pub enum StyleValue {
    /// A concrete style with actual formatting (colors, bold, etc.)
    Concrete(Style),
    /// An alias referencing another style by name
    Alias(String),
}

impl From<Style> for StyleValue {
    fn from(style: Style) -> Self {
        StyleValue::Concrete(style)
    }
}

impl From<&str> for StyleValue {
    fn from(name: &str) -> Self {
        StyleValue::Alias(name.to_string())
    }
}

impl From<String> for StyleValue {
    fn from(name: String) -> Self {
        StyleValue::Alias(name)
    }
}

/// Error returned when style validation fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleValidationError {
    /// An alias references a style that doesn't exist
    UnresolvedAlias { from: String, to: String },
    /// A cycle was detected in alias resolution
    CycleDetected { path: Vec<String> },
}

impl std::fmt::Display for StyleValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StyleValidationError::UnresolvedAlias { from, to } => {
                write!(f, "style '{}' aliases non-existent style '{}'", from, to)
            }
            StyleValidationError::CycleDetected { path } => {
                write!(f, "cycle detected in style aliases: {}", path.join(" -> "))
            }
        }
    }
}

impl std::error::Error for StyleValidationError {}

/// A collection of named styles.
///
/// Styles are registered by name and applied via the `style` filter in templates.
/// Styles can be concrete (with actual formatting) or aliases to other styles,
/// enabling layered styling (semantic -> presentation -> visual).
///
/// When a style name is not found, a configurable indicator is prepended to the text
/// to help catch typos in templates (defaults to `(!?)`).
///
/// # Example
///
/// ```rust
/// use outstanding::Styles;
/// use console::Style;
///
/// let styles = Styles::new()
///     // Concrete styles
///     .add("error", Style::new().bold().red())
///     .add("warning", Style::new().yellow())
///     .add("dim", Style::new().dim())
///     // Alias styles
///     .add("muted", "dim");
///
/// // Apply a style (returns styled string)
/// let styled = styles.apply("error", "Something went wrong");
///
/// // Aliases resolve to their target
/// let muted = styles.apply("muted", "Quiet");  // Uses "dim" style
///
/// // Unknown style shows indicator
/// let unknown = styles.apply("typo", "Hello");
/// assert!(unknown.starts_with("(!?)"));
/// ```
#[derive(Debug, Clone)]
pub struct Styles {
    styles: HashMap<String, StyleValue>,
    missing_indicator: String,
}

/// A named collection of styles used when rendering templates.
#[derive(Debug, Clone)]
pub struct Theme {
    styles: Styles,
}

impl Theme {
    /// Creates an empty theme.
    pub fn new() -> Self {
        Self {
            styles: Styles::new(),
        }
    }

    /// Creates a theme from an existing [`Styles`] collection.
    pub fn from_styles(styles: Styles) -> Self {
        Self { styles }
    }

    /// Adds a named style, returning an updated theme for chaining.
    ///
    /// The value can be either a concrete `Style` or a `&str`/`String` alias
    /// to another style name, enabling layered styling.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::Theme;
    /// use console::Style;
    ///
    /// let theme = Theme::new()
    ///     // Visual layer - concrete styles
    ///     .add("muted", Style::new().dim())
    ///     .add("accent", Style::new().cyan().bold())
    ///     // Presentation layer - aliases
    ///     .add("disabled", "muted")
    ///     .add("highlighted", "accent")
    ///     // Semantic layer - aliases to presentation
    ///     .add("timestamp", "disabled");
    /// ```
    pub fn add<V: Into<StyleValue>>(mut self, name: &str, value: V) -> Self {
        self.styles = self.styles.add(name, value);
        self
    }

    /// Returns the underlying styles.
    pub fn styles(&self) -> &Styles {
        &self.styles
    }

    /// Validates that all style aliases in this theme resolve correctly.
    ///
    /// This is called automatically at render time, but can be called
    /// explicitly for early error detection.
    pub fn validate(&self) -> Result<(), StyleValidationError> {
        self.styles.validate()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

/// A theme that adapts based on the user's display mode.
#[derive(Debug, Clone)]
pub struct AdaptiveTheme {
    light: Theme,
    dark: Theme,
}

impl AdaptiveTheme {
    pub fn new(light: Theme, dark: Theme) -> Self {
        Self { light, dark }
    }

    fn resolve(&self) -> Theme {
        match detect_color_mode() {
            ColorMode::Light => self.light.clone(),
            ColorMode::Dark => self.dark.clone(),
        }
    }
}

/// Reference to either a static theme or an adaptive theme.
#[derive(Debug)]
pub enum ThemeChoice<'a> {
    Theme(&'a Theme),
    Adaptive(&'a AdaptiveTheme),
}

impl<'a> ThemeChoice<'a> {
    fn resolve(&self) -> Theme {
        match self {
            ThemeChoice::Theme(theme) => (*theme).clone(),
            ThemeChoice::Adaptive(adaptive) => adaptive.resolve(),
        }
    }
}

impl<'a> From<&'a Theme> for ThemeChoice<'a> {
    fn from(theme: &'a Theme) -> Self {
        ThemeChoice::Theme(theme)
    }
}

impl<'a> From<&'a AdaptiveTheme> for ThemeChoice<'a> {
    fn from(adaptive: &'a AdaptiveTheme) -> Self {
        ThemeChoice::Adaptive(adaptive)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Light,
    Dark,
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
/// use outstanding::{render_with_output, Theme, ThemeChoice, OutputMode};
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
///     ThemeChoice::from(&theme),
///     OutputMode::Auto,
/// ).unwrap();
///
/// // Force plain text
/// let plain = render_with_output(
///     r#"{{ message | style("ok") }}"#,
///     &data,
///     ThemeChoice::from(&theme),
///     OutputMode::Text,
/// ).unwrap();
/// assert_eq!(plain, "Hello");
///
/// // Debug mode - renders bracket tags
/// let debug = render_with_output(
///     r#"{{ message | style("ok") }}"#,
///     &data,
///     ThemeChoice::from(&theme),
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

type ThemeDetector = fn() -> ColorMode;

static THEME_DETECTOR: Lazy<Mutex<ThemeDetector>> = Lazy::new(|| Mutex::new(os_theme_detector));

/// Overrides the detector used to determine whether the user prefers a light or dark theme.
/// Useful for testing.
pub fn set_theme_detector(detector: ThemeDetector) {
    let mut guard = THEME_DETECTOR.lock().unwrap();
    *guard = detector;
}

fn detect_color_mode() -> ColorMode {
    let detector = THEME_DETECTOR.lock().unwrap();
    (*detector)()
}

fn os_theme_detector() -> ColorMode {
    match detect_os_theme() {
        OsThemeMode::Dark => ColorMode::Dark,
        OsThemeMode::Light => ColorMode::Light,
    }
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            styles: HashMap::new(),
            missing_indicator: DEFAULT_MISSING_STYLE_INDICATOR.to_string(),
        }
    }
}

impl Styles {
    /// Creates an empty style registry with the default missing style indicator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a custom indicator to prepend when a style name is not found.
    ///
    /// This helps catch typos in templates. Set to empty string to disable.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::Styles;
    ///
    /// let styles = Styles::new()
    ///     .missing_indicator("[MISSING]")
    ///     .add("ok", console::Style::new().green());
    ///
    /// // Typo in style name
    /// let output = styles.apply("typo", "Hello");
    /// assert_eq!(output, "[MISSING] Hello");
    /// ```
    pub fn missing_indicator(mut self, indicator: &str) -> Self {
        self.missing_indicator = indicator.to_string();
        self
    }

    /// Adds a named style. Returns self for chaining.
    ///
    /// The value can be either a concrete `Style` or a `&str`/`String` alias
    /// to another style name.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::Styles;
    /// use console::Style;
    ///
    /// let styles = Styles::new()
    ///     .add("dim", Style::new().dim())      // Concrete style
    ///     .add("muted", "dim");                 // Alias to "dim"
    /// ```
    ///
    /// If a style with the same name exists, it is replaced.
    pub fn add<V: Into<StyleValue>>(mut self, name: &str, value: V) -> Self {
        self.styles.insert(name.to_string(), value.into());
        self
    }

    /// Resolves a style name to a concrete `Style`, following alias chains.
    ///
    /// Returns `None` if the style doesn't exist or if a cycle is detected.
    /// For detailed error information, use `validate()` instead.
    fn resolve(&self, name: &str) -> Option<&Style> {
        let mut current = name;
        let mut visited = std::collections::HashSet::new();

        loop {
            if !visited.insert(current) {
                return None; // Cycle detected
            }
            match self.styles.get(current)? {
                StyleValue::Concrete(style) => return Some(style),
                StyleValue::Alias(next) => current = next,
            }
        }
    }

    /// Checks if a style name can be resolved (exists and has no cycles).
    fn can_resolve(&self, name: &str) -> bool {
        self.resolve(name).is_some()
    }

    /// Validates that all style aliases resolve correctly.
    ///
    /// Returns `Ok(())` if all aliases point to existing styles with no cycles.
    /// Returns an error describing the first problem found.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::{Styles, StyleValidationError};
    /// use console::Style;
    ///
    /// // Valid: alias chain resolves
    /// let valid = Styles::new()
    ///     .add("dim", Style::new().dim())
    ///     .add("muted", "dim");
    /// assert!(valid.validate().is_ok());
    ///
    /// // Invalid: dangling alias
    /// let dangling = Styles::new()
    ///     .add("orphan", "nonexistent");
    /// assert!(matches!(
    ///     dangling.validate(),
    ///     Err(StyleValidationError::UnresolvedAlias { .. })
    /// ));
    ///
    /// // Invalid: cycle
    /// let cycle = Styles::new()
    ///     .add("a", "b")
    ///     .add("b", "a");
    /// assert!(matches!(
    ///     cycle.validate(),
    ///     Err(StyleValidationError::CycleDetected { .. })
    /// ));
    /// ```
    pub fn validate(&self) -> Result<(), StyleValidationError> {
        for (name, value) in &self.styles {
            if let StyleValue::Alias(target) = value {
                self.validate_alias_chain(name, target)?;
            }
        }
        Ok(())
    }

    /// Validates a single alias chain starting from `name` -> `target`.
    fn validate_alias_chain(&self, name: &str, target: &str) -> Result<(), StyleValidationError> {
        let mut current = target;
        let mut path = vec![name.to_string()];

        loop {
            // Check if target exists
            let value = self.styles.get(current).ok_or_else(|| {
                StyleValidationError::UnresolvedAlias {
                    from: path.last().unwrap().clone(),
                    to: current.to_string(),
                }
            })?;

            path.push(current.to_string());

            // Check for cycle (if we've seen this name before in our path)
            if path[..path.len() - 1].contains(&current.to_string()) {
                return Err(StyleValidationError::CycleDetected { path });
            }

            match value {
                StyleValue::Concrete(_) => return Ok(()),
                StyleValue::Alias(next) => current = next,
            }
        }
    }

    /// Applies a named style to text.
    ///
    /// Resolves aliases to find the concrete style, then applies it.
    /// If the style doesn't exist or can't be resolved, prepends the missing indicator.
    pub fn apply(&self, name: &str, text: &str) -> String {
        match self.resolve(name) {
            Some(style) => style.apply_to(text).to_string(),
            None if self.missing_indicator.is_empty() => text.to_string(),
            None => format!("{} {}", self.missing_indicator, text),
        }
    }

    /// Applies style checking without ANSI codes (plain text mode).
    ///
    /// If the style exists and resolves, returns the text unchanged.
    /// If not found or unresolvable, prepends the missing indicator (unless it's empty).
    pub fn apply_plain(&self, name: &str, text: &str) -> String {
        if self.can_resolve(name) || self.missing_indicator.is_empty() {
            text.to_string()
        } else {
            format!("{} {}", self.missing_indicator, text)
        }
    }

    /// Applies a style based on the output mode.
    ///
    /// - `Term` - Applies ANSI styling
    /// - `Text` - Returns plain text (no ANSI codes)
    /// - `Auto` - Should be resolved before calling this method
    ///
    /// Note: For `Auto` mode, call `OutputMode::should_use_color()` first
    /// to determine whether to use `Term` or `Text`.
    pub fn apply_with_mode(&self, name: &str, text: &str, use_color: bool) -> String {
        if use_color {
            self.apply(name, text)
        } else {
            self.apply_plain(name, text)
        }
    }

    /// Applies a style in debug mode, rendering as bracket tags.
    ///
    /// Returns `[name]text[/name]` for styles that resolve correctly,
    /// or applies the missing indicator for unknown/unresolvable styles.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::Styles;
    /// use console::Style;
    ///
    /// let styles = Styles::new()
    ///     .add("bold", Style::new().bold())
    ///     .add("emphasis", "bold");  // Alias
    ///
    /// // Direct style renders as bracket tags
    /// assert_eq!(styles.apply_debug("bold", "hello"), "[bold]hello[/bold]");
    ///
    /// // Alias also renders with its own name (not the target)
    /// assert_eq!(styles.apply_debug("emphasis", "hello"), "[emphasis]hello[/emphasis]");
    ///
    /// // Unknown style shows indicator
    /// assert_eq!(styles.apply_debug("unknown", "hello"), "(!?) hello");
    /// ```
    pub fn apply_debug(&self, name: &str, text: &str) -> String {
        if self.can_resolve(name) {
            format!("[{}]{}[/{}]", name, text, name)
        } else if self.missing_indicator.is_empty() {
            text.to_string()
        } else {
            format!("{} {}", self.missing_indicator, text)
        }
    }

    /// Returns true if a style with the given name exists (concrete or alias).
    pub fn has(&self, name: &str) -> bool {
        self.styles.contains_key(name)
    }

    /// Returns the number of registered styles (both concrete and aliases).
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    /// Returns true if no styles are registered.
    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }
}

/// Renders a template with automatic terminal color detection.
///
/// This is the simplest way to render styled output. It automatically detects
/// whether stdout supports colors and applies styles accordingly.
///
/// # Arguments
///
/// * `template` - A minijinja template string
/// * `data` - Any serializable data to pass to the template
/// * `theme` - Theme definitions (or adaptive theme) to use for the `style` filter
///
/// # Example
///
/// ```rust
/// use outstanding::{render, Theme, ThemeChoice};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data { message: String }
///
/// let theme = Theme::new().add("ok", Style::new().green());
/// let output = render(
///     r#"{{ message | style("ok") }}"#,
///     &Data { message: "Success!".into() },
///     ThemeChoice::from(&theme),
/// ).unwrap();
/// ```
pub fn render<T: Serialize>(
    template: &str,
    data: &T,
    theme: ThemeChoice<'_>,
) -> Result<String, Error> {
    render_with_output(template, data, theme, OutputMode::Auto)
}

/// Renders a template with explicit output mode control.
///
/// Use this when you need to override automatic terminal detection,
/// for example when honoring a `--output=text` CLI flag.
///
/// # Arguments
///
/// * `template` - A minijinja template string
/// * `data` - Any serializable data to pass to the template
/// * `theme` - Theme definitions to use for the `style` filter
/// * `mode` - Output mode: `Auto`, `Term`, or `Text`
///
/// # Example
///
/// ```rust
/// use outstanding::{render_with_output, Theme, ThemeChoice, OutputMode};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data { status: String }
///
/// let theme = Theme::new().add("ok", Style::new().green());
///
/// // Force plain text output
/// let plain = render_with_output(
///     r#"{{ status | style("ok") }}"#,
///     &Data { status: "done".into() },
///     ThemeChoice::from(&theme),
///     OutputMode::Text,
/// ).unwrap();
/// assert_eq!(plain, "done"); // No ANSI codes
///
/// // Force terminal output (with ANSI codes)
/// let term = render_with_output(
///     r#"{{ status | style("ok") }}"#,
///     &Data { status: "done".into() },
///     ThemeChoice::from(&theme),
///     OutputMode::Term,
/// ).unwrap();
/// // Contains ANSI codes for green
/// ```
pub fn render_with_output<T: Serialize>(
    template: &str,
    data: &T,
    theme: ThemeChoice<'_>,
    mode: OutputMode,
) -> Result<String, Error> {
    let theme = theme.resolve();

    // Validate style aliases before rendering
    theme.validate().map_err(|e| {
        Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
    })?;

    let mut env = Environment::new();
    register_filters(&mut env, theme, mode);

    env.add_template_owned("_inline".to_string(), template.to_string())?;
    let tmpl = env.get_template("_inline")?;
    tmpl.render(data)
}

/// Renders data using a template, or serializes directly for structured output modes.
///
/// This is the recommended function when you want to support both human-readable
/// output (terminal, text) and machine-readable output (JSON). For structured modes
/// like `Json`, the data is serialized directly, skipping template rendering entirely.
///
/// # Arguments
///
/// * `template` - A minijinja template string (ignored for structured modes)
/// * `data` - Any serializable data to render or serialize
/// * `theme` - Theme definitions for the `style` filter (ignored for structured modes)
/// * `mode` - Output mode determining the output format
///
/// # Example
///
/// ```rust
/// use outstanding::{render_or_serialize, Theme, ThemeChoice, OutputMode};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Report { title: String, count: usize }
///
/// let theme = Theme::new().add("title", Style::new().bold());
/// let data = Report { title: "Summary".into(), count: 42 };
///
/// // Terminal output uses the template
/// let term = render_or_serialize(
///     r#"{{ title | style("title") }}: {{ count }}"#,
///     &data,
///     ThemeChoice::from(&theme),
///     OutputMode::Text,
/// ).unwrap();
/// assert_eq!(term, "Summary: 42");
///
/// // JSON output serializes directly
/// let json = render_or_serialize(
///     r#"{{ title | style("title") }}: {{ count }}"#,
///     &data,
///     ThemeChoice::from(&theme),
///     OutputMode::Json,
/// ).unwrap();
/// assert!(json.contains("\"title\": \"Summary\""));
/// assert!(json.contains("\"count\": 42"));
/// ```
pub fn render_or_serialize<T: Serialize>(
    template: &str,
    data: &T,
    theme: ThemeChoice<'_>,
    mode: OutputMode,
) -> Result<String, Error> {
    if mode.is_structured() {
        match mode {
            OutputMode::Json => serde_json::to_string_pretty(data)
                .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())),
            _ => unreachable!("is_structured() returned true for non-structured mode"),
        }
    } else {
        render_with_output(template, data, theme, mode)
    }
}

/// A renderer with pre-registered templates.
///
/// Use this when your application has multiple templates that are rendered
/// repeatedly. Templates are compiled once and reused.
///
/// # Example
///
/// ```rust
/// use outstanding::{Renderer, Theme};
/// use console::Style;
/// use serde::Serialize;
///
/// let theme = Theme::new()
///     .add("title", Style::new().bold())
///     .add("count", Style::new().cyan());
///
/// let mut renderer = Renderer::new(theme).unwrap();
/// renderer.add_template("header", r#"{{ title | style("title") }}"#).unwrap();
/// renderer.add_template("stats", r#"Count: {{ n | style("count") }}"#).unwrap();
///
/// #[derive(Serialize)]
/// struct Header { title: String }
///
/// #[derive(Serialize)]
/// struct Stats { n: usize }
///
/// let h = renderer.render("header", &Header { title: "Report".into() }).unwrap();
/// let s = renderer.render("stats", &Stats { n: 42 }).unwrap();
/// ```
pub struct Renderer {
    env: Environment<'static>,
}

impl Renderer {
    /// Creates a new renderer with automatic color detection.
    ///
    /// # Errors
    ///
    /// Returns an error if any style aliases are invalid (dangling or cyclic).
    pub fn new(theme: Theme) -> Result<Self, Error> {
        Self::with_output(theme, OutputMode::Auto)
    }

    /// Creates a new renderer with explicit output mode.
    ///
    /// # Errors
    ///
    /// Returns an error if any style aliases are invalid (dangling or cyclic).
    pub fn with_output(theme: Theme, mode: OutputMode) -> Result<Self, Error> {
        // Validate style aliases before creating the renderer
        theme.validate().map_err(|e| {
            Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
        })?;

        let mut env = Environment::new();
        register_filters(&mut env, theme, mode);
        Ok(Self { env })
    }

    /// Registers a named template.
    ///
    /// The template is compiled immediately; errors are returned if syntax is invalid.
    pub fn add_template(&mut self, name: &str, source: &str) -> Result<(), Error> {
        self.env
            .add_template_owned(name.to_string(), source.to_string())
    }

    /// Renders a registered template with the given data.
    ///
    /// # Errors
    ///
    /// Returns an error if the template name is not found or rendering fails.
    pub fn render<T: Serialize>(&self, name: &str, data: &T) -> Result<String, Error> {
        let tmpl = self.env.get_template(name)?;
        tmpl.render(data)
    }
}

/// Registers all built-in filters on a minijinja environment.
fn register_filters(env: &mut Environment<'static>, theme: Theme, mode: OutputMode) {
    let styles = theme.styles.clone();
    let is_debug = mode.is_debug();
    let use_color = mode.should_use_color();
    env.add_filter("style", move |value: Value, name: String| -> String {
        let text = value.to_string();
        if is_debug {
            styles.apply_debug(&name, &text)
        } else {
            styles.apply_with_mode(&name, &text, use_color)
        }
    });

    // Filter to append a newline to the value, enabling explicit line break control.
    // Usage: {{ content | nl }} outputs content followed by \n
    //        {{ "" | nl }} outputs just \n (a blank line)
    env.add_filter("nl", |value: Value| -> String { format!("{}\n", value) });

    // Register table formatting filters (col, pad_left, pad_right, truncate_at, etc.)
    table::filters::register_table_filters(env);
}

/// Converts an RGB triplet to the nearest ANSI 256-color palette index.
pub fn rgb_to_ansi256((r, g, b): (u8, u8, u8)) -> u8 {
    if r == g && g == b {
        if r < 8 {
            16
        } else if r > 248 {
            231
        } else {
            232 + ((r as u16 - 8) * 24 / 247) as u8
        }
    } else {
        let red = (r as u16 * 5 / 255) as u8;
        let green = (g as u16 * 5 / 255) as u8;
        let blue = (b as u16 * 5 / 255) as u8;
        16 + 36 * red + 6 * green + blue
    }
}

/// Placeholder helper for true-color output; currently returns the RGB triplet unchanged so it
/// can be handed to future true-color aware APIs.
pub fn rgb_to_truecolor(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    rgb
}

/// Truncates a string to fit within a maximum display width, adding ellipsis if needed.
///
/// Uses Unicode width calculations for proper handling of CJK and other wide characters.
/// If the string fits within `max_width`, it is returned unchanged. If truncation is
/// needed, characters are removed from the end and replaced with `…` (ellipsis).
///
/// # Arguments
///
/// * `s` - The string to truncate
/// * `max_width` - Maximum display width (in terminal columns)
///
/// # Example
///
/// ```rust
/// use outstanding::truncate_to_width;
///
/// assert_eq!(truncate_to_width("Hello", 10), "Hello");
/// assert_eq!(truncate_to_width("Hello World", 6), "Hello…");
/// ```
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    // If the string fits, return it unchanged
    if s.width() <= max_width {
        return s.to_string();
    }

    let mut result = String::new();
    let mut current_width = 0;
    // Reserve 1 char for ellipsis
    let limit = max_width.saturating_sub(1);

    for c in s.chars() {
        let char_width = c.width().unwrap_or(0);
        if current_width + char_width > limit {
            result.push('…');
            return result;
        }
        result.push(c);
        current_width += char_width;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct SimpleData {
        message: String,
    }

    #[derive(Serialize)]
    struct ListData {
        items: Vec<String>,
        count: usize,
    }

    #[test]
    fn test_styles_new_is_empty() {
        let styles = Styles::new();
        assert!(styles.is_empty());
        assert_eq!(styles.len(), 0);
    }

    #[test]
    fn test_styles_add_and_has() {
        let styles = Styles::new()
            .add("error", Style::new().red())
            .add("ok", Style::new().green());

        assert!(styles.has("error"));
        assert!(styles.has("ok"));
        assert!(!styles.has("warning"));
        assert_eq!(styles.len(), 2);
    }

    #[test]
    fn test_styles_apply_unknown_shows_indicator() {
        let styles = Styles::new();
        let result = styles.apply("nonexistent", "hello");
        assert_eq!(result, "(!?) hello");
    }

    #[test]
    fn test_styles_apply_unknown_with_empty_indicator() {
        let styles = Styles::new().missing_indicator("");
        let result = styles.apply("nonexistent", "hello");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_styles_apply_unknown_with_custom_indicator() {
        let styles = Styles::new().missing_indicator("[MISSING]");
        let result = styles.apply("nonexistent", "hello");
        assert_eq!(result, "[MISSING] hello");
    }

    #[test]
    fn test_styles_apply_plain_known_style() {
        let styles = Styles::new().add("bold", Style::new().bold());
        let result = styles.apply_plain("bold", "hello");
        // apply_plain returns text without ANSI codes
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_styles_apply_plain_unknown_shows_indicator() {
        let styles = Styles::new();
        let result = styles.apply_plain("nonexistent", "hello");
        assert_eq!(result, "(!?) hello");
    }

    #[test]
    fn test_styles_apply_known_style() {
        let styles = Styles::new().add("bold", Style::new().bold().force_styling(true));
        let result = styles.apply("bold", "hello");
        // The result should contain ANSI codes for bold
        assert!(result.contains("hello"));
        // Bold ANSI code is \x1b[1m
        assert!(result.contains("\x1b[1m"));
    }

    #[test]
    fn test_render_with_output_text_no_ansi() {
        let styles = Styles::new().add("red", Style::new().red());
        let theme = Theme::from_styles(styles);
        let data = SimpleData {
            message: "test".into(),
        };

        let output = render_with_output(
            r#"{{ message | style("red") }}"#,
            &data,
            ThemeChoice::from(&theme),
            OutputMode::Text,
        )
        .unwrap();

        assert_eq!(output, "test");
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn test_render_with_output_term_has_ansi() {
        // Use force_styling to ensure ANSI codes are emitted even in test environment
        let styles = Styles::new().add("green", Style::new().green().force_styling(true));
        let theme = Theme::from_styles(styles);
        let data = SimpleData {
            message: "success".into(),
        };

        let output = render_with_output(
            r#"{{ message | style("green") }}"#,
            &data,
            ThemeChoice::from(&theme),
            OutputMode::Term,
        )
        .unwrap();

        assert!(output.contains("success"));
        assert!(output.contains("\x1b[")); // Contains ANSI escape
    }

    #[test]
    fn test_render_unknown_style_shows_indicator() {
        let styles = Styles::new();
        let theme = Theme::from_styles(styles);
        let data = SimpleData {
            message: "hello".into(),
        };

        let output = render_with_output(
            r#"{{ message | style("unknown") }}"#,
            &data,
            ThemeChoice::from(&theme),
            OutputMode::Term,
        )
        .unwrap();

        assert_eq!(output, "(!?) hello");
    }

    #[test]
    fn test_render_unknown_style_shows_indicator_no_color() {
        let styles = Styles::new();
        let theme = Theme::from_styles(styles);
        let data = SimpleData {
            message: "hello".into(),
        };

        // Even with colors disabled, missing indicator should appear
        let output = render_with_output(
            r#"{{ message | style("unknown") }}"#,
            &data,
            ThemeChoice::from(&theme),
            OutputMode::Text,
        )
        .unwrap();

        assert_eq!(output, "(!?) hello");
    }

    #[test]
    fn test_render_unknown_style_silent_with_empty_indicator() {
        let styles = Styles::new().missing_indicator("");
        let theme = Theme::from_styles(styles);
        let data = SimpleData {
            message: "hello".into(),
        };

        let output = render_with_output(
            r#"{{ message | style("unknown") }}"#,
            &data,
            ThemeChoice::from(&theme),
            OutputMode::Term,
        )
        .unwrap();

        assert_eq!(output, "hello");
    }

    #[test]
    fn test_render_template_with_loop() {
        let styles = Styles::new().add("item", Style::new().cyan());
        let theme = Theme::from_styles(styles);
        let data = ListData {
            items: vec!["one".into(), "two".into()],
            count: 2,
        };

        let template = r#"{% for item in items %}{{ item | style("item") }}
{% endfor %}"#;

        let output = render_with_output(template, &data, ThemeChoice::from(&theme), OutputMode::Text).unwrap();
        assert_eq!(output, "one\ntwo\n");
    }

    #[test]
    fn test_render_mixed_styled_and_plain() {
        let styles = Styles::new().add("count", Style::new().bold());
        let theme = Theme::from_styles(styles);
        let data = ListData {
            items: vec![],
            count: 42,
        };

        let template = r#"Total: {{ count | style("count") }} items"#;
        let output = render_with_output(template, &data, ThemeChoice::from(&theme), OutputMode::Text).unwrap();

        assert_eq!(output, "Total: 42 items");
    }

    #[test]
    fn test_render_literal_string_styled() {
        let styles = Styles::new().add("header", Style::new().bold());
        let theme = Theme::from_styles(styles);

        #[derive(Serialize)]
        struct Empty {}

        let output = render_with_output(
            r#"{{ "Header" | style("header") }}"#,
            &Empty {},
            ThemeChoice::from(&theme),
            OutputMode::Text,
        )
        .unwrap();

        assert_eq!(output, "Header");
    }

    #[test]
    fn test_renderer_add_and_render() {
        let theme = Theme::new().add("ok", Style::new().green());
        let mut renderer = Renderer::with_output(theme, OutputMode::Text).unwrap();

        renderer
            .add_template("test", r#"{{ message | style("ok") }}"#)
            .unwrap();

        let output = renderer
            .render(
                "test",
                &SimpleData {
                    message: "hi".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "hi");
    }

    #[test]
    fn test_renderer_unknown_template_error() {
        let theme = Theme::new();
        let renderer = Renderer::with_output(theme, OutputMode::Text).unwrap();

        let result = renderer.render(
            "nonexistent",
            &SimpleData {
                message: "x".into(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_renderer_multiple_templates() {
        let theme = Theme::new()
            .add("a", Style::new().red())
            .add("b", Style::new().blue());

        let mut renderer = Renderer::with_output(theme, OutputMode::Text).unwrap();
        renderer
            .add_template("tmpl_a", r#"A: {{ message | style("a") }}"#)
            .unwrap();
        renderer
            .add_template("tmpl_b", r#"B: {{ message | style("b") }}"#)
            .unwrap();

        let data = SimpleData {
            message: "test".into(),
        };

        assert_eq!(renderer.render("tmpl_a", &data).unwrap(), "A: test");
        assert_eq!(renderer.render("tmpl_b", &data).unwrap(), "B: test");
    }

    #[test]
    fn test_style_filter_with_nested_data() {
        #[derive(Serialize)]
        struct Item {
            name: String,
            value: i32,
        }

        #[derive(Serialize)]
        struct Container {
            items: Vec<Item>,
        }

        let styles = Styles::new().add("name", Style::new().bold());
        let theme = Theme::from_styles(styles);
        let data = Container {
            items: vec![
                Item {
                    name: "foo".into(),
                    value: 1,
                },
                Item {
                    name: "bar".into(),
                    value: 2,
                },
            ],
        };

        let template = r#"{% for item in items %}{{ item.name | style("name") }}={{ item.value }}
{% endfor %}"#;

        let output = render_with_output(template, &data, ThemeChoice::from(&theme), OutputMode::Text).unwrap();
        assert_eq!(output, "foo=1\nbar=2\n");
    }

    #[test]
    fn test_styles_can_be_replaced() {
        let styles = Styles::new()
            .add("x", Style::new().red())
            .add("x", Style::new().green()); // Replace

        // Should only have one style
        assert_eq!(styles.len(), 1);
        assert!(styles.has("x"));
    }

    #[test]
    fn test_empty_template() {
        let styles = Styles::new();
        let theme = Theme::from_styles(styles);

        #[derive(Serialize)]
        struct Empty {}

        let output = render_with_output("", &Empty {}, ThemeChoice::from(&theme), OutputMode::Text).unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_template_syntax_error() {
        let styles = Styles::new();
        let theme = Theme::from_styles(styles);

        #[derive(Serialize)]
        struct Empty {}

        let result = render_with_output("{{ unclosed", &Empty {}, ThemeChoice::from(&theme), OutputMode::Text);
        assert!(result.is_err());
    }

    #[test]
    fn test_rgb_to_ansi256_grayscale() {
        assert_eq!(rgb_to_ansi256((0, 0, 0)), 16);
        assert_eq!(rgb_to_ansi256((255, 255, 255)), 231);
        let mid = rgb_to_ansi256((128, 128, 128));
        assert!((232..=255).contains(&mid));
    }

    #[test]
    fn test_rgb_to_ansi256_color_cube() {
        assert_eq!(rgb_to_ansi256((255, 0, 0)), 196);
        assert_eq!(rgb_to_ansi256((0, 255, 0)), 46);
        assert_eq!(rgb_to_ansi256((0, 0, 255)), 21);
    }

    #[test]
    fn test_adaptive_theme_uses_detector() {
        console::set_colors_enabled(true);
        let light = Theme::new().add("tone", Style::new().green().force_styling(true));
        let dark = Theme::new().add("tone", Style::new().red().force_styling(true));
        let adaptive = AdaptiveTheme::new(light, dark);
        let data = SimpleData {
            message: "hi".into(),
        };

        set_theme_detector(|| ColorMode::Dark);
        let dark_output = render_with_output(
            r#"{{ message | style("tone") }}"#,
            &data,
            ThemeChoice::Adaptive(&adaptive),
            OutputMode::Term,
        )
        .unwrap();
        assert!(dark_output.contains("\x1b[31"));

        set_theme_detector(|| ColorMode::Light);
        let light_output = render_with_output(
            r#"{{ message | style("tone") }}"#,
            &data,
            ThemeChoice::Adaptive(&adaptive),
            OutputMode::Term,
        )
        .unwrap();
        assert!(light_output.contains("\x1b[32"));

        // Reset to default for other tests
        set_theme_detector(|| ColorMode::Light);
    }

    #[test]
    fn test_truncate_to_width_no_truncation() {
        assert_eq!(truncate_to_width("Hello", 10), "Hello");
        assert_eq!(truncate_to_width("Hello", 5), "Hello");
    }

    #[test]
    fn test_truncate_to_width_with_truncation() {
        assert_eq!(truncate_to_width("Hello World", 6), "Hello…");
        assert_eq!(truncate_to_width("Hello World", 7), "Hello …");
    }

    #[test]
    fn test_truncate_to_width_empty() {
        assert_eq!(truncate_to_width("", 5), "");
    }

    #[test]
    fn test_truncate_to_width_exact_fit() {
        assert_eq!(truncate_to_width("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_to_width_one_over() {
        assert_eq!(truncate_to_width("123456", 5), "1234…");
    }

    #[test]
    fn test_truncate_to_width_zero_width() {
        assert_eq!(truncate_to_width("Hello", 0), "…");
    }

    #[test]
    fn test_truncate_to_width_one_width() {
        assert_eq!(truncate_to_width("Hello", 1), "…");
    }

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
    fn test_styles_apply_with_mode_color() {
        let styles = Styles::new().add("bold", Style::new().bold().force_styling(true));
        let result = styles.apply_with_mode("bold", "hello", true);
        // Should contain ANSI codes
        assert!(result.contains("\x1b[1m"));
        assert!(result.contains("hello"));
    }

    #[test]
    fn test_styles_apply_with_mode_no_color() {
        let styles = Styles::new().add("bold", Style::new().bold());
        let result = styles.apply_with_mode("bold", "hello", false);
        // Should not contain ANSI codes
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_styles_apply_with_mode_missing_style() {
        let styles = Styles::new();
        // With color
        let result = styles.apply_with_mode("nonexistent", "hello", true);
        assert_eq!(result, "(!?) hello");
        // Without color
        let result = styles.apply_with_mode("nonexistent", "hello", false);
        assert_eq!(result, "(!?) hello");
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
        // TermDebug returns false for should_use_color because it's handled specially
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
    fn test_render_or_serialize_json_mode() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!({"name": "test", "count": 42});

        let output = render_or_serialize(
            "unused template",
            &data,
            ThemeChoice::from(&theme),
            OutputMode::Json,
        )
        .unwrap();

        // Should be valid JSON
        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"count\": 42"));
    }

    #[test]
    fn test_render_or_serialize_text_mode_uses_template() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!({"name": "test"});

        let output = render_or_serialize(
            "Name: {{ name }}",
            &data,
            ThemeChoice::from(&theme),
            OutputMode::Text,
        )
        .unwrap();

        assert_eq!(output, "Name: test");
    }

    #[test]
    fn test_render_or_serialize_term_mode_uses_template() {
        use serde_json::json;

        let theme = Theme::new().add("bold", Style::new().bold().force_styling(true));
        let data = json!({"name": "test"});

        let output = render_or_serialize(
            r#"{{ name | style("bold") }}"#,
            &data,
            ThemeChoice::from(&theme),
            OutputMode::Term,
        )
        .unwrap();

        // Should contain ANSI codes
        assert!(output.contains("\x1b[1m"));
        assert!(output.contains("test"));
    }

    #[test]
    fn test_render_or_serialize_json_with_struct() {
        #[derive(Serialize)]
        struct Report {
            title: String,
            items: Vec<String>,
        }

        let theme = Theme::new();
        let data = Report {
            title: "Summary".into(),
            items: vec!["one".into(), "two".into()],
        };

        let output = render_or_serialize(
            "unused",
            &data,
            ThemeChoice::from(&theme),
            OutputMode::Json,
        )
        .unwrap();

        assert!(output.contains("\"title\": \"Summary\""));
        assert!(output.contains("\"items\""));
        assert!(output.contains("\"one\""));
    }

    #[test]
    fn test_styles_apply_debug_known_style() {
        let styles = Styles::new().add("bold", Style::new().bold());
        let result = styles.apply_debug("bold", "hello");
        assert_eq!(result, "[bold]hello[/bold]");
    }

    #[test]
    fn test_styles_apply_debug_unknown_style() {
        let styles = Styles::new();
        let result = styles.apply_debug("unknown", "hello");
        assert_eq!(result, "(!?) hello");
    }

    #[test]
    fn test_styles_apply_debug_unknown_empty_indicator() {
        let styles = Styles::new().missing_indicator("");
        let result = styles.apply_debug("unknown", "hello");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_render_with_output_term_debug() {
        let styles = Styles::new()
            .add("title", Style::new().bold())
            .add("count", Style::new().cyan());
        let theme = Theme::from_styles(styles);

        #[derive(Serialize)]
        struct Data {
            name: String,
            value: usize,
        }

        let data = Data {
            name: "Test".into(),
            value: 42,
        };

        let output = render_with_output(
            r#"{{ name | style("title") }}: {{ value | style("count") }}"#,
            &data,
            ThemeChoice::from(&theme),
            OutputMode::TermDebug,
        )
        .unwrap();

        assert_eq!(output, "[title]Test[/title]: [count]42[/count]");
    }

    #[test]
    fn test_render_with_output_term_debug_missing_style() {
        let styles = Styles::new().add("known", Style::new().bold());
        let theme = Theme::from_styles(styles);

        #[derive(Serialize)]
        struct Data {
            message: String,
        }

        let data = Data {
            message: "hello".into(),
        };

        // Unknown style shows indicator
        let output = render_with_output(
            r#"{{ message | style("unknown") }}"#,
            &data,
            ThemeChoice::from(&theme),
            OutputMode::TermDebug,
        )
        .unwrap();

        assert_eq!(output, "(!?) hello");

        // Known style renders as bracket tags
        let output = render_with_output(
            r#"{{ message | style("known") }}"#,
            &data,
            ThemeChoice::from(&theme),
            OutputMode::TermDebug,
        )
        .unwrap();

        assert_eq!(output, "[known]hello[/known]");
    }

    // ==================== Style Aliasing Tests ====================

    mod style_aliasing {
        use super::*;

        // --- Resolution Tests ---

        #[test]
        fn test_resolve_concrete_style() {
            let styles = Styles::new().add("bold", Style::new().bold());
            assert!(styles.resolve("bold").is_some());
        }

        #[test]
        fn test_resolve_nonexistent_style() {
            let styles = Styles::new();
            assert!(styles.resolve("nonexistent").is_none());
        }

        #[test]
        fn test_resolve_single_alias() {
            let styles = Styles::new()
                .add("base", Style::new().dim())
                .add("alias", "base");

            assert!(styles.resolve("alias").is_some());
            assert!(styles.resolve("base").is_some());
        }

        #[test]
        fn test_resolve_chained_aliases() {
            let styles = Styles::new()
                .add("visual", Style::new().cyan())
                .add("presentation", "visual")
                .add("semantic", "presentation");

            // All should resolve to the same concrete style
            assert!(styles.resolve("visual").is_some());
            assert!(styles.resolve("presentation").is_some());
            assert!(styles.resolve("semantic").is_some());
        }

        #[test]
        fn test_resolve_deep_alias_chain() {
            let styles = Styles::new()
                .add("level0", Style::new().bold())
                .add("level1", "level0")
                .add("level2", "level1")
                .add("level3", "level2")
                .add("level4", "level3");

            assert!(styles.resolve("level4").is_some());
        }

        #[test]
        fn test_resolve_dangling_alias_returns_none() {
            let styles = Styles::new().add("orphan", "nonexistent");
            assert!(styles.resolve("orphan").is_none());
        }

        #[test]
        fn test_resolve_cycle_returns_none() {
            let styles = Styles::new()
                .add("a", "b")
                .add("b", "a");

            assert!(styles.resolve("a").is_none());
            assert!(styles.resolve("b").is_none());
        }

        #[test]
        fn test_resolve_self_referential_returns_none() {
            let styles = Styles::new().add("self", "self");
            assert!(styles.resolve("self").is_none());
        }

        #[test]
        fn test_resolve_three_way_cycle() {
            let styles = Styles::new()
                .add("a", "b")
                .add("b", "c")
                .add("c", "a");

            assert!(styles.resolve("a").is_none());
            assert!(styles.resolve("b").is_none());
            assert!(styles.resolve("c").is_none());
        }

        // --- Validation Tests ---

        #[test]
        fn test_validate_empty_styles() {
            let styles = Styles::new();
            assert!(styles.validate().is_ok());
        }

        #[test]
        fn test_validate_only_concrete_styles() {
            let styles = Styles::new()
                .add("a", Style::new().bold())
                .add("b", Style::new().dim())
                .add("c", Style::new().red());

            assert!(styles.validate().is_ok());
        }

        #[test]
        fn test_validate_valid_alias() {
            let styles = Styles::new()
                .add("base", Style::new().dim())
                .add("alias", "base");

            assert!(styles.validate().is_ok());
        }

        #[test]
        fn test_validate_valid_alias_chain() {
            let styles = Styles::new()
                .add("visual", Style::new().cyan())
                .add("presentation", "visual")
                .add("semantic", "presentation");

            assert!(styles.validate().is_ok());
        }

        #[test]
        fn test_validate_dangling_alias_error() {
            let styles = Styles::new().add("orphan", "nonexistent");

            let result = styles.validate();
            assert!(result.is_err());

            match result.unwrap_err() {
                StyleValidationError::UnresolvedAlias { from, to } => {
                    assert_eq!(from, "orphan");
                    assert_eq!(to, "nonexistent");
                }
                _ => panic!("Expected UnresolvedAlias error"),
            }
        }

        #[test]
        fn test_validate_dangling_in_chain() {
            let styles = Styles::new()
                .add("level1", "level2")
                .add("level2", "missing");

            let result = styles.validate();
            assert!(result.is_err());

            match result.unwrap_err() {
                StyleValidationError::UnresolvedAlias { from: _, to } => {
                    assert_eq!(to, "missing");
                    // from could be level1 or level2 depending on iteration order
                }
                _ => panic!("Expected UnresolvedAlias error"),
            }
        }

        #[test]
        fn test_validate_cycle_error() {
            let styles = Styles::new()
                .add("a", "b")
                .add("b", "a");

            let result = styles.validate();
            assert!(result.is_err());

            match result.unwrap_err() {
                StyleValidationError::CycleDetected { path } => {
                    // Path should contain both a and b
                    assert!(path.contains(&"a".to_string()));
                    assert!(path.contains(&"b".to_string()));
                }
                _ => panic!("Expected CycleDetected error"),
            }
        }

        #[test]
        fn test_validate_self_referential_cycle() {
            let styles = Styles::new().add("self", "self");

            let result = styles.validate();
            assert!(result.is_err());

            match result.unwrap_err() {
                StyleValidationError::CycleDetected { path } => {
                    assert!(path.contains(&"self".to_string()));
                }
                _ => panic!("Expected CycleDetected error"),
            }
        }

        #[test]
        fn test_validate_three_way_cycle() {
            let styles = Styles::new()
                .add("a", "b")
                .add("b", "c")
                .add("c", "a");

            let result = styles.validate();
            assert!(result.is_err());

            match result.unwrap_err() {
                StyleValidationError::CycleDetected { path } => {
                    assert!(path.len() >= 3);
                }
                _ => panic!("Expected CycleDetected error"),
            }
        }

        #[test]
        fn test_validate_mixed_valid_and_invalid() {
            // Some valid styles, one dangling alias
            let styles = Styles::new()
                .add("valid1", Style::new().bold())
                .add("valid2", "valid1")
                .add("invalid", "missing");

            assert!(styles.validate().is_err());
        }

        // --- Apply with Aliases Tests ---

        #[test]
        fn test_apply_through_alias() {
            let styles = Styles::new()
                .add("base", Style::new().bold().force_styling(true))
                .add("alias", "base");

            let result = styles.apply("alias", "text");
            // Should contain ANSI bold codes
            assert!(result.contains("\x1b[1m"));
            assert!(result.contains("text"));
        }

        #[test]
        fn test_apply_through_chain() {
            let styles = Styles::new()
                .add("visual", Style::new().red().force_styling(true))
                .add("presentation", "visual")
                .add("semantic", "presentation");

            let result = styles.apply("semantic", "error");
            // Should contain ANSI red codes
            assert!(result.contains("\x1b[31m"));
            assert!(result.contains("error"));
        }

        #[test]
        fn test_apply_dangling_alias_shows_indicator() {
            let styles = Styles::new().add("orphan", "missing");
            let result = styles.apply("orphan", "text");
            assert_eq!(result, "(!?) text");
        }

        #[test]
        fn test_apply_cycle_shows_indicator() {
            let styles = Styles::new()
                .add("a", "b")
                .add("b", "a");

            let result = styles.apply("a", "text");
            assert_eq!(result, "(!?) text");
        }

        #[test]
        fn test_apply_plain_through_alias() {
            let styles = Styles::new()
                .add("base", Style::new().bold())
                .add("alias", "base");

            let result = styles.apply_plain("alias", "text");
            assert_eq!(result, "text");
        }

        #[test]
        fn test_apply_debug_through_alias() {
            let styles = Styles::new()
                .add("base", Style::new().bold())
                .add("alias", "base");

            // Debug mode should use the requested name, not the resolved target
            let result = styles.apply_debug("alias", "text");
            assert_eq!(result, "[alias]text[/alias]");
        }

        #[test]
        fn test_apply_debug_dangling_alias() {
            let styles = Styles::new().add("orphan", "missing");
            let result = styles.apply_debug("orphan", "text");
            assert_eq!(result, "(!?) text");
        }

        // --- Theme with Aliases Tests ---

        #[test]
        fn test_theme_add_concrete() {
            let theme = Theme::new().add("bold", Style::new().bold());
            assert!(theme.styles().has("bold"));
        }

        #[test]
        fn test_theme_add_alias_str() {
            let theme = Theme::new()
                .add("base", Style::new().dim())
                .add("alias", "base");

            assert!(theme.styles().has("base"));
            assert!(theme.styles().has("alias"));
        }

        #[test]
        fn test_theme_add_alias_string() {
            let target = String::from("base");
            let theme = Theme::new()
                .add("base", Style::new().dim())
                .add("alias", target);

            assert!(theme.styles().has("alias"));
        }

        #[test]
        fn test_theme_validate_valid() {
            let theme = Theme::new()
                .add("visual", Style::new().cyan())
                .add("semantic", "visual");

            assert!(theme.validate().is_ok());
        }

        #[test]
        fn test_theme_validate_invalid() {
            let theme = Theme::new().add("orphan", "missing");
            assert!(theme.validate().is_err());
        }

        // --- Render with Aliases Tests ---

        #[test]
        fn test_render_with_alias() {
            let theme = Theme::new()
                .add("base", Style::new().bold())
                .add("alias", "base");

            let output = render_with_output(
                r#"{{ "text" | style("alias") }}"#,
                &serde_json::json!({}),
                ThemeChoice::from(&theme),
                OutputMode::Text,
            )
            .unwrap();

            assert_eq!(output, "text");
        }

        #[test]
        fn test_render_with_alias_chain() {
            let theme = Theme::new()
                .add("muted", Style::new().dim())
                .add("disabled", "muted")
                .add("timestamp", "disabled");

            let output = render_with_output(
                r#"{{ "12:00" | style("timestamp") }}"#,
                &serde_json::json!({}),
                ThemeChoice::from(&theme),
                OutputMode::Text,
            )
            .unwrap();

            assert_eq!(output, "12:00");
        }

        #[test]
        fn test_render_fails_with_dangling_alias() {
            let theme = Theme::new().add("orphan", "missing");

            let result = render_with_output(
                r#"{{ "text" | style("orphan") }}"#,
                &serde_json::json!({}),
                ThemeChoice::from(&theme),
                OutputMode::Text,
            );

            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(err.to_string().contains("orphan"));
            assert!(err.to_string().contains("missing"));
        }

        #[test]
        fn test_render_fails_with_cycle() {
            let theme = Theme::new()
                .add("a", "b")
                .add("b", "a");

            let result = render_with_output(
                r#"{{ "text" | style("a") }}"#,
                &serde_json::json!({}),
                ThemeChoice::from(&theme),
                OutputMode::Text,
            );

            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("cycle"));
        }

        #[test]
        fn test_renderer_fails_with_invalid_theme() {
            let theme = Theme::new().add("orphan", "missing");
            let result = Renderer::new(theme);
            assert!(result.is_err());
        }

        #[test]
        fn test_renderer_succeeds_with_valid_aliases() {
            let theme = Theme::new()
                .add("base", Style::new().bold())
                .add("alias", "base");

            let result = Renderer::new(theme);
            assert!(result.is_ok());
        }

        // --- StyleValue Tests ---

        #[test]
        fn test_style_value_from_style() {
            let value: StyleValue = Style::new().bold().into();
            assert!(matches!(value, StyleValue::Concrete(_)));
        }

        #[test]
        fn test_style_value_from_str() {
            let value: StyleValue = "target".into();
            match value {
                StyleValue::Alias(s) => assert_eq!(s, "target"),
                _ => panic!("Expected Alias"),
            }
        }

        #[test]
        fn test_style_value_from_string() {
            let value: StyleValue = String::from("target").into();
            match value {
                StyleValue::Alias(s) => assert_eq!(s, "target"),
                _ => panic!("Expected Alias"),
            }
        }

        // --- Error Display Tests ---

        #[test]
        fn test_unresolved_alias_error_display() {
            let err = StyleValidationError::UnresolvedAlias {
                from: "orphan".to_string(),
                to: "missing".to_string(),
            };
            let msg = err.to_string();
            assert!(msg.contains("orphan"));
            assert!(msg.contains("missing"));
        }

        #[test]
        fn test_cycle_detected_error_display() {
            let err = StyleValidationError::CycleDetected {
                path: vec!["a".to_string(), "b".to_string(), "a".to_string()],
            };
            let msg = err.to_string();
            assert!(msg.contains("cycle"));
            assert!(msg.contains("a -> b -> a"));
        }

        // --- Three-Layer Pattern Test ---

        #[test]
        fn test_three_layer_styling_pattern() {
            // This test demonstrates the full three-layer pattern from the user's docs
            let theme = Theme::new()
                // Visual layer - actual colors and decorations
                .add("dim_style", Style::new().dim())
                .add("cyan_bold", Style::new().cyan().bold())
                .add("yellow_bg", Style::new().on_yellow())
                // Presentation layer - consistent cross-app concepts
                .add("muted", "dim_style")
                .add("accent", "cyan_bold")
                .add("highlighted", "yellow_bg")
                // Semantic layer - data-specific names
                .add("timestamp", "muted")
                .add("title", "accent")
                .add("selected_item", "highlighted");

            // Validation should pass
            assert!(theme.validate().is_ok());

            // All semantic styles should resolve
            assert!(theme.styles().resolve("timestamp").is_some());
            assert!(theme.styles().resolve("title").is_some());
            assert!(theme.styles().resolve("selected_item").is_some());

            // Render should work
            let output = render_with_output(
                r#"{{ time | style("timestamp") }} - {{ name | style("title") }}"#,
                &serde_json::json!({"time": "12:00", "name": "Report"}),
                ThemeChoice::from(&theme),
                OutputMode::Text,
            )
            .unwrap();

            assert_eq!(output, "12:00 - Report");
        }
    }
}
