//! # Standout Render - Styled Terminal Output Library
//!
//! `standout-render` combines templating, theming, and adaptive color handling so you
//! can ship consistent terminal UI without re-implementing styling utilities.
//! Although it powers the `standout` CLI framework, the crate is fully usable as a
//! stand-alone renderer for any Rust application that needs rich TTY output.
//!
//! ## Core Concepts
//!
//! - [`Theme`]: Named, adaptive styles that automatically respect [`ColorMode`]
//! - [`Renderer`]: Compile and reuse templates for fast repeated rendering
//! - [`validate_template`]: Surface typos or unknown tags before you ship templates
//! - [`OutputMode`]: Control how content is emitted (Auto/Term/Text/TermDebug/Json/Yaml)
//! - [`TargetProperties`]: Destination facts for one invocation (per-stream terminal and color)
//! - [`RenderRequest`]: Owned explicit input to [`render_request`]
//! - [`ColorPolicy`]: Resolved color axis on a [`RenderRequest`], independent of [`OutputMode`]
//! - [`TemplateRef`]: Named, inline, or absent template on a [`RenderRequest`]
//! - Style syntax: Tag-based `[name]content[/name]` markup for inline styling
//!
//! ## Quick Start
//!
//! Create a [`Theme`], pass serializable data, and call [`render`] to produce a styled
//! `String` that you can print, persist, or feed into other systems:
//!
//! ```rust
//! use standout_render::{render, Theme};
//! use console::Style;
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct Summary {
//!     title: String,
//!     total: usize,
//! }
//!
//! let theme = Theme::new()
//!     .add("title", Style::new().bold())
//!     .add("count", Style::new().cyan());
//!
//! let template = r#"
//! [title]{{ title }}[/title]
//! ---------------------------
//! Total items: [count]{{ total }}[/count]
//! "#;
//!
//! let output = render(
//!     template,
//!     &Summary { title: "Report".into(), total: 3 },
//!     &theme,
//! ).unwrap();
//! println!("{}", output);
//! ```
//!
//! ## Tag-Based Styling
//!
//! Templates use lightweight `[name]content[/name]` tags, so you can mix static text
//! and template variables without sprinkling manual `console::Style` calls. The
//! renderer resolves each tag to the appropriate entry in your [`Theme`]:
//!
//! ```rust
//! use standout_render::{render_with_output, Theme, OutputMode};
//! use console::Style;
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct Data { name: String, count: usize }
//!
//! let theme = Theme::new()
//!     .add("title", Style::new().bold())
//!     .add("count", Style::new().cyan());
//!
//! let template = r#"[title]Report[/title]: [count]{{ count }}[/count] items by {{ name }}"#;
//!
//! let output = render_with_output(
//!     template,
//!     &Data { name: "Alice".into(), count: 42 },
//!     &theme,
//!     OutputMode::Text,
//! ).unwrap();
//!
//! assert_eq!(output, "Report: 42 items by Alice");
//! ```
//!
//! ## Adaptive Themes (Light & Dark)
//!
//! Styles automatically adapt to the current [`ColorMode`]. Provide explicit
//! overrides for light and dark variants, or rely on a shared default when you
//! do not need per-mode differences:
//!
//! ```rust
//! use standout_render::Theme;
//! use console::Style;
//!
//! let theme = Theme::new()
//!     // Non-adaptive style (same in all modes)
//!     .add("header", Style::new().bold().cyan())
//!     // Adaptive style with light/dark variants
//!     .add_adaptive(
//!         "panel",
//!         Style::new(),                                  // Base
//!         Some(Style::new().fg(console::Color::Black)), // Light mode
//!         Some(Style::new().fg(console::Color::White)), // Dark mode
//!     );
//! ```
//!
//! ## CSS-Based Themes
//!
//! Ship themes alongside your application or allow users to bring their own. The
//! [`Theme::from_css`] helper loads named styles (and adaptive overrides) directly
//! from a CSS definition:
//!
//! ```rust
//! use standout_render::Theme;
//!
//! let theme = Theme::from_css(r#"
//! .header { color: cyan; font-weight: bold; }
//! .panel { color: gray; }
//! @media (prefers-color-scheme: light) {
//!     .panel { color: black; }
//! }
//! @media (prefers-color-scheme: dark) {
//!     .panel { color: white; }
//! }
//! "#).unwrap();
//! ```
//!
//! ## Renderer Example
//!
//! For larger applications, use [`Renderer`] to register templates once and render
//! them repeatedly without reparsing:
//!
//! ```rust
//! use standout_render::{Renderer, Theme};
//! use console::Style;
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct Entry { label: String, value: i32 }
//!
//! let theme = Theme::new()
//!     .add("label", Style::new().bold())
//!     .add("value", Style::new().green());
//!
//! let mut renderer = Renderer::new(theme).unwrap();
//! renderer.add_template("row", "[label]{{ label }}[/label]: [value]{{ value }}[/value]").unwrap();
//! let rendered = renderer.render("row", &Entry { label: "Count".into(), value: 42 }).unwrap();
//! assert_eq!(rendered, "Count: 42");
//! ```

// Internal modules
pub mod colorspace;
pub mod context;
pub mod diagnostics;
mod embedded;
mod environment;
mod error;
pub mod file_loader;
pub mod output;
pub mod prelude;
mod projection;
mod request;
pub mod style;
pub mod tabular;
pub mod template;
pub mod theme;
mod util;
pub mod warnings;
pub mod width;

// Error type
pub use error::RenderError;

// Style module exports (including former stylesheet exports)
pub use style::{
    parse_css, parse_stylesheet, ColorDef, StyleAttributes, StyleDefinition, StyleValidationError,
    StyleValue, Styles, StylesheetError, StylesheetRegistry, ThemeVariants,
    DEFAULT_MISSING_STYLE_INDICATOR, STYLESHEET_EXTENSIONS,
};

// Theme module exports
pub use theme::{ColorMode, IconDefinition, IconMode, IconSet, Theme};

// Output module exports
pub use output::{write_binary_output, write_output, OutputDestination, OutputMode};
pub use projection::{
    CsvProjection, CsvProjectionBuilder, ProjectionError, StructuredOutputProjection,
};
pub use width::{AmbiguousWidth, WidthCalculator};

// Composition-contract types (explicit request; detect at the crate edge)
pub use request::{
    render_request, render_request_split, ColorPolicy, RenderRequest, SharedTemplateEngine,
    TargetProperties, TemplateRef,
};

// Render module exports
pub use template::{
    // Template registry
    load_named_template,
    render,
    render_auto,
    render_auto_with_context,
    render_auto_with_engine,
    render_auto_with_spec,
    render_with_context,
    render_with_mode,
    render_with_output,
    render_with_vars,
    validate_template,
    walk_template_dir,
    // Template engine abstraction
    MiniJinjaEngine,
    RegistryError,
    Renderer,
    ResolvedTemplate,
    TemplateEngine,
    TemplateFile,
    TemplateRegistry,
    TEMPLATE_EXTENSIONS,
};

// Re-export BBParser types for template validation
pub use standout_bbparser::{
    TagTransform, UnknownTagBehavior, UnknownTagError, UnknownTagErrors, UnknownTagKind,
};

// What the style-tag pass observed, for callers observing a run
pub use diagnostics::TagResolution;

// Utility exports
pub use util::{
    flatten_json_for_csv, rgb_to_ansi256, rgb_to_truecolor, serialize_to_xml, truncate_to_width,
    truncate_to_width_with_policy,
};

// File loader exports
pub use file_loader::{
    build_embedded_registry, extension_priority, resolve_in_map, strip_extension, walk_dir,
    FileRegistry, FileRegistryConfig, LoadError, LoadedEntry, LoadedFile,
};

// Embedded source types (for macros)
pub use embedded::{
    EmbeddedSource, EmbeddedStyles, EmbeddedTemplates, StylesheetResource, TemplateResource,
};
