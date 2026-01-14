//! Rendering subsystem for styled terminal output.
//!
//! This module provides all the machinery for transforming data and templates
//! into styled terminal output. It includes:
//!
//! - **Template processing** ([`template`]): MiniJinja-based template rendering
//!   with style tag processing
//! - **Theming** ([`theme`]): Adaptive themes with automatic light/dark mode support
//! - **Styles** ([`style`]): Style primitives, YAML parsing, and registries
//! - **Tables** ([`table`]): Unicode-aware column formatting
//! - **Output modes** ([`output`]): Terminal, text, JSON, YAML, etc.
//! - **Context injection** ([`context`]): Add values to template context
//!
//! ## Standalone Usage
//!
//! The rendering system is fully decoupled from CLI integration.
//! You can use it directly without the `cli` module:
//!
//! ```rust,ignore
//! use outstanding::rendering::{render, Theme, OutputMode};
//! use console::Style;
//!
//! let theme = Theme::new()
//!     .add("title", Style::new().bold().cyan());
//!
//! let output = render(
//!     "[title]{{ name }}[/title]",
//!     &data,
//!     &theme,
//! )?;
//! ```

pub mod context;
pub mod output;
pub mod prelude;
pub mod style;
pub mod table;
pub mod template;
pub mod theme;

// Re-export key types for convenience
