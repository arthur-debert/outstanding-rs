//! The typed results channel a handler produces values through.
//!
//! A command produces either one batch value, which the handler returns, or a
//! sequence of typed events it emits through [`Results`] before returning the
//! summary. Both are results: the values the command exists to produce, as
//! opposed to operational messages about the run.
//!
//! [`Results`] carries what retains a value and what writes it behind an `Rc`,
//! so it needs no lifetime parameter and `Handler::handle` gains none. A batch
//! command sets `Handler::Event` to [`NoEvents`], whose uninhabited type leaves
//! `emit` with no argument that can be constructed.
//!
//! [`RunRecorder`] retains each value as data whatever representation the run
//! selected, and carries the run's [`Delivery`] decision, so a test asserts on
//! the values and on the rendered bytes separately; the process edge installs
//! [`RunRecorder::summary_only`] instead, which writes each event and keeps
//! none, so a long run costs no memory for the events nobody reads back.
//! [`EventSink`] is the other
//! half: the representation-specific destination that renders or frames the
//! value and writes it before `emit` returns. The consuming framework
//! implements it, because the human representation of an event is a template
//! render and this crate does not render.
//!
//! `emit` serializes once and hands the same [`serde_json::Value`] to both. It
//! skips serializing only when there is nothing to retain and nothing open to
//! write to, which is what a sink whose reader went away reports.

use serde::Serialize;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The event type of a command that emits none: uninhabited, so `emit` has no
/// argument that can be constructed.
#[derive(Debug, Serialize)]
pub enum NoEvents {}

#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("event does not serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    /// The destination could not turn the value into bytes: a render failure
    /// carrying the message the run reports.
    #[error("{0}")]
    Render(String),
    #[error("event could not be written: {0}")]
    Write(#[from] std::io::Error),
}

/// Where the run's rendered bytes went.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Delivery {
    #[default]
    Stdout,
    File(PathBuf),
}

impl Delivery {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Delivery::Stdout => None,
            Delivery::File(path) => Some(path),
        }
    }
}

/// The representation's destination for one emitted event.
///
/// `deliver` returns once the value has been rendered or framed and written,
/// so the handler's next statement runs after the consumer could read it. It
/// returns `Err` for every reason the event did not reach the destination, so
/// the handler's `?` stops at the emit that failed.
pub trait EventSink {
    fn deliver(&self, event: &serde_json::Value) -> Result<(), EmitError>;

    /// False once the destination has gone away; nothing further is written.
    fn is_open(&self) -> bool {
        true
    }
}

#[derive(Debug)]
struct RunRecord {
    records: Vec<serde_json::Value>,
    delivery: Delivery,
    retain_events: bool,
}

/// Retains the run's result values and its delivery decision.
#[derive(Debug, Clone)]
pub struct RunRecorder(Rc<RefCell<RunRecord>>);

impl Default for RunRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl RunRecorder {
    /// Retains every value the run produces, events included, for an entry
    /// point that hands them back through `results()`.
    pub fn new() -> Self {
        Self::with_event_retention(true)
    }

    /// Retains the summary and the delivery decision and drops each event
    /// instead of keeping it. The process edge installs this one: it writes
    /// every event and returns none, so a run whose events nobody reads back
    /// costs memory for one value rather than for its whole length.
    pub fn summary_only() -> Self {
        Self::with_event_retention(false)
    }

    fn with_event_retention(retain_events: bool) -> Self {
        Self(Rc::new(RefCell::new(RunRecord {
            records: Vec::new(),
            delivery: Delivery::default(),
            retain_events,
        })))
    }

    pub fn record(&self, value: serde_json::Value) {
        self.0.borrow_mut().records.push(value);
    }

    /// Whether an emitted event is worth serializing for retention.
    pub fn retains_events(&self) -> bool {
        self.0.borrow().retain_events
    }

    pub fn set_delivery(&self, delivery: Delivery) {
        self.0.borrow_mut().delivery = delivery;
    }

    pub fn records(&self) -> Vec<serde_json::Value> {
        self.0.borrow().records.clone()
    }

    pub fn delivery(&self) -> Delivery {
        self.0.borrow().delivery.clone()
    }
}

/// The handler's channel for the values a command produces while it runs.
///
/// Not `Clone`: `Handler::handle` receives it as a `&mut` borrow the framework
/// owns, and a clone would let a handler keep emitting past its own run.
pub struct Results<E: Serialize> {
    recorder: Option<RunRecorder>,
    sink: Option<Rc<dyn EventSink>>,
    _event: PhantomData<fn(E)>,
}

impl<E: Serialize> std::fmt::Debug for Results<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Results")
            .field("recording", &self.recorder.is_some())
            .field("writing", &self.sink.is_some())
            .finish()
    }
}

impl<E: Serialize> Results<E> {
    pub fn discarding() -> Self {
        Self {
            recorder: None,
            sink: None,
            _event: PhantomData,
        }
    }

    pub fn recording(recorder: RunRecorder) -> Self {
        Self {
            recorder: Some(recorder),
            sink: None,
            _event: PhantomData,
        }
    }

    /// The channel a run installs: every value is written, and retained too
    /// when the entry point has a recorder that keeps events.
    pub fn for_run(recorder: Option<RunRecorder>, sink: Rc<dyn EventSink>) -> Self {
        Self {
            recorder,
            sink: Some(sink),
            _event: PhantomData,
        }
    }

    /// Returns once the value has been retained and written; fails when it
    /// does not serialize, does not render, or cannot be written.
    pub fn emit(&mut self, event: E) -> Result<(), EmitError> {
        let retaining = self
            .recorder
            .as_ref()
            .filter(|recorder| recorder.retains_events());
        let open = self.sink.as_ref().filter(|sink| sink.is_open());
        if retaining.is_none() && open.is_none() {
            return Ok(());
        }
        let value = serde_json::to_value(&event)?;
        if let Some(recorder) = retaining {
            recorder.record(value.clone());
        }
        if let Some(sink) = open {
            sink.deliver(&value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Delivered {
        values: RefCell<Vec<serde_json::Value>>,
        open: bool,
    }

    impl EventSink for Delivered {
        fn deliver(&self, event: &serde_json::Value) -> Result<(), EmitError> {
            self.values.borrow_mut().push(event.clone());
            Ok(())
        }
        fn is_open(&self) -> bool {
            self.open
        }
    }

    fn open() -> Rc<Delivered> {
        Rc::new(Delivered {
            values: RefCell::new(Vec::new()),
            open: true,
        })
    }

    #[test]
    fn a_recorder_retains_values_in_order() {
        let recorder = RunRecorder::new();
        recorder.record(serde_json::json!({"n": 1}));
        recorder.record(serde_json::json!({"n": 2}));
        assert_eq!(
            recorder.records(),
            vec![serde_json::json!({"n": 1}), serde_json::json!({"n": 2})]
        );
    }

    #[test]
    fn a_recorder_carries_the_delivery_decision() {
        let recorder = RunRecorder::new();
        assert_eq!(recorder.delivery(), Delivery::Stdout);
        assert_eq!(recorder.delivery().path(), None);
        recorder.set_delivery(Delivery::File(PathBuf::from("out.txt")));
        assert_eq!(recorder.delivery().path(), Some(Path::new("out.txt")));
    }

    #[test]
    fn an_emitted_event_is_retained_and_written_in_the_same_call() {
        let recorder = RunRecorder::new();
        let sink = open();
        let mut results = Results::for_run(Some(recorder.clone()), sink.clone());
        results
            .emit(serde_json::json!({"type": "apply_start"}))
            .unwrap();
        assert_eq!(recorder.records().len(), 1);
        assert_eq!(sink.values.borrow().len(), 1);
    }

    /// Serializing it at all is the failure, so a channel that reports success
    /// on it never asked for the bytes.
    struct Unserializable;

    impl Serialize for Unserializable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("never asked"))
        }
    }

    #[test]
    fn a_discarding_channel_keeps_nothing_and_never_serializes() {
        Results::discarding().emit(Unserializable).unwrap();
    }

    #[test]
    fn a_summary_only_recorder_writes_every_event_and_retains_none() {
        let recorder = RunRecorder::summary_only();
        let sink = open();
        let mut results = Results::for_run(Some(recorder.clone()), sink.clone());
        for n in 0..3 {
            results.emit(serde_json::json!({ "n": n })).unwrap();
        }
        recorder.record(serde_json::json!({"total": 3}));
        assert_eq!(sink.values.borrow().len(), 3);
        assert_eq!(recorder.records(), vec![serde_json::json!({"total": 3})]);
    }

    #[test]
    fn a_summary_only_recorder_over_a_closed_sink_never_serializes_an_event() {
        let closed = Rc::new(Delivered {
            values: RefCell::new(Vec::new()),
            open: false,
        });
        let mut results = Results::for_run(Some(RunRecorder::summary_only()), closed);
        results.emit(Unserializable).unwrap();
    }

    #[test]
    fn a_closed_sink_still_retains_the_value_and_writes_nothing() {
        let recorder = RunRecorder::new();
        let closed = Rc::new(Delivered {
            values: RefCell::new(Vec::new()),
            open: false,
        });
        let mut results = Results::for_run(Some(recorder.clone()), closed.clone());
        results.emit(serde_json::json!({"n": 1})).unwrap();
        assert_eq!(recorder.records().len(), 1);
        assert!(closed.values.borrow().is_empty());
    }

    #[test]
    fn an_unserializable_event_is_an_emit_error() {
        let mut results = Results::recording(RunRecorder::new());
        let mut map = std::collections::HashMap::new();
        map.insert((1u8, 2u8), 3u8);
        let error = results.emit(map).unwrap_err();
        assert!(matches!(error, EmitError::Serialize(_)), "{error}");
    }

    #[test]
    fn a_destination_that_refuses_the_bytes_is_an_emit_error() {
        struct Refuses;
        impl EventSink for Refuses {
            fn deliver(&self, _: &serde_json::Value) -> Result<(), EmitError> {
                Err(EmitError::Write(std::io::Error::other("no room")))
            }
        }
        let mut results = Results::for_run(None, Rc::new(Refuses));
        let error = results.emit(serde_json::json!({"n": 1})).unwrap_err();
        assert!(matches!(error, EmitError::Write(_)), "{error}");
    }
}
