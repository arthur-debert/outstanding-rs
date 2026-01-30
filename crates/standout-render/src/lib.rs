//! # Standout Render - Styled Terminal Output Library
//!
//! `standout-render` provides a complete rendering system for styled terminal output,
//! including template processing, theming, and adaptive color support.
//!
//! This crate is the rendering foundation for the `standout` CLI framework, but can
//! be used independently for any application that needs rich terminal output.
//!
//! ## Core Concepts
//!
//! - [`Theme`]: Named collection of adaptive styles that respond to light/dark mode
//! - [`ColorMode`]: Light or dark color mode enum
//! - [`OutputMode`]: Control output formatting (Auto/Term/Text/TermDebug/Json/Yaml)
//! - Style syntax: Tag-based styling `[name]content[/name]`
//! - [`Renderer`]: Pre-compile templates for repeated rendering
//! - [`validate_template`]: Check templates for unknown style tags
//!
//! ## Quick Start
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
//! Use tag syntax `[name]content[/name]` for styling both static and dynamic content:
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
//! Themes are inherently adaptive. Individual styles can define mode-specific
//! variations that are automatically selected based on the user's OS color mode.
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
//! ## YAML-Based Themes
//!
//! Themes can be loaded from YAML files:
//!
//! ```rust
//! use standout_render::Theme;
//!
//! let theme = Theme::from_yaml(r#"
//! header:
//!   fg: cyan
//!   bold: true
//! panel:
//!   fg: gray
//!   light:
//!     fg: black
//!   dark:
//!     fg: white
//! title: header
//! "#).unwrap();
//! ```
//!
//! ## More Examples
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
pub mod context;
mod embedded;
mod error;
pub mod file_loader;
pub mod output;
pub mod prelude;
pub mod style;
pub mod tabular;
pub mod template;
pub mod theme;
mod util;

// Error type
pub use error::RenderError;

// Style module exports (including former stylesheet exports)
pub use style::{
    parse_css, parse_stylesheet, ColorDef, StyleAttributes, StyleDefinition, StyleValidationError,
    StyleValue, Styles, StylesheetError, StylesheetRegistry, ThemeVariants,
    DEFAULT_MISSING_STYLE_INDICATOR, STYLESHEET_EXTENSIONS,
};

// Theme module exports
pub use theme::{detect_color_mode, set_theme_detector, ColorMode, Theme};

// Output module exports
pub use output::{write_binary_output, write_output, OutputDestination, OutputMode};

// Render module exports
pub use template::{
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
    // Template registry
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
pub use standout_bbparser::{UnknownTagError, UnknownTagErrors, UnknownTagKind};

// Utility exports
pub use util::{flatten_json_for_csv, rgb_to_ansi256, rgb_to_truecolor, truncate_to_width};

// File loader exports
pub use file_loader::{
    build_embedded_registry, extension_priority, strip_extension, walk_dir, FileRegistry,
    FileRegistryConfig, LoadError, LoadedEntry, LoadedFile,
};

// Embedded source types (for macros)
pub use embedded::{
    EmbeddedSource, EmbeddedStyles, EmbeddedTemplates, StylesheetResource, TemplateResource,
};
