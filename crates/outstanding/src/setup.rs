//! Unified setup builder for Outstanding applications.
//!
//! This module provides [`RenderSetup`], a builder that configures templates,
//! styles, and themes in a single fluent API. It handles both embedded resources
//! (via `embed_*!` macros) and runtime file loading, with automatic hot-reload
//! in debug mode.
//!
//! # Example
//!
//! ```rust,ignore
//! use outstanding::{embed_templates, embed_styles, RenderSetup, OutputMode};
//!
//! let app = RenderSetup::new()
//!     .templates(embed_templates!("src/templates"))
//!     .styles(embed_styles!("src/styles"))
//!     .default_theme("default")
//!     .build()?;
//!
//! // Render a template
//! let output = app.render("list", &data, OutputMode::Term)?;
//! ```
//!
//! # Design
//!
//! The builder separates configuration from the final application:
//!
//! - **RenderSetup**: Builder that collects configuration
//! - **OutstandingApp**: Immutable, ready-to-use renderer
//!
//! This design enables:
//!
//! - `render(&self, ...)` instead of `render(&mut self, ...)`
//! - Static usage via `Lazy<OutstandingApp>`
//! - All templates pre-loaded for `{% include %}` support

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use minijinja::{Environment, Error as JinjaError};
use serde::Serialize;

use crate::embedded::{EmbeddedStyles, EmbeddedTemplates};
use crate::output::OutputMode;
use crate::render::filters::register_filters;
use crate::render::TemplateRegistry;
use crate::stylesheet::StylesheetRegistry;
use crate::theme::Theme;

/// Error type for setup operations.
#[derive(Debug)]
pub enum SetupError {
    /// Template loading or rendering error.
    Template(String),
    /// Stylesheet loading or parsing error.
    Stylesheet(String),
    /// Theme not found.
    ThemeNotFound(String),
    /// Configuration error.
    Config(String),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupError::Template(msg) => write!(f, "template error: {}", msg),
            SetupError::Stylesheet(msg) => write!(f, "stylesheet error: {}", msg),
            SetupError::ThemeNotFound(name) => write!(f, "theme not found: {}", name),
            SetupError::Config(msg) => write!(f, "configuration error: {}", msg),
        }
    }
}

impl std::error::Error for SetupError {}

impl From<JinjaError> for SetupError {
    fn from(e: JinjaError) -> Self {
        SetupError::Template(e.to_string())
    }
}

/// Builder for configuring Outstanding applications.
///
/// Use this builder to set up templates, styles, and themes before creating
/// an [`OutstandingApp`] instance.
///
/// # Example
///
/// ```rust,ignore
/// use outstanding::{embed_templates, embed_styles, RenderSetup};
///
/// let app = RenderSetup::new()
///     .templates(embed_templates!("src/templates"))
///     .styles(embed_styles!("src/styles"))
///     .default_theme("default")
///     .build()?;
/// ```
#[derive(Default)]
pub struct RenderSetup {
    /// Embedded template sources.
    embedded_templates: Option<EmbeddedTemplates>,

    /// Additional template directories (runtime).
    template_dirs: Vec<PathBuf>,

    /// Embedded stylesheet sources.
    embedded_styles: Option<EmbeddedStyles>,

    /// Additional stylesheet directories (runtime).
    style_dirs: Vec<PathBuf>,

    /// Name of the default theme to use.
    default_theme_name: Option<String>,
}

impl RenderSetup {
    /// Creates a new setup builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets embedded templates from `embed_templates!` macro.
    ///
    /// In debug mode, if the source path exists, templates are loaded from disk
    /// for hot-reload. In release mode, embedded content is used.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// RenderSetup::new()
    ///     .templates(embed_templates!("src/templates"))
    /// ```
    pub fn templates(mut self, source: EmbeddedTemplates) -> Self {
        self.embedded_templates = Some(source);
        self
    }

    /// Adds a template directory for runtime loading.
    ///
    /// Templates from directories are loaded at build time and merged with
    /// any embedded templates. Directory templates take precedence over
    /// embedded templates with the same name.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// RenderSetup::new()
    ///     .templates(embed_templates!("src/templates"))
    ///     .templates_dir("~/.myapp/templates")  // User overrides
    /// ```
    pub fn templates_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.template_dirs.push(path.as_ref().to_path_buf());
        self
    }

    /// Sets embedded styles from `embed_styles!` macro.
    ///
    /// In debug mode, if the source path exists, styles are loaded from disk
    /// for hot-reload. In release mode, embedded content is used.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// RenderSetup::new()
    ///     .styles(embed_styles!("src/styles"))
    /// ```
    pub fn styles(mut self, source: EmbeddedStyles) -> Self {
        self.embedded_styles = Some(source);
        self
    }

    /// Adds a stylesheet directory for runtime loading.
    ///
    /// Stylesheets from directories are loaded at build time and merged with
    /// any embedded stylesheets. Directory styles take precedence over
    /// embedded styles with the same name.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// RenderSetup::new()
    ///     .styles(embed_styles!("src/styles"))
    ///     .styles_dir("~/.myapp/themes")  // User overrides
    /// ```
    pub fn styles_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.style_dirs.push(path.as_ref().to_path_buf());
        self
    }

    /// Sets the default theme name.
    ///
    /// If not specified, "default" is used.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// RenderSetup::new()
    ///     .styles(embed_styles!("src/styles"))
    ///     .default_theme("dark")
    /// ```
    pub fn default_theme(mut self, name: &str) -> Self {
        self.default_theme_name = Some(name.to_string());
        self
    }

    /// Builds the configured application.
    ///
    /// This method:
    /// 1. Loads all templates into the MiniJinja environment
    /// 2. Loads all stylesheets and extracts the default theme
    /// 3. Returns an immutable `OutstandingApp` ready for rendering
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Template directories don't exist or can't be read
    /// - Stylesheet YAML is invalid
    /// - The default theme is not found
    pub fn build(self) -> Result<OutstandingApp, SetupError> {
        // Build template registry
        let mut template_registry = if let Some(embedded) = self.embedded_templates {
            TemplateRegistry::from(embedded)
        } else {
            TemplateRegistry::new()
        };

        // Add additional template directories
        for dir in &self.template_dirs {
            template_registry
                .add_template_dir(dir)
                .map_err(|e| SetupError::Template(e.to_string()))?;
        }

        // Build stylesheet registry
        let mut stylesheet_registry = if let Some(embedded) = self.embedded_styles {
            StylesheetRegistry::from(embedded)
        } else {
            StylesheetRegistry::new()
        };

        // Add additional stylesheet directories
        for dir in &self.style_dirs {
            stylesheet_registry
                .add_dir(dir)
                .map_err(|e| SetupError::Stylesheet(e.to_string()))?;
        }

        // Extract default theme
        let theme_name = self.default_theme_name.as_deref().unwrap_or("default");
        let theme = stylesheet_registry
            .get(theme_name)
            .map_err(|_| SetupError::ThemeNotFound(theme_name.to_string()))?;

        // Collect all templates for later use
        let mut templates = HashMap::new();
        for name in template_registry.names() {
            if let Ok(content) = template_registry.get_content(name) {
                templates.insert(name.to_string(), content);
            }
        }

        Ok(OutstandingApp {
            theme,
            templates,
            stylesheet_registry,
        })
    }
}

/// A fully configured Outstanding application.
///
/// This type is immutable and can be stored in a `static` variable.
/// All templates are pre-loaded, enabling `{% include %}` directives
/// and `&self` rendering.
///
/// # Example
///
/// ```rust,ignore
/// use once_cell::sync::Lazy;
/// use outstanding::{embed_templates, embed_styles, RenderSetup, OutstandingApp};
///
/// static APP: Lazy<OutstandingApp> = Lazy::new(|| {
///     RenderSetup::new()
///         .templates(embed_templates!("src/templates"))
///         .styles(embed_styles!("src/styles"))
///         .build()
///         .expect("Failed to build app")
/// });
///
/// fn render_list(items: &[Item]) -> String {
///     APP.render("list", items, OutputMode::Term).unwrap()
/// }
/// ```
pub struct OutstandingApp {
    /// The default theme.
    theme: Theme,

    /// Template content cache (for re-registering with different modes).
    templates: HashMap<String, String>,

    /// Stylesheet registry (for accessing additional themes).
    stylesheet_registry: StylesheetRegistry,
}

impl OutstandingApp {
    /// Renders a template with the given data.
    ///
    /// Uses the default theme configured at setup time.
    ///
    /// # Arguments
    ///
    /// * `template` - Template name (without extension)
    /// * `data` - Serializable data to render
    /// * `mode` - Output mode (Term, Text, Json, etc.)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let output = app.render("list", &items, OutputMode::Term)?;
    /// ```
    pub fn render<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        // For JSON/YAML/XML/CSV modes, serialize directly
        if mode.is_structured() {
            return self.serialize_data(data, mode);
        }

        // For text modes, render through MiniJinja
        let mut env = Environment::new();
        register_filters(&mut env);

        for (name, content) in &self.templates {
            env.add_template_owned(name.clone(), content.clone())
                .map_err(|e| SetupError::Template(e.to_string()))?;
        }

        let tmpl = env.get_template(template)?;
        Ok(tmpl.render(data)?)
    }

    /// Serializes data to structured format (JSON, YAML, XML, CSV).
    fn serialize_data<T: Serialize>(
        &self,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        match mode {
            OutputMode::Json => {
                serde_json::to_string_pretty(data).map_err(|e| SetupError::Template(e.to_string()))
            }
            OutputMode::Yaml => {
                serde_yaml::to_string(data).map_err(|e| SetupError::Template(e.to_string()))
            }
            OutputMode::Xml => {
                quick_xml::se::to_string(data).map_err(|e| SetupError::Template(e.to_string()))
            }
            OutputMode::Csv => {
                let value =
                    serde_json::to_value(data).map_err(|e| SetupError::Template(e.to_string()))?;
                let (headers, rows) = crate::util::flatten_json_for_csv(&value);

                let mut wtr = csv::Writer::from_writer(Vec::new());
                wtr.write_record(&headers)
                    .map_err(|e| SetupError::Template(e.to_string()))?;
                for row in rows {
                    wtr.write_record(&row)
                        .map_err(|e| SetupError::Template(e.to_string()))?;
                }
                let bytes = wtr
                    .into_inner()
                    .map_err(|e| SetupError::Template(e.to_string()))?;
                String::from_utf8(bytes).map_err(|e| SetupError::Template(e.to_string()))
            }
            _ => Err(SetupError::Config(format!(
                "serialize_data called with non-structured mode: {:?}",
                mode
            ))),
        }
    }

    /// Returns the default theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Gets a theme by name from the stylesheet registry.
    ///
    /// This allows using themes other than the default at runtime.
    pub fn get_theme(&mut self, name: &str) -> Result<Theme, SetupError> {
        self.stylesheet_registry
            .get(name)
            .map_err(|_| SetupError::ThemeNotFound(name.to_string()))
    }

    /// Returns the names of all available templates.
    pub fn template_names(&self) -> impl Iterator<Item = &String> {
        self.templates.keys()
    }

    /// Returns the names of all available themes.
    pub fn theme_names(&self) -> Vec<String> {
        self.stylesheet_registry.names().map(String::from).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_setup_minimal() {
        // Test with no templates or styles - should fail on missing theme
        let result = RenderSetup::new().build();
        assert!(result.is_err());
    }

    #[test]
    fn test_render_setup_with_inline_styles() {
        // We can't easily test embedded sources without the macro,
        // but we can test the builder structure
        let setup = RenderSetup::new()
            .default_theme("custom")
            .templates_dir("/nonexistent");

        assert!(setup.default_theme_name == Some("custom".to_string()));
        assert_eq!(setup.template_dirs.len(), 1);
    }
}
