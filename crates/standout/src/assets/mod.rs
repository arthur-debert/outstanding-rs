//! Framework-supplied assets (templates and styles).
//!
//! This module contains default templates and styles provided by the standout
//! framework. They serve as sensible defaults that can be overridden by user
//! templates with the same name.
//!
//! ## Namespacing
//!
//! - **Templates**: Use the `standout/` prefix (e.g., `standout/list-view`)
//! - **Styles**: Use the `standout-` prefix (e.g., `standout-muted`)
//!
//! ## Resolution Priority
//!
//! Framework assets have the lowest priority:
//! 1. User inline templates (highest)
//! 2. User file-based templates
//! 3. User embedded templates
//! 4. Framework templates (lowest)
//!
//! This allows users to override any framework default by creating a template
//! with the same name.

mod templates;

pub use templates::FRAMEWORK_TEMPLATES;

/// Framework style definitions.
///
/// These are basic semantic styles used by framework templates.
pub const FRAMEWORK_STYLES: &str = r#"
# Standout Framework Styles
# These can be overridden by user styles with the same name.

standout-muted:
  fg: gray

standout-error:
  fg: red

standout-warning:
  fg: yellow

standout-info:
  fg: blue

standout-success:
  fg: green

standout-header:
  bold: true
"#;
