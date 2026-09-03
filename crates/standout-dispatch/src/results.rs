//! The typed results channel a handler produces values through.
//!
//! A command produces either one batch value, which the handler returns, or a
//! sequence of typed events it emits through [`Results`] before returning the
//! summary. Both are results: the values the command exists to produce, as
//! opposed to operational messages about the run.
//!
//! [`Results`] carries the run's destination behind an `Rc`, so it needs no
//! lifetime parameter and `Handler::handle` gains none. A batch command sets
//! `Handler::Event` to [`NoEvents`], whose uninhabited type leaves `emit` with
//! no argument that can be constructed.
//!
//! [`RunRecorder`] is the destination an in-process entry point installs: it
//! retains each value as data, whatever representation the run selected, and
//! carries the run's [`Delivery`] decision, so a test asserts on the values and
//! on the rendered bytes separately.

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

#[derive(Debug, Default)]
struct RunRecord {
    records: Vec<serde_json::Value>,
    delivery: Delivery,
}

/// Retains the run's result values and its delivery decision.
#[derive(Debug, Clone, Default)]
pub struct RunRecorder(Rc<RefCell<RunRecord>>);

impl RunRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, value: serde_json::Value) {
        self.0.borrow_mut().records.push(value);
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
#[derive(Debug, Clone, Default)]
pub struct Results<E: Serialize> {
    recorder: Option<RunRecorder>,
    _event: PhantomData<fn(E)>,
}

impl<E: Serialize> Results<E> {
    pub fn discarding() -> Self {
        Self {
            recorder: None,
            _event: PhantomData,
        }
    }

    pub fn recording(recorder: RunRecorder) -> Self {
        Self {
            recorder: Some(recorder),
            _event: PhantomData,
        }
    }

    /// Returns once the value has been retained; fails when it does not serialize.
    pub fn emit(&mut self, event: E) -> Result<(), EmitError> {
        let Some(recorder) = &self.recorder else {
            return Ok(());
        };
        recorder.record(serde_json::to_value(&event)?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn emitted_events_reach_the_recorder_and_a_discarding_sink_keeps_nothing() {
        let recorder = RunRecorder::new();
        let mut results = Results::recording(recorder.clone());
        results
            .emit(serde_json::json!({"type": "apply_start"}))
            .unwrap();
        assert_eq!(recorder.records().len(), 1);

        let mut discarding = Results::<serde_json::Value>::discarding();
        discarding
            .emit(serde_json::json!({"type": "note"}))
            .unwrap();
        assert_eq!(recorder.records().len(), 1);
    }

    #[test]
    fn an_unserializable_event_is_an_emit_error() {
        let mut results = Results::recording(RunRecorder::new());
        let mut map = std::collections::HashMap::new();
        map.insert((1u8, 2u8), 3u8);
        let error = results.emit(map).unwrap_err();
        assert!(matches!(error, EmitError::Serialize(_)), "{error}");
    }
}
