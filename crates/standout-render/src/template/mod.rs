//! Two-pass template rendering with style tag processing.
//!
//! This module provides the core rendering pipeline that transforms templates
//! and data into styled terminal output. Templates are processed in two passes:
//! template engine for logic, then BBParser for style tags.
//!
//! ## Template Engines
//!
//! Two template engines are available:
//!
//! | Engine | Syntax | Features | Use When |
//! |--------|--------|----------|----------|
//! | [`MiniJinjaEngine`] | `{{ var }}` | Loops, conditionals, filters, includes | Full template logic needed |
//! | [`SimpleEngine`] | `{var}` | Variable substitution only | Simple output, smaller binary |
//!
//! ### MiniJinja (Default)
//!
//! Full-featured Jinja2-compatible engine. File extensions: `.jinja`, `.jinja2`, `.j2`
//!
//! ```text
//! [title]{{ name | upper }}[/title] has {{ count }} items
//! {% for item in items %}{{ item.name }}{% endfor %}
//! ```
//!
//! ### Simple Engine
//!
//! Lightweight format-string style substitution. File extension: `.stpl`
//!
//! ```text
//! [title]{name}[/title] has {count} items
//! {user.profile.email}
//! ```
//!
//! ## Two-Pass Rendering
//!
//! Templates are processed in two distinct passes, which is why style tags use
//! bracket notation (`[name]...[/name]`) instead of template syntax:
//!
//! Pass 1 - Template Engine: Variable substitution (and control flow for MiniJinja).
//! ```text
//! Template: [title]{{ name | upper }}[/title] has {{ count }} items
//! After:    [title]WIDGET[/title] has 42 items
//! ```
//!
//! Pass 2 - BBParser: Style tags converted to ANSI codes (or stripped).
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
//! | [`render`] | Simple case, let Standout auto-detect everything |
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
//! Use bracket notation for styling (works with both engines):
//! ```text
//! [title]{{ name }}[/title] - [muted]{{ description }}[/muted]  // MiniJinja
//! [title]{name}[/title] - [muted]{description}[/muted]          // Simple
//! ```
//!
//! Tags can nest, span multiple lines, and contain template logic. Unknown tags
//! degrade to unstyled inner text; use [`validate_template`] to catch typos at
//! startup or in tests.
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
//! Supported extensions: `.jinja`, `.jinja2`, `.j2`, `.stpl`, `.txt` (in priority order).
//!
//! ## Key Types
//!
//! - [`Renderer`]: Pre-compiled template renderer for repeated rendering
//! - [`TemplateRegistry`]: Template resolution from multiple sources
//! - [`TemplateEngine`]: Trait for pluggable template backends
//! - [`MiniJinjaEngine`]: Full-featured Jinja2 engine (default)
//! - [`SimpleEngine`]: Lightweight format-string engine
//! - [`validate_template`]: Check templates for unknown style tags
//!
//! ## See Also
//!
//! - [`crate::theme`]: Theme and style definitions
//! - [`crate::tabular`]: Column formatting utilities and template filters
//! - [`crate::context`]: Context injection for templates

mod engine;
pub mod filters;
mod functions;
mod load;
pub mod registry;
mod renderer;
mod simple;
pub mod spelling;

pub use engine::{register_filters, register_filters_with_policy, MiniJinjaEngine, TemplateEngine};
pub use functions::{
    apply_style_tags, render, render_auto, render_auto_with_context, render_auto_with_engine,
    render_auto_with_engine_split, render_auto_with_engine_split_inline,
    render_auto_with_engine_split_named, render_auto_with_spec, render_with_context,
    render_with_mode, render_with_output, render_with_vars, validate_template, RenderResult,
};
pub(crate) use load::load_inline_dependencies;
pub use load::load_named_template;
pub use registry::{
    walk_template_dir, RegistryError, ResolvedTemplate, TemplateFile, TemplateRegistry,
    TEMPLATE_EXTENSIONS,
};
pub use renderer::Renderer;
pub use simple::SimpleEngine;
pub use spelling::{new_environment, stringify};
