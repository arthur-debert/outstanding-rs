//! Main stylesheet parser and theme variant builder.
//!
//! This module provides the entry point for parsing YAML stylesheets and
//! building [`ThemeVariants`] that can be resolved based on color mode.
//!
//! # Architecture
//!
//! The parsing process has two phases:
//!
//! 1. **Parse**: YAML → `HashMap<String, StyleDefinition>`
//! 2. **Build**: StyleDefinitions → `ThemeVariants` (base/light/dark style maps)
//!
//! During the build phase:
//! - Aliases are recorded for later resolution
//! - Base styles are computed from attribute definitions
//! - Light/dark variants are computed by merging mode overrides onto base
//!
//! # Mode Resolution
//!
//! When resolving styles for a specific mode, the variant merger:
//! - Returns the mode-specific style if one was defined
//! - Falls back to base style if no mode override exists
//!
//! This means styles with no `light:` or `dark:` sections work in all modes,
//! while adaptive styles provide mode-specific overrides.

use std::collections::HashMap;

use console::Style;

use super::definition::StyleDefinition;
use super::error::StylesheetError;
use crate::style::StyleValue;
use crate::theme::ColorMode;

/// Theme variants containing styles for base, light, and dark modes.
///
/// Each variant is a map of style names to concrete `console::Style` values.
/// Alias definitions are stored separately and resolved at lookup time.
///
/// # Resolution Strategy
///
/// When looking up a style for a given mode:
///
/// 1. If the style is an alias, follow the chain to find the concrete style
/// 2. For concrete styles, check if a mode-specific variant exists
/// 3. If yes, return the mode variant (base merged with mode overrides)
/// 4. If no, return the base style
///
/// # Pruning
///
/// During construction, mode variants are only stored if they differ from base.
/// This optimization means:
/// - Styles with no `light:` or `dark:` sections only have base entries
/// - Styles with overrides have entries in the relevant mode map
#[derive(Debug, Clone)]
pub struct ThemeVariants {
    /// Base styles (always populated for non-alias definitions).
    base: HashMap<String, Style>,

    /// Light mode styles (only populated for styles with light overrides).
    light: HashMap<String, Style>,

    /// Dark mode styles (only populated for styles with dark overrides).
    dark: HashMap<String, Style>,

    /// Alias definitions: style name → target style name.
    aliases: HashMap<String, String>,
}

impl ThemeVariants {
    /// Creates empty theme variants.
    pub fn new() -> Self {
        Self {
            base: HashMap::new(),
            light: HashMap::new(),
            dark: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    /// Resolves styles for the given color mode.
    ///
    /// Returns a `HashMap<String, StyleValue>` where:
    /// - Aliases are preserved as `StyleValue::Alias`
    /// - Concrete styles are `StyleValue::Concrete` with the mode-appropriate style
    ///
    /// For light/dark modes, mode-specific styles take precedence over base.
    /// For unknown mode (None), only base styles are used.
    pub fn resolve(&self, mode: Option<ColorMode>) -> HashMap<String, StyleValue> {
        let mut result = HashMap::new();

        // Add aliases
        for (name, target) in &self.aliases {
            result.insert(name.clone(), StyleValue::Alias(target.clone()));
        }

        // Add concrete styles based on mode
        let mode_styles = match mode {
            Some(ColorMode::Light) => &self.light,
            Some(ColorMode::Dark) => &self.dark,
            None => &HashMap::new(), // No mode-specific overrides
        };

        for (name, style) in &self.base {
            // Check for mode-specific override
            let style = mode_styles.get(name).unwrap_or(style);
            result.insert(name.clone(), StyleValue::Concrete(style.clone()));
        }

        result
    }

    /// Returns the base styles map.
    pub fn base(&self) -> &HashMap<String, Style> {
        &self.base
    }

    /// Returns the light mode styles map.
    pub fn light(&self) -> &HashMap<String, Style> {
        &self.light
    }

    /// Returns the dark mode styles map.
    pub fn dark(&self) -> &HashMap<String, Style> {
        &self.dark
    }

    /// Returns the aliases map.
    pub fn aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }

    /// Returns true if no styles are defined.
    pub fn is_empty(&self) -> bool {
        self.base.is_empty() && self.aliases.is_empty()
    }

    /// Returns the number of defined styles (base + aliases).
    pub fn len(&self) -> usize {
        self.base.len() + self.aliases.len()
    }
}

impl Default for ThemeVariants {
    fn default() -> Self {
        Self::new()
    }
}

/// Parses a YAML stylesheet and builds theme variants.
///
/// # Arguments
///
/// * `yaml` - YAML content as a string
///
/// # Returns
///
/// A `ThemeVariants` containing base, light, and dark style maps.
///
/// # Errors
///
/// Returns `StylesheetError` if:
/// - YAML parsing fails
/// - Style definitions are invalid
/// - Colors or attributes are unrecognized
///
/// # Example
///
/// ```rust,ignore
/// use outstanding::stylesheet::parse_stylesheet;
///
/// let yaml = r#"
/// header:
///   fg: cyan
///   bold: true
///
/// muted:
///   dim: true
///
/// footer:
///   fg: gray
///   light:
///     fg: black
///   dark:
///     fg: white
///
/// disabled: muted
/// "#;
///
/// let variants = parse_stylesheet(yaml)?;
/// ```
pub fn parse_stylesheet(yaml: &str) -> Result<ThemeVariants, StylesheetError> {
    // Parse YAML into a mapping
    let root: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| StylesheetError::Parse {
            path: None,
            message: e.to_string(),
        })?;

    let mapping = root.as_mapping().ok_or_else(|| StylesheetError::Parse {
        path: None,
        message: "Stylesheet must be a YAML mapping".to_string(),
    })?;

    // Parse each style definition
    let mut definitions: HashMap<String, StyleDefinition> = HashMap::new();

    for (key, value) in mapping {
        let name = key.as_str().ok_or_else(|| StylesheetError::Parse {
            path: None,
            message: format!("Style name must be a string, got {:?}", key),
        })?;

        let def = StyleDefinition::parse(value, name)?;
        definitions.insert(name.to_string(), def);
    }

    // Build theme variants from definitions
    build_variants(&definitions)
}

/// Builds theme variants from parsed style definitions.
fn build_variants(
    definitions: &HashMap<String, StyleDefinition>,
) -> Result<ThemeVariants, StylesheetError> {
    let mut variants = ThemeVariants::new();

    for (name, def) in definitions {
        match def {
            StyleDefinition::Alias(target) => {
                variants.aliases.insert(name.clone(), target.clone());
            }
            StyleDefinition::Attributes { base, light, dark } => {
                // Build base style
                let base_style = base.to_style();
                variants.base.insert(name.clone(), base_style);

                // Build light variant if overrides exist
                if let Some(light_attrs) = light {
                    let merged = base.merge(light_attrs);
                    variants.light.insert(name.clone(), merged.to_style());
                }

                // Build dark variant if overrides exist
                if let Some(dark_attrs) = dark {
                    let merged = base.merge(dark_attrs);
                    variants.dark.insert(name.clone(), merged.to_style());
                }
            }
        }
    }

    Ok(variants)
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // parse_stylesheet basic tests
    // =========================================================================

    #[test]
    fn test_parse_empty_stylesheet() {
        let yaml = "{}";
        let variants = parse_stylesheet(yaml).unwrap();
        assert!(variants.is_empty());
    }

    #[test]
    fn test_parse_simple_style() {
        let yaml = r#"
            header:
                fg: cyan
                bold: true
        "#;
        let variants = parse_stylesheet(yaml).unwrap();

        assert_eq!(variants.len(), 1);
        assert!(variants.base().contains_key("header"));
        assert!(variants.light().is_empty());
        assert!(variants.dark().is_empty());
    }

    #[test]
    fn test_parse_shorthand_style() {
        let yaml = r#"
            bold_text: bold
            accent: cyan
            warning: "yellow italic"
        "#;
        let variants = parse_stylesheet(yaml).unwrap();

        assert_eq!(variants.base().len(), 3);
        assert!(variants.base().contains_key("bold_text"));
        assert!(variants.base().contains_key("accent"));
        assert!(variants.base().contains_key("warning"));
    }

    #[test]
    fn test_parse_alias() {
        let yaml = r#"
            muted:
                dim: true
            disabled: muted
        "#;
        let variants = parse_stylesheet(yaml).unwrap();

        assert_eq!(variants.base().len(), 1);
        assert_eq!(variants.aliases().len(), 1);
        assert_eq!(
            variants.aliases().get("disabled"),
            Some(&"muted".to_string())
        );
    }

    #[test]
    fn test_parse_adaptive_style() {
        let yaml = r#"
            footer:
                fg: gray
                bold: true
                light:
                    fg: black
                dark:
                    fg: white
        "#;
        let variants = parse_stylesheet(yaml).unwrap();

        assert!(variants.base().contains_key("footer"));
        assert!(variants.light().contains_key("footer"));
        assert!(variants.dark().contains_key("footer"));
    }

    #[test]
    fn test_parse_light_only() {
        let yaml = r#"
            panel:
                bg: gray
                light:
                    bg: white
        "#;
        let variants = parse_stylesheet(yaml).unwrap();

        assert!(variants.base().contains_key("panel"));
        assert!(variants.light().contains_key("panel"));
        assert!(!variants.dark().contains_key("panel"));
    }

    #[test]
    fn test_parse_dark_only() {
        let yaml = r#"
            panel:
                bg: gray
                dark:
                    bg: black
        "#;
        let variants = parse_stylesheet(yaml).unwrap();

        assert!(variants.base().contains_key("panel"));
        assert!(!variants.light().contains_key("panel"));
        assert!(variants.dark().contains_key("panel"));
    }

    // =========================================================================
    // ThemeVariants::resolve tests
    // =========================================================================

    #[test]
    fn test_resolve_no_mode() {
        let yaml = r#"
            header:
                fg: cyan
            footer:
                fg: gray
                light:
                    fg: black
                dark:
                    fg: white
        "#;
        let variants = parse_stylesheet(yaml).unwrap();
        let resolved = variants.resolve(None);

        // Should have both styles from base
        assert!(matches!(
            resolved.get("header"),
            Some(StyleValue::Concrete(_))
        ));
        assert!(matches!(
            resolved.get("footer"),
            Some(StyleValue::Concrete(_))
        ));
    }

    #[test]
    fn test_resolve_light_mode() {
        let yaml = r#"
            footer:
                fg: gray
                light:
                    fg: black
                dark:
                    fg: white
        "#;
        let variants = parse_stylesheet(yaml).unwrap();
        let resolved = variants.resolve(Some(ColorMode::Light));

        // footer should use light variant
        assert!(matches!(
            resolved.get("footer"),
            Some(StyleValue::Concrete(_))
        ));
    }

    #[test]
    fn test_resolve_dark_mode() {
        let yaml = r#"
            footer:
                fg: gray
                light:
                    fg: black
                dark:
                    fg: white
        "#;
        let variants = parse_stylesheet(yaml).unwrap();
        let resolved = variants.resolve(Some(ColorMode::Dark));

        // footer should use dark variant
        assert!(matches!(
            resolved.get("footer"),
            Some(StyleValue::Concrete(_))
        ));
    }

    #[test]
    fn test_resolve_preserves_aliases() {
        let yaml = r#"
            muted:
                dim: true
            disabled: muted
        "#;
        let variants = parse_stylesheet(yaml).unwrap();
        let resolved = variants.resolve(Some(ColorMode::Light));

        // muted should be concrete
        assert!(matches!(
            resolved.get("muted"),
            Some(StyleValue::Concrete(_))
        ));
        // disabled should be alias
        assert!(matches!(resolved.get("disabled"), Some(StyleValue::Alias(t)) if t == "muted"));
    }

    #[test]
    fn test_resolve_non_adaptive_uses_base() {
        let yaml = r#"
            header:
                fg: cyan
                bold: true
        "#;
        let variants = parse_stylesheet(yaml).unwrap();

        // Light mode
        let light = variants.resolve(Some(ColorMode::Light));
        assert!(matches!(light.get("header"), Some(StyleValue::Concrete(_))));

        // Dark mode
        let dark = variants.resolve(Some(ColorMode::Dark));
        assert!(matches!(dark.get("header"), Some(StyleValue::Concrete(_))));

        // No mode
        let none = variants.resolve(None);
        assert!(matches!(none.get("header"), Some(StyleValue::Concrete(_))));
    }

    // =========================================================================
    // Error tests
    // =========================================================================

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = "not: [valid: yaml";
        let result = parse_stylesheet(yaml);
        assert!(matches!(result, Err(StylesheetError::Parse { .. })));
    }

    #[test]
    fn test_parse_non_mapping_root() {
        let yaml = "- item1\n- item2";
        let result = parse_stylesheet(yaml);
        assert!(matches!(result, Err(StylesheetError::Parse { .. })));
    }

    #[test]
    fn test_parse_invalid_color() {
        let yaml = r#"
            bad:
                fg: not_a_color
        "#;
        let result = parse_stylesheet(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unknown_attribute() {
        let yaml = r#"
            bad:
                unknown: true
        "#;
        let result = parse_stylesheet(yaml);
        assert!(matches!(
            result,
            Err(StylesheetError::UnknownAttribute { .. })
        ));
    }

    // =========================================================================
    // Complex stylesheet tests
    // =========================================================================

    #[test]
    fn test_parse_complete_stylesheet() {
        let yaml = r##"
            # Visual layer
            muted:
                dim: true

            accent:
                fg: cyan
                bold: true

            # Adaptive styles
            background:
                light:
                    bg: "#f8f8f8"
                dark:
                    bg: "#1e1e1e"

            text:
                light:
                    fg: "#333333"
                dark:
                    fg: "#d4d4d4"

            border:
                dim: true
                light:
                    fg: "#cccccc"
                dark:
                    fg: "#444444"

            # Semantic layer - aliases
            header: accent
            footer: muted
            timestamp: muted
            title: accent
            error: red
            success: green
            warning: "yellow bold"
        "##;

        let variants = parse_stylesheet(yaml).unwrap();

        // Check counts
        // Base: muted, accent, background, text, border, error, success, warning = 8
        // Aliases: header, footer, timestamp, title = 4
        assert_eq!(variants.base().len(), 8);
        assert_eq!(variants.aliases().len(), 4);

        // Check adaptive styles have light/dark variants
        assert!(variants.light().contains_key("background"));
        assert!(variants.light().contains_key("text"));
        assert!(variants.light().contains_key("border"));
        assert!(variants.dark().contains_key("background"));
        assert!(variants.dark().contains_key("text"));
        assert!(variants.dark().contains_key("border"));

        // Check aliases
        assert_eq!(
            variants.aliases().get("header"),
            Some(&"accent".to_string())
        );
        assert_eq!(variants.aliases().get("footer"), Some(&"muted".to_string()));
    }
}
