//! Rendering prelude for convenient imports.
//!
//! This module re-exports the most commonly used types for rendering,
//! allowing you to import everything you need in one line:
//!
//! ```rust,ignore
//! use standout_render::rendering::prelude::*;
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
pub use crate::{render, render_request, render_with_output};

// Theme and styling
pub use crate::{ColorMode, IconMode, Theme};

// Output control
pub use crate::{ColorPolicy, OutputMode};

// Composition-contract types
pub use crate::{RenderRequest, TargetProperties, TemplateRef};

// Re-export console::Style for convenience
