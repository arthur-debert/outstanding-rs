//! Style registry for managing named styles.

use console::Style;
use std::collections::HashMap;

use super::error::StyleValidationError;
use super::value::StyleValue;

pub const DEFAULT_MISSING_STYLE_INDICATOR: &str = "(!?)";

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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn missing_indicator(mut self, indicator: &str) -> Self {
        self.missing_indicator = indicator.to_string();
        self
    }

    pub fn add<V: Into<StyleValue>>(mut self, name: &str, value: V) -> Self {
        self.styles.insert(name.to_string(), value.into());
        self
    }

    pub(crate) fn resolve(&self, name: &str) -> Option<&Style> {
        let mut current = name;
        let mut visited = std::collections::HashSet::new();

        loop {
            if !visited.insert(current) {
                return None;
            }
            match self.styles.get(current)? {
                StyleValue::Concrete(style) => return Some(style),
                StyleValue::Alias(next) => current = next,
            }
        }
    }

    fn can_resolve(&self, name: &str) -> bool {
        self.resolve(name).is_some()
    }

    pub fn validate(&self) -> Result<(), StyleValidationError> {
        for (name, value) in &self.styles {
            if let StyleValue::Alias(target) = value {
                self.validate_alias_chain(name, target)?;
            }
        }
        Ok(())
    }

    fn validate_alias_chain(&self, name: &str, target: &str) -> Result<(), StyleValidationError> {
        let mut current = target;
        let mut path = vec![name.to_string()];

        loop {
            let value =
                self.styles
                    .get(current)
                    .ok_or_else(|| StyleValidationError::UnresolvedAlias {
                        from: path.last().unwrap().clone(),
                        to: current.to_string(),
                    })?;

            path.push(current.to_string());

            if path[..path.len() - 1].contains(&current.to_string()) {
                return Err(StyleValidationError::CycleDetected { path });
            }

            match value {
                StyleValue::Concrete(_) => return Ok(()),
                StyleValue::Alias(next) => current = next,
            }
        }
    }

    pub fn apply(&self, name: &str, text: &str) -> String {
        match self.resolve(name) {
            Some(style) => style.apply_to(text).to_string(),
            None if self.missing_indicator.is_empty() => text.to_string(),
            None => format!("{} {}", self.missing_indicator, text),
        }
    }

    pub fn apply_plain(&self, name: &str, text: &str) -> String {
        if self.can_resolve(name) || self.missing_indicator.is_empty() {
            text.to_string()
        } else {
            format!("{} {}", self.missing_indicator, text)
        }
    }

    pub fn apply_with_mode(&self, name: &str, text: &str, use_color: bool) -> String {
        if use_color {
            self.apply(name, text)
        } else {
            self.apply_plain(name, text)
        }
    }

    pub fn apply_debug(&self, name: &str, text: &str) -> String {
        if self.can_resolve(name) {
            format!("[{}]{}[/{}]", name, text, name)
        } else if self.missing_indicator.is_empty() {
            text.to_string()
        } else {
            format!("{} {}", self.missing_indicator, text)
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.styles.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.styles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }

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
        assert!(result.contains("hello"));
        assert!(result.contains("\x1b[1m"));
    }

    #[test]
    fn test_styles_can_be_replaced() {
        let styles = Styles::new()
            .add("x", Style::new().red())
            .add("x", Style::new().green());

        assert_eq!(styles.len(), 1);
        assert!(styles.has("x"));
    }

    #[test]
    fn test_styles_apply_with_mode_color() {
        let styles = Styles::new().add("bold", Style::new().bold().force_styling(true));
        let result = styles.apply_with_mode("bold", "hello", true);
        assert!(result.contains("\x1b[1m"));
        assert!(result.contains("hello"));
    }

    #[test]
    fn test_styles_apply_with_mode_no_color() {
        let styles = Styles::new().add("bold", Style::new().bold());
        let result = styles.apply_with_mode("bold", "hello", false);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_styles_apply_with_mode_missing_style() {
        let styles = Styles::new();
        let result = styles.apply_with_mode("nonexistent", "hello", true);
        assert_eq!(result, "(!?) hello");
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
