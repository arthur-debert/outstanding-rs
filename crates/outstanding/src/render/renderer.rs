//! Pre-compiled template renderer.

use minijinja::{Environment, Error};
use serde::Serialize;

use super::filters::register_filters;
use crate::output::OutputMode;
use crate::theme::Theme;

/// A renderer with pre-registered templates.
///
/// Use this when your application has multiple templates that are rendered
/// repeatedly. Templates are compiled once and reused.
///
/// # Example
///
/// ```rust
/// use outstanding::{Renderer, Theme};
/// use console::Style;
/// use serde::Serialize;
///
/// let theme = Theme::new()
///     .add("title", Style::new().bold())
///     .add("count", Style::new().cyan());
///
/// let mut renderer = Renderer::new(theme).unwrap();
/// renderer.add_template("header", r#"{{ title | style("title") }}"#).unwrap();
/// renderer.add_template("stats", r#"Count: {{ n | style("count") }}"#).unwrap();
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
pub struct Renderer {
    env: Environment<'static>,
}

impl Renderer {
    /// Creates a new renderer with automatic color detection.
    ///
    /// # Errors
    ///
    /// Returns an error if any style aliases are invalid (dangling or cyclic).
    pub fn new(theme: Theme) -> Result<Self, Error> {
        Self::with_output(theme, OutputMode::Auto)
    }

    /// Creates a new renderer with explicit output mode.
    ///
    /// # Errors
    ///
    /// Returns an error if any style aliases are invalid (dangling or cyclic).
    pub fn with_output(theme: Theme, mode: OutputMode) -> Result<Self, Error> {
        // Validate style aliases before creating the renderer
        theme.validate().map_err(|e| {
            Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
        })?;

        let mut env = Environment::new();
        register_filters(&mut env, theme, mode);
        Ok(Self { env })
    }

    /// Registers a named template.
    ///
    /// The template is compiled immediately; errors are returned if syntax is invalid.
    pub fn add_template(&mut self, name: &str, source: &str) -> Result<(), Error> {
        self.env
            .add_template_owned(name.to_string(), source.to_string())
    }

    /// Renders a registered template with the given data.
    ///
    /// # Errors
    ///
    /// Returns an error if the template name is not found or rendering fails.
    pub fn render<T: Serialize>(&self, name: &str, data: &T) -> Result<String, Error> {
        let tmpl = self.env.get_template(name)?;
        tmpl.render(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::Style;
    use serde::Serialize;

    #[derive(Serialize)]
    struct SimpleData {
        message: String,
    }

    #[test]
    fn test_renderer_add_and_render() {
        let theme = Theme::new().add("ok", Style::new().green());
        let mut renderer = Renderer::with_output(theme, OutputMode::Text).unwrap();

        renderer
            .add_template("test", r#"{{ message | style("ok") }}"#)
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
        let renderer = Renderer::with_output(theme, OutputMode::Text).unwrap();

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
            .add_template("tmpl_a", r#"A: {{ message | style("a") }}"#)
            .unwrap();
        renderer
            .add_template("tmpl_b", r#"B: {{ message | style("b") }}"#)
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
}
