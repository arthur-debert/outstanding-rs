//! Two-pass template rendering with style tag processing.
//!
//! Templates render in two passes: a template engine ([`MiniJinjaEngine`] for
//! full Jinja2 syntax, or [`SimpleEngine`] for `{var}`-only substitution) runs
//! first, then a bracket-tag pass converts `[name]...[/name]` style tags to
//! ANSI codes or strips them. Style tags use bracket notation rather than
//! either engine's own syntax so they survive both engines unmodified and the
//! styling pass stays independent of template logic. Tags can nest and span
//! multiple lines; unknown tags degrade to unstyled inner text — use
//! [`validate_template`] to catch typos at startup or in tests.
//!
//! Render entry points, in order of how much they take over: [`render`]
//! (auto-detects everything), [`render_with_output`] (honors `--output`),
//! [`render_with_mode`] (explicit output + color mode), [`render_auto`] (also
//! dispatches structured modes — JSON/YAML/XML/CSV — straight to
//! serialization, skipping the template entirely).
//!
//! [`TemplateRegistry`] resolves file-based templates with priority: inline →
//! embedded (compile-time) → file-based, across `.jinja`/`.jinja2`/`.j2`/
//! `.stpl`/`.txt`.
//!
//! See also [`crate::theme`] for style definitions, [`crate::tabular`] for
//! column filters, and [`crate::context`] for context injection.

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
pub(crate) use functions::{render_engine_split_inline, render_engine_split_named};
pub(crate) use load::{load_inline_dependencies, load_named_template, registry_error};
pub use registry::{
    walk_template_dir, RegistryError, ResolvedTemplate, TemplateFile, TemplateRegistry,
    TEMPLATE_EXTENSIONS,
};
pub use renderer::Renderer;
pub use simple::SimpleEngine;
pub use spelling::{new_environment, stringify};
