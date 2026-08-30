//! A named collection of styles, built programmatically or loaded from
//! YAML/CSS, that resolves to a [`Styles`] set for a given [`ColorMode`].
//! Styles have a base value plus optional light/dark overrides; aliases
//! point at another style name and are resolved at [`Theme::resolve_styles`]
//! time rather than eagerly, so themes can be [`Theme::merge`]d (e.g. a
//! framework base layered with user overrides) without breaking references.

use std::collections::HashMap;

use console::Style;

use crate::colorspace::ThemePalette;

use super::super::style::{
    parse_stylesheet, StyleValidationError, StyleValue, Styles, StylesheetError,
};

use super::adaptive::ColorMode;
use super::icon_def::{IconDefinition, IconSet};
use super::icon_mode::IconMode;

#[derive(Debug, Clone)]
pub struct Theme {
    name: Option<String>,
    base: HashMap<String, Style>,
    light: HashMap<String, Style>,
    dark: HashMap<String, Style>,
    aliases: HashMap<String, String>,
    icons: IconSet,
    palette: Option<ThemePalette>,
}

impl Theme {
    pub fn new() -> Self {
        Self {
            name: None,
            base: HashMap::new(),
            light: HashMap::new(),
            dark: HashMap::new(),
            aliases: HashMap::new(),
            icons: IconSet::new(),
            palette: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_palette(mut self, palette: ThemePalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn palette(&self) -> Option<&ThemePalette> {
        self.palette.as_ref()
    }

    /// Secondary path (ADR-0032): stylesheet text the application already holds; the YAML parse is the one that reads icons.
    pub fn from_yaml(yaml: &str) -> Result<Self, StylesheetError> {
        let icons = parse_icons_from_yaml_str(yaml)?;
        let variants = parse_stylesheet(yaml, None)?;
        Ok(Self {
            name: None,
            base: variants.base().clone(),
            light: variants.light().clone(),
            dark: variants.dark().clone(),
            aliases: variants.aliases().clone(),
            icons,
            palette: None,
        })
    }

    /// Secondary path (ADR-0032): stylesheet text the application already holds, in the CSS dialect.
    pub fn from_css(css: &str) -> Result<Self, StylesheetError> {
        let variants = crate::parse_css(css, None)?;
        Ok(Self {
            name: None,
            base: variants.base().clone(),
            light: variants.light().clone(),
            dark: variants.dark().clone(),
            aliases: variants.aliases().clone(),
            icons: IconSet::new(),
            palette: None,
        })
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn add<V: Into<StyleValue>>(mut self, name: &str, value: V) -> Self {
        match value.into() {
            StyleValue::Concrete(style) => {
                self.base.insert(name.to_string(), style);
            }
            StyleValue::Alias(target) => {
                self.aliases.insert(name.to_string(), target);
            }
        }
        self
    }

    /// Secondary path (ADR-0032): where an adaptive (light/dark) theme is expressed in Rust.
    pub fn add_adaptive(
        mut self,
        name: &str,
        base: Style,
        light: Option<Style>,
        dark: Option<Style>,
    ) -> Self {
        self.base.insert(name.to_string(), base);
        if let Some(light_style) = light {
            self.light.insert(name.to_string(), light_style);
        }
        if let Some(dark_style) = dark {
            self.dark.insert(name.to_string(), dark_style);
        }
        self
    }

    pub fn add_icon(mut self, name: &str, def: IconDefinition) -> Self {
        self.icons.insert(name.to_string(), def);
        self
    }

    pub fn resolve_icons(&self, mode: IconMode) -> HashMap<String, String> {
        self.icons.resolve(mode)
    }

    pub fn icons(&self) -> &IconSet {
        &self.icons
    }

    pub fn resolve_styles(&self, mode: Option<ColorMode>) -> Styles {
        let mut styles = Styles::new();

        let mode_overrides = match mode {
            Some(ColorMode::Light) => &self.light,
            Some(ColorMode::Dark) => &self.dark,
            None => &HashMap::new(),
        };

        for (name, base_style) in &self.base {
            let style = mode_overrides.get(name).unwrap_or(base_style);
            styles = styles.add(name, style.clone());
        }

        for (name, target) in &self.aliases {
            styles = styles.add(name, target.clone());
        }

        styles
    }

    pub fn validate(&self) -> Result<(), StyleValidationError> {
        self.resolve_styles(None).validate()
    }

    pub fn is_empty(&self) -> bool {
        self.base.is_empty() && self.aliases.is_empty()
    }

    pub fn len(&self) -> usize {
        self.base.len() + self.aliases.len()
    }

    pub fn get_style(&self, name: &str, mode: Option<ColorMode>) -> Option<Style> {
        let styles = self.resolve_styles(mode);
        styles.resolve(name).cloned()
    }

    pub fn light_override_count(&self) -> usize {
        self.light.len()
    }

    pub fn dark_override_count(&self) -> usize {
        self.dark.len()
    }

    pub fn merge(mut self, other: Theme) -> Self {
        for name in other
            .base
            .keys()
            .chain(other.light.keys())
            .chain(other.dark.keys())
            .chain(other.aliases.keys())
        {
            self.base.remove(name);
            self.light.remove(name);
            self.dark.remove(name);
            self.aliases.remove(name);
        }

        self.base.extend(other.base);
        self.light.extend(other.light);
        self.dark.extend(other.dark);
        self.aliases.extend(other.aliases);
        self.icons = self.icons.merge(other.icons);
        self.name = other.name.or(self.name);
        self.palette = other.palette.or(self.palette);
        self
    }
}

impl Default for Theme {
    fn default() -> Self {
        use console::{Color, Style};

        Self::new()
            .add(
                "standout_warning_banner",
                Style::new()
                    .fg(Color::Black)
                    .bg(Color::Color256(208))
                    .bold(),
            )
            .add("standout_warning_item", Style::new())
            .add("table_row_even", Style::new())
            .add_adaptive(
                "table_row_odd",
                Style::new(),
                Some(Style::new().bg(Color::Color256(254))),
                Some(Style::new().bg(Color::Color256(236))),
            )
            .add("table_row_even_gray", "table_row_even")
            .add("table_row_odd_gray", "table_row_odd")
            .add("table_row_even_blue", Style::new())
            .add_adaptive(
                "table_row_odd_blue",
                Style::new(),
                Some(Style::new().bg(Color::Color256(189))),
                Some(Style::new().bg(Color::Color256(17))),
            )
            .add("table_row_even_red", Style::new())
            .add_adaptive(
                "table_row_odd_red",
                Style::new(),
                Some(Style::new().bg(Color::Color256(224))),
                Some(Style::new().bg(Color::Color256(52))),
            )
            .add("table_row_even_green", Style::new())
            .add_adaptive(
                "table_row_odd_green",
                Style::new(),
                Some(Style::new().bg(Color::Color256(194))),
                Some(Style::new().bg(Color::Color256(22))),
            )
            .add("table_row_even_purple", Style::new())
            .add_adaptive(
                "table_row_odd_purple",
                Style::new(),
                Some(Style::new().bg(Color::Color256(225))),
                Some(Style::new().bg(Color::Color256(53))),
            )
    }
}

fn parse_icons_from_yaml_str(yaml: &str) -> Result<IconSet, StylesheetError> {
    let root: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| StylesheetError::Parse {
            path: None,
            message: e.to_string(),
        })?;

    parse_icons_from_yaml(&root)
}

fn parse_icons_from_yaml(root: &serde_yaml::Value) -> Result<IconSet, StylesheetError> {
    let mut icon_set = IconSet::new();

    let mapping = match root.as_mapping() {
        Some(m) => m,
        None => return Ok(icon_set),
    };

    let icons_value = match mapping.get(serde_yaml::Value::String("icons".into())) {
        Some(v) => v,
        None => return Ok(icon_set),
    };

    let icons_map = icons_value
        .as_mapping()
        .ok_or_else(|| StylesheetError::Parse {
            path: None,
            message: "'icons' must be a mapping".to_string(),
        })?;

    for (key, value) in icons_map {
        let name = key.as_str().ok_or_else(|| StylesheetError::Parse {
            path: None,
            message: format!("Icon name must be a string, got {:?}", key),
        })?;

        let def = match value {
            serde_yaml::Value::String(s) => IconDefinition::new(s.clone()),
            serde_yaml::Value::Mapping(map) => {
                let classic = map
                    .get(serde_yaml::Value::String("classic".into()))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| StylesheetError::InvalidDefinition {
                        style: name.to_string(),
                        message: "Icon mapping must have a 'classic' key".to_string(),
                        path: None,
                    })?;
                let nerdfont = map
                    .get(serde_yaml::Value::String("nerdfont".into()))
                    .and_then(|v| v.as_str());
                let mut def = IconDefinition::new(classic);
                if let Some(nf) = nerdfont {
                    def = def.with_nerdfont(nf);
                }
                def
            }
            _ => {
                return Err(StylesheetError::InvalidDefinition {
                    style: name.to_string(),
                    message: "Icon must be a string or mapping with 'classic' key".to_string(),
                    path: None,
                });
            }
        };

        icon_set.insert(name.to_string(), def);
    }

    Ok(icon_set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_new_is_empty() {
        let theme = Theme::new();
        assert!(theme.is_empty());
        assert_eq!(theme.len(), 0);
    }

    #[test]
    fn test_theme_add_concrete() {
        let theme = Theme::new().add("bold", Style::new().bold());
        assert!(!theme.is_empty());
        assert_eq!(theme.len(), 1);
    }

    #[test]
    fn test_theme_add_alias_str() {
        let theme = Theme::new()
            .add("base", Style::new().dim())
            .add("alias", "base");

        assert_eq!(theme.len(), 2);

        let styles = theme.resolve_styles(None);
        assert!(styles.has("base"));
        assert!(styles.has("alias"));
    }

    #[test]
    fn test_theme_add_alias_string() {
        let target = String::from("base");
        let theme = Theme::new()
            .add("base", Style::new().dim())
            .add("alias", target);

        let styles = theme.resolve_styles(None);
        assert!(styles.has("alias"));
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
        assert!(!theme.is_empty());
        let styles = theme.resolve_styles(Some(crate::ColorMode::Dark));
        assert!(styles.resolve("table_row_even").is_some());
        assert!(styles.resolve("table_row_odd").is_some());
    }

    #[test]
    fn test_theme_add_adaptive() {
        let theme = Theme::new().add_adaptive(
            "panel",
            Style::new().dim(),
            Some(Style::new().bold()),
            Some(Style::new().italic()),
        );

        assert_eq!(theme.len(), 1);
        assert_eq!(theme.light_override_count(), 1);
        assert_eq!(theme.dark_override_count(), 1);
    }

    #[test]
    fn test_theme_add_adaptive_light_only() {
        let theme =
            Theme::new().add_adaptive("panel", Style::new().dim(), Some(Style::new().bold()), None);

        assert_eq!(theme.light_override_count(), 1);
        assert_eq!(theme.dark_override_count(), 0);
    }

    #[test]
    fn test_theme_add_adaptive_dark_only() {
        let theme =
            Theme::new().add_adaptive("panel", Style::new().dim(), None, Some(Style::new().bold()));

        assert_eq!(theme.light_override_count(), 0);
        assert_eq!(theme.dark_override_count(), 1);
    }

    #[test]
    fn test_theme_resolve_styles_no_mode() {
        let theme = Theme::new()
            .add("header", Style::new().cyan())
            .add_adaptive(
                "panel",
                Style::new().dim(),
                Some(Style::new().bold()),
                Some(Style::new().italic()),
            );

        let styles = theme.resolve_styles(None);
        assert!(styles.has("header"));
        assert!(styles.has("panel"));
    }

    #[test]
    fn test_theme_resolve_styles_light_mode() {
        let theme = Theme::new().add_adaptive(
            "panel",
            Style::new().dim(),
            Some(Style::new().bold()),
            Some(Style::new().italic()),
        );

        let styles = theme.resolve_styles(Some(ColorMode::Light));
        assert!(styles.has("panel"));
    }

    #[test]
    fn test_theme_resolve_styles_dark_mode() {
        let theme = Theme::new().add_adaptive(
            "panel",
            Style::new().dim(),
            Some(Style::new().bold()),
            Some(Style::new().italic()),
        );

        let styles = theme.resolve_styles(Some(ColorMode::Dark));
        assert!(styles.has("panel"));
    }

    #[test]
    fn test_theme_resolve_styles_preserves_aliases() {
        let theme = Theme::new()
            .add("base", Style::new().dim())
            .add("alias", "base");

        let styles = theme.resolve_styles(Some(ColorMode::Light));
        assert!(styles.has("base"));
        assert!(styles.has("alias"));

        assert!(styles.validate().is_ok());
    }

    #[test]
    fn test_theme_from_yaml_simple() {
        let theme = Theme::from_yaml(
            r#"
            header:
                fg: cyan
                bold: true
            "#,
        )
        .unwrap();

        assert_eq!(theme.len(), 1);
        let styles = theme.resolve_styles(None);
        assert!(styles.has("header"));
    }

    #[test]
    fn test_theme_from_yaml_shorthand() {
        let theme = Theme::from_yaml(
            r#"
            bold_text: bold
            accent: cyan
            warning: "yellow italic"
            "#,
        )
        .unwrap();

        assert_eq!(theme.len(), 3);
    }

    #[test]
    fn test_theme_from_yaml_alias() {
        let theme = Theme::from_yaml(
            r#"
            muted:
                dim: true
            disabled: muted
            "#,
        )
        .unwrap();

        assert_eq!(theme.len(), 2);
        assert!(theme.validate().is_ok());
    }

    #[test]
    fn test_theme_from_yaml_adaptive() {
        let theme = Theme::from_yaml(
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

        assert_eq!(theme.len(), 1);
        assert_eq!(theme.light_override_count(), 1);
        assert_eq!(theme.dark_override_count(), 1);
    }

    #[test]
    fn test_theme_from_yaml_invalid() {
        let result = Theme::from_yaml("not valid yaml: [");
        assert!(result.is_err());
    }

    #[test]
    fn test_theme_from_yaml_complete() {
        let theme = Theme::from_yaml(
            r##"
            # Visual layer
            muted:
                dim: true

            accent:
                fg: cyan
                bold: true

            # Adaptive
            background:
                light:
                    bg: "#f8f8f8"
                dark:
                    bg: "#1e1e1e"

            # Aliases
            header: accent
            footer: muted
            "##,
        )
        .unwrap();

        assert_eq!(theme.len(), 5);
        assert!(theme.validate().is_ok());

        assert_eq!(theme.light_override_count(), 1);
        assert_eq!(theme.dark_override_count(), 1);
    }

    #[test]
    fn test_theme_new_has_no_name() {
        let theme = Theme::new();
        assert_eq!(theme.name(), None);
    }

    #[test]
    fn test_theme_merge() {
        let base = Theme::new()
            .add("keep", Style::new().dim())
            .add("overwrite", Style::new().red());

        let extension = Theme::new()
            .add("overwrite", Style::new().blue())
            .add("new", Style::new().bold());

        let merged = base.merge(extension);

        let styles = merged.resolve_styles(None);

        assert!(styles.has("keep"));

        assert!(styles.has("overwrite"));

        assert!(styles.has("new"));

        assert_eq!(merged.len(), 3);
    }

    fn rendered_style(theme: &Theme, name: &str, mode: Option<ColorMode>) -> String {
        theme
            .get_style(name, mode)
            .unwrap()
            .force_styling(true)
            .apply_to("x")
            .to_string()
    }

    fn rendered_color(color: impl FnOnce(Style) -> Style) -> String {
        color(Style::new())
            .force_styling(true)
            .apply_to("x")
            .to_string()
    }

    #[test]
    fn test_theme_merge_concrete_replaces_adaptive_definition() {
        let base = Theme::new().add_adaptive(
            "status",
            Style::new().red(),
            Some(Style::new().green()),
            Some(Style::new().blue()),
        );
        let merged = base.merge(Theme::new().add("status", Style::new().yellow()));
        let expected = rendered_color(Style::yellow);

        assert_eq!(rendered_style(&merged, "status", None), expected);
        assert_eq!(
            rendered_style(&merged, "status", Some(ColorMode::Light)),
            expected
        );
        assert_eq!(
            rendered_style(&merged, "status", Some(ColorMode::Dark)),
            expected
        );
    }

    #[test]
    fn test_theme_merge_concrete_replaces_alias_definition() {
        let base = Theme::new()
            .add("framework-status", Style::new().red())
            .add("status", "framework-status");
        let merged = base.merge(Theme::new().add("status", Style::new().yellow()));
        let expected = rendered_color(Style::yellow);

        assert_eq!(rendered_style(&merged, "status", None), expected);
        assert_eq!(
            rendered_style(&merged, "status", Some(ColorMode::Light)),
            expected
        );
        assert_eq!(
            rendered_style(&merged, "status", Some(ColorMode::Dark)),
            expected
        );
    }

    #[test]
    fn test_theme_merge_alias_replaces_concrete_definition() {
        let base = Theme::new().add("status", Style::new().red());
        let extension = Theme::new()
            .add("application-status", Style::new().yellow())
            .add("status", "application-status");
        let merged = base.merge(extension);
        let expected = rendered_color(Style::yellow);

        assert_eq!(rendered_style(&merged, "status", None), expected);
        assert_eq!(
            rendered_style(&merged, "status", Some(ColorMode::Light)),
            expected
        );
        assert_eq!(
            rendered_style(&merged, "status", Some(ColorMode::Dark)),
            expected
        );
    }

    #[test]
    fn test_theme_add_icon() {
        let theme = Theme::new()
            .add_icon("pending", IconDefinition::new("⚪"))
            .add_icon("done", IconDefinition::new("⚫").with_nerdfont("\u{f00c}"));

        assert_eq!(theme.icons().len(), 2);
        assert!(!theme.icons().is_empty());
    }

    #[test]
    fn test_theme_resolve_icons_classic() {
        let theme = Theme::new()
            .add_icon("pending", IconDefinition::new("⚪"))
            .add_icon("done", IconDefinition::new("⚫").with_nerdfont("\u{f00c}"));

        let resolved = theme.resolve_icons(IconMode::Classic);
        assert_eq!(resolved.get("pending").unwrap(), "⚪");
        assert_eq!(resolved.get("done").unwrap(), "⚫");
    }

    #[test]
    fn test_theme_resolve_icons_nerdfont() {
        let theme = Theme::new()
            .add_icon("pending", IconDefinition::new("⚪"))
            .add_icon("done", IconDefinition::new("⚫").with_nerdfont("\u{f00c}"));

        let resolved = theme.resolve_icons(IconMode::NerdFont);
        assert_eq!(resolved.get("pending").unwrap(), "⚪");
        assert_eq!(resolved.get("done").unwrap(), "\u{f00c}");
    }

    #[test]
    fn test_theme_icons_empty_by_default() {
        let theme = Theme::new();
        assert!(theme.icons().is_empty());
    }

    #[test]
    fn test_theme_merge_with_icons() {
        let base = Theme::new()
            .add_icon("keep", IconDefinition::new("K"))
            .add_icon("override", IconDefinition::new("OLD"));

        let ext = Theme::new()
            .add_icon("override", IconDefinition::new("NEW"))
            .add_icon("added", IconDefinition::new("A"));

        let merged = base.merge(ext);
        assert_eq!(merged.icons().len(), 3);

        let resolved = merged.resolve_icons(IconMode::Classic);
        assert_eq!(resolved.get("keep").unwrap(), "K");
        assert_eq!(resolved.get("override").unwrap(), "NEW");
        assert_eq!(resolved.get("added").unwrap(), "A");
    }

    #[test]
    fn test_theme_from_yaml_with_icons() {
        let theme = Theme::from_yaml(
            r#"
            header:
                fg: cyan
                bold: true
            icons:
                pending: "⚪"
                done:
                    classic: "⚫"
                    nerdfont: "nf_done"
            "#,
        )
        .unwrap();

        assert_eq!(theme.len(), 1);
        let styles = theme.resolve_styles(None);
        assert!(styles.has("header"));

        assert_eq!(theme.icons().len(), 2);
        let resolved = theme.resolve_icons(IconMode::Classic);
        assert_eq!(resolved.get("pending").unwrap(), "⚪");
        assert_eq!(resolved.get("done").unwrap(), "⚫");

        let resolved = theme.resolve_icons(IconMode::NerdFont);
        assert_eq!(resolved.get("done").unwrap(), "nf_done");
    }

    #[test]
    fn test_theme_from_yaml_no_icons() {
        let theme = Theme::from_yaml(
            r#"
            header:
                fg: cyan
            "#,
        )
        .unwrap();

        assert!(theme.icons().is_empty());
    }

    #[test]
    fn test_theme_from_yaml_icons_only() {
        let theme = Theme::from_yaml(
            r#"
            icons:
                check: "✓"
            "#,
        )
        .unwrap();

        assert_eq!(theme.icons().len(), 1);
        assert_eq!(theme.len(), 0);
    }

    #[test]
    fn test_theme_from_yaml_icons_invalid_type() {
        let result = Theme::from_yaml(
            r#"
            icons:
                bad: 42
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_theme_from_yaml_icons_mapping_without_classic() {
        let result = Theme::from_yaml(
            r#"
            icons:
                bad:
                    nerdfont: "nf"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_theme_no_palette_by_default() {
        let theme = Theme::new();
        assert!(theme.palette().is_none());
    }

    #[test]
    fn test_theme_with_palette() {
        use crate::colorspace::{Rgb, ThemePalette};

        let palette = ThemePalette::new([
            Rgb(40, 40, 40),
            Rgb(204, 36, 29),
            Rgb(152, 151, 26),
            Rgb(215, 153, 33),
            Rgb(69, 133, 136),
            Rgb(177, 98, 134),
            Rgb(104, 157, 106),
            Rgb(168, 153, 132),
        ]);

        let theme = Theme::new().with_palette(palette);
        assert!(theme.palette().is_some());
    }

    #[test]
    fn test_theme_merge_palette_from_other() {
        use crate::colorspace::ThemePalette;

        let base = Theme::new();
        let other = Theme::new().with_palette(ThemePalette::default_xterm());

        let merged = base.merge(other);
        assert!(merged.palette().is_some());
    }

    #[test]
    fn test_theme_merge_keeps_own_palette() {
        use crate::colorspace::ThemePalette;

        let base = Theme::new().with_palette(ThemePalette::default_xterm());
        let other = Theme::new();

        let merged = base.merge(other);
        assert!(merged.palette().is_some());
    }
}
