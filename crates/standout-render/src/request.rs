//! Boundary types for an explicit render: destination facts and the request.
//!
//! [`TargetProperties`] is what this invocation's destination looks like.
//! [`RenderRequest`] is what to render, including those properties and a
//! resolved [`ColorPolicy`] independent of [`crate::OutputMode`]. The pure
//! leaf entry is [`render_request`]; convenience wrappers stay as separate
//! functions and are not rewired onto this path in this workstream.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::context::ContextRegistry;
use crate::error::RenderError;
use crate::output::OutputMode;
use crate::projection::CsvProjection;
use crate::template::{TemplateEngine, TemplateRegistry};
use crate::theme::{ColorMode, IconMode, Theme};
use crate::AmbiguousWidth;

/// Shared handle to the template engine stored on a [`RenderRequest`].
///
/// The glue crate already shares the engine as `Rc<RefCell<Box<dyn TemplateEngine>>>`
/// so an owned request can outlive the call (the artifact path stores the
/// snapshot until after the write). There is no lifetime on the public API.
pub type SharedTemplateEngine = Rc<RefCell<Box<dyn TemplateEngine>>>;

/// Properties of the destination being rendered to for one invocation.
///
/// Width, stream terminal-ness, per-stream color capability, color-scheme,
/// and icon mode are detected facts: they have no App fallback. Ambiguous-width
/// rides this type as application policy; [`detect`](Self::detect) documents
/// [`AmbiguousWidth::Narrow`] as that field's default, and `App::run` later
/// overwrites it with the configured policy.
///
/// Color capability and terminal-ness are per stream because stdout and stderr
/// can differ (piped command, TTY warnings). Primary render consumes stdout
/// facts; warnings and progress consume stderr facts.
///
/// This type is `Copy`. Construct it directly in tests; do not call [`detect`]
/// from leaves or tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetProperties {
    /// Terminal width in columns, if known.
    ///
    /// `None` means the width was not determined. There is no silent 80-column
    /// default on this type.
    pub width: Option<usize>,

    /// Whether stdout is a terminal.
    pub stdout_is_terminal: bool,

    /// Whether stderr is a terminal.
    pub stderr_is_terminal: bool,

    /// Whether ANSI color is supported on stdout.
    ///
    /// Reuses the existing leaf result type (`bool` from
    /// [`crate::detect_color_capability`]).
    pub stdout_color_capability: bool,

    /// Whether ANSI color is supported on stderr.
    ///
    /// Independent of [`Self::stdout_color_capability`]: a piped stdout with
    /// a TTY stderr must be representable.
    pub stderr_color_capability: bool,

    /// Light or dark color-scheme (the existing [`ColorMode`] enum).
    pub color_scheme: ColorMode,

    /// Icon rendering mode (the existing [`IconMode`] enum).
    pub icon_mode: IconMode,

    /// East Asian Ambiguous width policy.
    ///
    /// Not a detected terminal fact. [`detect`](Self::detect) defaults this to
    /// [`AmbiguousWidth::Narrow`]; that default is documented here, not
    /// implemented in this workstream.
    pub ambiguous_width: AmbiguousWidth,
}

impl TargetProperties {
    /// Probes the process for destination properties.
    ///
    /// Fills width, stdout and stderr terminal-ness, stdout and stderr color
    /// capability, color-scheme, and icon mode. Ambiguous-width defaults to
    /// [`AmbiguousWidth::Narrow`]. Convenience wrappers and `App::run` call
    /// this at their edge; template functions, tabular, width helpers, and
    /// tests do not.
    ///
    /// The body is unimplemented in this workstream (`todo!()`). Callers that
    /// need a value construct [`TargetProperties`] directly.
    pub fn detect() -> Self {
        todo!("ROB04-WS01 lands this signature only; detection is a later workstream")
    }
}

/// Resolved color axis for one invocation.
///
/// Independent of [`OutputMode`] (format) and of per-stream color capability
/// on [`TargetProperties`]. Later `--color=auto|always|never` and the env
/// ladder (`NO_COLOR`, `CLICOLOR_FORCE`, …) resolve into this field; they are
/// not `--output`. This workstream names the fact only: no flag, no
/// resolution, no ANSI application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPolicy {
    /// Color when the consumed stream is color-capable.
    Auto,
    /// Color even when the consumed stream is not a TTY.
    Always,
    /// Never emit ANSI, including on a color-capable TTY.
    Never,
}

/// Named, inline, or declared-absent template carried on a [`RenderRequest`].
///
/// This is the render-time type. It has no `Convention` variant: convention
/// names exist only on the glue builder until `build()` materializes them to
/// [`Named`](Self::Named). Standalone help and topic template strings become
/// [`Inline`](Self::Inline).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateRef {
    /// A named template that must resolve through the template registry.
    Named(String),
    /// Inline template source carried directly on the request.
    Inline(String),
    /// No human template; structured modes serialize data directly.
    Absent,
}

/// What to render for one invocation.
///
/// Owned, no lifetime: the artifact path can store this type until after the
/// write instead of keeping a second snapshot type. The engine and template
/// registry sit behind `Rc` as glue already does. File-backed templates and
/// context-provider callbacks are explicit external dependencies of the
/// request; the leaf does not read framework-owned detectors or process
/// globals.
///
/// Format ([`OutputMode`]), color policy ([`ColorPolicy`]), and per-stream
/// color capability on [`TargetProperties`] are independent facts. A later
/// `--color` flag must have a home that is not `--output`.
pub struct RenderRequest {
    /// Handler data, already serialized.
    pub data: serde_json::Value,
    /// Render-time template: named, inline, or absent.
    pub template: TemplateRef,
    /// Resolved theme for this invocation.
    pub theme: Theme,
    /// Output format (`--output`), independent of [`Self::color_policy`].
    pub format: OutputMode,
    /// Resolved color policy, independent of [`Self::format`] and of
    /// per-stream capability on [`Self::target`].
    pub color_policy: ColorPolicy,
    /// Destination properties for this invocation.
    pub target: TargetProperties,
    /// Template engine, shared the way glue already shares it.
    pub engine: SharedTemplateEngine,
    /// Optional template registry for named templates and includes.
    pub registry: Option<Rc<TemplateRegistry>>,
    /// Optional context providers invoked at render time.
    pub context_registry: Option<ContextRegistry>,
    /// Optional CSV projection for structured CSV output.
    pub csv_projection: Option<CsvProjection>,
}

impl fmt::Debug for RenderRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderRequest")
            .field("data", &self.data)
            .field("template", &self.template)
            .field("theme", &self.theme)
            .field("format", &self.format)
            .field("color_policy", &self.color_policy)
            .field("target", &self.target)
            .field("has_registry", &self.registry.is_some())
            .field("has_context_registry", &self.context_registry.is_some())
            .field("csv_projection", &self.csv_projection)
            .finish_non_exhaustive()
    }
}

/// Pure render entry: a function of an explicit [`RenderRequest`].
///
/// Takes the owned request by reference. Convenience [`crate::render`] and
/// [`crate::render_with_output`] stay as separate functions and are not
/// rewired onto this path in this workstream.
///
/// The body is unimplemented (`todo!()`).
pub fn render_request(_request: &RenderRequest) -> Result<String, RenderError> {
    todo!("ROB04-WS01 lands this signature only; rendering is a later workstream")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::MiniJinjaEngine;
    use serde_json::json;

    fn sample_target() -> TargetProperties {
        TargetProperties {
            width: Some(80),
            stdout_is_terminal: true,
            stderr_is_terminal: true,
            stdout_color_capability: true,
            stderr_color_capability: true,
            color_scheme: ColorMode::Dark,
            icon_mode: IconMode::Classic,
            ambiguous_width: AmbiguousWidth::Narrow,
        }
    }

    fn sample_engine() -> SharedTemplateEngine {
        Rc::new(RefCell::new(Box::new(MiniJinjaEngine::new())))
    }

    fn sample_request() -> RenderRequest {
        RenderRequest {
            data: json!({"count": 1}),
            template: TemplateRef::Named("list".into()),
            theme: Theme::new(),
            format: OutputMode::Text,
            color_policy: ColorPolicy::Auto,
            target: sample_target(),
            engine: sample_engine(),
            registry: None,
            context_registry: None,
            csv_projection: None,
        }
    }

    fn assert_copy<T: Copy>(value: T) -> T {
        value
    }

    #[test]
    fn target_properties_is_copy() {
        let props = sample_target();
        let copied = assert_copy(props);
        let also = props;
        assert_eq!(copied, also);
        assert_eq!(copied.width, Some(80));
        assert_eq!(copied.color_scheme, ColorMode::Dark);
        assert_eq!(copied.icon_mode, IconMode::Classic);
        assert_eq!(copied.ambiguous_width, AmbiguousWidth::Narrow);
    }

    #[test]
    fn target_properties_color_capability_is_per_stream() {
        let props = TargetProperties {
            stdout_color_capability: true,
            stderr_color_capability: false,
            stdout_is_terminal: false,
            stderr_is_terminal: true,
            ..sample_target()
        };
        assert!(props.stdout_color_capability);
        assert!(!props.stderr_color_capability);
        assert!(!props.stdout_is_terminal);
        assert!(props.stderr_is_terminal);
    }

    #[test]
    fn render_request_has_no_lifetime_and_can_be_stored() {
        struct Stored {
            request: RenderRequest,
        }

        let stored = Stored {
            request: sample_request(),
        };
        let held: Vec<RenderRequest> = vec![stored.request];
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].format, OutputMode::Text);
        assert_eq!(held[0].color_policy, ColorPolicy::Auto);
        match &held[0].template {
            TemplateRef::Named(name) => assert_eq!(name, "list"),
            TemplateRef::Inline(_) | TemplateRef::Absent => {
                panic!("sample request uses a named template")
            }
        }
    }

    #[test]
    fn render_time_template_ref_has_no_convention_variant() {
        let variants = [
            TemplateRef::Named("list".into()),
            TemplateRef::Inline("{{ x }}".into()),
            TemplateRef::Absent,
        ];
        for template in variants {
            // Exhaustive on purpose: adding Convention (or any fourth
            // variant) fails this test at compile time.
            match template {
                TemplateRef::Named(_) | TemplateRef::Inline(_) | TemplateRef::Absent => {}
            }
        }
    }

    #[test]
    fn render_request_construction_carries_optional_registry_and_projection() {
        let registry = Rc::new(TemplateRegistry::new());
        let projection = CsvProjection::builder("items").build();
        let request = RenderRequest {
            template: TemplateRef::Absent,
            registry: Some(registry),
            context_registry: Some(ContextRegistry::new()),
            csv_projection: Some(projection),
            ..sample_request()
        };
        assert!(request.registry.is_some());
        assert!(request.context_registry.is_some());
        assert!(request.csv_projection.is_some());
        assert!(matches!(request.template, TemplateRef::Absent));
    }

    #[test]
    fn render_request_format_policy_and_capabilities_vary_independently() {
        let request = RenderRequest {
            format: OutputMode::Json,
            color_policy: ColorPolicy::Always,
            target: TargetProperties {
                stdout_color_capability: false,
                stderr_color_capability: true,
                stdout_is_terminal: false,
                stderr_is_terminal: true,
                ..sample_target()
            },
            ..sample_request()
        };
        assert_eq!(request.format, OutputMode::Json);
        assert_eq!(request.color_policy, ColorPolicy::Always);
        assert!(!request.target.stdout_color_capability);
        assert!(request.target.stderr_color_capability);
        assert!(!request.target.stdout_is_terminal);
        assert!(request.target.stderr_is_terminal);
    }

    #[test]
    fn color_policy_is_a_tri_state() {
        let variants = [ColorPolicy::Auto, ColorPolicy::Always, ColorPolicy::Never];
        for policy in variants {
            match policy {
                ColorPolicy::Auto | ColorPolicy::Always | ColorPolicy::Never => {}
            }
        }
        let copied = assert_copy(ColorPolicy::Never);
        assert_eq!(copied, ColorPolicy::Never);
    }

    #[test]
    fn render_request_debug_is_structural() {
        let request = sample_request();
        let debug = format!("{request:?}");
        assert!(debug.contains("RenderRequest"));
        assert!(debug.contains("color_policy: Auto"));
        assert!(debug.contains("has_registry: false"));
    }
}
