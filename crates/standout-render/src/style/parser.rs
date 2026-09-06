use std::collections::HashMap;

use console::Style;

use crate::colorspace::ThemePalette;

use super::super::theme::ColorMode;
use super::definition::StyleDefinition;
use super::error::StylesheetError;
use super::value::StyleValue;

#[derive(Debug, Clone)]
pub struct ThemeVariants {
    base: HashMap<String, Style>,
    light: HashMap<String, Style>,
    dark: HashMap<String, Style>,
    aliases: HashMap<String, String>,
}

impl ThemeVariants {
    pub fn new() -> Self {
        Self {
            base: HashMap::new(),
            light: HashMap::new(),
            dark: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn resolve(&self, mode: Option<ColorMode>) -> HashMap<String, StyleValue> {
        let mut result = HashMap::new();

        for (name, target) in &self.aliases {
            result.insert(name.clone(), StyleValue::Alias(target.clone()));
        }

        let mode_styles = match mode {
            Some(ColorMode::Light) => &self.light,
            Some(ColorMode::Dark) => &self.dark,
            None => &HashMap::new(),
        };

        for (name, style) in &self.base {
            let style = mode_styles.get(name).unwrap_or(style);
            result.insert(name.clone(), StyleValue::Concrete(style.clone()));
        }

        result
    }

    pub fn base(&self) -> &HashMap<String, Style> {
        &self.base
    }

    pub fn light(&self) -> &HashMap<String, Style> {
        &self.light
    }

    pub fn dark(&self) -> &HashMap<String, Style> {
        &self.dark
    }

    pub fn aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }

    pub fn is_empty(&self) -> bool {
        self.base.is_empty() && self.aliases.is_empty()
    }

    pub fn len(&self) -> usize {
        self.base.len() + self.aliases.len()
    }
}

impl Default for ThemeVariants {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_stylesheet(
    yaml: &str,
    palette: Option<&ThemePalette>,
) -> Result<ThemeVariants, StylesheetError> {
    let root: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| StylesheetError::Parse {
            path: None,
            message: e.to_string(),
        })?;

    let mapping = root.as_mapping().ok_or_else(|| StylesheetError::Parse {
        path: None,
        message: "Stylesheet must be a YAML mapping".to_string(),
    })?;

    let mut definitions: HashMap<String, StyleDefinition> = HashMap::new();

    for (key, value) in mapping {
        let name = key.as_str().ok_or_else(|| StylesheetError::Parse {
            path: None,
            message: format!("Style name must be a string, got {:?}", key),
        })?;

        // The 'icons' section is parsed separately by Theme.
        if name == "icons" {
            continue;
        }

        let def = StyleDefinition::parse(value, name)?;
        definitions.insert(name.to_string(), def);
    }

    build_variants(&definitions, palette)
}

pub(crate) fn build_variants(
    definitions: &HashMap<String, StyleDefinition>,
    palette: Option<&ThemePalette>,
) -> Result<ThemeVariants, StylesheetError> {
    let mut variants = ThemeVariants::new();

    for (name, def) in definitions {
        match def {
            StyleDefinition::Alias(target) => {
                variants.aliases.insert(name.clone(), target.clone());
            }
            StyleDefinition::Attributes { base, light, dark } => {
                let base_style = base.to_style(palette);
                variants.base.insert(name.clone(), base_style);

                if let Some(light_attrs) = light {
                    let merged = base.merge(light_attrs);
                    variants
                        .light
                        .insert(name.clone(), merged.to_style(palette));
                }

                if let Some(dark_attrs) = dark {
                    let merged = base.merge(dark_attrs);
                    variants.dark.insert(name.clone(), merged.to_style(palette));
                }
            }
        }
    }

    Ok(variants)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_stylesheet() {
        let yaml = "{}";
        let variants = parse_stylesheet(yaml, None).unwrap();
        assert!(variants.is_empty());
    }

    #[test]
    fn test_parse_simple_style() {
        let yaml = r#"
            header:
                fg: cyan
                bold: true
        "#;
        let variants = parse_stylesheet(yaml, None).unwrap();

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
        let variants = parse_stylesheet(yaml, None).unwrap();

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
        let variants = parse_stylesheet(yaml, None).unwrap();

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
        let variants = parse_stylesheet(yaml, None).unwrap();

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
        let variants = parse_stylesheet(yaml, None).unwrap();

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
        let variants = parse_stylesheet(yaml, None).unwrap();

        assert!(variants.base().contains_key("panel"));
        assert!(!variants.light().contains_key("panel"));
        assert!(variants.dark().contains_key("panel"));
    }

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
        let variants = parse_stylesheet(yaml, None).unwrap();
        let resolved = variants.resolve(None);

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
        let variants = parse_stylesheet(yaml, None).unwrap();
        let resolved = variants.resolve(Some(ColorMode::Light));

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
        let variants = parse_stylesheet(yaml, None).unwrap();
        let resolved = variants.resolve(Some(ColorMode::Dark));

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
        let variants = parse_stylesheet(yaml, None).unwrap();
        let resolved = variants.resolve(Some(ColorMode::Light));

        assert!(matches!(
            resolved.get("muted"),
            Some(StyleValue::Concrete(_))
        ));
        assert!(matches!(resolved.get("disabled"), Some(StyleValue::Alias(t)) if t == "muted"));
    }

    #[test]
    fn test_resolve_non_adaptive_uses_base() {
        let yaml = r#"
            header:
                fg: cyan
                bold: true
        "#;
        let variants = parse_stylesheet(yaml, None).unwrap();

        let light = variants.resolve(Some(ColorMode::Light));
        assert!(matches!(light.get("header"), Some(StyleValue::Concrete(_))));

        let dark = variants.resolve(Some(ColorMode::Dark));
        assert!(matches!(dark.get("header"), Some(StyleValue::Concrete(_))));

        let none = variants.resolve(None);
        assert!(matches!(none.get("header"), Some(StyleValue::Concrete(_))));
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = "not: [valid: yaml";
        let result = parse_stylesheet(yaml, None);
        assert!(matches!(result, Err(StylesheetError::Parse { .. })));
    }

    #[test]
    fn test_parse_non_mapping_root() {
        let yaml = "- item1\n- item2";
        let result = parse_stylesheet(yaml, None);
        assert!(matches!(result, Err(StylesheetError::Parse { .. })));
    }

    #[test]
    fn test_parse_invalid_color() {
        let yaml = r#"
            bad:
                fg: not_a_color
        "#;
        let result = parse_stylesheet(yaml, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unknown_attribute() {
        let yaml = r#"
            bad:
                unknown: true
        "#;
        let result = parse_stylesheet(yaml, None);
        assert!(matches!(
            result,
            Err(StylesheetError::UnknownAttribute { .. })
        ));
    }

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

        let variants = parse_stylesheet(yaml, None).unwrap();

        assert_eq!(variants.base().len(), 8);
        assert_eq!(variants.aliases().len(), 4);

        assert!(variants.light().contains_key("background"));
        assert!(variants.light().contains_key("text"));
        assert!(variants.light().contains_key("border"));
        assert!(variants.dark().contains_key("background"));
        assert!(variants.dark().contains_key("text"));
        assert!(variants.dark().contains_key("border"));

        assert_eq!(
            variants.aliases().get("header"),
            Some(&"accent".to_string())
        );
        assert_eq!(variants.aliases().get("footer"), Some(&"muted".to_string()));
    }

    #[test]
    fn test_parse_cube_color_in_stylesheet() {
        let yaml = r#"
            theme_accent:
                fg: "cube(60%, 20%, 0%)"
                bold: true
        "#;
        let variants = parse_stylesheet(yaml, None).unwrap();
        assert!(variants.base().contains_key("theme_accent"));
    }

    #[test]
    fn test_parse_cube_color_with_palette() {
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

        let yaml = r#"
            warm:
                fg: "cube(80%, 30%, 0%)"
            cool:
                fg: "cube(0%, 0%, 80%)"
        "#;
        let variants = parse_stylesheet(yaml, Some(&palette)).unwrap();
        assert!(variants.base().contains_key("warm"));
        assert!(variants.base().contains_key("cool"));
    }

    #[test]
    fn test_parse_cube_color_adaptive() {
        let yaml = r#"
            panel:
                fg: "cube(50%, 50%, 50%)"
                light:
                    fg: "cube(20%, 20%, 20%)"
                dark:
                    fg: "cube(80%, 80%, 80%)"
        "#;
        let variants = parse_stylesheet(yaml, None).unwrap();
        assert!(variants.base().contains_key("panel"));
        assert!(variants.light().contains_key("panel"));
        assert!(variants.dark().contains_key("panel"));
    }
}
