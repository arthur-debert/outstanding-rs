use super::output::{Representation, StyleMode};
use super::theme::Theme;
use crate::AmbiguousWidth;
use standout_types::RenderData;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct RenderContext<'a> {
    pub representation: Representation,

    pub style: StyleMode,

    pub terminal_width: Option<usize>,

    pub theme: &'a Theme,

    /// The request's serialized data: the handler's render output for a command,
    /// a request-specific shape for help, artifact reports, and direct renders.
    pub data: &'a standout_types::RenderData,

    pub extras: HashMap<String, String>,

    pub warnings: Option<crate::warnings::WarningBuffer>,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        representation: Representation,
        style: StyleMode,
        terminal_width: Option<usize>,
        theme: &'a Theme,
        data: &'a standout_types::RenderData,
    ) -> Self {
        Self::with_ambiguous_width(
            representation,
            style,
            terminal_width,
            AmbiguousWidth::Narrow,
            theme,
            data,
        )
    }

    pub fn with_ambiguous_width(
        representation: Representation,
        style: StyleMode,
        terminal_width: Option<usize>,
        ambiguous_width: AmbiguousWidth,
        theme: &'a Theme,
        data: &'a standout_types::RenderData,
    ) -> Self {
        let extras = match ambiguous_width {
            AmbiguousWidth::Narrow => HashMap::new(),
            AmbiguousWidth::Wide => {
                HashMap::from([("standout.ambiguous_width".to_string(), "wide".to_string())])
            }
        };
        Self {
            representation,
            style,
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

    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extras.insert(key.into(), value.into());
        self
    }

    pub fn get_extra(&self, key: &str) -> Option<&str> {
        self.extras.get(key).map(|s| s.as_str())
    }
}

pub trait ContextProvider {
    fn provide(&self, ctx: &RenderContext) -> RenderData;
}

impl<F> ContextProvider for F
where
    F: Fn(&RenderContext) -> RenderData,
{
    fn provide(&self, ctx: &RenderContext) -> RenderData {
        (self)(ctx)
    }
}

#[derive(Debug, Clone)]
pub struct StaticProvider {
    value: RenderData,
}

impl StaticProvider {
    pub fn new(value: RenderData) -> Self {
        Self { value }
    }
}

impl ContextProvider for StaticProvider {
    fn provide(&self, _ctx: &RenderContext) -> RenderData {
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

    pub fn add_static(&mut self, name: impl Into<String>, value: RenderData) {
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

    pub fn resolve(&self, ctx: &RenderContext) -> HashMap<String, RenderData> {
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

    fn test_context() -> (Theme, standout_types::RenderData) {
        (Theme::new(), crate::test_data!({"test": true}))
    }

    #[test]
    fn render_context_new() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Ansi,
            Some(80),
            &theme,
            &data,
        );

        assert_eq!(ctx.representation, Representation::Human);
        assert_eq!(ctx.style, StyleMode::Ansi);
        assert_eq!(ctx.terminal_width, Some(80));
        assert!(ctx.extras.is_empty());
    }

    #[test]
    fn render_context_with_extras() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(Representation::Human, StyleMode::Plain, None, &theme, &data)
            .with_extra("key1", "value1")
            .with_extra("key2", "value2");

        assert_eq!(ctx.get_extra("key1"), Some("value1"));
        assert_eq!(ctx.get_extra("key2"), Some("value2"));
        assert_eq!(ctx.get_extra("missing"), None);
    }

    #[test]
    fn static_provider() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(Representation::Human, StyleMode::Plain, None, &theme, &data);

        let provider = StaticProvider::new(RenderData::from(42));
        let result = provider.provide(&ctx);

        assert_eq!(result, RenderData::from(42));
    }

    #[test]
    fn closure_provider() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Ansi,
            Some(120),
            &theme,
            &data,
        );

        let provider = |ctx: &RenderContext| -> RenderData {
            RenderData::from(ctx.terminal_width.unwrap_or(80) as u64)
        };

        let result = provider.provide(&ctx);
        assert_eq!(result, RenderData::from(120));
    }

    #[test]
    fn context_registry_add_static() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(Representation::Human, StyleMode::Plain, None, &theme, &data);

        let mut registry = ContextRegistry::new();
        registry.add_static("version", RenderData::from("1.0.0"));

        let resolved = registry.resolve(&ctx);
        assert_eq!(resolved.get("version"), Some(&RenderData::from("1.0.0")));
    }

    #[test]
    fn context_registry_add_provider() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Ansi,
            Some(100),
            &theme,
            &data,
        );

        let mut registry = ContextRegistry::new();
        registry.add_provider("width", |ctx: &RenderContext| {
            RenderData::from(ctx.terminal_width.unwrap_or(80) as u64)
        });

        let resolved = registry.resolve(&ctx);
        assert_eq!(resolved.get("width"), Some(&RenderData::from(100)));
    }

    #[test]
    fn context_registry_multiple_entries() {
        let (theme, data) = test_context();
        let ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Ansi,
            Some(120),
            &theme,
            &data,
        );

        let mut registry = ContextRegistry::new();
        registry.add_static("app", RenderData::from("myapp"));
        registry.add_provider("terminal_width", |ctx: &RenderContext| {
            RenderData::from(ctx.terminal_width.unwrap_or(80) as u64)
        });

        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());

        let resolved = registry.resolve(&ctx);
        assert_eq!(resolved.get("app"), Some(&RenderData::from("myapp")));
        assert_eq!(resolved.get("terminal_width"), Some(&RenderData::from(120)));
    }

    #[test]
    fn context_registry_names() {
        let mut registry = ContextRegistry::new();
        registry.add_static("foo", RenderData::from(1));
        registry.add_static("bar", RenderData::from(2));

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
    fn provider_uses_the_style_mode() {
        let (theme, data) = test_context();

        let provider =
            |ctx: &RenderContext| -> RenderData { RenderData::from(format!("{:?}", ctx.style)) };

        let ctx_term =
            RenderContext::new(Representation::Human, StyleMode::Ansi, None, &theme, &data);
        assert_eq!(provider.provide(&ctx_term), RenderData::from("Ansi"));

        let ctx_text =
            RenderContext::new(Representation::Human, StyleMode::Plain, None, &theme, &data);
        assert_eq!(provider.provide(&ctx_text), RenderData::from("Plain"));
    }

    #[test]
    fn provider_uses_data() {
        let theme = Theme::new();
        let data = crate::test_data!({"count": 42});
        let ctx = RenderContext::new(Representation::Human, StyleMode::Plain, None, &theme, &data);

        let provider = |ctx: &RenderContext| -> RenderData {
            let count = ctx.data.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
            RenderData::from(count * 2)
        };

        assert_eq!(provider.provide(&ctx), RenderData::from(84));
    }
}
