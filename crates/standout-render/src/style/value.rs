use console::Style;

#[derive(Debug, Clone)]
pub enum StyleValue {
    Concrete(Style),
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
