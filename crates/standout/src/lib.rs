//! Standout is a CLI output framework that decouples application logic from
//! terminal presentation: template rendering (MiniJinja + styled tag syntax),
//! adaptive light/dark themes, terminal capability detection, output-mode
//! control (Auto/Term/Text/TermDebug), help topics, and pager support. It is
//! CLI-agnostic at its core; for clap integration see the [`cli`] module.
//!
//! [`render_request`] is the contract: [`render`] and its siblings detect
//! [`TargetProperties`] at their edge, build a [`RenderRequest`], and
//! delegate.
//!
//! ```rust
//! use standout::{render, Theme};
//! use console::Style;
//! use serde_json::json;
//!
//! let theme = Theme::new().add("title", Style::new().bold());
//! let output = render("[title]{{ title }}[/title]", &json!({"title": "Report"}), &theme).unwrap();
//! ```

mod setup;

pub mod assets;
pub mod topics;
pub mod views;

pub use standout_render::context;
pub use standout_render::diagnostics;
pub use standout_render::style;
pub use standout_render::tabular;
pub use standout_render::warnings;
pub use standout_render::warnings::WarningBuffer;

pub use standout_render::RenderError;

pub use standout_render::{
    parse_css, parse_stylesheet, ColorDef, StyleAttributes, StyleDefinition, StyleValidationError,
    StyleValue, Styles, StylesheetError, StylesheetRegistry, ThemeVariants,
    DEFAULT_MISSING_STYLE_INDICATOR, STYLESHEET_EXTENSIONS,
};

pub use standout_render::{ColorMode, IconDefinition, IconMode, IconSet, Theme};

pub use standout_input::InputSources;
pub use standout_render::{
    default_template_engine, render_request, render_request_split, ColorPolicy, RenderRequest,
    SharedTemplateEngine, TargetProperties, TemplateRef,
};

pub use standout_render::{
    open_output_file, write_binary_output, write_output, OutputDestination, OutputMode,
};
pub use standout_render::{AmbiguousWidth, WidthCalculator};
pub use standout_render::{
    CsvProjection, CsvProjectionBuilder, ProjectionError, StructuredOutputProjection,
};

pub use standout_render::{
    render, render_auto, render_auto_with_context, render_with_context, render_with_mode,
    render_with_output, render_with_vars, MiniJinjaEngine, RegistryError, Renderer,
    ResolvedTemplate, TemplateEngine, TemplateFile, TemplateRegistry, TEMPLATE_EXTENSIONS,
};

pub use standout_bbparser::{
    TagTransform, UnknownTagBehavior, UnknownTagError, UnknownTagErrors, UnknownTagKind,
};

pub use standout_render::TagResolution;

pub use standout_render::truncate_to_width;

pub use standout_render::{
    EmbeddedSource, EmbeddedStyles, EmbeddedTemplates, StylesheetResource, TemplateResource,
};

pub use setup::SetupError;

pub use standout_macros::{embed_styles, embed_templates, handler};

pub use standout_macros::{Tabular, TabularRow};

pub use standout_dispatch::{ContractSurface, Envelope};
pub use standout_macros::ContractSurface;

pub use standout_seeker as seeker;

pub use standout_dispatch as dispatch;
pub use standout_input as input;

pub use standout_macros::{Questionnaire, QuestionnaireChoices, Seekable};

pub mod cli;
