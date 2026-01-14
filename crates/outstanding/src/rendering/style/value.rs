//! Style value types for concrete styles and aliases.

use console::Style;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
