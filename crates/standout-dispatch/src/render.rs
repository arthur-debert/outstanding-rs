//! Render function abstraction.
//!
//! This module defines the contract between dispatch and renderers. The key design
//! principle is that dispatch is render-agnostic: it doesn't know about templates,
//! themes, output formats, or any rendering implementation details.
//!
//! # Design Rationale
//!
//! The render handler is a pluggable callback that the consuming framework provides.
//! This separation exists because:
//!
//! 1. Flexibility: Different applications may use different renderers (or none at all)
//! 2. Separation of concerns: Business logic (handlers) shouldn't know about presentation
//! 3. Runtime configuration: Format/theme decisions happen at runtime (from CLI args),
//!    not at compile time
//!
//! # The Closure Pattern
//!
//! Render handlers capture their context (format, theme, etc.) in a closure:
//!
//! ```rust,ignore
//! // At runtime, after parsing --output=json:
//! let format = extract_output_mode(&matches);
//! let theme = &app.theme;
//! let templates = &app.templates;
//!
//! // Create render handler with context baked in
//! let render_handler = from_fn(move |data| {
//!     render_with_format(templates, theme, format, data)
//! });
//! ```
//!
//! Dispatch calls `render_handler(data)` without knowing what's inside the closure.
//! All format/theme/template logic lives in the closure, created by the framework layer.
//!
//! # Thread Safety
//!
//! Two variants are provided:
//! - [`RenderFn`]: Thread-safe (`Send + Sync`), uses `Arc`
//! - [`LocalRenderFn`]: Single-threaded, uses `Rc<RefCell>`, allows `FnMut`

use std::sync::Arc;

/// The render function signature.
///
/// Takes handler data (as JSON) and returns formatted output. The render function
/// is a closure that captures all rendering context (format, theme, templates, etc.)
/// so dispatch doesn't need to know about any of it.
///
/// # Example
///
/// ```rust,ignore
/// // Framework creates render handler with context captured
/// let render_handler = from_fn(move |data| {
///     match format {
///         Format::Json => serde_json::to_string_pretty(data),
///         Format::Term => render_template(template, data, theme),
///         // ...
///     }
/// });
/// ```
pub type RenderFn = Arc<dyn Fn(&serde_json::Value) -> Result<String, RenderError> + Send + Sync>;

/// A local (non-Send) render function for single-threaded use.
///
/// Unlike [`RenderFn`], this uses `Rc<RefCell>` and allows `FnMut` closures,
/// enabling mutable state in the render handler without `Send + Sync` overhead.
pub type LocalRenderFn =
    std::rc::Rc<std::cell::RefCell<dyn FnMut(&serde_json::Value) -> Result<String, RenderError>>>;

/// Errors that can occur during rendering.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// Template rendering failed
    #[error("render error: {0}")]
    Render(String),

    /// Data serialization failed
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Other error
    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for RenderError {
    fn from(e: serde_json::Error) -> Self {
        RenderError::Serialization(e.to_string())
    }
}

/// Creates a render function from a closure.
///
/// This is the primary way to provide custom rendering logic. The closure
/// should capture any context it needs (format, theme, templates, etc.).
///
/// # Example
///
/// ```rust
/// use standout_dispatch::{from_fn, RenderError};
///
/// let render = from_fn(|data| {
///     Ok(serde_json::to_string_pretty(data)?)
/// });
/// ```
pub fn from_fn<F>(f: F) -> RenderFn
where
    F: Fn(&serde_json::Value) -> Result<String, RenderError> + Send + Sync + 'static,
{
    Arc::new(f)
}

/// Creates a local render function from a FnMut closure.
///
/// Use this when the render handler needs mutable state and doesn't need
/// to be thread-safe.
pub fn from_fn_mut<F>(f: F) -> LocalRenderFn
where
    F: FnMut(&serde_json::Value) -> Result<String, RenderError> + 'static,
{
    std::rc::Rc::new(std::cell::RefCell::new(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_from_fn_json() {
        let render = from_fn(|data| {
            serde_json::to_string_pretty(data)
                .map_err(|e| RenderError::Serialization(e.to_string()))
        });

        let data = json!({"name": "test"});
        let result = render(&data).unwrap();
        assert!(result.contains("\"name\": \"test\""));
    }

    #[test]
    fn test_from_fn_custom() {
        let render = from_fn(|data| {
            let name = data
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            Ok(format!("Hello, {}!", name))
        });

        let data = json!({"name": "world"});
        let result = render(&data).unwrap();
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_from_fn_mut() {
        let render = from_fn_mut(|data| Ok(data.to_string()));

        let data = json!({"key": "value"});
        let result = render.borrow_mut()(&data).unwrap();
        assert!(result.contains("key"));
    }

    #[test]
    fn test_render_error_from_serde() {
        let err: RenderError = serde_json::from_str::<serde_json::Value>("invalid")
            .unwrap_err()
            .into();
        assert!(matches!(err, RenderError::Serialization(_)));
    }
}
