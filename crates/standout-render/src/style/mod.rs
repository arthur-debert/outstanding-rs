//! Style system for named styles, aliases, and stylesheets.
//!
//! Stylesheets can be written in CSS (preferred) or YAML (legacy); the
//! [`StylesheetRegistry`] auto-detects the format from content when loading.
//! A [`StyleValue`] is either a concrete style or an alias to another named
//! style, and a style can carry `light`/`dark` overrides resolved by
//! [`ThemeVariants::resolve`] against the active [`ColorMode`](crate::ColorMode).
//!
//! ## YAML schema (legacy)
//!
//! ```yaml
//! header:
//!   fg: cyan
//!   bold: true
//!
//! bold_text: bold          # shorthand for a single attribute
//! warning: "yellow italic" # shorthand for multiple attributes
//!
//! panel:                   # adaptive: mode-specific overrides
//!   bg: gray
//!   light:
//!     bg: "#f5f5f5"
//!   dark:
//!     bg: "#1a1a1a"
//!
//! disabled: muted          # alias
//! ```
//!
//! ## Color formats
//!
//! ```yaml
//! fg: red               # named (16 ANSI colors)
//! fg: bright_yellow     # bright variants
//! fg: 208               # 256-color palette
//! fg: "#ff6b35"         # RGB hex
//! fg: [255, 107, 53]    # RGB tuple
//! ```

mod error;
mod registry;
mod value;

mod attributes;
mod color;
mod css_parser;
mod definition;
mod file_registry;
mod parser;

pub use error::{StyleValidationError, StylesheetError};
pub use registry::{Styles, DEFAULT_MISSING_STYLE_INDICATOR};
pub use value::StyleValue;

pub use attributes::StyleAttributes;
pub use color::ColorDef;
pub use css_parser::parse_css;
pub use definition::StyleDefinition;
pub(crate) use file_registry::parse_theme_content;
pub use file_registry::{StylesheetRegistry, STYLESHEET_EXTENSIONS};
pub use parser::{parse_stylesheet, ThemeVariants};
