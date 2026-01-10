//! # Outstanding - Non-Interactive CLI Framework
//!
//! Outstanding is a CLI output framework that decouples your application logic from
//! terminal presentation. It provides:
//!
//! - **Template rendering** with MiniJinja + styled output
//! - **Themes** for named style definitions (colors, bold, etc.)
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
//! - [`Theme`]: Named collection of `console::Style` values (e.g., `"header"` → bold cyan)
//! - [`AdaptiveTheme`]: Light/dark theme pair with OS detection
//! - [`OutputMode`]: Control output formatting (Auto/Term/Text/TermDebug)
//! - [`topics`]: Help topics system for extended documentation
//! - `style` filter: `{{ value | style("name") }}` applies registered styles in templates
//! - [`Renderer`]: Pre-compile templates for repeated rendering
//!
//! ## Quick Start
//!
//! ```rust
//! use outstanding::{render, Theme, ThemeChoice};
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
//!     ThemeChoice::from(&theme),
//! ).unwrap();
//! println!("{}", output);
//! ```
//!
//! ## Adaptive Themes (Light & Dark)
//!
//! ```rust
//! use outstanding::{AdaptiveTheme, Theme, ThemeChoice, OutputMode};
//! use console::Style;
//!
//! let light = Theme::new().add("tone", Style::new().green());
//! let dark  = Theme::new().add("tone", Style::new().yellow().italic());
//! let adaptive = AdaptiveTheme::new(light, dark);
//!
//! // Automatically renders with the user's OS theme (via the `dark-light` crate)
//! let banner = outstanding::render_with_output(
//!     r#"Mode: {{ "active" | style("tone") }}"#,
//!     &serde_json::json!({}),
//!     ThemeChoice::Adaptive(&adaptive),
//!     OutputMode::Term,
//! ).unwrap();
//! ```
//!
//! ## Rendering Strategy
//!
//! 1. Build a [`Theme`] (or [`AdaptiveTheme`]) using the fluent `console::Style` API.
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
mod output;
mod render;
mod style;
mod theme;
mod util;

// Public submodules
pub mod table;
pub mod topics;

// Re-export minijinja::Error for convenience
pub use minijinja::Error;

// Style module exports
pub use style::{StyleValidationError, StyleValue, Styles, DEFAULT_MISSING_STYLE_INDICATOR};

// Theme module exports
pub use theme::{set_theme_detector, AdaptiveTheme, ColorMode, Theme, ThemeChoice};

// Output module exports
pub use output::OutputMode;

// Render module exports
pub use render::{render, render_or_serialize, render_with_output, Renderer};

// Utility exports
pub use util::{rgb_to_ansi256, rgb_to_truecolor, truncate_to_width};
