//! Theme struct for building style collections.
//!
//! Themes are named collections of styles that can adapt to the user's
//! display mode (light/dark). They support both programmatic construction
//! and YAML-based file loading.
//!
//! # Adaptive Styles
//!
//! Individual styles can define mode-specific variations. When resolving
//! styles for rendering, the theme selects the appropriate variant based
//! on the current color mode:
//!
//! - **Base styles**: Used when no mode override exists
//! - **Light overrides**: Applied in light mode
//! - **Dark overrides**: Applied in dark mode
//!
//! # Construction Methods
//!
//! ## Programmatic (Builder API)
//!
//! ```rust
//! use outstanding::Theme;
//! use console::Style;
//!
//! let theme = Theme::new()
//!     // Non-adaptive styles work in all modes
//!     .add("muted", Style::new().dim())
//!     .add("accent", Style::new().cyan().bold())
//!     // Aliases reference other styles
//!     .add("disabled", "muted");
//! ```
//!
//! ## From YAML
//!
//! ```rust,ignore
//! use outstanding::Theme;
//!
//! let theme = Theme::from_yaml(r#"
//! header:
//!   fg: cyan
//!   bold: true
//!
//! footer:
//!   fg: gray
//!   light:
//!     fg: black
//!   dark:
//!     fg: white
//!
//! disabled: muted
//! "#)?;
//! ```
//!
//! # Mode Resolution
//!
//! Use [`resolve_styles`](Theme::resolve_styles) to get a `Styles` collection
//! for a specific color mode. This is typically called during rendering.

use std::collections::HashMap;

use console::Style;

use crate::style::{StyleValidationError, StyleValue, Styles};
use crate::stylesheet::{parse_stylesheet, StylesheetError, ThemeVariants};

use super::adaptive::ColorMode;

/// A named collection of styles used when rendering templates.
///
/// Themes can be constructed programmatically or loaded from YAML files.
/// They support adaptive styles that vary based on the user's color mode.
///
/// # Example: Programmatic Construction
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
///
/// # Example: From YAML
///
/// ```rust,ignore
/// use outstanding::Theme;
///
/// let theme = Theme::from_yaml(r#"
/// # Adaptive style - varies by mode
/// panel:
///   bg: gray
///   light:
///     bg: white
///   dark:
///     bg: black
///
/// # Non-adaptive styles work in all modes
/// header:
///   fg: cyan
///   bold: true
/// "#)?;
/// ```
#[derive(Debug, Clone)]
pub struct Theme {
    /// Base styles (always populated).
    base: HashMap<String, Style>,
    /// Light mode style overrides.
    light: HashMap<String, Style>,
    /// Dark mode style overrides.
    dark: HashMap<String, Style>,
    /// Alias definitions (name → target).
    aliases: HashMap<String, String>,
}

impl Theme {
    /// Creates an empty theme.
    pub fn new() -> Self {
        Self {
            base: HashMap::new(),
            light: HashMap::new(),
            dark: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    /// Creates a theme from YAML content.
    ///
    /// The YAML format supports:
    /// - Simple styles: `header: { fg: cyan, bold: true }`
    /// - Shorthand: `bold_text: bold` or `warning: "yellow italic"`
    /// - Aliases: `disabled: muted`
    /// - Adaptive styles with `light:` and `dark:` sections
    ///
    /// # Errors
    ///
    /// Returns a [`StylesheetError`] if parsing fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::Theme;
    ///
    /// let theme = Theme::from_yaml(r#"
    /// header:
    ///   fg: cyan
    ///   bold: true
    ///
    /// footer:
    ///   dim: true
    ///   light:
    ///     fg: black
    ///   dark:
    ///     fg: white
    /// "#)?;
    /// ```
    pub fn from_yaml(yaml: &str) -> Result<Self, StylesheetError> {
        let variants = parse_stylesheet(yaml)?;
        Ok(Self::from_variants(variants))
    }

    /// Creates a theme from pre-parsed theme variants.
    pub fn from_variants(variants: ThemeVariants) -> Self {
        Self {
            base: variants.base().clone(),
            light: variants.light().clone(),
            dark: variants.dark().clone(),
            aliases: variants.aliases().clone(),
        }
    }

    /// Adds a named style, returning an updated theme for chaining.
    ///
    /// The value can be either a concrete `Style` or a `&str`/`String` alias
    /// to another style name, enabling layered styling.
    ///
    /// # Non-Adaptive
    ///
    /// Styles added via this method are non-adaptive (same in all modes).
    /// For adaptive styles, use [`add_adaptive`](Self::add_adaptive) or YAML.
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
        match value.into() {
            StyleValue::Concrete(style) => {
                self.base.insert(name.to_string(), style);
            }
            StyleValue::Alias(target) => {
                self.aliases.insert(name.to_string(), target);
            }
        }
        self
    }

    /// Adds an adaptive style with separate light and dark variants.
    ///
    /// The base style is used when no mode override exists or when mode
    /// detection fails. Light and dark variants, if provided, override
    /// the base in their respective modes.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::Theme;
    /// use console::Style;
    ///
    /// let theme = Theme::new()
    ///     .add_adaptive(
    ///         "panel",
    ///         Style::new().dim(),                    // Base
    ///         Some(Style::new().fg(console::Color::Black)),  // Light mode
    ///         Some(Style::new().fg(console::Color::White)),  // Dark mode
    ///     );
    /// ```
    pub fn add_adaptive(
        mut self,
        name: &str,
        base: Style,
        light: Option<Style>,
        dark: Option<Style>,
    ) -> Self {
        self.base.insert(name.to_string(), base);
        if let Some(light_style) = light {
            self.light.insert(name.to_string(), light_style);
        }
        if let Some(dark_style) = dark {
            self.dark.insert(name.to_string(), dark_style);
        }
        self
    }

    /// Resolves styles for the given color mode.
    ///
    /// Returns a [`Styles`] collection with the appropriate style for each
    /// defined style name:
    ///
    /// - For styles with a mode-specific override, uses the override
    /// - For styles without an override, uses the base style
    /// - Aliases are preserved for resolution during rendering
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::{Theme, ColorMode};
    /// use console::Style;
    ///
    /// let theme = Theme::new()
    ///     .add("header", Style::new().cyan())
    ///     .add_adaptive(
    ///         "panel",
    ///         Style::new(),
    ///         Some(Style::new().fg(console::Color::Black)),
    ///         Some(Style::new().fg(console::Color::White)),
    ///     );
    ///
    /// // Get styles for dark mode
    /// let dark_styles = theme.resolve_styles(Some(ColorMode::Dark));
    /// ```
    pub fn resolve_styles(&self, mode: Option<ColorMode>) -> Styles {
        let mut styles = Styles::new();

        // Select the mode-specific overrides map
        let mode_overrides = match mode {
            Some(ColorMode::Light) => &self.light,
            Some(ColorMode::Dark) => &self.dark,
            None => &HashMap::new(),
        };

        // Add concrete styles (base, with mode overrides applied)
        for (name, base_style) in &self.base {
            let style = mode_overrides.get(name).unwrap_or(base_style);
            styles = styles.add(name, style.clone());
        }

        // Add aliases
        for (name, target) in &self.aliases {
            styles = styles.add(name, target.clone());
        }

        styles
    }

    /// Returns the underlying styles for the default mode (no override).
    ///
    /// This is provided for backward compatibility. Prefer using
    /// [`resolve_styles`](Self::resolve_styles) with an explicit mode.
    #[deprecated(since = "0.8.0", note = "Use resolve_styles(mode) instead")]
    pub fn styles(&self) -> Styles {
        self.resolve_styles(None)
    }

    /// Validates that all style aliases in this theme resolve correctly.
    ///
    /// This is called automatically at render time, but can be called
    /// explicitly for early error detection.
    pub fn validate(&self) -> Result<(), StyleValidationError> {
        // Validate using a resolved Styles instance
        self.resolve_styles(None).validate()
    }

    /// Returns true if no styles are defined.
    pub fn is_empty(&self) -> bool {
        self.base.is_empty() && self.aliases.is_empty()
    }

    /// Returns the number of defined styles (base + aliases).
    pub fn len(&self) -> usize {
        self.base.len() + self.aliases.len()
    }

    /// Returns the number of light mode overrides.
    pub fn light_override_count(&self) -> usize {
        self.light.len()
    }

    /// Returns the number of dark mode overrides.
    pub fn dark_override_count(&self) -> usize {
        self.dark.len()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_new_is_empty() {
        let theme = Theme::new();
        assert!(theme.is_empty());
        assert_eq!(theme.len(), 0);
    }

    #[test]
    fn test_theme_add_concrete() {
        let theme = Theme::new().add("bold", Style::new().bold());
        assert!(!theme.is_empty());
        assert_eq!(theme.len(), 1);
    }

    #[test]
    fn test_theme_add_alias_str() {
        let theme = Theme::new()
            .add("base", Style::new().dim())
            .add("alias", "base");

        assert_eq!(theme.len(), 2);

        let styles = theme.resolve_styles(None);
        assert!(styles.has("base"));
        assert!(styles.has("alias"));
    }

    #[test]
    fn test_theme_add_alias_string() {
        let target = String::from("base");
        let theme = Theme::new()
            .add("base", Style::new().dim())
            .add("alias", target);

        let styles = theme.resolve_styles(None);
        assert!(styles.has("alias"));
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

    #[test]
    fn test_theme_default() {
        let theme = Theme::default();
        assert!(theme.is_empty());
    }

    // =========================================================================
    // Adaptive style tests
    // =========================================================================

    #[test]
    fn test_theme_add_adaptive() {
        let theme = Theme::new().add_adaptive(
            "panel",
            Style::new().dim(),
            Some(Style::new().bold()),
            Some(Style::new().italic()),
        );

        assert_eq!(theme.len(), 1);
        assert_eq!(theme.light_override_count(), 1);
        assert_eq!(theme.dark_override_count(), 1);
    }

    #[test]
    fn test_theme_add_adaptive_light_only() {
        let theme =
            Theme::new().add_adaptive("panel", Style::new().dim(), Some(Style::new().bold()), None);

        assert_eq!(theme.light_override_count(), 1);
        assert_eq!(theme.dark_override_count(), 0);
    }

    #[test]
    fn test_theme_add_adaptive_dark_only() {
        let theme =
            Theme::new().add_adaptive("panel", Style::new().dim(), None, Some(Style::new().bold()));

        assert_eq!(theme.light_override_count(), 0);
        assert_eq!(theme.dark_override_count(), 1);
    }

    #[test]
    fn test_theme_resolve_styles_no_mode() {
        let theme = Theme::new()
            .add("header", Style::new().cyan())
            .add_adaptive(
                "panel",
                Style::new().dim(),
                Some(Style::new().bold()),
                Some(Style::new().italic()),
            );

        let styles = theme.resolve_styles(None);
        assert!(styles.has("header"));
        assert!(styles.has("panel"));
    }

    #[test]
    fn test_theme_resolve_styles_light_mode() {
        let theme = Theme::new().add_adaptive(
            "panel",
            Style::new().dim(),
            Some(Style::new().bold()),
            Some(Style::new().italic()),
        );

        let styles = theme.resolve_styles(Some(ColorMode::Light));
        assert!(styles.has("panel"));
        // The style should be the light override, not base
        // We can't easily check the actual style, but we verify resolution works
    }

    #[test]
    fn test_theme_resolve_styles_dark_mode() {
        let theme = Theme::new().add_adaptive(
            "panel",
            Style::new().dim(),
            Some(Style::new().bold()),
            Some(Style::new().italic()),
        );

        let styles = theme.resolve_styles(Some(ColorMode::Dark));
        assert!(styles.has("panel"));
    }

    #[test]
    fn test_theme_resolve_styles_preserves_aliases() {
        let theme = Theme::new()
            .add("base", Style::new().dim())
            .add("alias", "base");

        let styles = theme.resolve_styles(Some(ColorMode::Light));
        assert!(styles.has("base"));
        assert!(styles.has("alias"));

        // Validate that alias resolution still works
        assert!(styles.validate().is_ok());
    }

    // =========================================================================
    // YAML parsing tests
    // =========================================================================

    #[test]
    fn test_theme_from_yaml_simple() {
        let theme = Theme::from_yaml(
            r#"
            header:
                fg: cyan
                bold: true
            "#,
        )
        .unwrap();

        assert_eq!(theme.len(), 1);
        let styles = theme.resolve_styles(None);
        assert!(styles.has("header"));
    }

    #[test]
    fn test_theme_from_yaml_shorthand() {
        let theme = Theme::from_yaml(
            r#"
            bold_text: bold
            accent: cyan
            warning: "yellow italic"
            "#,
        )
        .unwrap();

        assert_eq!(theme.len(), 3);
    }

    #[test]
    fn test_theme_from_yaml_alias() {
        let theme = Theme::from_yaml(
            r#"
            muted:
                dim: true
            disabled: muted
            "#,
        )
        .unwrap();

        assert_eq!(theme.len(), 2);
        assert!(theme.validate().is_ok());
    }

    #[test]
    fn test_theme_from_yaml_adaptive() {
        let theme = Theme::from_yaml(
            r#"
            panel:
                fg: gray
                light:
                    fg: black
                dark:
                    fg: white
            "#,
        )
        .unwrap();

        assert_eq!(theme.len(), 1);
        assert_eq!(theme.light_override_count(), 1);
        assert_eq!(theme.dark_override_count(), 1);
    }

    #[test]
    fn test_theme_from_yaml_invalid() {
        let result = Theme::from_yaml("not valid yaml: [");
        assert!(result.is_err());
    }

    #[test]
    fn test_theme_from_yaml_complete() {
        let theme = Theme::from_yaml(
            r##"
            # Visual layer
            muted:
                dim: true

            accent:
                fg: cyan
                bold: true

            # Adaptive
            background:
                light:
                    bg: "#f8f8f8"
                dark:
                    bg: "#1e1e1e"

            # Aliases
            header: accent
            footer: muted
            "##,
        )
        .unwrap();

        // 3 concrete styles + 2 aliases = 5 total
        assert_eq!(theme.len(), 5);
        assert!(theme.validate().is_ok());

        // background is adaptive
        assert_eq!(theme.light_override_count(), 1);
        assert_eq!(theme.dark_override_count(), 1);
    }
}
