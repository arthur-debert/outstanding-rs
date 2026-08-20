//! Boundary types for an explicit render: destination facts and the request.
//!
//! [`TargetProperties`] is what this invocation's destination looks like.
//! [`RenderRequest`] is what to render, including those properties and a
//! resolved [`ColorPolicy`] independent of [`crate::OutputMode`]. The pure
//! leaf entry is [`render_request`]; convenience wrappers detect at their
//! edge, build a request, and delegate here. Detection lives at
//! [`TargetProperties::detect`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use std::io::IsTerminal;

use crate::context::{ContextRegistry, RenderContext};
use crate::environment::{
    probe_stderr_color_capability, probe_stdout_color_capability, probe_terminal_width,
};
use crate::error::RenderError;
use crate::output::OutputMode;
use crate::projection::CsvProjection;
use crate::template::{
    load_inline_dependencies, load_named_template, render_engine_split_inline,
    render_engine_split_named, MiniJinjaEngine, RenderResult, TemplateEngine, TemplateRegistry,
};
use crate::theme::{probe_color_mode, probe_icon_mode, ColorMode, IconMode, Theme};
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
    /// [`AmbiguousWidth::Narrow`]; `App::run` overwrites it with the
    /// application's configured policy.
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
    pub fn detect() -> Self {
        Self {
            width: probe_terminal_width(),
            stdout_is_terminal: std::io::stdout().is_terminal(),
            stderr_is_terminal: std::io::stderr().is_terminal(),
            stdout_color_capability: probe_stdout_color_capability(),
            stderr_color_capability: probe_stderr_color_capability(),
            color_scheme: probe_color_mode(),
            icon_mode: probe_icon_mode(),
            ambiguous_width: AmbiguousWidth::Narrow,
        }
    }
}

/// Resolved color axis for one invocation.
///
/// Independent of [`OutputMode`] (format) and of per-stream color capability
/// on [`TargetProperties`]. Later `--color=auto|always|never` and the env
/// ladder (`NO_COLOR`, `CLICOLOR_FORCE`, …) resolve into this field; they are
/// not `--output`.
///
/// [`render_request`] applies this policy to style-tag transformation for
/// human formats: [`Always`](Self::Always) emits ANSI, [`Never`](Self::Never)
/// never does, and [`Auto`](Self::Auto) follows stdout color capability.
/// [`OutputMode::TermDebug`] keeps bracket tags regardless of policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPolicy {
    /// Defer to the requested format: [`OutputMode::Term`] colors,
    /// [`OutputMode::Text`] strips, and [`OutputMode::Auto`] follows stdout
    /// color capability.
    Auto,
    /// Color even when the consumed stream is not a TTY, including
    /// [`OutputMode::Text`].
    Always,
    /// Never emit ANSI, including on a color-capable TTY and
    /// [`OutputMode::Term`].
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
/// `--color` flag must have a home that is not `--output`. Caller extras
/// for context providers ride [`Self::extras`].
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
    /// Caller-supplied extras forwarded onto [`RenderContext`] for providers.
    ///
    /// Convenience wrappers that take a [`RenderContext`] copy these through
    /// so [`RenderContext::with_extra`] values survive `render_request`.
    /// The reserved `standout.ambiguous_width` key is owned by
    /// [`TargetProperties::ambiguous_width`] and is not copied from here.
    pub extras: HashMap<String, String>,
    /// Optional per-run warning buffer. Unresolved-tag warnings land here
    /// when the request path threads it through style-tag application.
    pub warnings: Option<crate::warnings::WarningBuffer>,
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
            .field("extras", &self.extras)
            .field("has_warnings", &self.warnings.is_some())
            .finish_non_exhaustive()
    }
}

/// Pure render entry: a function of an explicit [`RenderRequest`].
///
/// Takes the owned request by reference and returns the formatted string
/// (ANSI applied when [`ColorPolicy`] and capability say so). Convenience
/// [`crate::render`] and [`crate::render_with_output`] detect at their edge,
/// build a request, and delegate here.
///
/// Callers that need both formatted and stripped output (CLI pipes, output
/// files, post-output hooks) should use [`render_request_split`].
///
/// Width and ambiguous-width come from [`RenderRequest::target`]. Style-tag
/// transformation honors [`RenderRequest::color_policy`] for every human
/// format: [`ColorPolicy::Always`] forces ANSI, [`ColorPolicy::Never`] never
/// emits it, and [`ColorPolicy::Auto`] defers to the requested format
/// (`Term` colors, `Text` strips, `Auto` follows stdout capability).
/// [`OutputMode::TermDebug`] keeps bracket tags. Named templates and their
/// includes are loaded from [`RenderRequest::registry`] when present.
/// Color-scheme and icon mode come from [`RenderRequest::target`]. ANSI
/// is applied via `force_styling` on the request's styles, not
/// `console::colors_enabled()`.
pub fn render_request(request: &RenderRequest) -> Result<String, RenderError> {
    Ok(render_request_split(request)?.formatted)
}

/// Pure render entry that returns both formatted and stripped output.
///
/// Same request function as [`render_request`]: formatted carries ANSI when
/// the color policy and stdout capability say so; raw is the same text with
/// style tags stripped, for pipes and output files.
pub fn render_request_split(request: &RenderRequest) -> Result<RenderResult, RenderError> {
    render_from_request(request)
}

/// Format fact passed to [`RenderContext`]: `Auto` resolves via color policy
/// and capability; an explicit human format is left as requested.
fn resolve_render_format(request: &RenderRequest) -> OutputMode {
    match request.format {
        OutputMode::Auto => resolve_style_mode(request),
        other => other,
    }
}

/// Style-tag transformation from [`ColorPolicy`], format, and stdout capability.
///
/// [`ColorPolicy::Always`] forces ANSI and [`ColorPolicy::Never`] never emits
/// it, including when the requested format is [`OutputMode::Term`] or
/// [`OutputMode::Text`]. [`ColorPolicy::Auto`] defers to that format:
/// `Term` still colors, `Text` still strips, and `Auto` follows stdout
/// capability. [`OutputMode::TermDebug`] keeps bracket tags.
fn resolve_style_mode(request: &RenderRequest) -> OutputMode {
    if request.format == OutputMode::TermDebug {
        return OutputMode::TermDebug;
    }
    match request.color_policy {
        ColorPolicy::Never => OutputMode::Text,
        ColorPolicy::Always => OutputMode::Term,
        ColorPolicy::Auto => match request.format {
            OutputMode::Text => OutputMode::Text,
            OutputMode::Term => OutputMode::Term,
            OutputMode::Auto => {
                if request.target.stdout_color_capability {
                    OutputMode::Term
                } else {
                    OutputMode::Text
                }
            }
            other => other,
        },
    }
}

fn serialize_structured(
    data: &serde_json::Value,
    format: OutputMode,
) -> Result<RenderResult, RenderError> {
    let output = match format {
        OutputMode::Json => serde_json::to_string_pretty(data)?,
        OutputMode::Yaml => serde_yaml::to_string(data)?,
        OutputMode::Xml => crate::util::serialize_to_xml(data)?,
        OutputMode::Csv => {
            let (headers, rows) = crate::util::flatten_json_for_csv(data);
            let mut wtr = csv::Writer::from_writer(Vec::new());
            wtr.write_record(&headers)?;
            for row in rows {
                wtr.write_record(&row)?;
            }
            let bytes = wtr.into_inner()?;
            String::from_utf8(bytes)?
        }
        _ => unreachable!("serialize_structured requires a structured OutputMode"),
    };
    Ok(RenderResult::plain(output))
}

fn render_from_request(request: &RenderRequest) -> Result<RenderResult, RenderError> {
    if matches!(request.template, TemplateRef::Absent) && request.format == OutputMode::Auto {
        return serialize_structured(&request.data, OutputMode::Json);
    }

    if request.format.is_structured() {
        if request.format == OutputMode::Csv {
            if let Some(projection) = &request.csv_projection {
                let csv = projection
                    .render(&request.data)
                    .map_err(|e| RenderError::OperationError(e.to_string()))?;
                return Ok(RenderResult::plain(csv));
            }
        }
        return serialize_structured(&request.data, request.format);
    }

    let style_mode = resolve_style_mode(request);
    let empty_registry = ContextRegistry::new();
    let context_registry = request.context_registry.as_ref().unwrap_or(&empty_registry);
    let render_ctx = render_context_from_request(request);

    match &request.template {
        TemplateRef::Inline(source) => {
            if let Some(registry) = &request.registry {
                load_inline_dependencies(&mut **request.engine.borrow_mut(), registry)?;
            }
            let engine = request.engine.borrow();
            render_engine_split_inline(
                &**engine,
                source,
                &request.data,
                &request.theme,
                style_mode,
                context_registry,
                &render_ctx,
                request.target.color_scheme,
                request.target.icon_mode,
            )
        }
        TemplateRef::Named(name) => {
            if let Some(registry) = &request.registry {
                load_named_template(&mut **request.engine.borrow_mut(), registry, name)?;
            }
            let engine = request.engine.borrow();
            render_engine_split_named(
                &**engine,
                name,
                &request.data,
                &request.theme,
                style_mode,
                context_registry,
                &render_ctx,
                request.target.color_scheme,
                request.target.icon_mode,
            )
        }
        TemplateRef::Absent => Err(RenderError::TemplateError(
            "absent template cannot render in a human output mode".into(),
        )),
    }
}

/// Shared engine for callers that do not retain one from `AppBuilder::build()`.
///
/// Convenience wrappers and standalone `render_help` / `render_topic` use this
/// so the glue crate never constructs [`MiniJinjaEngine`] outside `build()`.
pub fn default_template_engine() -> SharedTemplateEngine {
    Rc::new(RefCell::new(Box::new(MiniJinjaEngine::new())))
}

/// Shared engine handle for a convenience wrapper that detects, then calls
/// [`render_request`].
pub(crate) fn convenience_engine() -> SharedTemplateEngine {
    default_template_engine()
}

/// Builds a [`RenderRequest`] for a convenience wrapper that already detected
/// destination facts at its edge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn convenience_request(
    template: TemplateRef,
    data: serde_json::Value,
    theme: Theme,
    format: OutputMode,
    target: TargetProperties,
    context_registry: Option<ContextRegistry>,
    registry: Option<Rc<TemplateRegistry>>,
    csv_projection: Option<CsvProjection>,
) -> RenderRequest {
    RenderRequest {
        data,
        template,
        theme,
        format,
        color_policy: ColorPolicy::Auto,
        target,
        engine: convenience_engine(),
        registry,
        context_registry,
        csv_projection,
        extras: HashMap::new(),
        warnings: None,
    }
}

/// Provider view reconstructed from the request, including caller extras.
///
/// Ambiguous-width on [`TargetProperties`] wins over a reserved extra of the
/// same name so width stays a destination fact, not a leftover context key.
fn render_context_from_request(request: &RenderRequest) -> RenderContext<'_> {
    let mut ctx = RenderContext::with_ambiguous_width(
        resolve_render_format(request),
        request.target.width,
        request.target.ambiguous_width,
        &request.theme,
        &request.data,
    );
    for (key, value) in &request.extras {
        if key == "standout.ambiguous_width" {
            continue;
        }
        ctx.extras.insert(key.clone(), value.clone());
    }
    ctx.warnings = request.warnings.clone();
    ctx
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
            extras: HashMap::new(),
            warnings: None,
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
    fn render_request_carries_extras_to_context_providers() {
        use minijinja::Value;

        let mut registry = ContextRegistry::new();
        registry.add_provider("label", |ctx: &RenderContext| {
            Value::from(ctx.get_extra("label").unwrap_or("missing"))
        });
        let request = RenderRequest {
            data: json!({"name": "Ada"}),
            template: TemplateRef::Inline("{{ name }} {{ label }}".into()),
            format: OutputMode::Text,
            context_registry: Some(registry),
            extras: HashMap::from([("label".into(), "from-extra".into())]),
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "Ada from-extra");
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

    #[test]
    fn render_request_renders_inline_template_from_the_request() {
        let request = RenderRequest {
            data: json!({"msg": "hello"}),
            template: TemplateRef::Inline("{{ msg }}".into()),
            format: OutputMode::Text,
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "hello");
    }

    #[test]
    fn render_request_auto_without_stdout_color_strips_style_tags() {
        let request = RenderRequest {
            data: json!({"msg": "hi"}),
            template: TemplateRef::Inline("[tone]{{ msg }}[/tone]".into()),
            format: OutputMode::Auto,
            color_policy: ColorPolicy::Auto,
            target: TargetProperties {
                stdout_color_capability: false,
                ..sample_target()
            },
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "hi");
    }

    fn styled_theme() -> Theme {
        Theme::new().add("tone", console::Style::new().red().force_styling(true))
    }

    fn styled_inline_request(
        format: OutputMode,
        color_policy: ColorPolicy,
        stdout_color_capability: bool,
    ) -> RenderRequest {
        RenderRequest {
            data: json!({"msg": "hi"}),
            template: TemplateRef::Inline("[tone]{{ msg }}[/tone]".into()),
            theme: styled_theme(),
            format,
            color_policy,
            target: TargetProperties {
                stdout_color_capability,
                ..sample_target()
            },
            ..sample_request()
        }
    }

    #[test]
    fn color_policy_controls_ansi_independently_of_human_format_and_capability() {
        let formats = [OutputMode::Auto, OutputMode::Term, OutputMode::Text];
        let policies = [ColorPolicy::Auto, ColorPolicy::Always, ColorPolicy::Never];
        for format in formats {
            for policy in policies {
                for capable in [true, false] {
                    let expect_ansi = match policy {
                        ColorPolicy::Always => true,
                        ColorPolicy::Never => false,
                        ColorPolicy::Auto => match format {
                            OutputMode::Term => true,
                            OutputMode::Text => false,
                            OutputMode::Auto => capable,
                            _ => unreachable!("matrix only covers human formats"),
                        },
                    };
                    let request = styled_inline_request(format, policy, capable);
                    let rendered = render_request_split(&request).unwrap();
                    assert_eq!(
                        rendered.formatted.contains("\x1b["),
                        expect_ansi,
                        "format={format:?} policy={policy:?} capable={capable} formatted={:?}",
                        rendered.formatted
                    );
                    assert_eq!(
                        rendered.raw, "hi",
                        "format={format:?} policy={policy:?} capable={capable}"
                    );
                    assert!(
                        !rendered.raw.contains("\x1b["),
                        "raw must never carry ANSI: {:?}",
                        rendered.raw
                    );
                }
            }
        }
    }

    #[test]
    fn term_debug_keeps_bracket_tags_regardless_of_color_policy() {
        let request = styled_inline_request(OutputMode::TermDebug, ColorPolicy::Never, true);
        let rendered = render_request_split(&request).unwrap();
        assert_eq!(rendered.formatted, "[tone]hi[/tone]");
        assert_eq!(rendered.raw, "hi");
    }

    #[test]
    fn named_request_loads_static_includes_from_the_registry() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include 'partial' %}");
        registry.add_inline("partial", "{{ msg }}");
        let request = RenderRequest {
            data: json!({"msg": "hello"}),
            template: TemplateRef::Named("list".into()),
            format: OutputMode::Text,
            registry: Some(Rc::new(registry)),
            engine: sample_engine(),
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "hello");
    }

    #[test]
    fn named_request_loads_dynamic_includes_from_the_registry() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include extra %}");
        registry.add_inline("greeting", "Ada");
        let request = RenderRequest {
            data: json!({"extra": "greeting"}),
            template: TemplateRef::Named("list".into()),
            format: OutputMode::Text,
            registry: Some(Rc::new(registry)),
            engine: sample_engine(),
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "Ada");
    }

    #[test]
    fn second_render_of_the_same_request_does_not_add_templates_again() {
        use std::cell::Cell;

        struct CountingEngine {
            inner: MiniJinjaEngine,
            adds: Rc<Cell<usize>>,
        }

        impl TemplateEngine for CountingEngine {
            fn render_template(
                &self,
                template: &str,
                data: &serde_json::Value,
            ) -> Result<String, RenderError> {
                self.inner.render_template(template, data)
            }

            fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError> {
                self.adds.set(self.adds.get() + 1);
                self.inner.add_template(name, source)
            }

            fn render_named(
                &self,
                name: &str,
                data: &serde_json::Value,
            ) -> Result<String, RenderError> {
                self.inner.render_named(name, data)
            }

            fn has_template(&self, name: &str) -> bool {
                self.inner.has_template(name)
            }

            fn render_with_context(
                &self,
                template: &str,
                data: &serde_json::Value,
                context: HashMap<String, serde_json::Value>,
            ) -> Result<String, RenderError> {
                self.inner.render_with_context(template, data, context)
            }

            fn supports_includes(&self) -> bool {
                true
            }
            fn supports_filters(&self) -> bool {
                true
            }
            fn supports_control_flow(&self) -> bool {
                true
            }
        }

        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include 'partial' %}");
        registry.add_inline("partial", "{{ msg }}");
        let adds = Rc::new(Cell::new(0));
        let engine: SharedTemplateEngine = Rc::new(RefCell::new(Box::new(CountingEngine {
            inner: MiniJinjaEngine::new(),
            adds: adds.clone(),
        })));
        let request = RenderRequest {
            data: json!({"msg": "hello"}),
            template: TemplateRef::Named("list".into()),
            format: OutputMode::Text,
            registry: Some(Rc::new(registry)),
            engine,
            ..sample_request()
        };

        assert_eq!(render_request(&request).unwrap(), "hello");
        let first = adds.get();
        assert!(first >= 2, "expected list and partial, got {first}");
        assert_eq!(render_request(&request).unwrap(), "hello");
        assert_eq!(adds.get(), first);
    }

    #[test]
    fn inline_request_loads_static_includes_from_the_registry() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("partial", "{{ msg }}");
        let request = RenderRequest {
            data: json!({"msg": "hello"}),
            template: TemplateRef::Inline("{% include 'partial' %}".into()),
            format: OutputMode::Text,
            registry: Some(Rc::new(registry)),
            engine: sample_engine(),
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "hello");
    }

    #[test]
    fn named_request_skips_absent_ignore_missing_include() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include 'optional' ignore missing %}ok");
        let request = RenderRequest {
            template: TemplateRef::Named("list".into()),
            format: OutputMode::Text,
            registry: Some(Rc::new(registry)),
            engine: sample_engine(),
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "ok");
    }

    #[test]
    fn named_request_falls_back_to_the_present_include_list_candidate() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include ['override', 'default'] %}");
        registry.add_inline("default", "fallback");
        let request = RenderRequest {
            template: TemplateRef::Named("list".into()),
            format: OutputMode::Text,
            registry: Some(Rc::new(registry)),
            engine: sample_engine(),
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "fallback");
    }

    #[test]
    fn inline_request_skips_absent_ignore_missing_include() {
        let registry = TemplateRegistry::new();
        let request = RenderRequest {
            template: TemplateRef::Inline("{% include 'optional' ignore missing %}ok".into()),
            format: OutputMode::Text,
            registry: Some(Rc::new(registry)),
            engine: sample_engine(),
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "ok");
    }

    #[test]
    fn inline_request_falls_back_to_the_present_include_list_candidate() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("default", "fallback");
        let request = RenderRequest {
            template: TemplateRef::Inline("{% include ['override', 'default'] %}".into()),
            format: OutputMode::Text,
            registry: Some(Rc::new(registry)),
            engine: sample_engine(),
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), "fallback");
    }

    #[test]
    fn inline_request_still_finds_includes_after_an_unclosed_tag_in_raw() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("actual", "hello");
        let request = RenderRequest {
            template: TemplateRef::Inline(
                r#"{% raw %}{% "unclosed {% endraw %}{% include 'actual' %}"#.into(),
            ),
            format: OutputMode::Text,
            registry: Some(Rc::new(registry)),
            engine: sample_engine(),
            ..sample_request()
        };
        assert_eq!(render_request(&request).unwrap(), r#"{% "unclosed hello"#);
    }

    #[test]
    fn term_emits_ansi_without_force_styling_on_the_theme() {
        let request = RenderRequest {
            data: json!({"msg": "hi"}),
            template: TemplateRef::Inline("[tone]{{ msg }}[/tone]".into()),
            theme: Theme::new().add("tone", console::Style::new().red()),
            format: OutputMode::Term,
            color_policy: ColorPolicy::Auto,
            target: TargetProperties {
                stdout_color_capability: false,
                ..sample_target()
            },
            ..sample_request()
        };
        let rendered = render_request(&request).unwrap();
        assert!(
            rendered.contains("\x1b["),
            "Term applies force_styling from the request, got {rendered:?}"
        );
        console::set_colors_enabled(false);
        let again = render_request(&request).unwrap();
        assert_eq!(
            again, rendered,
            "console::colors_enabled must not change the request result"
        );
    }

    #[test]
    fn same_request_is_stable_under_perturbed_env() {
        let request = RenderRequest {
            data: json!({"msg": "hello"}),
            template: TemplateRef::Inline("{{ msg }}".into()),
            format: OutputMode::Text,
            ..sample_request()
        };
        let first = render_request(&request).unwrap();
        let original_columns = std::env::var_os("COLUMNS");
        std::env::set_var("COLUMNS", "20");
        let second = render_request(&request).unwrap();
        match original_columns {
            Some(value) => std::env::set_var("COLUMNS", value),
            None => std::env::remove_var("COLUMNS"),
        }
        assert_eq!(first, second);
        assert_eq!(first, "hello");
    }

    #[test]
    fn convenience_render_with_output_text_matches_render_request() {
        let theme = Theme::new();
        let data = json!({"msg": "hello"});
        let via_wrapper =
            crate::render_with_output("{{ msg }}", &data, &theme, OutputMode::Text).unwrap();
        let request = RenderRequest {
            data,
            template: TemplateRef::Inline("{{ msg }}".into()),
            theme,
            format: OutputMode::Text,
            color_policy: ColorPolicy::Auto,
            target: TargetProperties::detect(),
            engine: sample_engine(),
            registry: None,
            context_registry: None,
            csv_projection: None,
            extras: HashMap::new(),
            warnings: None,
        };
        assert_eq!(via_wrapper, render_request(&request).unwrap());
        assert_eq!(via_wrapper, "hello");
    }
}
