//! # Outstanding - Non-Interactive CLI Framework
//!
//! Outstanding is a CLI output framework that decouples your application logic from
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
//! For clap integration, enable the `clap` feature and see the `cli` module.
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
//! use outstanding::{render, Theme};
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
//! use outstanding::{render_with_output, Theme, OutputMode};
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
//! use outstanding::Theme;
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
//! let output = outstanding::render(
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
//! use outstanding::Theme;
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
//! 4. Outstanding processes style tags, auto-detects colors, and returns the final string.
//!
//! Everything from the theme inward is pure Rust data: no code outside Outstanding needs
//! to touch stdout/stderr or ANSI escape sequences directly.
//!
//! ## More Examples
//!
//! ```rust
//! use outstanding::{Renderer, Theme};
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
//! use outstanding::topics::{Topic, TopicRegistry, TopicType, render_topic};
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
//! The `cli` module (requires `clap` feature) provides full clap integration with:
//! - Command dispatch with automatic template rendering
//! - Help command interception (`help`, `help <topic>`, `help topics`)
//! - Output flag injection (`--output=auto|term|text|json`)
//! - Styled help rendering
//!
//! ```rust,ignore
//! use clap::Command;
//! use outstanding::cli::{App, HandlerResult, Output};
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

// Internal modules
mod embedded;
pub mod file_loader;
mod output;
mod render;
mod setup;
mod style;
pub mod stylesheet;
mod theme;
mod util;

// Public submodules
pub mod context;
pub mod table;
pub mod topics;

// Re-export minijinja::Error for convenience
pub use minijinja::Error;

// Style module exports
pub use style::{StyleValidationError, StyleValue, Styles, DEFAULT_MISSING_STYLE_INDICATOR};

// Theme module exports
pub use theme::{detect_color_mode, set_theme_detector, ColorMode, Theme};

// Output module exports
pub use output::{write_binary_output, write_output, OutputDestination, OutputMode};

// Render module exports
pub use render::{
    render,
    render_auto,
    render_auto_with_context,
    render_auto_with_spec,
    render_with_context,
    render_with_mode,
    render_with_output,
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
pub use outstanding_bbparser::{UnknownTagError, UnknownTagErrors, UnknownTagKind};

// Utility exports
pub use util::{rgb_to_ansi256, rgb_to_truecolor, truncate_to_width};

// Embedded source types (for macros)
pub use embedded::{
    EmbeddedSource, EmbeddedStyles, EmbeddedTemplates, StylesheetResource, TemplateResource,
};

// Setup error type
pub use setup::SetupError;

// Macro re-exports (when `macros` feature is enabled)
#[cfg(feature = "macros")]
pub use outstanding_macros::{embed_styles, embed_templates};

// CLI integration (when `clap` feature is enabled)
#[cfg(feature = "clap")]
pub mod cli;
