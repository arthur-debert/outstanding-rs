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
//! This crate is **CLI-agnostic** - it doesn't care how you parse arguments.
//! For easy integration with clap, see the `outstanding-clap` crate.
//!
//! ## Core Concepts
//!
//! - [`Theme`]: Named collection of adaptive styles that respond to light/dark mode
//! - [`ColorMode`]: Light or dark color mode enum
//! - [`OutputMode`]: Control output formatting (Auto/Term/Text/TermDebug)
//! - [`topics`]: Help topics system for extended documentation
//! - `style` filter: `{{ value | style("name") }}` applies registered styles in templates
//! - [`Renderer`]: Pre-compile templates for repeated rendering
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
//! {{ title | style("title") }}
//! ---------------------------
//! Total items: {{ total | style("count") }}
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
//!     r#"{{ "active" | style("panel") }}"#,
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
//! ```rust,ignore
//! use outstanding::Theme;
//!
//! let theme = Theme::from_yaml(r#"
//! # Non-adaptive styles
//! header:
//!   fg: cyan
//!   bold: true
//!
//! # Adaptive style with light/dark overrides
//! panel:
//!   fg: gray
//!   light:
//!     fg: black
//!   dark:
//!     fg: white
//!
//! # Aliases
//! title: header
//! "#)?;
//! ```
//!
//! ## Rendering Strategy
//!
//! 1. Build a [`Theme`] using the fluent builder API or YAML.
//! 2. Load/define templates using regular MiniJinja syntax (`{{ value }}`, `{% for %}`, etc.).
//! 3. Call [`render`] for ad-hoc rendering or create a [`Renderer`] if you have many templates.
//! 4. Outstanding injects the `style` filter, auto-detects colors, and returns the final string.
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
//! renderer.add_template("row", "{{ label | style(\"label\") }}: {{ value | style(\"value\") }}").unwrap();
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
//! For clap-based CLIs, use the `outstanding-clap` crate which handles:
//! - Help command interception (`help`, `help <topic>`, `help topics`)
//! - Output flag injection (`--output=auto|term|text`)
//! - Styled help rendering
//!
//! ```rust,ignore
//! use clap::Command;
//! use outstanding_clap::Outstanding;
//!
//! // Simplest usage - all features enabled by default
//! let matches = Outstanding::run(Command::new("my-app"));
//! ```

// Internal modules
pub mod file_loader;
mod output;
mod render;
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
pub use output::OutputMode;

// Render module exports
pub use render::{
    render,
    render_or_serialize,
    render_or_serialize_with_context,
    render_or_serialize_with_spec,
    render_with_context,
    render_with_output,
    // Template registry
    walk_template_dir,
    RegistryError,
    Renderer,
    ResolvedTemplate,
    TemplateFile,
    TemplateRegistry,
    TEMPLATE_EXTENSIONS,
};

// Utility exports
pub use util::{rgb_to_ansi256, rgb_to_truecolor, truncate_to_width};
