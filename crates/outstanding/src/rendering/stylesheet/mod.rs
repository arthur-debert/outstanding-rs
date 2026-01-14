//! File-based stylesheet parsing for themes.
//!
//! This module provides YAML-based stylesheet parsing that produces themes with
//! adaptive styles. Each style can define base attributes plus optional light/dark
//! mode overrides, enabling themes that adapt to the user's OS color mode.
//!
//! # Design Overview
//!
//! The stylesheet system cleanly separates themes and display modes:
//!
//! - **Themes** are named collections of styles (e.g., "darcula", "solarized")
//! - **Styles** are adaptive—individual styles define their mode-specific variations
//! - **Display modes** (light/dark) are resolved at the style level, not theme level
//!
//! This design eliminates the duplication inherent in separate light/dark themes
//! by allowing non-varying styles to be defined once while mode-specific styles
//! override only what differs.
//!
//! # YAML Schema
//!
//! ## Basic Styles
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
//! ```
//!
//! ## Adaptive Styles
//!
//! ```yaml
//! # Style with mode-specific overrides
//! panel:
//!   bg: gray           # Base background
//!   fg: black          # Base foreground
//!   light:
//!     bg: "#f5f5f5"    # Light mode override
//!   dark:
//!     bg: "#1a1a1a"    # Dark mode override
//!     fg: white        # Also override foreground
//! ```
//!
//! The `light:` and `dark:` sections merge onto the base—shared attributes stay
//! at the root, overrides are specified in mode sections.
//!
//! ## Aliases
//!
//! ```yaml
//! # Visual layer - concrete styles
//! muted:
//!   dim: true
//!
//! # Semantic layer - aliases
//! disabled: muted
//! timestamp: disabled
//! ```
//!
//! ## Color Formats
//!
//! ```yaml
//! # Named colors (16 ANSI)
//! error:
//!   fg: red
//!
//! # Bright variants
//! warning:
//!   fg: bright_yellow
//!
//! # 256-color palette
//! accent:
//!   fg: 208
//!
//! # RGB hex
//! brand:
//!   fg: "#ff6b35"
//!
//! # RGB tuple
//! highlight:
//!   fg: [255, 107, 53]
//! ```
//!
//! # Module Structure
//!
//! - `color`: Color value parsing (named, hex, RGB, 256-palette)
//! - `attributes`: Style attribute types and merging
//! - `definition`: Style definition enum (alias, shorthand, full)
//! - `parser`: Main parsing entry point and theme building
//! - `error`: Error types for parse failures
//!
//! # Example Usage
//!
//! ```rust
//! use outstanding::stylesheet::{parse_stylesheet, ThemeVariants};
//! use outstanding::ColorMode;
//!
//! let yaml = r#"
//! header:
//!   fg: cyan
//!   bold: true
//!
//! footer:
//!   dim: true
//!   light:
//!     fg: black
//!   dark:
//!     fg: white
//! "#;
//!
//! let variants = parse_stylesheet(yaml).unwrap();
//!
//! // Get styles for dark mode
//! let dark_styles = variants.resolve(Some(ColorMode::Dark));
//! ```

mod attributes;
mod color;
mod definition;
mod error;
mod parser;
mod registry;

pub use attributes::StyleAttributes;
pub use color::ColorDef;
pub use definition::StyleDefinition;
pub use error::StylesheetError;
pub use parser::{parse_stylesheet, ThemeVariants};
pub use registry::{StylesheetRegistry, STYLESHEET_EXTENSIONS};
