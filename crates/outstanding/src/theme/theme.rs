//! Theme struct for building style collections.

use crate::style::{StyleValidationError, StyleValue, Styles};

/// A named collection of styles used when rendering templates.
///
/// Themes wrap a [`Styles`] registry and provide a fluent builder API
/// for constructing style collections.
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
#[derive(Debug, Clone)]
pub struct Theme {
    pub(crate) styles: Styles,
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

#[cfg(test)]
mod tests {
    use super::*;
    use console::Style;

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

    #[test]
    fn test_theme_default() {
        let theme = Theme::default();
        assert!(theme.styles().is_empty());
    }

    #[test]
    fn test_theme_from_styles() {
        let styles = Styles::new()
            .add("bold", Style::new().bold())
            .add("dim", Style::new().dim());

        let theme = Theme::from_styles(styles);
        assert!(theme.styles().has("bold"));
        assert!(theme.styles().has("dim"));
    }
}
