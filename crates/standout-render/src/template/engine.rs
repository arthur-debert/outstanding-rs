//! Template engine abstraction.
//!
//! This module defines the [`TemplateEngine`] trait which allows standout-render
//! to work with different template backends. The default implementation is
//! [`MiniJinjaEngine`], which provides full template functionality.

use minijinja::{Environment, Value};

use std::collections::HashMap;
use std::marker::PhantomData;

use crate::error::RenderError;
use crate::template::spelling::{self, stringify};
use crate::width::RenderWidthSource;
use crate::AmbiguousWidth;

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
pub trait TemplateEngine {
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

    /// Renders with an explicit ambiguous-width policy.
    fn render_template_with_width(
        &self,
        template: &str,
        data: &serde_json::Value,
        _policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        self.render_template(template, data)
    }

    /// Renders with terminal columns and an explicit ambiguous-width policy.
    fn render_template_with_render_widths(
        &self,
        template: &str,
        data: &serde_json::Value,
        _terminal_width: Option<usize>,
        policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        self.render_template_with_width(template, data, policy)
    }

    /// Adds a named template to the engine.
    ///
    /// The template is compiled and cached for later use via [`render_named`](Self::render_named).
    fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError>;

    /// Renders a previously registered template.
    ///
    /// The template must have been added via [`add_template`](Self::add_template).
    fn render_named(&self, name: &str, data: &serde_json::Value) -> Result<String, RenderError>;

    /// Renders a named template with an explicit ambiguous-width policy.
    fn render_named_with_width(
        &self,
        name: &str,
        data: &serde_json::Value,
        _policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        self.render_named(name, data)
    }

    /// Renders a named template with terminal columns and an explicit
    /// ambiguous-width policy.
    fn render_named_with_render_widths(
        &self,
        name: &str,
        data: &serde_json::Value,
        _terminal_width: Option<usize>,
        policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        self.render_named_with_width(name, data, policy)
    }

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

    /// Renders with context and an explicit ambiguous-width policy.
    fn render_with_context_and_width(
        &self,
        template: &str,
        data: &serde_json::Value,
        context: HashMap<String, serde_json::Value>,
        _policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        self.render_with_context(template, data, context)
    }

    /// Renders with additional context, terminal columns, and an explicit
    /// ambiguous-width policy.
    fn render_with_context_and_render_widths(
        &self,
        template: &str,
        data: &serde_json::Value,
        context: HashMap<String, serde_json::Value>,
        _terminal_width: Option<usize>,
        policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        self.render_with_context_and_width(template, data, context, policy)
    }

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
///
/// Not `Send` or `Sync`: the framework is single-threaded (#84). Filter width
/// state is scoped per render without a mutex (ADR-0030). Concurrent renders
/// on a shared engine are unsupported at the type level.
pub struct MiniJinjaEngine {
    env: Environment<'static>,
    render_widths: RenderWidthSource,
    _not_threaded: PhantomData<*const ()>,
}

impl MiniJinjaEngine {
    /// Creates a new MiniJinja engine with default filters registered.
    pub fn new() -> Self {
        let mut env = spelling::new_environment();
        let render_widths = RenderWidthSource::new(AmbiguousWidth::Narrow);
        register_filters_with_source(&mut env, render_widths.clone());
        Self {
            env,
            render_widths,
            _not_threaded: PhantomData,
        }
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
        let _widths = self.render_widths.scoped(AmbiguousWidth::Narrow, None);
        self.render_template_inner(template, data)
    }

    fn render_template_with_width(
        &self,
        template: &str,
        data: &serde_json::Value,
        policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        let _widths = self.render_widths.scoped(policy, None);
        self.render_template_inner(template, data)
    }

    fn render_template_with_render_widths(
        &self,
        template: &str,
        data: &serde_json::Value,
        terminal_width: Option<usize>,
        policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        let _widths = self.render_widths.scoped(policy, terminal_width);
        self.render_template_inner(template, data)
    }

    fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError> {
        self.env
            .add_template_owned(name.to_string(), source.to_string())?;
        Ok(())
    }

    fn render_named(&self, name: &str, data: &serde_json::Value) -> Result<String, RenderError> {
        let _widths = self.render_widths.scoped(AmbiguousWidth::Narrow, None);
        self.render_named_inner(name, data)
    }

    fn render_named_with_width(
        &self,
        name: &str,
        data: &serde_json::Value,
        policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        let _widths = self.render_widths.scoped(policy, None);
        self.render_named_inner(name, data)
    }

    fn render_named_with_render_widths(
        &self,
        name: &str,
        data: &serde_json::Value,
        terminal_width: Option<usize>,
        policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        let _widths = self.render_widths.scoped(policy, terminal_width);
        self.render_named_inner(name, data)
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
        let _widths = self.render_widths.scoped(AmbiguousWidth::Narrow, None);
        self.render_with_context_inner(template, data, context)
    }

    fn render_with_context_and_width(
        &self,
        template: &str,
        data: &serde_json::Value,
        context: HashMap<String, serde_json::Value>,
        policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        let _widths = self.render_widths.scoped(policy, None);
        self.render_with_context_inner(template, data, context)
    }

    fn render_with_context_and_render_widths(
        &self,
        template: &str,
        data: &serde_json::Value,
        context: HashMap<String, serde_json::Value>,
        terminal_width: Option<usize>,
        policy: AmbiguousWidth,
    ) -> Result<String, RenderError> {
        let _widths = self.render_widths.scoped(policy, terminal_width);
        self.render_with_context_inner(template, data, context)
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

impl MiniJinjaEngine {
    fn render_template_inner(
        &self,
        template: &str,
        data: &serde_json::Value,
    ) -> Result<String, RenderError> {
        let value = Value::from_serialize(data);
        Ok(self.env.render_str(template, value)?)
    }

    fn render_named_inner(
        &self,
        name: &str,
        data: &serde_json::Value,
    ) -> Result<String, RenderError> {
        let tmpl = self.env.get_template(name)?;
        let value = Value::from_serialize(data);
        Ok(tmpl.render(value)?)
    }

    fn render_with_context_inner(
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
}

/// Registers standout's custom filters with a MiniJinja environment.
///
/// This is called automatically by [`MiniJinjaEngine::new`]. If you're using
/// the environment directly, call this to get standout's filters.
pub fn register_filters(env: &mut Environment<'static>) {
    register_filters_with_policy(env, AmbiguousWidth::Narrow);
}

/// Registers Standout's custom filters with an explicit ambiguous-width policy.
pub fn register_filters_with_policy(env: &mut Environment<'static>, policy: AmbiguousWidth) {
    register_filters_with_source(env, RenderWidthSource::new(policy));
}

fn register_filters_with_source(env: &mut Environment<'static>, widths: RenderWidthSource) {
    use minijinja::{Error, ErrorKind};

    spelling::install(env);

    // Newline filter
    env.add_filter("nl", |value: Value| -> String {
        format!("{}\n", stringify(&value))
    });

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
    crate::tabular::filters::register_tabular_filters_with_source(env, widths);
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

    /// The engine's thread-affinity is a claim its docstring makes, so the
    /// compiler is made to check it: filter width state is scoped per render
    /// without a mutex, and a shared engine sent across threads would race it.
    ///
    /// `Probe`'s inherent method exists only when `T: Send`, and an inherent
    /// method wins over a trait method of the same name — so resolution lands on
    /// the trait's `false` exactly when the bound does not hold.
    #[test]
    fn minijinja_engine_is_neither_send_nor_sync() {
        struct Probe<T>(PhantomData<T>);

        trait NotSend {
            fn is_send(&self) -> bool {
                false
            }
        }
        impl<T> NotSend for Probe<T> {}
        impl<T: Send> Probe<T> {
            fn is_send(&self) -> bool {
                true
            }
        }

        trait NotSync {
            fn is_sync(&self) -> bool {
                false
            }
        }
        impl<T> NotSync for Probe<T> {}
        impl<T: Sync> Probe<T> {
            fn is_sync(&self) -> bool {
                true
            }
        }

        assert!(
            Probe::<String>(PhantomData).is_send(),
            "the probe detects a Send type, so a false below means something"
        );
        assert!(Probe::<String>(PhantomData).is_sync());

        assert!(
            !Probe::<MiniJinjaEngine>(PhantomData).is_send(),
            "a shared engine sent across threads would race per-render width state"
        );
        assert!(!Probe::<MiniJinjaEngine>(PhantomData).is_sync());
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

    #[test]
    fn width_policy_is_restored_after_a_filter_panics() {
        let mut engine = MiniJinjaEngine::new();
        engine
            .environment_mut()
            .add_filter("panic_now", |_value: Value| -> String {
                panic!("intentional filter panic")
            });

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = engine.render_template_with_width(
                "{{ '≈' | panic_now }}",
                &serde_json::Value::Null,
                AmbiguousWidth::Wide,
            );
        }));
        assert!(panic.is_err());

        // Width is restored after unwind; the default remains Narrow.
        assert_eq!(
            engine
                .render_template("{{ '≈' | display_width }}", &serde_json::Value::Null)
                .unwrap(),
            "1"
        );
    }

    #[test]
    fn named_and_context_renders_cross_the_same_policy_seam() {
        let mut engine = MiniJinjaEngine::new();
        engine
            .add_template("width", "{{ value | display_width }}")
            .unwrap();
        let data = serde_json::json!({ "value": "≈" });

        assert_eq!(
            engine
                .render_named_with_width("width", &data, AmbiguousWidth::Wide)
                .unwrap(),
            "2"
        );
        assert_eq!(
            engine
                .render_with_context_and_width(
                    "{{ value | display_width }}",
                    &serde_json::Value::Null,
                    HashMap::from([("value".to_string(), serde_json::json!("≈"))]),
                    AmbiguousWidth::Wide,
                )
                .unwrap(),
            "2"
        );
    }

    #[test]
    fn table_helpers_use_render_width_unless_explicitly_overridden() {
        let engine = MiniJinjaEngine::new();
        let data = serde_json::Value::Null;
        let tabular = r#"{% set t = tabular([{"width": "fill"}]) %}{{ t.row(["x"]) }}"#;
        let table = r#"{% set t = table([{"width": "fill"}]) %}{{ t.row(["x"]) }}"#;
        let explicit_tabular =
            r#"{% set t = tabular([{"width": "fill"}], width=23) %}{{ t.row(["x"]) }}"#;
        let explicit_table =
            r#"{% set t = table([{"width": "fill"}], width=23) %}{{ t.row(["x"]) }}"#;

        for template in [tabular, table] {
            let output = engine
                .render_template_with_render_widths(
                    template,
                    &data,
                    Some(37),
                    AmbiguousWidth::Narrow,
                )
                .unwrap();
            assert_eq!(output.chars().count(), 37);
        }

        for template in [explicit_tabular, explicit_table] {
            let output = engine
                .render_template_with_render_widths(
                    template,
                    &data,
                    Some(37),
                    AmbiguousWidth::Narrow,
                )
                .unwrap();
            assert_eq!(output.chars().count(), 23);
        }
    }

    #[test]
    fn table_helpers_fall_back_to_eighty_columns_without_a_render_width() {
        let engine = MiniJinjaEngine::new();
        let data = serde_json::Value::Null;

        for template in [
            r#"{% set t = tabular([{"width": "fill"}]) %}{{ t.row(["x"]) }}"#,
            r#"{% set t = table([{"width": "fill"}]) %}{{ t.row(["x"]) }}"#,
        ] {
            let output = engine
                .render_template_with_render_widths(template, &data, None, AmbiguousWidth::Narrow)
                .unwrap();
            assert_eq!(output.chars().count(), 80);
        }
    }
}
