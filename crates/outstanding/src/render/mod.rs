//! Template rendering engine.
//!
//! This module provides the core rendering functionality:
//!
//! - [`render`]: Simple rendering with automatic color detection
//! - [`render_with_output`]: Rendering with explicit output mode control
//! - [`render_or_serialize`]: Render templates or serialize to JSON
//! - [`Renderer`]: Pre-compiled template renderer for repeated use
//! - [`TemplateRegistry`]: Template resolution from files and inline sources
//!
//! The rendering engine uses MiniJinja for template processing and
//! integrates with the style and theme systems.
//!
//! # File-Based Templates
//!
//! Templates can be loaded from the filesystem using [`TemplateRegistry`] and
//! the [`walk_template_dir`] function:
//!
//! ```rust,ignore
//! use outstanding::render::{TemplateRegistry, walk_template_dir};
//!
//! let files = walk_template_dir("./templates")?;
//! let mut registry = TemplateRegistry::new();
//! registry.add_from_files(files)?;
//!
//! let content = registry.get_content("config")?;
//! ```
//!
//! See the [`registry`] module for detailed documentation on template resolution,
//! extension priority, and collision handling.

pub(crate) mod filters;
mod functions;
pub mod registry;
mod renderer;

pub use functions::{
    render, render_or_serialize, render_or_serialize_with_context, render_or_serialize_with_spec,
    render_with_context, render_with_mode, render_with_output,
};
pub use registry::{
    walk_template_dir, RegistryError, ResolvedTemplate, TemplateFile, TemplateRegistry,
    TEMPLATE_EXTENSIONS,
};
pub use renderer::Renderer;
