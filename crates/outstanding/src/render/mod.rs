//! Template rendering engine.
//!
//! This module provides the core rendering functionality:
//!
//! - [`render`]: Simple rendering with automatic color detection
//! - [`render_with_output`]: Rendering with explicit output mode control
//! - [`render_or_serialize`]: Render templates or serialize to JSON
//! - [`Renderer`]: Pre-compiled template renderer for repeated use
//!
//! The rendering engine uses MiniJinja for template processing and
//! integrates with the style and theme systems.

mod filters;
mod functions;
mod renderer;

pub use functions::{render, render_or_serialize, render_with_output};
pub use renderer::Renderer;
