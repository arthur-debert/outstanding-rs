//! Style definition types for stylesheet parsing.
//!
//! This module defines [`StyleDefinition`], the parsed representation of a single
//! style entry in a YAML stylesheet. Styles can be:
//!
//! - **Alias**: Reference to another style by name
//! - **Attributes**: Direct style with optional light/dark overrides
//!
//! # YAML Formats
//!
//! ```yaml
//! # Alias - string value that's a valid style name
//! disabled: muted
//!
//! # Shorthand - string with color/attribute keywords
//! warning: "yellow bold"
//!
//! # Full definition - mapping with attributes
//! header:
//!   fg: cyan
//!   bold: true
//!
//! # Adaptive definition - base plus light/dark overrides
//! panel:
//!   fg: gray
//!   light:
//!     fg: black
//!   dark:
//!     fg: white
//! ```

use super::attributes::{parse_shorthand, StyleAttributes};
use super::error::StylesheetError;

/// Parsed style definition from YAML.
///
/// Represents a single style entry before building into `console::Style`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleDefinition {
    /// Alias to another style by name.
    ///
    /// Alias chains are resolved during theme building.
    Alias(String),

    /// Concrete style definition with optional mode overrides.
    ///
    /// - `base`: Attributes shared across all modes
    /// - `light`: Optional overrides for light mode (merged onto base)
    /// - `dark`: Optional overrides for dark mode (merged onto base)
    Attributes {
        /// Base style attributes (used when no mode override exists).
        base: StyleAttributes,
        /// Light mode overrides (merged onto base).
        light: Option<StyleAttributes>,
        /// Dark mode overrides (merged onto base).
        dark: Option<StyleAttributes>,
    },
}

impl StyleDefinition {
    /// Parses a style definition from a YAML value.
    ///
    /// Determines the definition type based on the value structure:
    /// - String → Alias or Shorthand (depends on content)
    /// - Mapping → Full definition with optional light/dark
    pub fn parse(value: &serde_yaml::Value, style_name: &str) -> Result<Self, StylesheetError> {
        match value {
            serde_yaml::Value::String(s) => Self::parse_string(s, style_name),
            serde_yaml::Value::Mapping(map) => Self::parse_mapping(map, style_name),
            _ => Err(StylesheetError::InvalidDefinition {
                style: style_name.to_string(),
                message: format!("Expected string or mapping, got {:?}", value),
                path: None,
            }),
        }
    }

    /// Parses a string value as either an alias or shorthand.
    ///
    /// Heuristic: If the string contains spaces or known attribute keywords,
    /// treat it as shorthand. Otherwise, treat it as an alias.
    fn parse_string(s: &str, style_name: &str) -> Result<Self, StylesheetError> {
        let s = s.trim();

        // Empty string is invalid
        if s.is_empty() {
            return Err(StylesheetError::InvalidDefinition {
                style: style_name.to_string(),
                message: "Empty style definition".to_string(),
                path: None,
            });
        }

        // If it contains spaces, it's definitely shorthand
        if s.contains(' ') {
            let attrs = parse_shorthand(s, style_name)?;
            return Ok(StyleDefinition::Attributes {
                base: attrs,
                light: None,
                dark: None,
            });
        }

        // Single word: could be alias, color shorthand, or attribute shorthand
        // Try to parse as shorthand first (covers colors and attributes like "bold")
        match parse_shorthand(s, style_name) {
            Ok(attrs) => {
                // Check if this looks like an alias (valid identifier, not a color or attribute)
                if is_likely_alias(s) {
                    // It's an alias
                    Ok(StyleDefinition::Alias(s.to_string()))
                } else {
                    // It's shorthand (color or attribute)
                    Ok(StyleDefinition::Attributes {
                        base: attrs,
                        light: None,
                        dark: None,
                    })
                }
            }
            Err(_) => {
                // Not valid shorthand, must be an alias
                Ok(StyleDefinition::Alias(s.to_string()))
            }
        }
    }

    /// Parses a mapping value as a full style definition.
    fn parse_mapping(map: &serde_yaml::Mapping, style_name: &str) -> Result<Self, StylesheetError> {
        // Parse base attributes (excludes light/dark keys)
        let base = StyleAttributes::parse_mapping(map, style_name)?;

        // Parse light mode overrides if present
        let light = if let Some(light_val) = map.get(serde_yaml::Value::String("light".into())) {
            let light_map =
                light_val
                    .as_mapping()
                    .ok_or_else(|| StylesheetError::InvalidDefinition {
                        style: style_name.to_string(),
                        message: "'light' must be a mapping".to_string(),
                        path: None,
                    })?;
            Some(StyleAttributes::parse_mapping(light_map, style_name)?)
        } else {
            None
        };

        // Parse dark mode overrides if present
        let dark = if let Some(dark_val) = map.get(serde_yaml::Value::String("dark".into())) {
            let dark_map =
                dark_val
                    .as_mapping()
                    .ok_or_else(|| StylesheetError::InvalidDefinition {
                        style: style_name.to_string(),
                        message: "'dark' must be a mapping".to_string(),
                        path: None,
                    })?;
            Some(StyleAttributes::parse_mapping(dark_map, style_name)?)
        } else {
            None
        };

        Ok(StyleDefinition::Attributes { base, light, dark })
    }

    /// Returns true if this is an alias definition.
    pub fn is_alias(&self) -> bool {
        matches!(self, StyleDefinition::Alias(_))
    }

    /// Returns the alias target if this is an alias, None otherwise.
    pub fn alias_target(&self) -> Option<&str> {
        match self {
            StyleDefinition::Alias(target) => Some(target),
            _ => None,
        }
    }
}

/// Checks if a single-word string is likely an alias rather than shorthand.
///
/// Returns true for strings that don't match known colors or attributes.
fn is_likely_alias(s: &str) -> bool {
    let lower = s.to_lowercase();

    // Known attribute keywords (not aliases)
    let attributes = [
        "bold",
        "dim",
        "italic",
        "underline",
        "blink",
        "reverse",
        "hidden",
        "strikethrough",
    ];

    if attributes.contains(&lower.as_str()) {
        return false;
    }

    // Known color names (not aliases)
    let colors = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "gray", "grey",
    ];

    if colors.contains(&lower.as_str()) {
        return false;
    }

    // Bright colors (not aliases)
    if lower.starts_with("bright_") {
        return false;
    }

    // Hex colors (not aliases)
    if s.starts_with('#') {
        return false;
    }

    // Everything else is likely an alias
    true
}

#[cfg(test)]
mod tests {
    use super::super::color::ColorDef;
    use super::*;
    use console::Color;

    // =========================================================================
    // Alias parsing tests
    // =========================================================================

    #[test]
    fn test_parse_alias() {
        let value = serde_yaml::Value::String("muted".into());
        let def = StyleDefinition::parse(&value, "test").unwrap();
        assert!(matches!(def, StyleDefinition::Alias(s) if s == "muted"));
    }

    #[test]
    fn test_parse_alias_with_underscore() {
        let value = serde_yaml::Value::String("my_style".into());
        let def = StyleDefinition::parse(&value, "test").unwrap();
        assert!(matches!(def, StyleDefinition::Alias(s) if s == "my_style"));
    }

    #[test]
    fn test_parse_alias_with_hyphen() {
        let value = serde_yaml::Value::String("my-style".into());
        let def = StyleDefinition::parse(&value, "test").unwrap();
        assert!(matches!(def, StyleDefinition::Alias(s) if s == "my-style"));
    }

    // =========================================================================
    // Shorthand parsing tests
    // =========================================================================

    #[test]
    fn test_parse_shorthand_single_attribute() {
        let value = serde_yaml::Value::String("bold".into());
        let def = StyleDefinition::parse(&value, "test").unwrap();
        match def {
            StyleDefinition::Attributes { base, light, dark } => {
                assert_eq!(base.bold, Some(true));
                assert!(light.is_none());
                assert!(dark.is_none());
            }
            _ => panic!("Expected Attributes"),
        }
    }

    #[test]
    fn test_parse_shorthand_single_color() {
        let value = serde_yaml::Value::String("cyan".into());
        let def = StyleDefinition::parse(&value, "test").unwrap();
        match def {
            StyleDefinition::Attributes { base, .. } => {
                assert_eq!(base.fg, Some(ColorDef::Named(Color::Cyan)));
            }
            _ => panic!("Expected Attributes"),
        }
    }

    #[test]
    fn test_parse_shorthand_multiple() {
        let value = serde_yaml::Value::String("yellow bold italic".into());
        let def = StyleDefinition::parse(&value, "test").unwrap();
        match def {
            StyleDefinition::Attributes { base, .. } => {
                assert_eq!(base.fg, Some(ColorDef::Named(Color::Yellow)));
                assert_eq!(base.bold, Some(true));
                assert_eq!(base.italic, Some(true));
            }
            _ => panic!("Expected Attributes"),
        }
    }

    // =========================================================================
    // Full definition parsing tests
    // =========================================================================

    #[test]
    fn test_parse_mapping_simple() {
        let yaml = r#"
            fg: cyan
            bold: true
        "#;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let def = StyleDefinition::parse(&value, "test").unwrap();

        match def {
            StyleDefinition::Attributes { base, light, dark } => {
                assert_eq!(base.fg, Some(ColorDef::Named(Color::Cyan)));
                assert_eq!(base.bold, Some(true));
                assert!(light.is_none());
                assert!(dark.is_none());
            }
            _ => panic!("Expected Attributes"),
        }
    }

    #[test]
    fn test_parse_mapping_with_light_dark() {
        let yaml = r#"
            fg: gray
            bold: true
            light:
                fg: black
            dark:
                fg: white
        "#;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let def = StyleDefinition::parse(&value, "test").unwrap();

        match def {
            StyleDefinition::Attributes { base, light, dark } => {
                assert_eq!(base.fg, Some(ColorDef::Named(Color::White))); // gray maps to white
                assert_eq!(base.bold, Some(true));

                let light = light.expect("light should be Some");
                assert_eq!(light.fg, Some(ColorDef::Named(Color::Black)));
                assert!(light.bold.is_none()); // Not overridden in light

                let dark = dark.expect("dark should be Some");
                assert_eq!(dark.fg, Some(ColorDef::Named(Color::White)));
                assert!(dark.bold.is_none()); // Not overridden in dark
            }
            _ => panic!("Expected Attributes"),
        }
    }

    #[test]
    fn test_parse_mapping_only_light() {
        let yaml = r#"
            fg: gray
            light:
                fg: black
        "#;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let def = StyleDefinition::parse(&value, "test").unwrap();

        match def {
            StyleDefinition::Attributes { light, dark, .. } => {
                assert!(light.is_some());
                assert!(dark.is_none());
            }
            _ => panic!("Expected Attributes"),
        }
    }

    #[test]
    fn test_parse_mapping_only_dark() {
        let yaml = r#"
            fg: gray
            dark:
                fg: white
        "#;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let def = StyleDefinition::parse(&value, "test").unwrap();

        match def {
            StyleDefinition::Attributes { light, dark, .. } => {
                assert!(light.is_none());
                assert!(dark.is_some());
            }
            _ => panic!("Expected Attributes"),
        }
    }

    // =========================================================================
    // Edge cases
    // =========================================================================

    #[test]
    fn test_parse_empty_string_error() {
        let value = serde_yaml::Value::String("".into());
        let result = StyleDefinition::parse(&value, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_whitespace_only_error() {
        let value = serde_yaml::Value::String("   ".into());
        let result = StyleDefinition::parse(&value, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_type_error() {
        let value = serde_yaml::Value::Number(42.into());
        let result = StyleDefinition::parse(&value, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_light_not_mapping_error() {
        let yaml = r#"
            fg: cyan
            light: invalid
        "#;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = StyleDefinition::parse(&value, "test");
        assert!(matches!(
            result,
            Err(StylesheetError::InvalidDefinition { .. })
        ));
    }

    // =========================================================================
    // is_alias and alias_target tests
    // =========================================================================

    #[test]
    fn test_is_alias_true() {
        let def = StyleDefinition::Alias("target".into());
        assert!(def.is_alias());
        assert_eq!(def.alias_target(), Some("target"));
    }

    #[test]
    fn test_is_alias_false() {
        let def = StyleDefinition::Attributes {
            base: StyleAttributes::new(),
            light: None,
            dark: None,
        };
        assert!(!def.is_alias());
        assert!(def.alias_target().is_none());
    }

    // =========================================================================
    // is_likely_alias tests
    // =========================================================================

    #[test]
    fn test_is_likely_alias_true() {
        assert!(is_likely_alias("muted"));
        assert!(is_likely_alias("accent"));
        assert!(is_likely_alias("my_style"));
        assert!(is_likely_alias("headerStyle"));
    }

    #[test]
    fn test_is_likely_alias_false_for_colors() {
        assert!(!is_likely_alias("red"));
        assert!(!is_likely_alias("cyan"));
        assert!(!is_likely_alias("bright_red"));
    }

    #[test]
    fn test_is_likely_alias_false_for_attributes() {
        assert!(!is_likely_alias("bold"));
        assert!(!is_likely_alias("italic"));
        assert!(!is_likely_alias("dim"));
    }
}
