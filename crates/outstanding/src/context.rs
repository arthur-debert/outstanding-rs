//! Context injection for template rendering.
//!
//! This module provides types for injecting additional context objects into templates
//! beyond the handler's serialized data. This enables templates to access utilities,
//! formatters, and runtime-computed values that cannot be represented as JSON.
//!
//! # Overview
//!
//! The context injection system has two main components:
//!
//! 1. **[`RenderContext`]**: Information available at render time (output mode, terminal
//!    width, theme, etc.)
//! 2. **[`ContextProvider`]**: Trait for objects that can produce context values, either
//!    statically or dynamically based on `RenderContext`
//!
//! # Use Cases
//!
//! - **Table formatters**: Inject `TableFormatter` instances with resolved terminal width
//! - **Terminal info**: Provide `terminal.width`, `terminal.is_tty` to templates
//! - **Environment**: Expose environment variables or paths
//! - **User preferences**: Date formats, timezone, locale
//! - **Utilities**: Custom formatters, validators callable from templates
//!
//! # Example
//!
//! ```rust,ignore
//! use outstanding::context::{RenderContext, ContextProvider};
//! use minijinja::value::Object;
//! use std::sync::Arc;
//!
//! // A simple context object
//! struct TerminalInfo {
//!     width: usize,
//!     is_tty: bool,
//! }
//!
//! impl Object for TerminalInfo {
//!     fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
//!         match key.as_str()? {
//!             "width" => Some(minijinja::Value::from(self.width)),
//!             "is_tty" => Some(minijinja::Value::from(self.is_tty)),
//!             _ => None,
//!         }
//!     }
//! }
//!
//! // Create a dynamic provider using a closure
//! let provider = |ctx: &RenderContext| TerminalInfo {
//!     width: ctx.terminal_width.unwrap_or(80),
//!     is_tty: ctx.output_mode == OutputMode::Term,
//! };
//! ```

use crate::output::OutputMode;
use crate::theme::Theme;
use minijinja::Value;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

/// Information available at render time for dynamic context providers.
///
/// This struct is passed to [`ContextProvider::provide`] to allow context objects
/// to be configured based on runtime conditions.
///
/// # Fields
///
/// - `output_mode`: The current output mode (Term, Text, Json, etc.)
/// - `terminal_width`: Terminal width in columns, if known
/// - `theme`: The theme being used for rendering
/// - `data`: The handler's output data as a JSON value
/// - `extras`: Additional string key-value pairs for extension
///
/// # Example
///
/// ```rust
/// use outstanding::context::RenderContext;
/// use outstanding::{OutputMode, Theme};
///
/// let ctx = RenderContext {
///     output_mode: OutputMode::Term,
///     terminal_width: Some(120),
///     theme: &Theme::new(),
///     data: &serde_json::json!({"count": 42}),
///     extras: std::collections::HashMap::new(),
/// };
///
/// // Use context to configure a formatter
/// let width = ctx.terminal_width.unwrap_or(80);
/// ```
#[derive(Debug, Clone)]
pub struct RenderContext<'a> {
    /// The output mode for rendering (Term, Text, Json, etc.)
    pub output_mode: OutputMode,

    /// Terminal width in columns, if available.
    ///
    /// This is `None` when:
    /// - Output is not to a terminal (piped, redirected)
    /// - Terminal width cannot be determined
    /// - Running in a non-TTY environment
    pub terminal_width: Option<usize>,

    /// The theme being used for rendering.
    pub theme: &'a Theme,

    /// The handler's output data, serialized as JSON.
    ///
    /// This allows context providers to inspect the data being rendered
    /// and adjust their behavior accordingly.
    pub data: &'a serde_json::Value,

    /// Additional string key-value pairs for extension.
    ///
    /// This allows passing arbitrary metadata to context providers
    /// without modifying the struct definition.
    pub extras: HashMap<String, String>,
}

impl<'a> RenderContext<'a> {
    /// Creates a new render context with the given parameters.
    pub fn new(
        output_mode: OutputMode,
        terminal_width: Option<usize>,
        theme: &'a Theme,
        data: &'a serde_json::Value,
    ) -> Self {
        Self {
            output_mode,
            terminal_width,
            theme,
            data,
            extras: HashMap::new(),
        }
    }

    /// Adds an extra key-value pair to the context.
    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extras.insert(key.into(), value.into());
        self
    }

    /// Gets an extra value by key.
    pub fn get_extra(&self, key: &str) -> Option<&str> {
        self.extras.get(key).map(|s| s.as_str())
    }
}

/// Trait for types that can provide context objects for template rendering.
///
/// Context providers are called at render time to produce objects that will
/// be available in templates. They receive a [`RenderContext`] with information
/// about the current render environment.
///
/// # Static vs Dynamic Providers
///
/// - **Static providers**: Return the same object regardless of context
/// - **Dynamic providers**: Use context to configure the returned object
///
/// # Implementing for Closures
///
/// A blanket implementation is provided for closures, making it easy to
/// create dynamic providers:
///
/// ```rust,ignore
/// use outstanding::context::{RenderContext, ContextProvider};
///
/// // Closure-based provider
/// let provider = |ctx: &RenderContext| MyObject {
///     width: ctx.terminal_width.unwrap_or(80),
/// };
/// ```
///
/// # Thread Safety
///
/// All context providers must be `Send + Sync` to support concurrent rendering.
pub trait ContextProvider: Send + Sync {
    /// Produce a context object for the given render context.
    ///
    /// The returned value will be made available in templates under the
    /// name specified when registering the provider.
    fn provide(&self, ctx: &RenderContext) -> Value;
}

/// Blanket implementation for closures that return values convertible to minijinja::Value.
impl<F> ContextProvider for F
where
    F: Fn(&RenderContext) -> Value + Send + Sync,
{
    fn provide(&self, ctx: &RenderContext) -> Value {
        (self)(ctx)
    }
}

/// A static context provider that always returns the same value.
///
/// This is used internally for `.context(name, value)` calls where
/// the value doesn't depend on render context.
#[derive(Debug, Clone)]
pub struct StaticProvider {
    value: Value,
}

impl StaticProvider {
    /// Creates a new static provider with the given value.
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

impl ContextProvider for StaticProvider {
    fn provide(&self, _ctx: &RenderContext) -> Value {
        self.value.clone()
    }
}

/// Storage for context entries, supporting both static and dynamic providers.
///
/// `ContextRegistry` is cheap to clone since it stores providers as `Arc`.
#[derive(Default, Clone)]
pub struct ContextRegistry {
    providers: HashMap<String, Arc<dyn ContextProvider>>,
}

impl ContextRegistry {
    /// Creates a new empty context registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a static context value.
    ///
    /// The value will be available in templates under the given name.
    pub fn add_static(&mut self, name: impl Into<String>, value: Value) {
        self.providers
            .insert(name.into(), Arc::new(StaticProvider::new(value)));
    }

    /// Registers a dynamic context provider.
    ///
    /// The provider will be called at render time to produce a value.
    pub fn add_provider<P: ContextProvider + 'static>(
        &mut self,
        name: impl Into<String>,
        provider: P,
    ) {
        self.providers.insert(name.into(), Arc::new(provider));
    }

    /// Returns true if the registry has no entries.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// Returns the number of registered context entries.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Resolves all context providers into values for the given render context.
    ///
    /// Returns a map of names to values that can be merged into the template context.
    pub fn resolve(&self, ctx: &RenderContext) -> HashMap<String, Value> {
        self.providers
            .iter()
            .map(|(name, provider)| (name.clone(), provider.provide(ctx)))
            .collect()
    }

    /// Gets the names of all registered context entries.
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
