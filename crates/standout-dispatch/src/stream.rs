//! The entry stream a handler writes to under the `ndjson` output mode
//! (`docs/spec/parity-machine-contract.md`, D3).
//!
//! [`EntryStream::emit`] serializes one value as compact JSON and writes it
//! as one line, followed by a flush, so a consumer reading the pipe sees the
//! entry when the handler produced it. The framework builds the stream at the
//! dispatch edge: live, over a [`StreamSink`], when the resolved mode is
//! `ndjson`; discarding otherwise, in which case `emit` neither serializes
//! nor writes. Nothing more than line-per-value: no buffering, no
//! backpressure, no async.

use serde::Serialize;
use std::cell::RefCell;
use std::fmt;
use std::io::Write;
use std::rc::Rc;

#[derive(Clone)]
pub struct StreamSink(Rc<RefCell<dyn Write>>);

impl StreamSink {
    pub fn new(writer: impl Write + 'static) -> Self {
        Self(Rc::new(RefCell::new(writer)))
    }

    pub fn process_stdout() -> Self {
        Self::new(std::io::stdout())
    }

    fn write_line(&self, line: &[u8]) -> std::io::Result<()> {
        let mut writer = self.0.borrow_mut();
        writer.write_all(line)?;
        writer.write_all(b"\n")?;
        writer.flush()
    }
}

impl fmt::Debug for StreamSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StreamSink")
    }
}

#[derive(Clone, Debug, Default)]
pub struct EntryStream {
    sink: Option<StreamSink>,
}

impl EntryStream {
    pub fn discarding() -> Self {
        Self { sink: None }
    }

    pub fn writing_to(sink: StreamSink) -> Self {
        Self { sink: Some(sink) }
    }

    /// Whether `emit` writes: true only under `ndjson`, so a handler that
    /// streams its result as entries can return `Output::Silent` here and
    /// `Output::Render` elsewhere.
    pub fn is_live(&self) -> bool {
        self.sink.is_some()
    }

    /// Write `entry` as one JSON line, or do nothing on a discarding stream.
    /// Fails when the value does not serialize or the line cannot be
    /// written; a handler propagates the error and the run fails with it.
    pub fn emit<T: Serialize + ?Sized>(&self, entry: &T) -> Result<(), StreamError> {
        let Some(sink) = &self.sink else {
            return Ok(());
        };
        let line = serde_json::to_vec(entry)?;
        sink.write_line(&line)?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("stream entry does not serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("stream entry could not be written: {0}")]
    Write(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Entry<'a> {
        #[serde(rename = "type")]
        entry_type: &'a str,
        resource: &'a str,
    }

    #[derive(Clone, Default)]
    struct Captured(Rc<RefCell<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_live_stream_writes_one_compact_line_per_entry() {
        let captured = Captured::default();
        let stream = EntryStream::writing_to(StreamSink::new(captured.clone()));
        assert!(stream.is_live());
        stream
            .emit(&Entry {
                entry_type: "apply_start",
                resource: "web",
            })
            .unwrap();
        stream
            .emit(&Entry {
                entry_type: "note",
                resource: "line\nbreak",
            })
            .unwrap();
        assert_eq!(
            String::from_utf8(captured.0.borrow().clone()).unwrap(),
            "{\"type\":\"apply_start\",\"resource\":\"web\"}\n{\"type\":\"note\",\"resource\":\"line\\nbreak\"}\n"
        );
    }

    #[test]
    fn a_discarding_stream_neither_serializes_nor_writes() {
        struct Unserializable;
        impl Serialize for Unserializable {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("never asked"))
            }
        }
        let stream = EntryStream::discarding();
        assert!(!stream.is_live());
        stream.emit(&Unserializable).unwrap();
        assert!(EntryStream::default().emit(&Unserializable).is_ok());
    }

    #[test]
    fn serialization_and_write_failures_are_distinct_errors() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "closed",
                ))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let stream = EntryStream::writing_to(StreamSink::new(Closed));
        let write = stream.emit(&serde_json::json!({})).unwrap_err();
        assert!(matches!(write, StreamError::Write(_)), "{write}");

        let mut map = std::collections::HashMap::new();
        map.insert((1u8, 2u8), 3u8);
        let stream = EntryStream::writing_to(StreamSink::new(Vec::new()));
        let serialize = stream.emit(&map).unwrap_err();
        assert!(
            matches!(serialize, StreamError::Serialize(_)),
            "{serialize}"
        );
    }
}
