use std::collections::HashMap;

use super::icon_mode::IconMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconDefinition {
    pub classic: String,
    pub nerdfont: Option<String>,
}

impl IconDefinition {
    pub fn new(classic: impl Into<String>) -> Self {
        Self {
            classic: classic.into(),
            nerdfont: None,
        }
    }

    pub fn with_nerdfont(mut self, nerdfont: impl Into<String>) -> Self {
        self.nerdfont = Some(nerdfont.into());
        self
    }

    pub fn resolve(&self, mode: IconMode) -> &str {
        match mode {
            IconMode::NerdFont => self.nerdfont.as_deref().unwrap_or(&self.classic),
            IconMode::Classic | IconMode::Auto => &self.classic,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct IconSet {
    icons: HashMap<String, IconDefinition>,
}

impl IconSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(mut self, name: impl Into<String>, def: IconDefinition) -> Self {
        self.icons.insert(name.into(), def);
        self
    }

    pub fn insert(&mut self, name: impl Into<String>, def: IconDefinition) {
        self.icons.insert(name.into(), def);
    }

    pub fn resolve(&self, mode: IconMode) -> HashMap<String, String> {
        self.icons
            .iter()
            .map(|(name, def)| (name.clone(), def.resolve(mode).to_string()))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.icons.is_empty()
    }

    pub fn len(&self) -> usize {
        self.icons.len()
    }

    pub fn merge(mut self, other: IconSet) -> Self {
        self.icons.extend(other.icons);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icon_definition_classic_only() {
        let icon = IconDefinition::new("⚪");
        assert_eq!(icon.classic, "⚪");
        assert_eq!(icon.nerdfont, None);
    }

    #[test]
    fn test_icon_definition_with_nerdfont() {
        let icon = IconDefinition::new("⚫").with_nerdfont("\u{f00c}");
        assert_eq!(icon.classic, "⚫");
        assert_eq!(icon.nerdfont, Some("\u{f00c}".to_string()));
    }

    #[test]
    fn test_icon_definition_resolve_classic_mode() {
        let icon = IconDefinition::new("⚫").with_nerdfont("\u{f00c}");
        assert_eq!(icon.resolve(IconMode::Classic), "⚫");
    }

    #[test]
    fn test_icon_definition_resolve_nerdfont_mode() {
        let icon = IconDefinition::new("⚫").with_nerdfont("\u{f00c}");
        assert_eq!(icon.resolve(IconMode::NerdFont), "\u{f00c}");
    }

    #[test]
    fn test_icon_definition_resolve_nerdfont_fallback() {
        let icon = IconDefinition::new("⚪");
        assert_eq!(icon.resolve(IconMode::NerdFont), "⚪");
    }

    #[test]
    fn test_icon_definition_resolve_auto_mode() {
        let icon = IconDefinition::new("⚫").with_nerdfont("\u{f00c}");
        assert_eq!(icon.resolve(IconMode::Auto), "⚫");
    }

    #[test]
    fn test_icon_definition_multi_char() {
        let icon = IconDefinition::new("[ok]").with_nerdfont("\u{f00c}");
        assert_eq!(icon.resolve(IconMode::Classic), "[ok]");
        assert_eq!(icon.resolve(IconMode::NerdFont), "\u{f00c}");
    }

    #[test]
    fn test_icon_definition_empty_string() {
        let icon = IconDefinition::new("");
        assert_eq!(icon.resolve(IconMode::Classic), "");
    }

    #[test]
    fn test_icon_definition_equality() {
        let a = IconDefinition::new("⚪").with_nerdfont("nf");
        let b = IconDefinition::new("⚪").with_nerdfont("nf");
        assert_eq!(a, b);
    }

    #[test]
    fn test_icon_set_new_is_empty() {
        let set = IconSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_icon_set_add() {
        let set = IconSet::new()
            .add("pending", IconDefinition::new("⚪"))
            .add("done", IconDefinition::new("⚫"));
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
    }

    #[test]
    fn test_icon_set_insert() {
        let mut set = IconSet::new();
        set.insert("pending", IconDefinition::new("⚪"));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_icon_set_resolve_classic() {
        let set = IconSet::new()
            .add("pending", IconDefinition::new("⚪"))
            .add("done", IconDefinition::new("⚫").with_nerdfont("\u{f00c}"));

        let resolved = set.resolve(IconMode::Classic);
        assert_eq!(resolved.get("pending").unwrap(), "⚪");
        assert_eq!(resolved.get("done").unwrap(), "⚫");
    }

    #[test]
    fn test_icon_set_resolve_nerdfont() {
        let set = IconSet::new()
            .add("pending", IconDefinition::new("⚪"))
            .add("done", IconDefinition::new("⚫").with_nerdfont("\u{f00c}"));

        let resolved = set.resolve(IconMode::NerdFont);
        assert_eq!(resolved.get("pending").unwrap(), "⚪");
        assert_eq!(resolved.get("done").unwrap(), "\u{f00c}");
    }

    #[test]
    fn test_icon_set_merge() {
        let base = IconSet::new()
            .add("keep", IconDefinition::new("K"))
            .add("override", IconDefinition::new("OLD"));

        let extension = IconSet::new()
            .add("override", IconDefinition::new("NEW"))
            .add("added", IconDefinition::new("A"));

        let merged = base.merge(extension);

        assert_eq!(merged.len(), 3);
        let resolved = merged.resolve(IconMode::Classic);
        assert_eq!(resolved.get("keep").unwrap(), "K");
        assert_eq!(resolved.get("override").unwrap(), "NEW");
        assert_eq!(resolved.get("added").unwrap(), "A");
    }

    #[test]
    fn test_icon_set_resolve_empty() {
        let set = IconSet::new();
        let resolved = set.resolve(IconMode::Classic);
        assert!(resolved.is_empty());
    }
}
