//! The one destination a run's rendered bytes reach.
//!
//! Every byte a run produces goes through a [`StreamSink`]: a handler's
//! incremental events as they are emitted, then the result or the diagnostic,
//! then the warning entries. The process edge writes through it to stdout; a
//! capture entry point hands it a [`StreamCapture`] and reads the bytes back;
//! an output file override retargets it before the handler runs, so the file
//! receives the whole run and stdout nothing — [`StreamSink::redirect`] straight
//! away, or [`StreamSink::redirect_on_first_write`] when a run that writes
//! nothing should leave the file uncreated.
//!
//! The sink classifies a `BrokenPipe` from its writer rather than reporting
//! it: a reader that left closes the sink, every later write is discarded and
//! reports success, and the handler runs to completion and the run reports the
//! command's own status.

use std::cell::RefCell;
use std::fmt;
use std::io::{ErrorKind, Write};
use std::rc::Rc;

type OpenWriter = Box<dyn FnOnce() -> std::io::Result<Box<dyn Write>>>;

struct Destination {
    writer: Box<dyn Write>,
    pending: Option<OpenWriter>,
    /// A deferred destination that could not be opened. The redirect replaced
    /// the writer the run would otherwise have used, so every later write
    /// reports the same reason rather than falling back to that writer.
    unopened: Option<(ErrorKind, String)>,
    open: bool,
}

impl Destination {
    fn ready(&mut self) -> std::io::Result<()> {
        if let Some((kind, message)) = &self.unopened {
            return Err(std::io::Error::new(*kind, message.clone()));
        }
        let Some(open) = self.pending.take() else {
            return Ok(());
        };
        match open() {
            Ok(writer) => {
                self.writer = writer;
                Ok(())
            }
            Err(error) => {
                self.unopened = Some((error.kind(), error.to_string()));
                Err(error)
            }
        }
    }
}

impl Write for Destination {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if !self.open {
            return Ok(buf.len());
        }
        self.ready()?;
        match self.writer.write(buf) {
            Err(error) if error.kind() == ErrorKind::BrokenPipe => {
                self.open = false;
                Ok(buf.len())
            }
            other => other,
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.open {
            return Ok(());
        }
        if let Some((kind, message)) = &self.unopened {
            return Err(std::io::Error::new(*kind, message.clone()));
        }
        if self.pending.is_some() {
            return Ok(());
        }
        match self.writer.flush() {
            Err(error) if error.kind() == ErrorKind::BrokenPipe => {
                self.open = false;
                Ok(())
            }
            other => other,
        }
    }
}

#[derive(Clone)]
pub struct StreamSink(Rc<RefCell<Destination>>);

impl StreamSink {
    pub fn new(writer: impl Write + 'static) -> Self {
        Self(Rc::new(RefCell::new(Destination {
            writer: Box::new(writer),
            pending: None,
            unopened: None,
            open: true,
        })))
    }

    pub fn process_stdout() -> Self {
        Self::new(std::io::stdout())
    }

    /// Replace the destination; every clone of this sink follows.
    pub fn redirect(&self, writer: impl Write + 'static) {
        let mut destination = self.0.borrow_mut();
        destination.writer = Box::new(writer);
        destination.pending = None;
        destination.unopened = None;
        destination.open = true;
    }

    /// Replace the destination when there is a first byte to write, so a run
    /// that writes none leaves the writer unopened. Failing to open it fails
    /// that write and every later one.
    pub fn redirect_on_first_write<W: Write + 'static>(
        &self,
        open: impl FnOnce() -> std::io::Result<W> + 'static,
    ) {
        let mut destination = self.0.borrow_mut();
        destination.pending = Some(Box::new(move || {
            open().map(|w| Box::new(w) as Box<dyn Write>)
        }));
        destination.unopened = None;
        destination.open = true;
    }

    /// Drop a destination [`redirect_on_first_write`](Self::redirect_on_first_write)
    /// armed but never needed, so the run keeps the destination it had.
    pub fn cancel_pending_redirect(&self) {
        self.0.borrow_mut().pending = None;
    }

    /// False once a write met a `BrokenPipe`: what follows is discarded.
    pub fn is_open(&self) -> bool {
        self.0.borrow().open
    }

    pub fn with_writer<R>(&self, write: impl FnOnce(&mut dyn Write) -> R) -> R {
        write(&mut *self.0.borrow_mut())
    }

    /// One write of `bytes` and the newline that terminates it, then a flush.
    pub fn write_line(&self, bytes: &[u8]) -> std::io::Result<()> {
        self.with_writer(|writer| {
            writer.write_all(bytes)?;
            writer.write_all(b"\n")?;
            writer.flush()
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct StreamCapture(Rc<RefCell<Vec<u8>>>);

impl StreamCapture {
    pub fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

impl Write for StreamCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl fmt::Debug for StreamSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StreamSink")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Closed;

    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(ErrorKind::BrokenPipe, "closed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_sink_writes_one_line_per_call() {
        let captured = StreamCapture::default();
        let sink = StreamSink::new(captured.clone());
        sink.write_line(b"{\"n\":1}").unwrap();
        sink.write_line(b"{\"n\":2}").unwrap();
        assert_eq!(captured.take(), b"{\"n\":1}\n{\"n\":2}\n");
    }

    #[test]
    fn a_redirected_sink_moves_every_clone_to_the_new_destination() {
        let first = StreamCapture::default();
        let second = StreamCapture::default();
        let sink = StreamSink::new(first.clone());
        let clone = sink.clone();
        clone.write_line(b"{\"n\":1}").unwrap();
        sink.redirect(second.clone());
        clone.write_line(b"{\"n\":2}").unwrap();
        sink.with_writer(|w| w.write_all(b"tail\n")).unwrap();
        assert_eq!(first.take(), b"{\"n\":1}\n");
        assert_eq!(second.take(), b"{\"n\":2}\ntail\n");
    }

    #[test]
    fn a_reader_that_left_closes_the_sink_and_every_later_write_succeeds() {
        let sink = StreamSink::new(Closed);
        assert!(sink.is_open());
        sink.write_line(b"first").unwrap();
        assert!(!sink.is_open());
        sink.write_line(b"second").unwrap();
        sink.with_writer(|w| writeln!(w, "third")).unwrap();
    }

    #[test]
    fn a_deferred_destination_opens_on_the_first_write_and_not_before() {
        let opened = Rc::new(RefCell::new(0));
        let captured = StreamCapture::default();
        let sink = StreamSink::new(Vec::new());
        let count = opened.clone();
        let target = captured.clone();
        sink.redirect_on_first_write(move || {
            *count.borrow_mut() += 1;
            Ok(target)
        });
        assert_eq!(*opened.borrow(), 0);
        sink.write_line(b"first").unwrap();
        sink.write_line(b"second").unwrap();
        assert_eq!(*opened.borrow(), 1);
        assert_eq!(captured.take(), b"first\nsecond\n");
    }

    #[test]
    fn a_deferred_destination_that_cannot_open_fails_the_write_that_needed_it() {
        let sink = StreamSink::new(Vec::new());
        sink.redirect_on_first_write(|| -> std::io::Result<Vec<u8>> {
            Err(std::io::Error::new(
                ErrorKind::NotFound,
                "no such directory",
            ))
        });
        let error = sink.write_line(b"first").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn a_deferred_destination_that_cannot_open_keeps_failing_and_writes_nowhere() {
        let before = StreamCapture::default();
        let sink = StreamSink::new(before.clone());
        sink.redirect_on_first_write(|| -> std::io::Result<Vec<u8>> {
            Err(std::io::Error::new(
                ErrorKind::NotFound,
                "no such directory",
            ))
        });
        assert_eq!(
            sink.write_line(b"first").unwrap_err().kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            sink.write_line(b"second").unwrap_err().kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            sink.with_writer(|w| w.flush()).unwrap_err().kind(),
            ErrorKind::NotFound
        );
        assert!(before.take().is_empty());
    }

    #[test]
    fn a_write_failure_that_is_not_a_broken_pipe_is_reported() {
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(ErrorKind::StorageFull, "no room"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let sink = StreamSink::new(Full);
        assert!(sink.write_line(b"first").is_err());
        assert!(sink.is_open());
    }
}
