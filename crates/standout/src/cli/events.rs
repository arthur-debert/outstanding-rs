//! What one emitted event becomes on the run's destination.
//!
//! A handler emits typed events through `Results<E>`; this module is the
//! [`EventSink`] behind that channel, one per run:
//!
//! - the human representation renders the command's `<name>.event` template
//!   with the value bound to `event`, one flushed line per event;
//! - line framing writes the value as the handler produced it, compact JSON on
//!   its own line;
//! - an encoding carrying a whole run as one document writes no event while the
//!   command runs and retains each record instead. A run that fails never asks
//!   for them, which is how nothing partial goes out.
//!
//! Every reason an event does not reach the destination is returned to the
//! handler, so it stops at the `emit` that failed, and the first of them is
//! remembered for [`EventDestination::take_failure`] whether or not the handler
//! propagates it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::cli::builder::{SharedTemplateEngine, TemplateRef};
use crate::cli::handler::{EmitError, EventSink, OutputKind, RunError, RunErrorKind, StreamSink};
use crate::context::ContextRegistry;
use crate::{ColorPolicy, RenderRequest, Representation, TargetProperties, Theme};

/// The command's event template: its own template name with `.event` appended.
/// An inline or absent template has no event sibling to name.
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

/// The encodings that carry a whole run as one document, so an event is retained
/// until the command ends rather than written as it arrives.
pub(crate) fn retains_events(representation: Representation) -> bool {
    matches!(
        representation,
        Representation::Json | Representation::Yaml | Representation::Csv
    )
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
    pub strict_style_tags: bool,
}

pub(crate) struct EventDestination {
    sink: StreamSink,
    command_path: String,
    representation: Representation,
    strict_style_tags: bool,
    warnings: Option<standout_render::warnings::WarningBuffer>,
    /// The whole render but its `data`, built once: each event varies only the
    /// value bound to `event`.
    request: Option<RefCell<RenderRequest>>,
    failure: RefCell<Option<RunError>>,
    retained: Option<RefCell<Vec<serde_json::Value>>>,
}

impl EventDestination {
    pub(crate) fn new(sink: StreamSink, context: EventContext) -> Self {
        let request = context
            .template
            .filter(|_| context.representation.is_human())
            .map(|template| {
                RefCell::new(RenderRequest {
                    data: serde_json::Value::Null,
                    template,
                    theme: context.theme,
                    format: context.representation,
                    color_policy: context.color_policy,
                    target: context.target,
                    engine: context.template_engine,
                    registry: context.template_registry,
                    context_registry: Some(context.context_registry),
                    csv_projection: None,
                    extras: HashMap::new(),
                    warnings: context.warnings.clone(),
                })
            });
        Self {
            sink,
            command_path: context.command_path,
            representation: context.representation,
            strict_style_tags: context.strict_style_tags,
            warnings: context.warnings,
            request,
            failure: RefCell::new(None),
            retained: retains_events(context.representation).then(|| RefCell::new(Vec::new())),
        }
    }

    /// The framework's own reason the run cannot stand, if an event met one.
    pub(crate) fn take_failure(&self) -> Option<RunError> {
        self.failure.borrow_mut().take()
    }

    /// The event records this run retained, in emit order, or `None` when the
    /// representation already wrote each event as it arrived. Taking them empties
    /// the destination.
    pub(crate) fn take_document_records(&self) -> Option<Vec<serde_json::Value>> {
        self.retained
            .as_ref()
            .map(|retained| std::mem::take(&mut *retained.borrow_mut()))
    }

    /// Read before the event's line is written, so a `.event` template with an
    /// unresolved style tag fails the run before degraded bytes go out.
    fn strict_style_tags_error(&self) -> Option<RunError> {
        if !self.strict_style_tags {
            return None;
        }
        crate::cli::builder::execution::unresolved_style_tags_error(self.warnings.as_ref())
    }

    fn render(&self, event: &serde_json::Value) -> Result<String, EmitError> {
        let Some(request) = self.request.as_ref() else {
            return Err(EmitError::Render(format!(
                "command `{}` emitted an event but declares no template to render one; \
                 an incremental command renders each event from `<name>.event` beside its \
                 own template",
                self.command_path
            )));
        };
        let mut request = request.borrow_mut();
        request.data = serde_json::json!({ "event": event });
        let text = standout_render::render_request_split(&request)
            .map(|rendered| rendered.formatted)
            .map_err(|error| EmitError::Render(error.to_string()))?;
        match self.strict_style_tags_error() {
            Some(error) => Err(EmitError::Render(error.to_string())),
            None => Ok(text),
        }
    }

    fn write(&self, event: &serde_json::Value) -> Result<(), EmitError> {
        if let Some(retained) = self.retained.as_ref() {
            retained.borrow_mut().push(event.clone());
            return Ok(());
        }
        if self.representation.is_human() {
            let text = self.render(event)?;
            return Ok(self.sink.write_line(text.as_bytes())?);
        }
        let line = standout_render::serialize_document(event, self.representation)
            .map_err(|error| EmitError::Render(error.to_string()))?;
        Ok(self.sink.with_writer(|writer| {
            writer.write_all(line.as_bytes())?;
            writer.flush()
        })?)
    }
}

/// The phase an emit failure belongs to. An event that never became bytes
/// failed while rendering; one the destination refused failed the same write
/// that carries a whole run's text, and reaches machine consumers under the
/// `final-write` kind rather than `render`.
fn failure_kind(error: &EmitError) -> RunErrorKind {
    match error {
        EmitError::Serialize(_) | EmitError::Render(_) => RunErrorKind::Render,
        EmitError::Write(_) => RunErrorKind::FinalWrite(OutputKind::Text),
    }
}

// Rebuilt, not moved: `record_failure` borrows an error it must also return, and `io::Error` does not clone.
fn rebuilt(error: &std::io::Error) -> std::io::Error {
    error.raw_os_error().map_or_else(
        || std::io::Error::new(error.kind(), error.to_string()),
        std::io::Error::from_raw_os_error,
    )
}

impl EventSink for EventDestination {
    fn deliver(&self, event: &serde_json::Value) -> Result<(), EmitError> {
        let Err(error) = self.write(event) else {
            return Ok(());
        };
        self.record_failure(&error);
        Err(error)
    }

    fn is_open(&self) -> bool {
        self.retained.is_some() || self.sink.is_open()
    }

    fn record_failure(&self, error: &EmitError) {
        let mut failure = self.failure.borrow_mut();
        if failure.is_none() {
            let recorded = RunError::new(error.to_string(), failure_kind(error));
            *failure = Some(match error {
                EmitError::Write(io) => recorded.with_source(rebuilt(io)),
                EmitError::Serialize(_) | EmitError::Render(_) => recorded,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::handler::DiagnosticKind;

    #[test]
    fn every_emit_failure_keeps_the_phase_it_failed_in() {
        let serialize = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let cases = [
            (EmitError::Serialize(serialize), DiagnosticKind::Render),
            (
                EmitError::Render("no template".into()),
                DiagnosticKind::Render,
            ),
            (
                EmitError::Write(std::io::Error::other("no room")),
                DiagnosticKind::FinalWrite,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(
                DiagnosticKind::from(failure_kind(&error)),
                expected,
                "{error}"
            );
        }
    }
}
