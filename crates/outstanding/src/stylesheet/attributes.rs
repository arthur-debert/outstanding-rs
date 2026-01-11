//! Style attribute types and merging logic.
//!
//! This module defines [`StyleAttributes`], the intermediate representation of
//! style properties parsed from YAML before conversion to `console::Style`.
//!
//! # Attribute Merging
//!
//! When resolving adaptive styles, mode-specific attributes merge onto base:
//!
//! ```yaml
//! footer:
//!   fg: gray        # Base
//!   bold: true      # Shared
//!   light:
//!     fg: black     # Override in light mode
//!   dark:
//!     fg: white     # Override in dark mode
//! ```
//!
//! The merge uses `Option<T>` semantics: `Some` values in the override replace
//! base values, `None` values preserve the base.

use console::Style;

use super::color::ColorDef;
use super::error::StylesheetError;

/// Parsed style attributes from YAML.
///
/// All fields are optional to support both full definitions and partial overrides.
/// When merging, `Some` values override, `None` values preserve the base.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleAttributes {
    /// Foreground (text) color.
    pub fg: Option<ColorDef>,
    /// Background color.
    pub bg: Option<ColorDef>,
    /// Bold text.
    pub bold: Option<bool>,
    /// Dimmed/faded text.
    pub dim: Option<bool>,
    /// Italic text.
    pub italic: Option<bool>,
    /// Underlined text.
    pub underline: Option<bool>,
    /// Blinking text (limited terminal support).
    pub blink: Option<bool>,
    /// Swap fg/bg colors.
    pub reverse: Option<bool>,
    /// Hidden text.
    pub hidden: Option<bool>,
    /// Strikethrough text.
    pub strikethrough: Option<bool>,
}

impl StyleAttributes {
    /// Creates empty attributes (all None).
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses attributes from a YAML mapping.
    ///
    /// Ignores `light` and `dark` keys (handled separately by the parser).
    pub fn parse_mapping(
        map: &serde_yaml::Mapping,
        style_name: &str,
    ) -> Result<Self, StylesheetError> {
        let mut attrs = StyleAttributes::new();

        for (key, value) in map {
            let key_str = key
                .as_str()
                .ok_or_else(|| StylesheetError::InvalidDefinition {
                    style: style_name.to_string(),
                    message: format!("Non-string key in style definition: {:?}", key),
                    path: None,
                })?;

            // Skip mode sections (handled by parent parser)
            if key_str == "light" || key_str == "dark" {
                continue;
            }

            attrs.set_attribute(key_str, value, style_name)?;
        }

        Ok(attrs)
    }

    /// Sets a single attribute from a YAML value.
    fn set_attribute(
        &mut self,
        name: &str,
        value: &serde_yaml::Value,
        style_name: &str,
    ) -> Result<(), StylesheetError> {
        match name {
            "fg" => {
                self.fg = Some(ColorDef::parse_value(value).map_err(|e| {
                    StylesheetError::InvalidColor {
                        style: style_name.to_string(),
                        value: e,
                        path: None,
                    }
                })?);
            }
            "bg" => {
                self.bg = Some(ColorDef::parse_value(value).map_err(|e| {
                    StylesheetError::InvalidColor {
                        style: style_name.to_string(),
                        value: e,
                        path: None,
                    }
                })?);
            }
            "bold" => {
                self.bold = Some(parse_bool(value, name, style_name)?);
            }
            "dim" => {
                self.dim = Some(parse_bool(value, name, style_name)?);
            }
            "italic" => {
                self.italic = Some(parse_bool(value, name, style_name)?);
            }
            "underline" => {
                self.underline = Some(parse_bool(value, name, style_name)?);
            }
            "blink" => {
                self.blink = Some(parse_bool(value, name, style_name)?);
            }
            "reverse" => {
                self.reverse = Some(parse_bool(value, name, style_name)?);
            }
            "hidden" => {
                self.hidden = Some(parse_bool(value, name, style_name)?);
            }
            "strikethrough" => {
                self.strikethrough = Some(parse_bool(value, name, style_name)?);
            }
            _ => {
                return Err(StylesheetError::UnknownAttribute {
                    style: style_name.to_string(),
                    attribute: name.to_string(),
                    path: None,
                });
            }
        }

        Ok(())
    }

    /// Merges another set of attributes onto this one.
    ///
    /// `Some` values in `other` override values in `self`.
    /// `None` values in `other` preserve values in `self`.
    ///
    /// Returns a new `StyleAttributes` with the merged result.
    pub fn merge(&self, other: &StyleAttributes) -> StyleAttributes {
        StyleAttributes {
            fg: other.fg.clone().or_else(|| self.fg.clone()),
            bg: other.bg.clone().or_else(|| self.bg.clone()),
            bold: other.bold.or(self.bold),
            dim: other.dim.or(self.dim),
            italic: other.italic.or(self.italic),
            underline: other.underline.or(self.underline),
            blink: other.blink.or(self.blink),
            reverse: other.reverse.or(self.reverse),
            hidden: other.hidden.or(self.hidden),
            strikethrough: other.strikethrough.or(self.strikethrough),
        }
    }

    /// Returns true if all attributes are None.
    pub fn is_empty(&self) -> bool {
        self.fg.is_none()
            && self.bg.is_none()
            && self.bold.is_none()
            && self.dim.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.blink.is_none()
            && self.reverse.is_none()
            && self.hidden.is_none()
            && self.strikethrough.is_none()
    }

    /// Converts these attributes to a `console::Style`.
    pub fn to_style(&self) -> Style {
        let mut style = Style::new();

        if let Some(ref fg) = self.fg {
            style = style.fg(fg.to_console_color());
        }
        if let Some(ref bg) = self.bg {
            style = style.bg(bg.to_console_color());
        }
        if self.bold == Some(true) {
            style = style.bold();
        }
        if self.dim == Some(true) {
            style = style.dim();
        }
        if self.italic == Some(true) {
            style = style.italic();
        }
        if self.underline == Some(true) {
            style = style.underlined();
        }
        if self.blink == Some(true) {
            style = style.blink();
        }
        if self.reverse == Some(true) {
            style = style.reverse();
        }
        if self.hidden == Some(true) {
            style = style.hidden();
        }
        if self.strikethrough == Some(true) {
            style = style.strikethrough();
        }

        style
    }
}

/// Parses a boolean value from YAML.
fn parse_bool(
    value: &serde_yaml::Value,
    attr: &str,
    style_name: &str,
) -> Result<bool, StylesheetError> {
    value
        .as_bool()
        .ok_or_else(|| StylesheetError::InvalidDefinition {
            style: style_name.to_string(),
            message: format!("'{}' must be a boolean, got {:?}", attr, value),
            path: None,
        })
}

/// Parses a shorthand string into attributes.
///
/// Shorthand format: space-separated attribute names and/or a color.
///
/// Examples:
/// - `"bold"` → bold: true
/// - `"cyan"` → fg: cyan
/// - `"cyan bold"` → fg: cyan, bold: true
/// - `"yellow italic"` → fg: yellow, italic: true
pub fn parse_shorthand(s: &str, style_name: &str) -> Result<StyleAttributes, StylesheetError> {
    let mut attrs = StyleAttributes::new();

    for part in s.split_whitespace() {
        match part.to_lowercase().as_str() {
            "bold" => attrs.bold = Some(true),
            "dim" => attrs.dim = Some(true),
            "italic" => attrs.italic = Some(true),
            "underline" => attrs.underline = Some(true),
            "blink" => attrs.blink = Some(true),
            "reverse" => attrs.reverse = Some(true),
            "hidden" => attrs.hidden = Some(true),
            "strikethrough" => attrs.strikethrough = Some(true),
            // If not a known attribute, try as a color
            _ => {
                if attrs.fg.is_some() {
                    return Err(StylesheetError::InvalidShorthand {
                        style: style_name.to_string(),
                        value: format!(
                            "Multiple colors in shorthand: already have fg, got '{}'",
                            part
                        ),
                        path: None,
                    });
                }
                attrs.fg = Some(ColorDef::parse_string(part).map_err(|e| {
                    StylesheetError::InvalidShorthand {
                        style: style_name.to_string(),
                        value: e,
                        path: None,
                    }
                })?);
            }
        }
    }

    if attrs.is_empty() {
        return Err(StylesheetError::InvalidShorthand {
            style: style_name.to_string(),
            value: format!("Empty or invalid shorthand: '{}'", s),
            path: None,
        });
    }

    Ok(attrs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::Color;
    use serde_yaml::{Mapping, Value};

    // =========================================================================
    // StyleAttributes::parse_mapping tests
    // =========================================================================

    #[test]
    fn test_parse_mapping_fg_only() {
        let mut map = Mapping::new();
        map.insert(Value::String("fg".into()), Value::String("red".into()));

        let attrs = StyleAttributes::parse_mapping(&map, "test").unwrap();
        assert_eq!(attrs.fg, Some(ColorDef::Named(Color::Red)));
        assert!(attrs.bg.is_none());
        assert!(attrs.bold.is_none());
    }

    #[test]
    fn test_parse_mapping_full() {
        let mut map = Mapping::new();
        map.insert(Value::String("fg".into()), Value::String("cyan".into()));
        map.insert(Value::String("bg".into()), Value::String("black".into()));
        map.insert(Value::String("bold".into()), Value::Bool(true));
        map.insert(Value::String("dim".into()), Value::Bool(false));
        map.insert(Value::String("italic".into()), Value::Bool(true));

        let attrs = StyleAttributes::parse_mapping(&map, "test").unwrap();
        assert_eq!(attrs.fg, Some(ColorDef::Named(Color::Cyan)));
        assert_eq!(attrs.bg, Some(ColorDef::Named(Color::Black)));
        assert_eq!(attrs.bold, Some(true));
        assert_eq!(attrs.dim, Some(false));
        assert_eq!(attrs.italic, Some(true));
    }

    #[test]
    fn test_parse_mapping_ignores_light_dark() {
        let mut map = Mapping::new();
        map.insert(Value::String("fg".into()), Value::String("red".into()));
        map.insert(
            Value::String("light".into()),
            Value::Mapping(Mapping::new()),
        );
        map.insert(Value::String("dark".into()), Value::Mapping(Mapping::new()));

        let attrs = StyleAttributes::parse_mapping(&map, "test").unwrap();
        assert_eq!(attrs.fg, Some(ColorDef::Named(Color::Red)));
        // light and dark should be ignored, not cause errors
    }

    #[test]
    fn test_parse_mapping_unknown_attribute() {
        let mut map = Mapping::new();
        map.insert(
            Value::String("unknown".into()),
            Value::String("value".into()),
        );

        let result = StyleAttributes::parse_mapping(&map, "test");
        assert!(matches!(
            result,
            Err(StylesheetError::UnknownAttribute { attribute, .. }) if attribute == "unknown"
        ));
    }

    #[test]
    fn test_parse_mapping_hex_color() {
        let mut map = Mapping::new();
        map.insert(Value::String("fg".into()), Value::String("#ff6b35".into()));

        let attrs = StyleAttributes::parse_mapping(&map, "test").unwrap();
        assert_eq!(attrs.fg, Some(ColorDef::Rgb(255, 107, 53)));
    }

    // =========================================================================
    // StyleAttributes::merge tests
    // =========================================================================

    #[test]
    fn test_merge_empty_onto_full() {
        let base = StyleAttributes {
            fg: Some(ColorDef::Named(Color::Red)),
            bold: Some(true),
            ..Default::default()
        };
        let empty = StyleAttributes::new();

        let merged = base.merge(&empty);
        assert_eq!(merged.fg, Some(ColorDef::Named(Color::Red)));
        assert_eq!(merged.bold, Some(true));
    }

    #[test]
    fn test_merge_full_onto_empty() {
        let empty = StyleAttributes::new();
        let full = StyleAttributes {
            fg: Some(ColorDef::Named(Color::Blue)),
            italic: Some(true),
            ..Default::default()
        };

        let merged = empty.merge(&full);
        assert_eq!(merged.fg, Some(ColorDef::Named(Color::Blue)));
        assert_eq!(merged.italic, Some(true));
    }

    #[test]
    fn test_merge_override() {
        let base = StyleAttributes {
            fg: Some(ColorDef::Named(Color::Red)),
            bold: Some(true),
            ..Default::default()
        };
        let override_attrs = StyleAttributes {
            fg: Some(ColorDef::Named(Color::Blue)),
            ..Default::default()
        };

        let merged = base.merge(&override_attrs);
        // fg overridden
        assert_eq!(merged.fg, Some(ColorDef::Named(Color::Blue)));
        // bold preserved
        assert_eq!(merged.bold, Some(true));
    }

    #[test]
    fn test_merge_preserves_unset() {
        let base = StyleAttributes {
            fg: Some(ColorDef::Named(Color::Red)),
            bg: Some(ColorDef::Named(Color::White)),
            bold: Some(true),
            dim: Some(true),
            ..Default::default()
        };
        let override_attrs = StyleAttributes {
            fg: Some(ColorDef::Named(Color::Blue)),
            bold: Some(false),
            ..Default::default()
        };

        let merged = base.merge(&override_attrs);
        assert_eq!(merged.fg, Some(ColorDef::Named(Color::Blue))); // overridden
        assert_eq!(merged.bg, Some(ColorDef::Named(Color::White))); // preserved
        assert_eq!(merged.bold, Some(false)); // overridden
        assert_eq!(merged.dim, Some(true)); // preserved
    }

    // =========================================================================
    // StyleAttributes::to_style tests
    // =========================================================================

    #[test]
    fn test_to_style_empty() {
        let attrs = StyleAttributes::new();
        let style = attrs.to_style();
        // Empty style - hard to test directly, but should not panic
        let _ = style.apply_to("test");
    }

    #[test]
    fn test_to_style_with_attributes() {
        let attrs = StyleAttributes {
            fg: Some(ColorDef::Named(Color::Red)),
            bold: Some(true),
            italic: Some(true),
            ..Default::default()
        };
        let style = attrs.to_style().force_styling(true);
        let output = style.apply_to("test").to_string();
        // Should contain ANSI codes
        assert!(output.contains("\x1b["));
        assert!(output.contains("test"));
    }

    // =========================================================================
    // parse_shorthand tests
    // =========================================================================

    #[test]
    fn test_parse_shorthand_single_attribute() {
        let attrs = parse_shorthand("bold", "test").unwrap();
        assert_eq!(attrs.bold, Some(true));
        assert!(attrs.fg.is_none());
    }

    #[test]
    fn test_parse_shorthand_single_color() {
        let attrs = parse_shorthand("cyan", "test").unwrap();
        assert_eq!(attrs.fg, Some(ColorDef::Named(Color::Cyan)));
        assert!(attrs.bold.is_none());
    }

    #[test]
    fn test_parse_shorthand_color_and_attribute() {
        let attrs = parse_shorthand("cyan bold", "test").unwrap();
        assert_eq!(attrs.fg, Some(ColorDef::Named(Color::Cyan)));
        assert_eq!(attrs.bold, Some(true));
    }

    #[test]
    fn test_parse_shorthand_multiple_attributes() {
        let attrs = parse_shorthand("bold italic underline", "test").unwrap();
        assert_eq!(attrs.bold, Some(true));
        assert_eq!(attrs.italic, Some(true));
        assert_eq!(attrs.underline, Some(true));
        assert!(attrs.fg.is_none());
    }

    #[test]
    fn test_parse_shorthand_color_with_multiple_attributes() {
        let attrs = parse_shorthand("yellow bold italic", "test").unwrap();
        assert_eq!(attrs.fg, Some(ColorDef::Named(Color::Yellow)));
        assert_eq!(attrs.bold, Some(true));
        assert_eq!(attrs.italic, Some(true));
    }

    #[test]
    fn test_parse_shorthand_multiple_colors_error() {
        let result = parse_shorthand("red blue", "test");
        assert!(matches!(
            result,
            Err(StylesheetError::InvalidShorthand { .. })
        ));
    }

    #[test]
    fn test_parse_shorthand_empty_error() {
        let result = parse_shorthand("", "test");
        assert!(matches!(
            result,
            Err(StylesheetError::InvalidShorthand { .. })
        ));
    }

    #[test]
    fn test_parse_shorthand_invalid_token_error() {
        let result = parse_shorthand("boldx", "test");
        assert!(matches!(
            result,
            Err(StylesheetError::InvalidShorthand { .. })
        ));
    }

    #[test]
    fn test_parse_shorthand_case_insensitive() {
        let attrs = parse_shorthand("BOLD ITALIC", "test").unwrap();
        assert_eq!(attrs.bold, Some(true));
        assert_eq!(attrs.italic, Some(true));
    }

    // =========================================================================
    // StyleAttributes::is_empty tests
    // =========================================================================

    #[test]
    fn test_is_empty_true() {
        assert!(StyleAttributes::new().is_empty());
    }

    #[test]
    fn test_is_empty_false() {
        let attrs = StyleAttributes {
            bold: Some(true),
            ..Default::default()
        };
        assert!(!attrs.is_empty());
    }
}
