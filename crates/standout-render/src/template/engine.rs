//! Template engine abstraction.
//!
//! This module defines the [`TemplateEngine`] trait which allows standout-render
//! to work with different template backends. The default implementation is
//! [`MiniJinjaEngine`], which provides full template functionality.

use minijinja::{Environment, Value};

use std::collections::HashMap;

use crate::error::RenderError;

/// A template engine that can render templates with data.
///
/// This trait abstracts over the template rendering backend, allowing
/// different implementations (e.g., MiniJinja, simple string substitution).
///
/// Template engines handle:
/// - Template compilation and caching
/// - Variable substitution
/// - Template logic (loops, conditionals) - if supported
/// - Custom filters and functions - if supported
pub trait TemplateEngine: Send + Sync {
    /// Renders a template string with the given data.
    ///
    /// This compiles and renders the template in one step. For repeated
    /// rendering of the same template, use [`add_template`](Self::add_template)
    /// and [`render_named`](Self::render_named).
    fn render_template(
        &self,
        template: &str,
        data: &serde_json::Value,
    ) -> Result<String, RenderError>;

    /// Adds a named template to the engine.
    ///
    /// The template is compiled and cached for later use via [`render_named`](Self::render_named).
    fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError>;

    /// Renders a previously registered template.
    ///
    /// The template must have been added via [`add_template`](Self::add_template).
    fn render_named(&self, name: &str, data: &serde_json::Value) -> Result<String, RenderError>;

    /// Checks if a template with the given name exists.
    fn has_template(&self, name: &str) -> bool;

    /// Renders a template with additional context values merged in.
    ///
    /// The `context` values are merged with the serialized `data`. If there are
    /// key conflicts, `data` takes precedence.
    fn render_with_context(
        &self,
        template: &str,
        data: &serde_json::Value,
        context: HashMap<String, serde_json::Value>,
    ) -> Result<String, RenderError>;

    /// Whether this engine supports template includes (`{% include %}`).
    fn supports_includes(&self) -> bool;

    /// Whether this engine supports filters (`{{ value | filter }}`).
    fn supports_filters(&self) -> bool;

    /// Whether this engine supports control flow (`{% for %}`, `{% if %}`).
    fn supports_control_flow(&self) -> bool;
}

/// MiniJinja-based template engine.
///
/// This is the default template engine, providing full template functionality:
/// - Jinja2-compatible syntax
/// - Loops, conditionals, macros
/// - Custom filters and functions
/// - Template includes
///
/// # Example
///
/// ```rust
/// use standout_render::template::MiniJinjaEngine;
/// use standout_render::template::TemplateEngine;
/// use serde::Serialize;
/// use serde_json::json;
///
/// #[derive(Serialize)]
/// struct Data { name: String }
///
/// let engine = MiniJinjaEngine::new();
/// let data = Data { name: "World".into() };
/// let data_value = serde_json::to_value(&data).unwrap();
///
/// let output = engine.render_template(
///     "Hello, {{ name }}!",
///     &data_value,
/// ).unwrap();
/// assert_eq!(output, "Hello, World!");
/// ```
pub struct MiniJinjaEngine {
    env: Environment<'static>,
}

impl MiniJinjaEngine {
    /// Creates a new MiniJinja engine with default filters registered.
    pub fn new() -> Self {
        let mut env = Environment::new();
        register_filters(&mut env);
        Self { env }
    }

    /// Returns a reference to the underlying MiniJinja environment.
    ///
    /// This allows advanced users to register custom filters, functions,
    /// or configure the environment directly.
    pub fn environment(&self) -> &Environment<'static> {
        &self.env
    }

    /// Returns a mutable reference to the underlying MiniJinja environment.
    ///
    /// This allows advanced users to register custom filters, functions,
    /// or configure the environment directly.
    pub fn environment_mut(&mut self) -> &mut Environment<'static> {
        &mut self.env
    }
}

impl Default for MiniJinjaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateEngine for MiniJinjaEngine {
    fn render_template(
        &self,
        template: &str,
        data: &serde_json::Value,
    ) -> Result<String, RenderError> {
        let value = Value::from_serialize(data);
        Ok(self.env.render_str(template, value)?)
    }

    fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError> {
        self.env
            .add_template_owned(name.to_string(), source.to_string())?;
        Ok(())
    }

    fn render_named(&self, name: &str, data: &serde_json::Value) -> Result<String, RenderError> {
        let tmpl = self.env.get_template(name)?;
        let value = Value::from_serialize(data);
        Ok(tmpl.render(value)?)
    }

    fn has_template(&self, name: &str) -> bool {
        self.env.get_template(name).is_ok()
    }

    fn render_with_context(
        &self,
        template: &str,
        data: &serde_json::Value,
        context: HashMap<String, serde_json::Value>,
    ) -> Result<String, RenderError> {
        // Merge data into context (data takes precedence)
        let mut combined = HashMap::new();
        for (key, value) in context {
            combined.insert(key, Value::from_serialize(value));
        }

        if let serde_json::Value::Object(map) = data {
            for (key, value) in map {
                combined.insert(key.clone(), Value::from_serialize(value));
            }
        }

        Ok(self.env.render_str(template, &combined)?)
    }

    fn supports_includes(&self) -> bool {
        true
    }

    fn supports_filters(&self) -> bool {
        true
    }

    fn supports_control_flow(&self) -> bool {
        true
    }
}

/// Registers standout's custom filters with a MiniJinja environment.
///
/// This is called automatically by [`MiniJinjaEngine::new`]. If you're using
/// the environment directly, call this to get standout's filters.
pub fn register_filters(env: &mut Environment<'static>) {
    use minijinja::{Error, ErrorKind};

    // Newline filter
    env.add_filter("nl", |value: Value| -> String { format!("{}\n", value) });

    // Deprecated style filter with helpful error message
    env.add_filter(
        "style",
        |_value: Value, _name: String| -> Result<String, Error> {
            Err(Error::new(
                ErrorKind::InvalidOperation,
                "The `style()` filter was removed in Standout 1.0. \
                 Use tag syntax instead: [stylename]{{ value }}[/stylename]",
            ))
        },
    );

    // Register tabular filters
    crate::tabular::filters::register_tabular_filters(env);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestData {
        name: String,
        count: usize,
    }

    #[test]
    fn test_minijinja_engine_simple() {
        let engine = MiniJinjaEngine::new();
        let data = TestData {
            name: "World".into(),
            count: 42,
        };
        let data_value = serde_json::to_value(&data).unwrap();
        let output = engine
            .render_template("Hello, {{ name }}!", &data_value)
            .unwrap();
        assert_eq!(output, "Hello, World!");
    }

    #[test]
    fn test_minijinja_engine_with_loop() {
        let engine = MiniJinjaEngine::new();

        #[derive(Serialize)]
        struct ListData {
            items: Vec<String>,
        }

        let data = ListData {
            items: vec!["a".into(), "b".into(), "c".into()],
        };
        let data_value = serde_json::to_value(&data).unwrap();
        let output = engine
            .render_template(
                "{% for item in items %}{{ item }},{% endfor %}",
                &data_value,
            )
            .unwrap();
        assert_eq!(output, "a,b,c,");
    }

    #[test]
    fn test_minijinja_engine_named_template() {
        let mut engine = MiniJinjaEngine::new();
        engine
            .add_template("greeting", "Hello, {{ name }}!")
            .unwrap();

        let data = TestData {
            name: "World".into(),
            count: 0,
        };
        let data_value = serde_json::to_value(&data).unwrap();
        let output = engine.render_named("greeting", &data_value).unwrap();
        assert_eq!(output, "Hello, World!");
    }

    #[test]
    fn test_minijinja_engine_template_error() {
        let engine = MiniJinjaEngine::new();
        let result = engine.render_template("{{ unclosed", &serde_json::Value::Null);
        assert!(result.is_err());
    }

    #[test]
    fn test_minijinja_engine_with_context() {
        let engine = MiniJinjaEngine::new();

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let mut context = HashMap::new();
        context.insert(
            "version".to_string(),
            serde_json::Value::String("1.0.0".into()),
        );

        let data = Data {
            name: "Test".into(),
        };
        let data_value = serde_json::to_value(&data).unwrap();
        let output = engine
            .render_with_context("{{ name }} v{{ version }}", &data_value, context)
            .unwrap();
        assert_eq!(output, "Test v1.0.0");
    }

    #[test]
    fn test_minijinja_engine_supports_features() {
        let engine = MiniJinjaEngine::new();
        assert!(engine.supports_includes());
        assert!(engine.supports_filters());
        assert!(engine.supports_control_flow());
    }
}
