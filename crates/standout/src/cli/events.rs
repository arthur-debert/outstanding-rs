//! What one emitted event becomes on the run's destination.
//!
//! A handler emits typed events through `Results<E>`; this module is the
//! [`EventSink`] behind that channel, one per run, holding everything a
//! representation needs to turn a value into bytes:
//!
//! - the human representation renders the command's `<name>.event` template
//!   with the value bound to `event`, and writes one flushed line per event,
//!   on a terminal and into a pipe alike;
//! - line framing writes the value as the handler produced it, compact JSON on
//!   its own line, with the discriminator the application gave it;
//! - the encodings that produce one document have no incremental form yet, so
//!   an emitting command under them fails the run.
//!
//! A failure that belongs to the framework rather than to the bytes — a
//! missing event template, an encoding that cannot carry events — is retained
//! here rather than returned, because `emit` reports only serialization and
//! write failures. The dispatch closure takes it with [`EventDestination::take_failure`]
//! once the handler returns, so the run reports a render error with its own
//! message whether or not the handler propagated the `emit`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::cli::builder::{output_mode_flag_spelling, SharedTemplateEngine, TemplateRef};
use crate::cli::handler::{EventSink, RunError, RunErrorKind, StreamSink};
use crate::context::ContextRegistry;
use crate::{ColorPolicy, RenderRequest, Representation, TargetProperties, Theme};

/// The command's event template: its own template name with `.event` appended,
/// resolved through the same directories and theme as the summary's. An inline
/// or absent template has no event sibling to name.
pub(crate) fn event_template(template: &TemplateRef) -> Option<standout_render::TemplateRef> {
    match template {
        TemplateRef::Named(name) => Some(named_event_template(name)),
        TemplateRef::Inline(_) | TemplateRef::Absent(_) => None,
    }
}

pub(crate) fn rendered_event_template(
    template: &standout_render::TemplateRef,
) -> Option<standout_render::TemplateRef> {
    match template {
        standout_render::TemplateRef::Named(name) => Some(named_event_template(name)),
        standout_render::TemplateRef::Inline(_) | standout_render::TemplateRef::Absent => None,
    }
}

fn named_event_template(name: &str) -> standout_render::TemplateRef {
    standout_render::TemplateRef::Named(format!("{name}.event"))
}

pub(crate) struct EventContext {
    pub command_path: String,
    pub template: Option<standout_render::TemplateRef>,
    pub theme: Theme,
    pub context_registry: ContextRegistry,
    pub template_engine: SharedTemplateEngine,
    pub template_registry: Option<Rc<crate::TemplateRegistry>>,
    pub representation: Representation,
    pub color_policy: ColorPolicy,
    pub target: TargetProperties,
    pub warnings: Option<standout_render::warnings::WarningBuffer>,
}

pub(crate) struct EventDestination {
    sink: StreamSink,
    context: EventContext,
    failure: RefCell<Option<RunError>>,
    emitted: Cell<usize>,
}

impl EventDestination {
    pub(crate) fn new(sink: StreamSink, context: EventContext) -> Self {
        Self {
            sink,
            context,
            failure: RefCell::new(None),
            emitted: Cell::new(0),
        }
    }

    /// How many events the command produced, so the caller can reject an
    /// outcome that cannot follow them.
    pub(crate) fn emitted(&self) -> usize {
        self.emitted.get()
    }

    /// The framework's own reason the run cannot stand, if an event met one.
    pub(crate) fn take_failure(&self) -> Option<RunError> {
        self.failure.borrow_mut().take()
    }

    fn fail(&self, error: RunError) {
        let mut failure = self.failure.borrow_mut();
        if failure.is_none() {
            *failure = Some(error);
        }
    }

    fn render(&self, event: &serde_json::Value) -> Result<String, RunError> {
        let Some(template) = self.context.template.clone() else {
            return Err(RunError::new(
                format!(
                    "command `{}` emitted an event but declares no template to render one; \
                     an incremental command renders each event from `<name>.event` beside its \
                     own template",
                    self.context.command_path
                ),
                RunErrorKind::Render,
            ));
        };
        let request = RenderRequest {
            data: serde_json::json!({ "event": event }),
            template,
            theme: self.context.theme.clone(),
            format: self.context.representation,
            color_policy: self.context.color_policy,
            target: self.context.target,
            engine: self.context.template_engine.clone(),
            registry: self.context.template_registry.clone(),
            context_registry: Some(self.context.context_registry.clone()),
            csv_projection: None,
            extras: HashMap::new(),
            warnings: self.context.warnings.clone(),
        };
        standout_render::render_request_split(&request)
            .map(|rendered| rendered.formatted)
            .map_err(|error| RunError::new(error.to_string(), RunErrorKind::Render))
    }
}

impl EventSink for EventDestination {
    fn deliver(&self, event: &serde_json::Value) -> Result<(), std::io::Error> {
        self.emitted.set(self.emitted.get() + 1);
        let representation = self.context.representation;
        if representation.is_human() {
            return match self.render(event) {
                Ok(text) => self.sink.write_line(text.as_bytes()),
                Err(error) => {
                    self.fail(error);
                    Ok(())
                }
            };
        }
        if representation.is_stream() {
            let line = standout_render::serialize_document(event, representation)
                .map_err(std::io::Error::other)?;
            return self.sink.with_writer(|writer| {
                writer.write_all(line.as_bytes())?;
                writer.flush()
            });
        }
        let encoding = output_mode_flag_spelling(representation)
            .map(|flag| format!("--output {flag}"))
            .unwrap_or_else(|| format!("{representation:?}"));
        self.fail(RunError::new(
            format!(
                "command `{}` emitted an event under {encoding}; that encoding carries a \
                 command's events as one document and standout does not build one yet",
                self.context.command_path,
            ),
            RunErrorKind::Render,
        ));
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.sink.is_open() && self.failure.borrow().is_none()
    }
}
