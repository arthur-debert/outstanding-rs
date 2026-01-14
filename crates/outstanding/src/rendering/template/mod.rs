//! Two-pass template rendering with style tag processing.
//!
//! This module provides the core rendering pipeline that transforms templates
//! and data into styled terminal output. Templates are processed in two passes:
//! MiniJinja for logic, then BBParser for style tags.
//!
//! ## Two-Pass Rendering
//!
//! Templates are processed in two distinct passes, which is why style tags use
//! bracket notation (`[name]...[/name]`) instead of Jinja syntax:
//!
//! **Pass 1 - MiniJinja**: Variable substitution, control flow, filters.
//! ```text
//! Template: [title]{{ name | upper }}[/title] has {{ count }} items
//! After:    [title]WIDGET[/title] has 42 items
//! ```
//!
//! **Pass 2 - BBParser**: Style tags converted to ANSI codes (or stripped).
//! ```text
//! Input:  [title]WIDGET[/title] has 42 items
//! Output: \x1b[1;32mWIDGET\x1b[0m has 42 items
//! ```
//!
//! This separation keeps template logic independent from styling concerns.
//!
//! ## Which Render Function?
//!
//! Choose based on your needs:
//!
//! | Function | Use When |
//! |----------|----------|
//! | [`render`] | Simple case, let Outstanding auto-detect everything |
//! | [`render_with_output`] | Honoring `--output` flag (Term/Text/Auto) |
//! | [`render_with_mode`] | Full control over output mode AND color mode |
//! | [`render_auto`] | CLI with `--output=json` support (skips template for structured modes) |
//!
//! The "auto" in [`render_auto`] refers to template-vs-serialization dispatch,
//! not color detection. Structured modes (JSON, YAML, XML, CSV) serialize data
//! directly, skipping the template entirely.
//!
//! ## Style Tags in Templates
//!
//! Use bracket notation for styling:
//! ```jinja
//! [title]{{ name }}[/title] - [muted]{{ description }}[/muted]
//! ```
//!
//! Tags can nest, span multiple lines, and contain template logic. Unknown tags
//! show a `?` marker (`[unknown?]text[/unknown?]`) — use [`validate_template`]
//! to catch typos at startup or in tests.
//!
//! ## Template Registry
//!
//! For file-based templates, use [`TemplateRegistry`]:
//!
//! ```rust,ignore
//! let mut registry = TemplateRegistry::new();
//! registry.add_from_files(walk_template_dir("./templates")?)?;
//! let content = registry.get_content("config")?;
//! ```
//!
//! Resolution priority: inline templates → embedded (compile-time) → file-based.
//! Supported extensions: `.jinja`, `.jinja2`, `.j2`, `.txt` (in priority order).
//!
//! ## Key Types
//!
//! - [`Renderer`]: Pre-compiled template renderer for repeated rendering
//! - [`TemplateRegistry`]: Template resolution from multiple sources
//! - [`validate_template`]: Check templates for unknown style tags
//!
//! ## See Also
//!
//! - [`crate::theme`]: Theme and style definitions
//! - [`crate::table`]: Column formatting utilities and template filters
//! - [`crate::context`]: Context injection for templates

pub(crate) mod filters;
mod functions;
pub mod registry;
mod renderer;

pub use functions::{
    render, render_auto, render_auto_with_context, render_auto_with_spec, render_with_context,
    render_with_mode, render_with_output, render_with_vars, validate_template,
};
pub use registry::{
    walk_template_dir, RegistryError, ResolvedTemplate, TemplateFile, TemplateRegistry,
    TEMPLATE_EXTENSIONS,
};
pub use renderer::Renderer;
