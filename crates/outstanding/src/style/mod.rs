//! Style system for named styles and aliases.
//!
//! This module provides the core styling primitives:
//!
//! - [`StyleValue`]: A style that can be either concrete or an alias
//! - [`Styles`]: A registry of named styles
//! - [`StyleValidationError`]: Errors from style validation
//!
//! Styles support a layered pattern where semantic styles can alias presentation
//! styles, which in turn alias visual styles with concrete formatting.

mod error;
mod registry;
mod value;

pub use error::StyleValidationError;
pub use registry::{Styles, DEFAULT_MISSING_STYLE_INDICATOR};
pub use value::StyleValue;
