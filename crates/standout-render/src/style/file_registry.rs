use std::collections::HashMap;
use std::path::Path;

use super::super::theme::Theme;
use crate::file_loader::{
    build_embedded_registry, resolve_in_map, FileRegistry, FileRegistryConfig, LoadError,
};

use super::error::StylesheetError;

pub const STYLESHEET_EXTENSIONS: &[&str] = &[".css", ".yaml", ".yml"];

fn stylesheet_config() -> FileRegistryConfig<Theme> {
    FileRegistryConfig {
        extensions: STYLESHEET_EXTENSIONS,
        transform: |content| {
            parse_theme_content(content).map_err(|e| LoadError::Transform {
                name: String::new(),
                message: e.to_string(),
            })
        },
    }
}

pub(crate) fn parse_theme_content(content: &str) -> Result<Theme, StylesheetError> {
    let trimmed = content.trim_start();
    if trimmed.starts_with('.') || trimmed.starts_with("/*") || trimmed.starts_with("@media") {
        Theme::from_css(content)
    } else {
        Theme::from_yaml(content)
    }
}

pub struct StylesheetRegistry {
    inner: FileRegistry<Theme>,
    inline: HashMap<String, Theme>,
}

impl Default for StylesheetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl StylesheetRegistry {
    pub fn new() -> Self {
        Self {
            inner: FileRegistry::new(stylesheet_config()),
            inline: HashMap::new(),
        }
    }

    pub fn add_inline(
        &mut self,
        name: impl Into<String>,
        content: &str,
    ) -> Result<(), StylesheetError> {
        let theme = parse_theme_content(content)?;
        self.inline.insert(name.into(), theme);
        Ok(())
    }

    pub fn add_theme(&mut self, name: impl Into<String>, theme: Theme) {
        self.inline.insert(name.into(), theme);
    }

    pub fn add_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), StylesheetError> {
        self.inner.add_dir(path).map_err(|e| StylesheetError::Load {
            message: e.to_string(),
        })
    }

    pub fn add_embedded(&mut self, themes: HashMap<String, Theme>) {
        for (name, theme) in themes {
            self.inline.insert(name, theme);
        }
    }

    pub fn add_embedded_theme(&mut self, name: impl Into<String>, theme: Theme) {
        self.inner.add_embedded(&name.into(), theme);
    }

    /// `entries` are `(name_with_ext, content)` pairs; each theme is reachable
    /// by base name and by full name.
    pub fn from_embedded_entries(entries: &[(&str, &str)]) -> Result<Self, StylesheetError> {
        let mut registry = Self::new();

        registry.inline = build_embedded_registry(entries, STYLESHEET_EXTENSIONS, |content| {
            parse_theme_content(content)
        })?;

        Ok(registry)
    }

    pub fn get(&mut self, name: &str) -> Result<Theme, StylesheetError> {
        if let Some(theme) = resolve_in_map(&self.inline, name, STYLESHEET_EXTENSIONS) {
            return Ok(theme.clone());
        }

        let theme = self.inner.get(name).map_err(|e| StylesheetError::Load {
            message: e.to_string(),
        })?;

        let base_name = crate::file_loader::strip_extension(name, STYLESHEET_EXTENSIONS);
        Ok(theme.with_name(base_name))
    }

    pub fn contains(&self, name: &str) -> bool {
        resolve_in_map(&self.inline, name, STYLESHEET_EXTENSIONS).is_some()
            || self.inner.get_entry(name).is_some()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.inline
            .keys()
            .map(|s| s.as_str())
            .chain(self.inner.names())
    }

    pub fn len(&self) -> usize {
        self.inline.len() + self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inline.is_empty() && self.inner.is_empty()
    }

    pub fn clear(&mut self) {
        self.inline.clear();
        self.inner.clear();
    }

    pub fn refresh(&mut self) -> Result<(), StylesheetError> {
        self.inner.refresh().map_err(|e| StylesheetError::Load {
            message: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_registry_new_is_empty() {
        let registry = StylesheetRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_add_inline() {
        let mut registry = StylesheetRegistry::new();
        registry
            .add_inline(
                "test",
                r#"
                header:
                    fg: cyan
                    bold: true
                "#,
            )
            .unwrap();

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("test"));
    }

    #[test]
    fn test_registry_add_theme() {
        let mut registry = StylesheetRegistry::new();
        let theme = Theme::new().add("header", console::Style::new().cyan().bold());
        registry.add_theme("custom", theme);

        assert!(registry.contains("custom"));
        let retrieved = registry.get("custom").unwrap();
        assert!(retrieved.resolve_styles(None).has("header"));
    }

    #[test]
    fn test_registry_get_inline() {
        let mut registry = StylesheetRegistry::new();
        registry
            .add_inline(
                "darcula",
                r#"
                header:
                    fg: cyan
                muted:
                    dim: true
                "#,
            )
            .unwrap();

        let theme = registry.get("darcula").unwrap();
        let styles = theme.resolve_styles(None);
        assert!(styles.has("header"));
        assert!(styles.has("muted"));
    }

    #[test]
    fn test_registry_add_dir() {
        let temp_dir = TempDir::new().unwrap();
        let theme_path = temp_dir.path().join("monokai.yaml");
        fs::write(
            &theme_path,
            r#"
            keyword:
                fg: magenta
                bold: true
            string:
                fg: green
            "#,
        )
        .unwrap();

        let mut registry = StylesheetRegistry::new();
        registry.add_dir(temp_dir.path()).unwrap();

        let theme = registry.get("monokai").unwrap();
        let styles = theme.resolve_styles(None);
        assert!(styles.has("keyword"));
        assert!(styles.has("string"));
    }

    #[test]
    fn test_registry_inline_shadows_file() {
        let temp_dir = TempDir::new().unwrap();
        let theme_path = temp_dir.path().join("test.yaml");
        fs::write(
            &theme_path,
            r#"
            from_file:
                fg: red
            header:
                fg: red
            "#,
        )
        .unwrap();

        let mut registry = StylesheetRegistry::new();
        registry.add_dir(temp_dir.path()).unwrap();
        registry
            .add_inline(
                "test",
                r#"
            from_inline:
                fg: blue
            header:
                fg: blue
            "#,
            )
            .unwrap();

        let theme = registry.get("test").unwrap();
        let styles = theme.resolve_styles(None);
        assert!(styles.has("from_inline"));
        assert!(!styles.has("from_file"));
    }

    #[test]
    fn test_registry_extension_priority() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("theme.yaml"),
            r#"
            from_yaml:
                fg: cyan
            source:
                fg: cyan
            "#,
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("theme.yml"),
            r#"
            from_yml:
                fg: red
            source:
                fg: red
            "#,
        )
        .unwrap();

        let mut registry = StylesheetRegistry::new();
        registry.add_dir(temp_dir.path()).unwrap();

        let theme = registry.get("theme").unwrap();
        let styles = theme.resolve_styles(None);
        assert!(styles.has("from_yaml"));
        assert!(!styles.has("from_yml"));
    }

    #[test]
    fn test_registry_names() {
        let mut registry = StylesheetRegistry::new();
        registry.add_inline("alpha", "header: bold").unwrap();
        registry.add_inline("beta", "header: dim").unwrap();

        let names: Vec<&str> = registry.names().collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = StylesheetRegistry::new();
        registry.add_inline("test", "header: bold").unwrap();
        assert!(!registry.is_empty());

        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_not_found() {
        let mut registry = StylesheetRegistry::new();
        let result = registry.get("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_invalid_yaml() {
        let mut registry = StylesheetRegistry::new();
        let result = registry.add_inline("bad", "not: [valid: yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_hot_reload() {
        let temp_dir = TempDir::new().unwrap();
        let theme_path = temp_dir.path().join("dynamic.yaml");
        fs::write(
            &theme_path,
            r#"
            version_v1:
                fg: red
            header:
                fg: red
            "#,
        )
        .unwrap();

        let mut registry = StylesheetRegistry::new();
        registry.add_dir(temp_dir.path()).unwrap();

        let theme1 = registry.get("dynamic").unwrap();
        let styles1 = theme1.resolve_styles(None);
        assert!(styles1.has("version_v1"));

        fs::write(
            &theme_path,
            r#"
            version_v2:
                fg: green
            updated_style:
                fg: blue
            header:
                fg: blue
            "#,
        )
        .unwrap();

        registry.refresh().unwrap();
        let theme2 = registry.get("dynamic").unwrap();
        let styles2 = theme2.resolve_styles(None);
        assert!(styles2.has("updated_style"));
    }

    #[test]
    fn test_registry_adaptive_theme() {
        let mut registry = StylesheetRegistry::new();
        registry
            .add_inline(
                "adaptive",
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

        let theme = registry.get("adaptive").unwrap();

        let light_styles = theme.resolve_styles(Some(crate::ColorMode::Light));
        assert!(light_styles.has("panel"));

        let dark_styles = theme.resolve_styles(Some(crate::ColorMode::Dark));
        assert!(dark_styles.has("panel"));
    }

    #[test]
    fn test_from_embedded_entries_single() {
        let entries: &[(&str, &str)] = &[("test.yaml", "header:\n    fg: cyan\n    bold: true")];
        let registry = StylesheetRegistry::from_embedded_entries(entries).unwrap();

        assert!(registry.contains("test"));
        assert!(registry.contains("test.yaml"));
    }

    #[test]
    fn test_from_embedded_entries_multiple() {
        let entries: &[(&str, &str)] = &[
            ("light.yaml", "header:\n    fg: black"),
            ("dark.yaml", "header:\n    fg: white"),
        ];
        let registry = StylesheetRegistry::from_embedded_entries(entries).unwrap();

        assert_eq!(registry.len(), 4);
        assert!(registry.contains("light"));
        assert!(registry.contains("dark"));
    }

    #[test]
    fn test_from_embedded_entries_nested_paths() {
        let entries: &[(&str, &str)] = &[
            ("themes/monokai.yaml", "keyword:\n    fg: magenta"),
            ("themes/solarized.yaml", "keyword:\n    fg: cyan"),
        ];
        let registry = StylesheetRegistry::from_embedded_entries(entries).unwrap();

        assert!(registry.contains("themes/monokai"));
        assert!(registry.contains("themes/monokai.yaml"));
        assert!(registry.contains("themes/solarized"));
    }

    #[test]
    fn test_from_embedded_entries_extension_priority() {
        let entries: &[(&str, &str)] = &[
            ("config.yml", "from_yml:\n    fg: red"),
            ("config.yaml", "from_yaml:\n    fg: cyan"),
        ];
        let mut registry = StylesheetRegistry::from_embedded_entries(entries).unwrap();

        let theme = registry.get("config").unwrap();
        let styles = theme.resolve_styles(None);
        assert!(styles.has("from_yaml"));
        assert!(!styles.has("from_yml"));

        let yml_theme = registry.get("config.yml").unwrap();
        assert!(yml_theme.resolve_styles(None).has("from_yml"));
    }

    #[test]
    fn test_from_embedded_entries_extension_priority_reverse_order() {
        let entries: &[(&str, &str)] = &[
            ("config.yaml", "from_yaml:\n    fg: cyan"),
            ("config.yml", "from_yml:\n    fg: red"),
        ];
        let mut registry = StylesheetRegistry::from_embedded_entries(entries).unwrap();

        let theme = registry.get("config").unwrap();
        let styles = theme.resolve_styles(None);
        assert!(styles.has("from_yaml"));
    }

    #[test]
    fn test_from_embedded_entries_names_iterator() {
        let entries: &[(&str, &str)] =
            &[("a.yaml", "header: bold"), ("nested/b.yaml", "header: dim")];
        let registry = StylesheetRegistry::from_embedded_entries(entries).unwrap();

        let names: Vec<&str> = registry.names().collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"a.yaml"));
        assert!(names.contains(&"nested/b"));
        assert!(names.contains(&"nested/b.yaml"));
    }

    #[test]
    fn test_from_embedded_entries_empty() {
        let entries: &[(&str, &str)] = &[];
        let registry = StylesheetRegistry::from_embedded_entries(entries).unwrap();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_from_embedded_entries_invalid_yaml() {
        let entries: &[(&str, &str)] = &[("bad.yaml", "not: [valid: yaml")];
        let result = StylesheetRegistry::from_embedded_entries(entries);

        assert!(result.is_err());
    }
}
