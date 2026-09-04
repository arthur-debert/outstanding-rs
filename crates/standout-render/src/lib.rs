//! Templating, theming, and adaptive color handling for styled terminal output.
//!
//! [`Theme`] maps names to styles that adapt to [`ColorMode`]; templates use
//! `[name]content[/name]` tags resolved against a theme. [`render_request`] is
//! the contract (owned [`RenderRequest`] in, bytes out); [`render`] and its
//! siblings are detect-then-call wrappers around it. [`Renderer`] compiles and
//! reuses templates for repeated rendering.

pub mod colorspace;
pub mod context;
pub mod diagnostics;
pub mod document;
mod embedded;
mod environment;
mod error;
pub mod file_loader;
pub mod output;
pub mod prelude;
mod projection;
mod request;
pub mod style;
pub mod tabular;
pub mod template;
pub mod theme;
mod util;
pub mod warnings;
pub mod width;

pub use error::RenderError;

pub use style::{
    parse_css, parse_stylesheet, ColorDef, StyleAttributes, StyleDefinition, StyleValidationError,
    StyleValue, Styles, StylesheetError, StylesheetRegistry, ThemeVariants,
    DEFAULT_MISSING_STYLE_INDICATOR, STYLESHEET_EXTENSIONS,
};

pub use theme::{ColorMode, IconDefinition, IconMode, IconSet, Theme};

pub use output::{
    open_output_file, write_binary_output, write_output, OutputDestination, Representation,
    StyleMode,
};
pub use projection::{
    CsvProjection, CsvProjectionBuilder, ProjectionError, StructuredOutputProjection,
};
pub use width::{AmbiguousWidth, WidthCalculator};

pub use request::{
    default_template_engine, render_request, render_request_split, ColorPolicy, RenderRequest,
    SharedTemplateEngine, TargetProperties, TemplateRef,
};

pub use template::{
    render, render_auto, render_auto_with_context, render_auto_with_engine, render_auto_with_spec,
    render_with_context, render_with_mode, render_with_output, render_with_vars, validate_template,
    walk_template_dir, MiniJinjaEngine, RegistryError, Renderer, ResolvedTemplate, TemplateEngine,
    TemplateFile, TemplateRegistry, TEMPLATE_EXTENSIONS,
};

pub use standout_bbparser::{
    TagTransform, UnknownTagBehavior, UnknownTagError, UnknownTagErrors, UnknownTagKind,
};

pub use diagnostics::TagResolution;
pub use document::{
    deserialize_document, result_entry, result_record, serialize_document, serialize_record_array,
};

pub use util::{
    csv_records, rgb_to_ansi256, rgb_to_truecolor, truncate_to_width,
    truncate_to_width_with_policy, write_csv,
};

pub use file_loader::{
    build_embedded_registry, extension_priority, resolve_in_map, strip_extension, walk_dir,
    FileRegistry, FileRegistryConfig, LoadError, LoadedEntry, LoadedFile,
};

pub use embedded::{
    EmbeddedSource, EmbeddedStyles, EmbeddedTemplates, StylesheetResource, TemplateResource,
};
