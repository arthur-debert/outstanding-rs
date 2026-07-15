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
//! 2. Separation of concerns: Handler adapters shouldn't know about presentation
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
//! # Single-Threaded Design
//!
//! CLI applications are single-threaded, so render functions use `Rc<RefCell>`
//! and accept `FnMut` closures for flexible mutable state handling.

use std::cell::RefCell;
use std::rc::Rc;

/// The render function signature.
///
/// Takes handler data (as JSON) and returns formatted output. The render function
/// is a closure that captures all rendering context (format, theme, templates, etc.)
/// so dispatch doesn't need to know about any of it.
///
/// Uses `Rc<RefCell>` since CLI applications are single-threaded, and accepts
/// `FnMut` closures for flexible mutable state handling.
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
pub type RenderFn = Rc<RefCell<dyn FnMut(&serde_json::Value) -> Result<String, RenderError>>>;

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
/// Accepts `FnMut` closures, allowing mutable state in the render handler.
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
    F: FnMut(&serde_json::Value) -> Result<String, RenderError> + 'static,
{
    Rc::new(RefCell::new(f))
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
        let result = render.borrow_mut()(&data).unwrap();
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
        let result = render.borrow_mut()(&data).unwrap();
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_from_fn_mutable_state() {
        let mut call_count = 0;
        let render = from_fn(move |data| {
            call_count += 1;
            Ok(format!("Call {}: {}", call_count, data))
        });

        let data = json!({"key": "value"});
        let result1 = render.borrow_mut()(&data).unwrap();
        let result2 = render.borrow_mut()(&data).unwrap();
        assert!(result1.contains("Call 1"));
        assert!(result2.contains("Call 2"));
    }

    #[test]
    fn test_render_error_from_serde() {
        let err: RenderError = serde_json::from_str::<serde_json::Value>("invalid")
            .unwrap_err()
            .into();
        assert!(matches!(err, RenderError::Serialization(_)));
    }
}
