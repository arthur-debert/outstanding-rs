//! Injects extra objects into template rendering beyond the handler's
//! serialized data — table formatters, terminal info, environment values —
//! via [`ContextProvider`]s that receive a [`RenderContext`] and return a
//! MiniJinja [`Value`], either statically or computed from render-time
//! conditions (output mode, terminal width, theme, data).

use super::output::OutputMode;
use super::theme::Theme;
use crate::AmbiguousWidth;
use minijinja::Value;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct RenderContext<'a> {
    pub output_mode: OutputMode,

    pub terminal_width: Option<usize>,

    pub theme: &'a Theme,

    pub data: &'a serde_json::Value,

    pub extras: HashMap<String, String>,

    pub warnings: Option<crate::warnings::WarningBuffer>,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        output_mode: OutputMode,
        terminal_width: Option<usize>,
        theme: &'a Theme,
        data: &'a serde_json::Value,
    ) -> Self {
        Self::with_ambiguous_width(
            output_mode,
            terminal_width,
            AmbiguousWidth::Narrow,
            theme,
            data,
        )
    }

    pub fn with_ambiguous_width(
        output_mode: OutputMode,
        terminal_width: Option<usize>,
        ambiguous_width: AmbiguousWidth,
        theme: &'a Theme,
        data: &'a serde_json::Value,
    ) -> Self {
        let extras = match ambiguous_width {
            AmbiguousWidth::Narrow => HashMap::new(),
            AmbiguousWidth::Wide => {
                HashMap::from([("standout.ambiguous_width".to_string(), "wide".to_string())])
            }
        };
        Self {
            output_mode,
            terminal_width,
            theme,
            data,
            extras,
            warnings: None,
        }
    }

    pub fn ambiguous_width(&self) -> AmbiguousWidth {
        match self.get_extra("standout.ambiguous_width") {
            Some("wide") => AmbiguousWidth::Wide,
            _ => AmbiguousWidth::Narrow,
        }
    }

    /// The handler's serialized `Output::Render` payload for the command being
    /// rendered. A [`ContextProvider`] reads it to derive a template value from
    /// handler data the template does not name directly. Returns the same
    /// reference as the public `data` field.
    pub fn data(&self) -> &serde_json::Value {
        self.data
    }

    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extras.insert(key.into(), value.into());
        self
    }

    pub fn get_extra(&self, key: &str) -> Option<&str> {
        self.extras.get(key).map(|s| s.as_str())
    }
}

pub trait ContextProvider {
    fn provide(&self, ctx: &RenderContext) -> Value;
}

impl<F> ContextProvider for F
where
    F: Fn(&RenderContext) -> Value,
{
    fn provide(&self, ctx: &RenderContext) -> Value {
        (self)(ctx)
    }
}

#[derive(Debug, Clone)]
pub struct StaticProvider {
    value: Value,
}

impl StaticProvider {
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

impl ContextProvider for StaticProvider {
    fn provide(&self, _ctx: &RenderContext) -> Value {
        self.value.clone()
    }
}

#[derive(Default, Clone)]
pub struct ContextRegistry {
    providers: HashMap<String, Rc<dyn ContextProvider>>,
}

impl ContextRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_static(&mut self, name: impl Into<String>, value: Value) {
        self.providers
            .insert(name.into(), Rc::new(StaticProvider::new(value)));
    }

    pub fn add_provider<P: ContextProvider + 'static>(
        &mut self,
        name: impl Into<String>,
        provider: P,
    ) {
        self.providers.insert(name.into(), Rc::new(provider));
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn resolve(&self, ctx: &RenderContext) -> HashMap<String, Value> {
        self.providers
            .iter()
            .map(|(name, provider)| (name.clone(), provider.provide(ctx)))
            .collect()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.providers.keys().map(|s| s.as_str())
    }
}

impl std::fmt::Debug for ContextRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextRegistry")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;

    fn test_context() -> (Theme, serde_json::Value) {
        (Theme::new(), serde_json::json!({"test": true}))
    }

    #[test]
    fn render_context_new() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(OutputMode::Term, Some(80), &theme, &data);

        assert_eq!(ctx.output_mode, OutputMode::Term);
        assert_eq!(ctx.terminal_width, Some(80));
        assert!(ctx.extras.is_empty());
    }

    #[test]
    fn render_context_with_extras() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(OutputMode::Text, None, &theme, &data)
            .with_extra("key1", "value1")
            .with_extra("key2", "value2");

        assert_eq!(ctx.get_extra("key1"), Some("value1"));
        assert_eq!(ctx.get_extra("key2"), Some("value2"));
        assert_eq!(ctx.get_extra("missing"), None);
    }

    #[test]
    fn static_provider() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(OutputMode::Text, None, &theme, &data);

        let provider = StaticProvider::new(Value::from(42));
        let result = provider.provide(&ctx);

        assert_eq!(result, Value::from(42));
    }

    #[test]
    fn closure_provider() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(OutputMode::Term, Some(120), &theme, &data);

        let provider =
            |ctx: &RenderContext| -> Value { Value::from(ctx.terminal_width.unwrap_or(80)) };

        let result = provider.provide(&ctx);
        assert_eq!(result, Value::from(120));
    }

    #[test]
    fn context_registry_add_static() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(OutputMode::Text, None, &theme, &data);

        let mut registry = ContextRegistry::new();
        registry.add_static("version", Value::from("1.0.0"));

        let resolved = registry.resolve(&ctx);
        assert_eq!(resolved.get("version"), Some(&Value::from("1.0.0")));
    }

    #[test]
    fn context_registry_add_provider() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(OutputMode::Term, Some(100), &theme, &data);

        let mut registry = ContextRegistry::new();
        registry.add_provider("width", |ctx: &RenderContext| {
            Value::from(ctx.terminal_width.unwrap_or(80))
        });

        let resolved = registry.resolve(&ctx);
        assert_eq!(resolved.get("width"), Some(&Value::from(100)));
    }

    #[test]
    fn context_registry_multiple_entries() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(OutputMode::Term, Some(120), &theme, &data);

        let mut registry = ContextRegistry::new();
        registry.add_static("app", Value::from("myapp"));
        registry.add_provider("terminal_width", |ctx: &RenderContext| {
            Value::from(ctx.terminal_width.unwrap_or(80))
        });

        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());

        let resolved = registry.resolve(&ctx);
        assert_eq!(resolved.get("app"), Some(&Value::from("myapp")));
        assert_eq!(resolved.get("terminal_width"), Some(&Value::from(120)));
    }

    #[test]
    fn context_registry_names() {
        let mut registry = ContextRegistry::new();
        registry.add_static("foo", Value::from(1));
        registry.add_static("bar", Value::from(2));

        let names: Vec<&str> = registry.names().collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
    }

    #[test]
    fn context_registry_empty() {
        let registry = ContextRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn provider_uses_output_mode() {
        let (theme, data) = test_context();

        let provider =
            |ctx: &RenderContext| -> Value { Value::from(format!("{:?}", ctx.output_mode)) };

        let ctx_term = RenderContext::new(OutputMode::Term, None, &theme, &data);
        assert_eq!(provider.provide(&ctx_term), Value::from("Term"));

        let ctx_text = RenderContext::new(OutputMode::Text, None, &theme, &data);
        assert_eq!(provider.provide(&ctx_text), Value::from("Text"));
    }

    #[test]
    fn data_getter_returns_the_payload() {
        let theme = Theme::new();
        let data = serde_json::json!({"count": 42});
        let ctx = RenderContext::new(OutputMode::Text, None, &theme, &data);

        assert_eq!(ctx.data(), &data);
        assert_eq!(ctx.data().get("count").and_then(|v| v.as_i64()), Some(42));
    }

    #[test]
    fn provider_uses_data() {
        let theme = Theme::new();
        let data = serde_json::json!({"count": 42});
        let ctx = RenderContext::new(OutputMode::Text, None, &theme, &data);

        let provider = |ctx: &RenderContext| -> Value {
            let count = ctx.data.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
            Value::from(count * 2)
        };

        assert_eq!(provider.provide(&ctx), Value::from(84));
    }
}
