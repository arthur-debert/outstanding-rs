//! # Standout - Non-Interactive CLI Framework
//!
//! Standout is a CLI output framework that decouples your application logic from
//! terminal presentation. It provides:
//!
//! - **Template rendering** with MiniJinja + styled output
//! - **Adaptive themes** for named style definitions with light/dark mode support
//! - **Automatic terminal capability detection** (TTY, CLICOLOR, etc.)
//! - **Output mode control** (Auto/Term/Text/TermDebug)
//! - **Help topics system** for extended documentation
//! - **Pager support** for long content
//!
//! This crate is **CLI-agnostic** at its core - it doesn't care how you parse arguments.
//! For clap integration, see the [`cli`] module.
//!
//! ## Core Concepts
//!
//! - [`Theme`]: Named collection of adaptive styles that respond to light/dark mode
//! - [`ColorMode`]: Light or dark color mode enum
//! - [`OutputMode`]: Control output formatting (Auto/Term/Text/TermDebug)
//! - [`topics`]: Help topics system for extended documentation
//! - **Style syntax**: Tag-based styling `[name]content[/name]`
//! - [`Renderer`]: Pre-compile templates for repeated rendering
//! - [`validate_template`]: Check templates for unknown style tags
//!
//! ## Quick Start
//!
//! ```rust
//! use standout::{render, Theme};
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
//! use standout::{render_with_output, Theme, OutputMode};
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
//! Unknown tags show a `?` marker in terminal output: `[unknown?]text[/unknown?]`.
//! Use [`validate_template`] to catch typos during development.
//!
//! ## Adaptive Themes (Light & Dark)
//!
//! Themes are inherently adaptive. Individual styles can define mode-specific
//! variations that are automatically selected based on the user's OS color mode.
//!
//! ```rust
//! use standout::Theme;
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
//!
//! // Rendering automatically detects OS color mode
//! let output = standout::render(
//!     r#"[panel]active[/panel]"#,
//!     &serde_json::json!({}),
//!     &theme,
//! ).unwrap();
//! ```
//!
//! ## YAML-Based Themes
//!
//! Themes can also be loaded from YAML files, which is convenient for
//! UI designers who may not be Rust programmers.
//!
//! ```rust
//! use standout::Theme;
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
//! ## Rendering Strategy
//!
//! 1. Build a [`Theme`] using the fluent builder API or YAML.
//! 2. Load/define templates using regular MiniJinja syntax (`{{ value }}`, `{% for %}`, etc.)
//!    with tag-based styling (`[name]content[/name]`).
//! 3. Call [`render`] for ad-hoc rendering or create a [`Renderer`] if you have many templates.
//! 4. Standout processes style tags, auto-detects colors, and returns the final string.
//!
//! Everything from the theme inward is pure Rust data: no code outside Standout needs
//! to touch stdout/stderr or ANSI escape sequences directly.
//!
//! ## More Examples
//!
//! ```rust
//! use standout::{Renderer, Theme};
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
//!
//! ## Help Topics System
//!
//! The [`topics`] module provides a help topics system for extended documentation:
//!
//! ```rust
//! use standout::topics::{Topic, TopicRegistry, TopicType, render_topic};
//!
//! // Create and populate a registry
//! let mut registry = TopicRegistry::new();
//! registry.add_topic(Topic::new(
//!     "Storage",
//!     "Notes are stored in ~/.notes/\n\nEach note is a separate file.",
//!     TopicType::Text,
//!     Some("storage".to_string()),
//! ));
//!
//! // Render a topic
//! if let Some(topic) = registry.get_topic("storage") {
//!     let output = render_topic(topic, None).unwrap();
//!     println!("{}", output);
//! }
//!
//! // Load topics from a directory
//! registry.add_from_directory_if_exists("docs/topics").ok();
//! ```
//!
//! ## Integration with Clap
//!
//! The [`cli`] module provides full clap integration with:
//! - Command dispatch with automatic template rendering
//! - Help command interception (`help`, `help <topic>`, `help topics`)
//! - Output flag injection (`--output=auto|term|text|json`)
//! - Styled help rendering
//!
//! ```rust,ignore
//! use clap::Command;
//! use standout::cli::{App, HandlerResult, Output};
//!
//! // Simple parsing with styled help
//! let matches = App::parse(Command::new("my-app"));
//!
//! // Full application with command dispatch
//! App::builder()
//!     .command("list", |_m, _ctx| {
//!         Ok(Output::Render(json!({"items": ["a", "b"]})))
//!     }, "{% for item in items %}{{ item }}\n{% endfor %}")
//!     .build()?
//!     .run(cmd, std::env::args());
//! ```

// Internal modules (standout-specific)
mod setup;

// Public submodules
pub mod topics;

// Re-export everything from standout-render
// This provides the rendering layer without CLI knowledge
pub use standout_render::context;
pub use standout_render::file_loader;
pub use standout_render::style;
pub use standout_render::tabular;

// Re-export minijinja::Error for convenience
pub use minijinja::Error;

// Style module exports (from standout-render)
pub use standout_render::{
    parse_css, parse_stylesheet, ColorDef, StyleAttributes, StyleDefinition, StyleValidationError,
    StyleValue, Styles, StylesheetError, StylesheetRegistry, ThemeVariants,
    DEFAULT_MISSING_STYLE_INDICATOR, STYLESHEET_EXTENSIONS,
};

// Theme module exports (from standout-render)
pub use standout_render::{detect_color_mode, set_theme_detector, ColorMode, Theme};

// Output module exports (from standout-render)
pub use standout_render::{write_binary_output, write_output, OutputDestination, OutputMode};

// Render module exports (from standout-render)
pub use standout_render::{
    render,
    render_auto,
    render_auto_with_context,
    render_auto_with_spec,
    render_with_context,
    render_with_mode,
    render_with_output,
    render_with_vars,
    validate_template,
    // Template registry
    walk_template_dir,
    RegistryError,
    Renderer,
    ResolvedTemplate,
    TemplateFile,
    TemplateRegistry,
    TEMPLATE_EXTENSIONS,
};

// Re-export BBParser types for template validation
pub use standout_bbparser::{UnknownTagError, UnknownTagErrors, UnknownTagKind};

// Utility exports (from standout-render)
pub use standout_render::{
    flatten_json_for_csv, rgb_to_ansi256, rgb_to_truecolor, truncate_to_width,
};

// File loader exports (from standout-render)
pub use standout_render::{
    build_embedded_registry, extension_priority, strip_extension, walk_dir, FileRegistry,
    FileRegistryConfig, LoadError, LoadedEntry, LoadedFile,
};

// Embedded source types (from standout-render, for macros)
pub use standout_render::{
    EmbeddedSource, EmbeddedStyles, EmbeddedTemplates, StylesheetResource, TemplateResource,
};

// Setup error type (standout-specific)
pub use setup::SetupError;

// Macro re-exports
pub use standout_macros::{embed_styles, embed_templates};

// Tabular derive macros
pub use standout_macros::{Tabular, TabularRow};

// CLI integration
pub mod cli;
