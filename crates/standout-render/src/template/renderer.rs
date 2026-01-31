//! Pre-compiled template renderer.
//!
//! This module provides [`Renderer`], a high-level interface for template
//! rendering that supports both inline and file-based templates.
//!
//! # File-Based Templates
//!
//! Templates can be loaded from directories on the filesystem:
//!
//! ```rust,ignore
//! use standout_render::{Renderer, Theme};
//!
//! let mut renderer = Renderer::new(Theme::new())?;
//! renderer.add_template_dir("./templates")?;
//!
//! // Renders templates/todos/list.jinja
//! let output = renderer.render("todos/list", &data)?;
//! ```
//!
//! See [`Renderer::add_template_dir`] for details on template resolution
//! and the [`super::registry`] module for the underlying mechanism.
//!
//! # Development vs Release
//!
//! In development mode (`debug_assertions` enabled):
//! - Template content is re-read from disk on each render
//! - This enables hot reloading without recompilation
//!
//! In release mode:
//! - Templates can be embedded at compile time for deployment
//! - Use [`Renderer::with_embedded`] to load pre-embedded templates

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use standout_bbparser::{BBParser, TagTransform, UnknownTagBehavior};

use super::engine::{MiniJinjaEngine, TemplateEngine};
use super::registry::{walk_template_dir, ResolvedTemplate, TemplateRegistry};
use crate::error::RenderError;
use crate::output::OutputMode;
use crate::style::Styles;
use crate::theme::Theme;
use crate::EmbeddedTemplates;

/// A renderer with pre-registered templates.
///
/// Use this when your application has multiple templates that are rendered
/// repeatedly. Templates are compiled once and reused.
///
/// # Template Sources
///
/// Templates can come from multiple sources:
///
/// 1. Inline strings via [`add_template`](Self::add_template) - highest priority
/// 2. Filesystem directories via [`add_template_dir`](Self::add_template_dir)
/// 3. Embedded content via [`with_embedded`](Self::with_embedded)
///
/// When the same name exists in multiple sources, inline templates take
/// precedence over file-based templates.
///
/// Note: File-based templates must have unique names across all registered
/// directories. If the same name exists in multiple directories, it is treated
/// as a collision error.
///
/// # Example: Inline Templates
///
/// ```rust
/// use standout_render::{Renderer, Theme};
/// use console::Style;
/// use serde::Serialize;
///
/// let theme = Theme::new()
///     .add("title", Style::new().bold())
///     .add("count", Style::new().cyan());
///
/// let mut renderer = Renderer::new(theme).unwrap();
/// renderer.add_template("header", r#"[title]{{ title }}[/title]"#).unwrap();
/// renderer.add_template("stats", r#"Count: [count]{{ n }}[/count]"#).unwrap();
///
/// #[derive(Serialize)]
/// struct Header { title: String }
///
/// #[derive(Serialize)]
/// struct Stats { n: usize }
///
/// let h = renderer.render("header", &Header { title: "Report".into() }).unwrap();
/// let s = renderer.render("stats", &Stats { n: 42 }).unwrap();
/// ```
///
/// # Example: File-Based Templates
///
/// ```rust,ignore
/// use standout_render::{Renderer, Theme};
///
/// let mut renderer = Renderer::new(Theme::new())?;
///
/// // Register template directory
/// renderer.add_template_dir("./templates")?;
///
/// // Templates are resolved by relative path:
/// // "config" -> ./templates/config.jinja
/// // "todos/list" -> ./templates/todos/list.jinja
/// let output = renderer.render("config", &data)?;
/// ```
///
/// # Hot Reloading (Development)
///
/// In debug builds, file-based templates are re-read from disk on each render.
/// This enables editing templates without recompiling:
///
/// ```bash
/// # Edit template
/// vim templates/todos/list.jinja
///
/// # Re-run - changes are picked up immediately
/// cargo run -- todos list
/// ```
pub struct Renderer {
    engine: Box<dyn TemplateEngine>,
    /// Registry for file-based template resolution
    registry: TemplateRegistry,
    /// Whether the registry has been initialized from directories
    registry_initialized: bool,
    /// Registered template directories (for lazy initialization)
    template_dirs: Vec<std::path::PathBuf>,
    /// Resolved styles for BBParser post-processing
    styles: Styles,
    /// Output mode for BBParser transform selection
    output_mode: OutputMode,
}

impl Renderer {
    /// Creates a new renderer with automatic color detection.
    ///
    /// Color mode is detected automatically from the OS settings.
    /// Styles are resolved for the detected mode.
    ///
    /// # Errors
    ///
    /// Returns an error if any style aliases are invalid (dangling or cyclic).
    /// Returns an error if any style aliases are invalid (dangling or cyclic).
    pub fn new(theme: Theme) -> Result<Self, RenderError> {
        Self::with_output(theme, OutputMode::Auto)
    }

    /// Creates a new renderer with explicit output mode.
    ///
    /// Color mode is detected automatically from the OS settings.
    /// Styles are resolved for the detected mode.
    ///
    /// # Errors
    ///
    /// Returns an error if any style aliases are invalid (dangling or cyclic).
    pub fn with_output(theme: Theme, mode: OutputMode) -> Result<Self, RenderError> {
        Self::with_output_and_engine(theme, mode, Box::new(MiniJinjaEngine::new()))
    }

    /// Creates a new renderer with explicit output mode and template engine.
    ///
    /// This allows injecting a custom template engine implementation.
    pub fn with_output_and_engine(
        theme: Theme,
        mode: OutputMode,
        engine: Box<dyn TemplateEngine>,
    ) -> Result<Self, RenderError> {
        // Validate style aliases before creating the renderer
        theme
            .validate()
            .map_err(|e| RenderError::StyleError(e.to_string()))?;

        // Detect color mode and resolve styles for that mode
        let color_mode = super::super::theme::detect_color_mode();
        let styles = theme.resolve_styles(Some(color_mode));

        Ok(Self {
            engine,
            registry: TemplateRegistry::new(),
            registry_initialized: false,
            template_dirs: Vec::new(),
            styles,
            output_mode: mode,
        })
    }

    /// Registers a named inline template.
    ///
    /// Inline templates have the highest priority and will shadow any
    /// file-based templates with the same name.
    ///
    /// The template is compiled immediately; errors are returned if syntax is invalid.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// renderer.add_template("header", r#"[title]{{ title }}[/title]"#)?;
    /// ```
    pub fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError> {
        // Add to engine for compilation
        self.engine.add_template(name, source)?;
        // Also add to registry for consistency
        self.registry.add_inline(name, source);
        Ok(())
    }

    /// Adds a directory to search for template files.
    ///
    /// Templates in the directory are resolved by their relative path without
    /// extension. For example, with directory `./templates`:
    ///
    /// - `"config"` → `./templates/config.jinja`
    /// - `"todos/list"` → `./templates/todos/list.jinja`
    ///
    /// # Extension Priority
    ///
    /// Recognized extensions in priority order: `.jinja`, `.jinja2`, `.j2`, `.txt`
    ///
    /// If multiple files share the same base name with different extensions,
    /// the higher-priority extension wins for extensionless lookups.
    ///
    /// # Multiple Directories
    ///
    /// Multiple directories can be registered. However, template names must be
    /// unique across all directories.
    ///
    /// # Collision Detection
    ///
    /// If the same template name exists in multiple directories, an error
    /// is returned (either immediately or during `refresh()`) with details
    /// about the conflicting files. Strict uniqueness is enforced to prevent
    /// ambiguous template resolution.
    ///
    /// # Lazy Initialization
    ///
    /// Directory walking happens lazily on first render (or explicit [`refresh`](Self::refresh)).
    /// In development mode, this is automatic. Call `refresh()` if you add
    /// directories after the first render.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory doesn't exist or isn't readable.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// renderer.add_template_dir("./templates")?;
    /// renderer.add_template_dir("./plugin-templates")?;
    ///
    /// // "config" resolves from first directory that has it
    /// let output = renderer.render("config", &data)?;
    /// ```
    pub fn add_template_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), RenderError> {
        let path = path.as_ref();

        // Validate the directory exists
        if !path.exists() {
            return Err(RenderError::OperationError(format!(
                "Template directory does not exist: {}",
                path.display()
            )));
        }
        if !path.is_dir() {
            return Err(RenderError::OperationError(format!(
                "Path is not a directory: {}",
                path.display()
            )));
        }

        self.template_dirs.push(path.to_path_buf());
        // Mark as needing re-initialization
        self.registry_initialized = false;
        Ok(())
    }

    /// Loads pre-embedded templates for release builds.
    ///
    /// Embedded templates are stored directly in memory, avoiding filesystem
    /// access at runtime. This is useful for deployment where template files
    /// may not be available.
    ///
    /// # Arguments
    ///
    /// * `templates` - Map of template name to content
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Generated at build time
    /// let embedded = standout_render::embed_templates!("./templates");
    ///
    /// let mut renderer = Renderer::new(theme)?;
    /// renderer.with_embedded(embedded);
    /// ```
    pub fn with_embedded(&mut self, templates: HashMap<String, String>) -> &mut Self {
        self.registry.add_embedded(templates);
        self
    }

    /// Loads templates from an `EmbeddedTemplates` source.
    ///
    /// This is the recommended way to use `embed_templates!` with `Renderer`.
    /// The embedded templates are converted to a registry that supports both
    /// extensionless and with-extension lookups.
    ///
    /// In debug mode, if the source path exists, templates are loaded from disk
    /// (enabling hot-reload). Otherwise, embedded content is used.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout_render::{embed_templates, Renderer, Theme};
    ///
    /// let mut renderer = Renderer::new(Theme::new())?;
    /// renderer.with_embedded_source(embed_templates!("./templates"));
    ///
    /// // Now you can render any template from the embedded source
    /// let output = renderer.render("list", &data)?;
    /// ```
    pub fn with_embedded_source(&mut self, source: EmbeddedTemplates) -> &mut Self {
        // Convert EmbeddedTemplates to TemplateRegistry
        let template_registry = TemplateRegistry::from(source);

        // Add all templates from the registry to both engine and registry
        // This mirrors the behavior of add_template()
        for name in template_registry.names() {
            if let Ok(content) = template_registry.get_content(name) {
                // Add to engine (required for includes to work)
                // Ignore errors for duplicate names (e.g., "foo" and "foo.jinja" have same content)
                let _ = self.engine.add_template(name, &content);
                // Add to registry for name resolution
                self.registry.add_inline(name, &content);
            }
        }
        self
    }

    /// Sets the output mode for subsequent renders.
    ///
    /// This allows changing the output mode without creating a new renderer,
    /// which is useful when the same templates need to be rendered with
    /// different output modes.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut renderer = Renderer::new(theme)?;
    ///
    /// // Render with terminal colors
    /// renderer.set_output_mode(OutputMode::Term);
    /// let colored = renderer.render("list", &data)?;
    ///
    /// // Render plain text
    /// renderer.set_output_mode(OutputMode::Text);
    /// let plain = renderer.render("list", &data)?;
    /// ```
    pub fn set_output_mode(&mut self, mode: OutputMode) {
        self.output_mode = mode;
    }

    /// Forces a rebuild of the template resolution map.
    ///
    /// This re-walks all registered template directories and rebuilds the
    /// resolution map. Call this if:
    ///
    /// - You've added template directories after the first render
    /// - Template files have been added/removed from disk
    ///
    /// In development mode, this is called automatically on first render.
    ///
    /// # Errors
    ///
    /// Returns an error if directory walking fails or template collisions are detected.
    pub fn refresh(&mut self) -> Result<(), RenderError> {
        self.initialize_registry()
    }

    /// Initializes the registry from registered template directories.
    ///
    /// Called lazily on first render or explicitly via `refresh()`.
    fn initialize_registry(&mut self) -> Result<(), RenderError> {
        // Clear existing file-based templates (keep inline)
        let mut new_registry = TemplateRegistry::new();

        // Walk each directory and collect templates
        for dir in &self.template_dirs {
            let files = walk_template_dir(dir).map_err(|e| {
                RenderError::OperationError(format!(
                    "Failed to walk template directory {}: {}",
                    dir.display(),
                    e
                ))
            })?;

            new_registry
                .add_from_files(files)
                .map_err(|e| RenderError::OperationError(e.to_string()))?;
        }

        self.registry = new_registry;
        self.registry_initialized = true;
        Ok(())
    }

    /// Ensures the registry is initialized, doing so lazily if needed.
    fn ensure_registry_initialized(&mut self) -> Result<(), RenderError> {
        if !self.registry_initialized && !self.template_dirs.is_empty() {
            self.initialize_registry()?;
        }
        Ok(())
    }

    /// Renders a registered template with the given data.
    ///
    /// Templates are looked up in this order:
    ///
    /// 1. Inline templates (added via [`add_template`](Self::add_template))
    /// 2. File-based templates (from [`add_template_dir`](Self::add_template_dir))
    ///
    /// # Hot Reloading (Development)
    ///
    /// In debug builds, file-based templates are re-read from disk on each render.
    /// This enables editing templates without recompiling the application.
    ///
    /// # Errors
    ///
    /// Returns an error if the template name is not found or rendering fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let output = renderer.render("todos/list", &data)?;
    /// ```
    pub fn render<T: Serialize>(&mut self, name: &str, data: &T) -> Result<String, RenderError> {
        // First, check if it's an inline template
        // We check this first to avoid filesystem lookups for known templates.
        // In debug mode, if it's a file-based template, we want to skip this check
        // to force a reload from disk.

        let is_inline = self
            .registry
            .get(name)
            .is_ok_and(|t| matches!(t, ResolvedTemplate::Inline(_)));

        // Convert data to serde_json::Value for the engine
        let data_value = serde_json::to_value(data)?;

        // In release mode: always use engine cache if available.
        // In debug mode: only use engine cache if it's an inline template (which doesn't change on disk).
        let template_output = if !cfg!(debug_assertions) || is_inline {
            // Try to render with the engine's cached template
            match self.engine.render_named(name, &data_value) {
                Ok(output) => output,
                Err(_) => {
                    // Template not in cache, load and render
                    self.ensure_registry_initialized()?;
                    let content = self.get_template_content(name)?;
                    self.engine.add_template(name, &content)?;
                    self.engine.render_named(name, &data_value)?
                }
            }
        } else {
            // Debug mode with file-based template: always reload
            self.ensure_registry_initialized()?;
            let content = self.get_template_content(name)?;
            self.engine.add_template(name, &content)?;
            self.engine.render_named(name, &data_value)?
        };

        // Pass 2: BBParser style tag processing
        let final_output = self.apply_style_tags(&template_output);

        Ok(final_output)
    }

    /// Applies BBParser style tag post-processing.
    fn apply_style_tags(&self, output: &str) -> String {
        let transform = match self.output_mode {
            OutputMode::Auto => {
                if self.output_mode.should_use_color() {
                    TagTransform::Apply
                } else {
                    TagTransform::Remove
                }
            }
            OutputMode::Term => TagTransform::Apply,
            OutputMode::Text => TagTransform::Remove,
            OutputMode::TermDebug => TagTransform::Keep,
            OutputMode::Json | OutputMode::Yaml | OutputMode::Xml | OutputMode::Csv => {
                TagTransform::Remove
            }
        };

        let resolved_styles = self.styles.to_resolved_map();
        let parser = BBParser::new(resolved_styles, transform)
            .unknown_behavior(UnknownTagBehavior::Passthrough);
        parser.parse(output)
    }

    /// Gets template content, re-reading from disk in debug mode.
    fn get_template_content(&self, name: &str) -> Result<String, RenderError> {
        let resolved = self
            .registry
            .get(name)
            .map_err(|e| RenderError::TemplateNotFound(e.to_string()))?;

        match resolved {
            ResolvedTemplate::Inline(content) => Ok(content),
            ResolvedTemplate::File(path) => {
                // In debug mode, always re-read for hot reloading
                // In release mode, we still read (could optimize with caching)
                std::fs::read_to_string(&path).map_err(|e| {
                    RenderError::IoError(std::io::Error::other(format!(
                        "Failed to read template {}: {}",
                        path.display(),
                        e
                    )))
                })
            }
        }
    }

    /// Returns the number of registered templates.
    ///
    /// This includes both inline and file-based templates.
    /// Note: File-based templates are counted with both extensionless and
    /// with-extension names, so this may be higher than the number of files.
    pub fn template_count(&self) -> usize {
        self.registry.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::Style;
    use serde::Serialize;
    use std::io::Write;
    use tempfile::TempDir;

    #[derive(Serialize)]
    struct SimpleData {
        message: String,
    }

    #[test]
    fn test_renderer_add_and_render() {
        let theme = Theme::new().add("ok", Style::new().green());
        let mut renderer = Renderer::with_output(theme, OutputMode::Text).unwrap();

        renderer
            .add_template("test", r#"[ok]{{ message }}[/ok]"#)
            .unwrap();

        let output = renderer
            .render(
                "test",
                &SimpleData {
                    message: "hi".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "hi");
    }

    #[test]
    fn test_renderer_unknown_template_error() {
        let theme = Theme::new();
        let mut renderer = Renderer::with_output(theme, OutputMode::Text).unwrap();

        let result = renderer.render(
            "nonexistent",
            &SimpleData {
                message: "x".into(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_renderer_multiple_templates() {
        let theme = Theme::new()
            .add("a", Style::new().red())
            .add("b", Style::new().blue());

        let mut renderer = Renderer::with_output(theme, OutputMode::Text).unwrap();
        renderer
            .add_template("tmpl_a", r#"A: [a]{{ message }}[/a]"#)
            .unwrap();
        renderer
            .add_template("tmpl_b", r#"B: [b]{{ message }}[/b]"#)
            .unwrap();

        let data = SimpleData {
            message: "test".into(),
        };

        assert_eq!(renderer.render("tmpl_a", &data).unwrap(), "A: test");
        assert_eq!(renderer.render("tmpl_b", &data).unwrap(), "B: test");
    }

    #[test]
    fn test_renderer_fails_with_invalid_theme() {
        let theme = Theme::new().add("orphan", "missing");
        let result = Renderer::new(theme);
        assert!(result.is_err());
    }

    #[test]
    fn test_renderer_succeeds_with_valid_aliases() {
        let theme = Theme::new()
            .add("base", Style::new().bold())
            .add("alias", "base");

        let result = Renderer::new(theme);
        assert!(result.is_ok());
    }

    // =========================================================================
    // File-based template tests
    // =========================================================================

    fn create_template_file(dir: &Path, relative_path: &str, content: &str) {
        let full_path = dir.join(relative_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut file = std::fs::File::create(&full_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_renderer_add_template_dir() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "config.jinja", "Config: {{ value }}");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct Data {
            value: String,
        }

        let output = renderer
            .render(
                "config",
                &Data {
                    value: "test".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "Config: test");
    }

    #[test]
    fn test_renderer_nested_template_dir() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "todos/list.jinja", "List: {{ count }}");
        create_template_file(temp_dir.path(), "todos/detail.jinja", "Detail: {{ id }}");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct ListData {
            count: usize,
        }

        #[derive(Serialize)]
        struct DetailData {
            id: usize,
        }

        let list_output = renderer
            .render("todos/list", &ListData { count: 5 })
            .unwrap();
        assert_eq!(list_output, "List: 5");

        let detail_output = renderer
            .render("todos/detail", &DetailData { id: 42 })
            .unwrap();
        assert_eq!(detail_output, "Detail: 42");
    }

    #[test]
    fn test_renderer_template_with_extension() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "config.jinja", "Content");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct Empty {}

        // Both with and without extension should work
        assert!(renderer.render("config", &Empty {}).is_ok());
        assert!(renderer.render("config.jinja", &Empty {}).is_ok());
    }

    #[test]
    fn test_renderer_inline_shadows_file() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "config.jinja", "From file");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        // Add inline template with same name (should shadow file)
        renderer.add_template("config", "From inline").unwrap();

        #[derive(Serialize)]
        struct Empty {}

        let output = renderer.render("config", &Empty {}).unwrap();
        assert_eq!(output, "From inline");
    }

    #[test]
    fn test_renderer_nonexistent_dir_error() {
        let mut renderer = Renderer::new(Theme::new()).unwrap();
        let result = renderer.add_template_dir("/nonexistent/path/that/does/not/exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_renderer_hot_reload() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "hot.jinja", "Version 1");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct Empty {}

        // First render
        let output1 = renderer.render("hot", &Empty {}).unwrap();
        assert_eq!(output1, "Version 1");

        // Modify the file
        create_template_file(temp_dir.path(), "hot.jinja", "Version 2");

        // Second render should see the change (hot reload)
        let output2 = renderer.render("hot", &Empty {}).unwrap();
        assert_eq!(output2, "Version 2");
    }

    #[test]
    fn test_renderer_extension_priority() {
        let temp_dir = TempDir::new().unwrap();
        // Create files with different extensions
        create_template_file(temp_dir.path(), "config.j2", "From j2");
        create_template_file(temp_dir.path(), "config.jinja", "From jinja");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct Empty {}

        // Extensionless should resolve to .jinja (higher priority)
        let output = renderer.render("config", &Empty {}).unwrap();
        assert_eq!(output, "From jinja");
    }

    #[test]
    fn test_renderer_with_embedded() {
        let mut renderer = Renderer::new(Theme::new()).unwrap();

        let mut embedded = HashMap::new();
        embedded.insert("embedded".to_string(), "Embedded: {{ val }}".to_string());
        renderer.with_embedded(embedded);

        #[derive(Serialize)]
        struct Data {
            val: String,
        }

        let output = renderer
            .render("embedded", &Data { val: "ok".into() })
            .unwrap();
        assert_eq!(output, "Embedded: ok");
    }

    #[test]
    fn test_renderer_set_output_mode() {
        use console::Style;

        // Use force_styling(true) to ensure ANSI codes are output even in tests
        let theme = Theme::new().add("highlight", Style::new().green().force_styling(true));
        let mut renderer = Renderer::with_output(theme, OutputMode::Term).unwrap();
        renderer
            .add_template("test", "[highlight]hello[/highlight]")
            .unwrap();

        #[derive(Serialize)]
        struct Empty {}

        // With Term mode, should have ANSI codes
        let term_output = renderer.render("test", &Empty {}).unwrap();
        assert!(
            term_output.contains("\x1b["),
            "Expected ANSI codes in Term mode, got: {:?}",
            term_output
        );

        // Switch to Text mode
        renderer.set_output_mode(OutputMode::Text);
        let text_output = renderer.render("test", &Empty {}).unwrap();
        assert_eq!(text_output, "hello", "Expected plain text in Text mode");
    }

    #[test]
    fn test_renderer_with_embedded_source() {
        use crate::{EmbeddedSource, TemplateResource};

        // Create an EmbeddedTemplates source (simulating embed_templates! output)
        static ENTRIES: &[(&str, &str)] = &[
            ("greeting.jinja", "Hello, {{ name }}!"),
            ("_partial.jinja", "PARTIAL"),
            (
                "with_include.jinja",
                "Before {% include '_partial' %} After",
            ),
        ];
        let source: EmbeddedSource<TemplateResource> =
            EmbeddedSource::new(ENTRIES, "/nonexistent/path");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.with_embedded_source(source);

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        // Test basic rendering
        let output = renderer
            .render(
                "greeting",
                &Data {
                    name: "World".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "Hello, World!");

        // Test extensionless access
        let output2 = renderer
            .render(
                "greeting.jinja",
                &Data {
                    name: "Test".into(),
                },
            )
            .unwrap();
        assert_eq!(output2, "Hello, Test!");

        // Test includes work with extensionless names
        #[derive(Serialize)]
        struct Empty {}
        let output3 = renderer.render("with_include", &Empty {}).unwrap();
        assert_eq!(output3, "Before PARTIAL After");
    }
    #[test]
    fn test_renderer_with_custom_engine() {
        use std::collections::HashMap;

        struct MockEngine {
            templates: HashMap<String, String>,
        }

        impl TemplateEngine for MockEngine {
            fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError> {
                self.templates.insert(name.to_string(), source.to_string());
                Ok(())
            }

            fn has_template(&self, name: &str) -> bool {
                self.templates.contains_key(name)
            }

            fn render_template(
                &self,
                source: &str,
                data: &serde_json::Value,
            ) -> Result<String, RenderError> {
                Ok(format!("Mock Render: {} data={}", source, data))
            }

            fn render_named(
                &self,
                name: &str,
                data: &serde_json::Value,
            ) -> Result<String, RenderError> {
                if let Some(src) = self.templates.get(name) {
                    Ok(format!("Mock Named: {} data={}", src, data))
                } else {
                    Err(RenderError::TemplateNotFound(name.to_string()))
                }
            }

            fn render_with_context(
                &self,
                template: &str,
                data: &serde_json::Value,
                _context: HashMap<String, serde_json::Value>,
            ) -> Result<String, RenderError> {
                self.render_template(template, data)
            }

            fn supports_includes(&self) -> bool {
                false
            }
            fn supports_filters(&self) -> bool {
                false
            }
            fn supports_control_flow(&self) -> bool {
                false
            }
        }

        let engine = Box::new(MockEngine {
            templates: HashMap::new(),
        });
        let mut renderer =
            Renderer::with_output_and_engine(Theme::new(), OutputMode::Text, engine).unwrap();

        renderer.add_template("test", "content").unwrap();

        #[derive(Serialize)]
        struct Data {
            val: i32,
        }

        let output = renderer.render("test", &Data { val: 42 }).unwrap();
        // The mock engine formats as "Mock Render: {}" or "Mock Named: {}"
        // Since we added it as named template, render() calls render_named logic.
        // Wait, render() logic:
        // if debug_assertions || is_inline -> render_named
        // The MockEngine::render_named returns "Mock Named: content data={...}"
        assert_eq!(output, "Mock Named: content data={\"val\":42}");
    }

    #[test]
    fn test_renderer_with_simple_engine() {
        use crate::template::SimpleEngine;

        let engine = Box::new(SimpleEngine::new());
        let mut renderer =
            Renderer::with_output_and_engine(Theme::new(), OutputMode::Text, engine).unwrap();

        // Add an inline template using SimpleEngine syntax
        renderer.add_template("welcome", "Hello, {name}!").unwrap();

        #[derive(Serialize)]
        struct User {
            name: String,
        }

        // Render it
        let output = renderer
            .render(
                "welcome",
                &User {
                    name: "Standout".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "Hello, Standout!");
    }
}
