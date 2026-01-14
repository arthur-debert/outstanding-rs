//! Rendering prelude for convenient imports.
//!
//! This module re-exports the most commonly used types for rendering,
//! allowing you to import everything you need in one line:
//!
//! ```rust,ignore
//! use outstanding::rendering::prelude::*;
//!
//! let theme = Theme::new()
//!     .add("title", Style::new().bold());
//!
//! let output = render(
//!     "[title]{{ name }}[/title]",
//!     &data,
//!     &theme,
//! )?;
//! ```

// Core rendering functions

// Theme and styling

// Output control

// Re-export console::Style for convenience
