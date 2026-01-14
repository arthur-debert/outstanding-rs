//! Style system for named styles, aliases, and YAML-based stylesheets.
//!
//! This module provides the complete styling infrastructure:
//!
//! ## Core Types
//!
//! - [`StyleValue`]: A style that can be either concrete or an alias
//! - [`Styles`]: A registry of named styles
//! - [`StyleValidationError`]: Errors from style validation
//!
//! ## YAML Stylesheet Parsing
//!
//! - [`parse_stylesheet`]: Parse YAML into theme variants
//! - [`ThemeVariants`]: Styles resolved for base/light/dark modes
//! - [`StylesheetRegistry`]: File-based theme management
//!
//! ## YAML Schema
//!
//! ```yaml
//! # Simple style with attributes
//! header:
//!   fg: cyan
//!   bold: true
//!
//! # Shorthand for single attribute
//! bold_text: bold
//! accent: cyan
//!
//! # Shorthand for multiple attributes
//! warning: "yellow italic"
//!
//! # Adaptive style with mode-specific overrides
//! panel:
//!   bg: gray
//!   light:
//!     bg: "#f5f5f5"
//!   dark:
//!     bg: "#1a1a1a"
//!
//! # Aliases
//! disabled: muted
//! ```
//!
//! ## Color Formats
//!
//! ```yaml
//! fg: red               # Named (16 ANSI colors)
//! fg: bright_yellow     # Bright variants
//! fg: 208               # 256-color palette
//! fg: "#ff6b35"         # RGB hex
//! fg: [255, 107, 53]    # RGB tuple
//! ```
//!
//! ## Example
//!
//! ```rust
//! use outstanding::style::{parse_stylesheet, ThemeVariants};
//! use outstanding::ColorMode;
//!
//! let yaml = r#"
//! header:
//!   fg: cyan
//!   bold: true
//! footer:
//!   dim: true
//!   light:
//!     fg: black
//!   dark:
//!     fg: white
//! "#;
//!
//! let variants = parse_stylesheet(yaml).unwrap();
//! let dark_styles = variants.resolve(Some(ColorMode::Dark));
//! ```

// Core style types
mod error;
mod registry;
mod value;

// YAML stylesheet parsing
mod attributes;
mod color;
mod definition;
mod file_registry;
mod parser;

// Core exports
pub use error::{StyleValidationError, StylesheetError};
pub use registry::{Styles, DEFAULT_MISSING_STYLE_INDICATOR};
pub use value::StyleValue;

// Stylesheet parsing exports
pub use attributes::StyleAttributes;
pub use color::ColorDef;
pub use definition::StyleDefinition;
pub use file_registry::{StylesheetRegistry, STYLESHEET_EXTENSIONS};
pub use parser::{parse_stylesheet, ThemeVariants};
