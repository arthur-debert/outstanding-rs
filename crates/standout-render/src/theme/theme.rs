//! Theme struct for building style collections.
//!
//! Themes are named collections of styles that can adapt to the user's
//! display mode (light/dark). They support programmatic construction and
//! file loading from CSS (preferred) or YAML (legacy) stylesheets.
//!
//! # Adaptive Styles
//!
//! Individual styles can define mode-specific variations. When resolving
//! styles for rendering, the theme selects the appropriate variant based
//! on the current color mode:
//!
//! - Base styles: Used when no mode override exists
//! - Light overrides: Applied in light mode
//! - Dark overrides: Applied in dark mode
//!
//! # Construction Methods
//!
//! ## Programmatic (Builder API)
//!
//! ```rust
//! use standout_render::Theme;
//! use console::Style;
//!
//! let theme = Theme::new()
//!     // Non-adaptive styles work in all modes
//!     .add("muted", Style::new().dim())
//!     .add("accent", Style::new().cyan().bold())
//!     // Aliases reference other styles
//!     .add("disabled", "muted");
//! ```
//!
//! ## From YAML
//!
//! ```rust
//! use standout_render::Theme;
//!
//! let theme = Theme::from_yaml(r#"
//! header:
//!   fg: cyan
//!   bold: true
//!
//! footer:
//!   fg: gray
//!   light:
//!     fg: black
//!   dark:
//!     fg: white
//!
//! muted:
//!   dim: true
//!
//! disabled: muted
//! "#).unwrap();
//! ```
//!
//! # Mode Resolution
//!
//! Use [`resolve_styles`](Theme::resolve_styles) to get a `Styles` collection
//! for a specific color mode. This is typically called during rendering.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use console::Style;

use crate::colorspace::ThemePalette;

use super::super::style::{
    parse_stylesheet, StyleValidationError, StyleValue, Styles, StylesheetError, ThemeVariants,
};

use super::adaptive::ColorMode;
use super::icon_def::{IconDefinition, IconSet};
use super::icon_mode::IconMode;

/// A named collection of styles used when rendering templates.
///
/// Themes can be constructed programmatically or loaded from YAML files.
/// They support adaptive styles that vary based on the user's color mode.
///
/// # Example: Programmatic Construction
///
/// ```rust
/// use standout_render::Theme;
/// use console::Style;
///
/// let theme = Theme::new()
///     // Visual layer - concrete styles
///     .add("muted", Style::new().dim())
///     .add("accent", Style::new().cyan().bold())
///     // Presentation layer - aliases
///     .add("disabled", "muted")
///     .add("highlighted", "accent")
///     // Semantic layer - aliases to presentation
///     .add("timestamp", "disabled");
/// ```
///
/// # Example: From YAML
///
/// ```rust
/// use standout_render::Theme;
///
/// let theme = Theme::from_yaml(r#"
/// panel:
///   bg: gray
///   light:
///     bg: white
///   dark:
///     bg: black
/// header:
///   fg: cyan
///   bold: true
/// "#).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme name (optional, typically derived from filename).
    name: Option<String>,
    /// Source file path (for refresh support).
    source_path: Option<PathBuf>,
    /// Base styles (always populated).
    base: HashMap<String, Style>,
    /// Light mode style overrides.
    light: HashMap<String, Style>,
    /// Dark mode style overrides.
    dark: HashMap<String, Style>,
    /// Alias definitions (name → target).
    aliases: HashMap<String, String>,
    /// Icon definitions (classic + optional nerdfont variants).
    icons: IconSet,
    /// Theme palette for resolving [`ColorDef::Cube`] colors.
    palette: Option<ThemePalette>,
}

impl Theme {
    /// Creates an empty, unnamed theme.
    pub fn new() -> Self {
        Self {
            name: None,
            source_path: None,
            base: HashMap::new(),
            light: HashMap::new(),
            dark: HashMap::new(),
            aliases: HashMap::new(),
            icons: IconSet::new(),
            palette: None,
        }
    }

    /// Creates an empty theme with the given name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            source_path: None,
            base: HashMap::new(),
            light: HashMap::new(),
            dark: HashMap::new(),
            aliases: HashMap::new(),
            icons: IconSet::new(),
            palette: None,
        }
    }

    /// Sets the name on this theme, returning `self` for chaining.
    ///
    /// This is useful when loading themes from content where the name
    /// is known separately (e.g., from a filename).
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the theme palette used to resolve [`ColorDef::Cube`] colors.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::Theme;
    /// use standout_render::colorspace::{ThemePalette, Rgb};
    ///
    /// let palette = ThemePalette::new([
    ///     Rgb(40, 40, 40), Rgb(204, 36, 29), Rgb(152, 151, 26), Rgb(215, 153, 33),
    ///     Rgb(69, 133, 136), Rgb(177, 98, 134), Rgb(104, 157, 106), Rgb(168, 153, 132),
    /// ]);
    ///
    /// let theme = Theme::new().with_palette(palette);
    /// ```
    pub fn with_palette(mut self, palette: ThemePalette) -> Self {
        self.palette = Some(palette);
        self
    }

    /// Returns a reference to the theme palette, if set.
    pub fn palette(&self) -> Option<&ThemePalette> {
        self.palette.as_ref()
    }

    /// Loads a theme from a YAML file.
    ///
    /// The theme name is derived from the filename (without extension).
    /// The source path is stored for [`refresh`](Theme::refresh) support.
    ///
    /// # Errors
    ///
    /// Returns a [`StylesheetError`] if the file cannot be read or parsed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout_render::Theme;
    ///
    /// let theme = Theme::from_file("./themes/darcula.yaml")?;
    /// assert_eq!(theme.name(), Some("darcula"));
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, StylesheetError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| StylesheetError::Load {
            message: format!("Failed to read {}: {}", path.display(), e),
        })?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        let icons = parse_icons_from_yaml_str(&content)?;
        let variants = parse_stylesheet(&content, None)?;
        Ok(Self {
            name,
            source_path: Some(path.to_path_buf()),
            base: variants.base().clone(),
            light: variants.light().clone(),
            dark: variants.dark().clone(),
            aliases: variants.aliases().clone(),
            icons,
            palette: None,
        })
    }

    /// Creates a theme from YAML content.
    ///
    /// The YAML format supports:
    /// - Simple styles: `header: { fg: cyan, bold: true }`
    /// - Shorthand: `bold_text: bold` or `warning: "yellow italic"`
    /// - Aliases: `disabled: muted`
    /// - Adaptive styles with `light:` and `dark:` sections
    ///
    /// # Errors
    ///
    /// Returns a [`StylesheetError`] if parsing fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::Theme;
    ///
    /// let theme = Theme::from_yaml(r#"
    /// header:
    ///   fg: cyan
    ///   bold: true
    ///
    /// footer:
    ///   dim: true
    ///   light:
    ///     fg: black
    ///   dark:
    ///     fg: white
    /// "#).unwrap();
    /// ```
    pub fn from_yaml(yaml: &str) -> Result<Self, StylesheetError> {
        let icons = parse_icons_from_yaml_str(yaml)?;
        let variants = parse_stylesheet(yaml, None)?;
        Ok(Self {
            name: None,
            source_path: None,
            base: variants.base().clone(),
            light: variants.light().clone(),
            dark: variants.dark().clone(),
            aliases: variants.aliases().clone(),
            icons,
            palette: None,
        })
    }

    /// Creates a theme from CSS content.
    ///
    /// The CSS format supports a subset of CSS Level 3 tailored for terminals.
    /// Class selectors map to style names (`.title { ... }` defines the `title` style).
    /// Adaptive styles use `@media (prefers-color-scheme: light|dark)` queries.
    ///
    /// # Errors
    ///
    /// Returns a [`StylesheetError`] if parsing fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::Theme;
    ///
    /// let theme = Theme::from_css(r#"
    /// .header { color: cyan; font-weight: bold; }
    /// .muted { opacity: 0.5; }
    /// "#).unwrap();
    /// ```
    pub fn from_css(css: &str) -> Result<Self, StylesheetError> {
        let variants = crate::parse_css(css, None)?;
        Ok(Self {
            name: None,
            source_path: None,
            base: variants.base().clone(),
            light: variants.light().clone(),
            dark: variants.dark().clone(),
            aliases: variants.aliases().clone(),
            icons: IconSet::new(),
            palette: None,
        })
    }

    /// Loads a theme from a CSS file.
    ///
    /// The theme name is derived from the filename (without extension).
    /// The source path is stored for [`refresh`](Theme::refresh) support.
    ///
    /// # Errors
    ///
    /// Returns a [`StylesheetError`] if the file cannot be read or parsed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout_render::Theme;
    ///
    /// let theme = Theme::from_css_file("./themes/default.css")?;
    /// assert_eq!(theme.name(), Some("default"));
    /// ```
    pub fn from_css_file<P: AsRef<Path>>(path: P) -> Result<Self, StylesheetError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| StylesheetError::Load {
            message: format!("Failed to read {}: {}", path.display(), e),
        })?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        let variants = crate::parse_css(&content, None)?;
        Ok(Self {
            name,
            source_path: Some(path.to_path_buf()),
            base: variants.base().clone(),
            light: variants.light().clone(),
            dark: variants.dark().clone(),
            aliases: variants.aliases().clone(),
            icons: IconSet::new(),
            palette: None,
        })
    }

    /// Creates a theme from pre-parsed theme variants.
    pub fn from_variants(variants: ThemeVariants) -> Self {
        Self {
            name: None,
            source_path: None,
            base: variants.base().clone(),
            light: variants.light().clone(),
            dark: variants.dark().clone(),
            aliases: variants.aliases().clone(),
            icons: IconSet::new(),
            palette: None,
        }
    }

    /// Returns the theme name, if set.
    ///
    /// The name is typically derived from the source filename when using
    /// [`from_file`](Theme::from_file), or set explicitly with [`named`](Theme::named).
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the source file path, if this theme was loaded from a file.
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    /// Reloads the theme from its source file.
    ///
    /// This is useful for hot-reloading during development. If the theme
    /// was not loaded from a file, this method returns an error.
    ///
    /// # Errors
    ///
    /// Returns a [`StylesheetError`] if:
    /// - The theme has no source file (wasn't loaded with [`from_file`](Theme::from_file))
    /// - The file cannot be read or parsed
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut theme = Theme::from_file("./themes/darcula.yaml")?;
    ///
    /// // After editing the file...
    /// theme.refresh()?;
    /// ```
    pub fn refresh(&mut self) -> Result<(), StylesheetError> {
        let path = self
            .source_path
            .as_ref()
            .ok_or_else(|| StylesheetError::Load {
                message: "Cannot refresh: theme has no source file".to_string(),
            })?;

        let content = std::fs::read_to_string(path).map_err(|e| StylesheetError::Load {
            message: format!("Failed to read {}: {}", path.display(), e),
        })?;

        let icons = parse_icons_from_yaml_str(&content)?;
        let variants = parse_stylesheet(&content, self.palette.as_ref())?;
        self.base = variants.base().clone();
        self.light = variants.light().clone();
        self.dark = variants.dark().clone();
        self.aliases = variants.aliases().clone();
        self.icons = icons;

        Ok(())
    }

    /// Adds a named style, returning an updated theme for chaining.
    ///
    /// The value can be either a concrete `Style` or a `&str`/`String` alias
    /// to another style name, enabling layered styling.
    ///
    /// # Non-Adaptive
    ///
    /// Styles added via this method are non-adaptive (same in all modes).
    /// For adaptive styles, use [`add_adaptive`](Self::add_adaptive) or YAML.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::Theme;
    /// use console::Style;
    ///
    /// let theme = Theme::new()
    ///     // Visual layer - concrete styles
    ///     .add("muted", Style::new().dim())
    ///     .add("accent", Style::new().cyan().bold())
    ///     // Presentation layer - aliases
    ///     .add("disabled", "muted")
    ///     .add("highlighted", "accent")
    ///     // Semantic layer - aliases to presentation
    ///     .add("timestamp", "disabled");
    /// ```
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

    /// Adds an adaptive style with separate light and dark variants.
    ///
    /// The base style is used when no mode override exists or when mode
    /// detection fails. Light and dark variants, if provided, override
    /// the base in their respective modes.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::Theme;
    /// use console::Style;
    ///
    /// let theme = Theme::new()
    ///     .add_adaptive(
    ///         "panel",
    ///         Style::new().dim(),                    // Base
    ///         Some(Style::new().fg(console::Color::Black)),  // Light mode
    ///         Some(Style::new().fg(console::Color::White)),  // Dark mode
    ///     );
    /// ```
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

    /// Adds an icon definition to the theme, returning `self` for chaining.
    ///
    /// Icons are characters (not images) that adapt between classic Unicode
    /// and Nerd Font glyphs. Each icon has a classic variant and an optional
    /// Nerd Font variant.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::{Theme, IconDefinition};
    ///
    /// let theme = Theme::new()
    ///     .add_icon("pending", IconDefinition::new("⚪"))
    ///     .add_icon("done", IconDefinition::new("⚫").with_nerdfont("\u{f00c}"));
    /// ```
    pub fn add_icon(mut self, name: &str, def: IconDefinition) -> Self {
        self.icons.insert(name.to_string(), def);
        self
    }

    /// Resolves icons for the given icon mode.
    ///
    /// Returns a map of icon names to resolved strings for the given mode.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::{Theme, IconDefinition, IconMode};
    ///
    /// let theme = Theme::new()
    ///     .add_icon("done", IconDefinition::new("⚫").with_nerdfont("\u{f00c}"));
    ///
    /// let classic = theme.resolve_icons(IconMode::Classic);
    /// assert_eq!(classic.get("done").unwrap(), "⚫");
    ///
    /// let nerdfont = theme.resolve_icons(IconMode::NerdFont);
    /// assert_eq!(nerdfont.get("done").unwrap(), "\u{f00c}");
    /// ```
    pub fn resolve_icons(&self, mode: IconMode) -> HashMap<String, String> {
        self.icons.resolve(mode)
    }

    /// Returns a reference to the icon set.
    pub fn icons(&self) -> &IconSet {
        &self.icons
    }

    /// Resolves styles for the given color mode.
    ///
    /// Returns a [`Styles`] collection with the appropriate style for each
    /// defined style name:
    ///
    /// - For styles with a mode-specific override, uses the override
    /// - For styles without an override, uses the base style
    /// - Aliases are preserved for resolution during rendering
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::{Theme, ColorMode};
    /// use console::Style;
    ///
    /// let theme = Theme::new()
    ///     .add("header", Style::new().cyan())
    ///     .add_adaptive(
    ///         "panel",
    ///         Style::new(),
    ///         Some(Style::new().fg(console::Color::Black)),
    ///         Some(Style::new().fg(console::Color::White)),
    ///     );
    ///
    /// // Get styles for dark mode
    /// let dark_styles = theme.resolve_styles(Some(ColorMode::Dark));
    /// ```
    pub fn resolve_styles(&self, mode: Option<ColorMode>) -> Styles {
        let mut styles = Styles::new();

        // Select the mode-specific overrides map
        let mode_overrides = match mode {
            Some(ColorMode::Light) => &self.light,
            Some(ColorMode::Dark) => &self.dark,
            None => &HashMap::new(),
        };

        // Add concrete styles (base, with mode overrides applied)
        for (name, base_style) in &self.base {
            let style = mode_overrides.get(name).unwrap_or(base_style);
            styles = styles.add(name, style.clone());
        }

        // Add aliases
        for (name, target) in &self.aliases {
            styles = styles.add(name, target.clone());
        }

        styles
    }

    /// Validates that all style aliases in this theme resolve correctly.
    ///
    /// This is called automatically at render time, but can be called
    /// explicitly for early error detection.
    pub fn validate(&self) -> Result<(), StyleValidationError> {
        // Validate using a resolved Styles instance
        self.resolve_styles(None).validate()
    }

    /// Returns true if no styles are defined.
    pub fn is_empty(&self) -> bool {
        self.base.is_empty() && self.aliases.is_empty()
    }

    /// Returns the number of defined styles (base + aliases).
    pub fn len(&self) -> usize {
        self.base.len() + self.aliases.len()
    }

    /// Resolves a single style for the given mode.
    ///
    /// This is a convenience wrapper around [`resolve_styles`](Self::resolve_styles).
    pub fn get_style(&self, name: &str, mode: Option<ColorMode>) -> Option<Style> {
        let styles = self.resolve_styles(mode);
        // Styles::resolve is crate-private, so we have to use to_resolved_map or check internal.
        // Wait, Styles::resolve is pub(crate). We are in rendering/theme/theme.rs,
        // Styles is in rendering/style/registry.rs. Same crate.
        // But Theme is in `rendering::theme`, Styles in `rendering::style`.
        // They are different modules. `pub(crate)` is visible.
        styles.resolve(name).cloned()
    }

    /// Returns the number of light mode overrides.
    pub fn light_override_count(&self) -> usize {
        self.light.len()
    }

    /// Returns the number of dark mode overrides.
    pub fn dark_override_count(&self) -> usize {
        self.dark.len()
    }

    /// Merges another theme into this one.
    ///
    /// Styles, icons, palette, and the optional name from `other` take
    /// precedence over values in `self`. This allows layering themes, e.g.,
    /// loading a framework base and applying user overrides.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_render::Theme;
    /// use console::Style;
    ///
    /// let base = Theme::new().add("text", Style::new().dim());
    /// let user = Theme::new().add("text", Style::new().bold());
    ///
    /// let merged = base.merge(user);
    /// // "text" is now bold (from user)
    /// ```
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

        // ANSI 256 color reference for tints:
        //
        // Dark mode (very dark tinted bg):        Light mode (pastel tinted bg):
        //   gray:   236 (#303030)                   gray:   254 (#e4e4e4)
        //   blue:    17 (#00005f)                   blue:   189 (#d7d7ff)
        //   red:     52 (#5f0000)                   red:    224 (#ffd7d7)
        //   green:   22 (#005f00)                   green:  194 (#d7ffd7)
        //   purple:  53 (#5f005f)                   purple: 225 (#ffd7ff)

        Self::new()
            // ── Framework warnings ──────────────────────────────────────
            // Banner shown before the per-warning list when the framework
            // emits deferred warnings (e.g. stylesheet hot-reload failures).
            // Black text on orange (ANSI 256 #208) with bold for prominence.
            // Items are rendered without extra styling; the leading tab is
            // applied by the CLI flush path, not the style.
            .add(
                "standout_warning_banner",
                Style::new()
                    .fg(Color::Black)
                    .bg(Color::Color256(208))
                    .bold(),
            )
            .add("standout_warning_item", Style::new())
            // ── Base (gray) ─────────────────────────────────────────────
            .add("table_row_even", Style::new())
            .add_adaptive(
                "table_row_odd",
                Style::new(),
                Some(Style::new().bg(Color::Color256(254))),
                Some(Style::new().bg(Color::Color256(236))),
            )
            // gray is an alias for the base variant
            .add("table_row_even_gray", "table_row_even")
            .add("table_row_odd_gray", "table_row_odd")
            // ── Blue ────────────────────────────────────────────────────
            .add("table_row_even_blue", Style::new())
            .add_adaptive(
                "table_row_odd_blue",
                Style::new(),
                Some(Style::new().bg(Color::Color256(189))),
                Some(Style::new().bg(Color::Color256(17))),
            )
            // ── Red ─────────────────────────────────────────────────────
            .add("table_row_even_red", Style::new())
            .add_adaptive(
                "table_row_odd_red",
                Style::new(),
                Some(Style::new().bg(Color::Color256(224))),
                Some(Style::new().bg(Color::Color256(52))),
            )
            // ── Green ───────────────────────────────────────────────────
            .add("table_row_even_green", Style::new())
            .add_adaptive(
                "table_row_odd_green",
                Style::new(),
                Some(Style::new().bg(Color::Color256(194))),
                Some(Style::new().bg(Color::Color256(22))),
            )
            // ── Purple ──────────────────────────────────────────────────
            .add("table_row_even_purple", Style::new())
            .add_adaptive(
                "table_row_odd_purple",
                Style::new(),
                Some(Style::new().bg(Color::Color256(225))),
                Some(Style::new().bg(Color::Color256(53))),
            )
    }
}

/// Parses icon definitions from a YAML string.
///
/// Extracts the `icons:` section from the YAML root mapping and
/// parses each entry into an [`IconDefinition`].
///
/// # YAML Format
///
/// ```yaml
/// icons:
///   # String shorthand (classic only)
///   pending: "⚪"
///
///   # Mapping with both variants
///   done:
///     classic: "⚫"
///     nerdfont: "\uf00c"
/// ```
fn parse_icons_from_yaml_str(yaml: &str) -> Result<IconSet, StylesheetError> {
    let root: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| StylesheetError::Parse {
            path: None,
            message: e.to_string(),
        })?;

    parse_icons_from_yaml(&root)
}

/// Parses icon definitions from a parsed YAML value.
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
            serde_yaml::Value::String(s) => {
                // Shorthand: classic-only
                IconDefinition::new(s.clone())
            }
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
        // Default theme includes built-in table row styles
        assert!(!theme.is_empty());
        let styles = theme.resolve_styles(Some(crate::ColorMode::Dark));
        assert!(styles.resolve("table_row_even").is_some());
        assert!(styles.resolve("table_row_odd").is_some());
    }

    // =========================================================================
    // Adaptive style tests
    // =========================================================================

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
        // The style should be the light override, not base
        // We can't easily check the actual style, but we verify resolution works
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

        // Validate that alias resolution still works
        assert!(styles.validate().is_ok());
    }

    // =========================================================================
    // YAML parsing tests
    // =========================================================================

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

        // 3 concrete styles + 2 aliases = 5 total
        assert_eq!(theme.len(), 5);
        assert!(theme.validate().is_ok());

        // background is adaptive
        assert_eq!(theme.light_override_count(), 1);
        assert_eq!(theme.dark_override_count(), 1);
    }

    // =========================================================================
    // Name and source path tests
    // =========================================================================

    #[test]
    fn test_theme_named() {
        let theme = Theme::named("darcula");
        assert_eq!(theme.name(), Some("darcula"));
        assert!(theme.is_empty());
    }

    #[test]
    fn test_theme_new_has_no_name() {
        let theme = Theme::new();
        assert_eq!(theme.name(), None);
        assert_eq!(theme.source_path(), None);
    }

    #[test]
    fn test_theme_from_file() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let theme_path = temp_dir.path().join("darcula.yaml");
        fs::write(
            &theme_path,
            r#"
            header:
                fg: cyan
                bold: true
            muted:
                dim: true
            "#,
        )
        .unwrap();

        let theme = Theme::from_file(&theme_path).unwrap();
        assert_eq!(theme.name(), Some("darcula"));
        assert_eq!(theme.source_path(), Some(theme_path.as_path()));
        assert_eq!(theme.len(), 2);
    }

    #[test]
    fn test_theme_from_file_not_found() {
        let result = Theme::from_file("/nonexistent/path/theme.yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_theme_refresh() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let theme_path = temp_dir.path().join("dynamic.yaml");
        fs::write(
            &theme_path,
            r#"
            header:
                fg: red
            "#,
        )
        .unwrap();

        let mut theme = Theme::from_file(&theme_path).unwrap();
        assert_eq!(theme.len(), 1);

        // Update the file
        fs::write(
            &theme_path,
            r#"
            header:
                fg: blue
            footer:
                dim: true
            "#,
        )
        .unwrap();

        // Refresh
        theme.refresh().unwrap();
        assert_eq!(theme.len(), 2);
    }

    #[test]
    fn test_theme_refresh_without_source() {
        let mut theme = Theme::new();
        let result = theme.refresh();
        assert!(result.is_err());
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

        // "keep" should be from base
        assert!(styles.has("keep"));

        // "overwrite" should be from extension (blue, not red)
        assert!(styles.has("overwrite"));

        // "new" should be from extension
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

    // =========================================================================
    // Icon tests
    // =========================================================================

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
        assert_eq!(resolved.get("pending").unwrap(), "⚪"); // No nerdfont, falls back
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

        // Styles
        assert_eq!(theme.len(), 1);
        let styles = theme.resolve_styles(None);
        assert!(styles.has("header"));

        // Icons
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
        assert_eq!(theme.len(), 0); // No styles
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
    fn test_theme_from_file_with_icons() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let theme_path = temp_dir.path().join("iconic.yaml");
        fs::write(
            &theme_path,
            r#"
            header:
                fg: cyan
            icons:
                check:
                    classic: "[ok]"
                    nerdfont: "nf_check"
            "#,
        )
        .unwrap();

        let theme = Theme::from_file(&theme_path).unwrap();
        assert_eq!(theme.icons().len(), 1);
        let resolved = theme.resolve_icons(IconMode::NerdFont);
        assert_eq!(resolved.get("check").unwrap(), "nf_check");
    }

    #[test]
    fn test_theme_refresh_with_icons() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let theme_path = temp_dir.path().join("refresh.yaml");
        fs::write(
            &theme_path,
            r#"
            icons:
                v1: "one"
            "#,
        )
        .unwrap();

        let mut theme = Theme::from_file(&theme_path).unwrap();
        assert_eq!(theme.icons().len(), 1);

        fs::write(
            &theme_path,
            r#"
            icons:
                v1: "one"
                v2: "two"
            "#,
        )
        .unwrap();

        theme.refresh().unwrap();
        assert_eq!(theme.icons().len(), 2);
    }

    // =========================================================================
    // Palette tests
    // =========================================================================

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
