//! Style registry for managing named styles.

use console::Style;
use std::collections::HashMap;

use super::error::StyleValidationError;
use super::value::StyleValue;

/// Default prefix shown when a style name is not found.
pub const DEFAULT_MISSING_STYLE_INDICATOR: &str = "(!?)";

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
    pub(crate) fn resolve(&self, name: &str) -> Option<&Style> {
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
            let value =
                self.styles
                    .get(current)
                    .ok_or_else(|| StyleValidationError::UnresolvedAlias {
                        from: path.last().unwrap().clone(),
                        to: current.to_string(),
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

    /// Returns a map of all style names to their resolved concrete styles.
    ///
    /// This is useful for passing styles to external processors like BBParser.
    /// Aliases are resolved to their target concrete styles, and styles that
    /// cannot be resolved (cycles, dangling aliases) are omitted.
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
    /// let resolved = styles.to_resolved_map();
    /// assert!(resolved.contains_key("bold"));
    /// assert!(resolved.contains_key("emphasis"));
    /// assert_eq!(resolved.len(), 2);
    /// ```
    pub fn to_resolved_map(&self) -> HashMap<String, Style> {
        let mut result = HashMap::new();
        for name in self.styles.keys() {
            if let Some(style) = self.resolve(name) {
                result.insert(name.clone(), style.clone());
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_styles_can_be_replaced() {
        let styles = Styles::new()
            .add("x", Style::new().red())
            .add("x", Style::new().green()); // Replace

        // Should only have one style
        assert_eq!(styles.len(), 1);
        assert!(styles.has("x"));
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
        let styles = Styles::new().add("a", "b").add("b", "a");

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
        let styles = Styles::new().add("a", "b").add("b", "c").add("c", "a");

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
            }
            _ => panic!("Expected UnresolvedAlias error"),
        }
    }

    #[test]
    fn test_validate_cycle_error() {
        let styles = Styles::new().add("a", "b").add("b", "a");

        let result = styles.validate();
        assert!(result.is_err());

        match result.unwrap_err() {
            StyleValidationError::CycleDetected { path } => {
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
        let styles = Styles::new().add("a", "b").add("b", "c").add("c", "a");

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
        let styles = Styles::new().add("a", "b").add("b", "a");

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

        let result = styles.apply_debug("alias", "text");
        assert_eq!(result, "[alias]text[/alias]");
    }

    #[test]
    fn test_apply_debug_dangling_alias() {
        let styles = Styles::new().add("orphan", "missing");
        let result = styles.apply_debug("orphan", "text");
        assert_eq!(result, "(!?) text");
    }
}
