//! Explicit stdin, clipboard, and prompt-responder for one invocation.
//!
//! Production `App::run` will construct [`InputSources`] from the real process;
//! tests put mocks in the same type. It is owned and not `Copy` (readers, not
//! scalars). It is not bundled with destination properties into a combined
//! run-environment type.

use std::sync::Arc;

use crate::env::{ClipboardReader, StdinReader};
use crate::responder::PromptResponder;

/// Stdin, clipboard, and prompt-responder used for one invocation.
///
/// Constructed from the real process in production ([`from_process`](Self::from_process));
/// constructed with mocks by tests ([`new`](Self::new)). Passed into `App`
/// next to target properties as a second argument, not stored on `App`.
pub struct InputSources {
    stdin: Arc<dyn StdinReader>,
    clipboard: Arc<dyn ClipboardReader>,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl InputSources {
    /// Builds sources from explicit readers.
    ///
    /// Tests inject [`crate::MockStdin`], [`crate::MockClipboard`], and an
    /// optional [`crate::PromptResponder`] here. This stores the values; it
    /// does not probe the process.
    pub fn new(
        stdin: impl StdinReader + 'static,
        clipboard: impl ClipboardReader + 'static,
        responder: Option<Arc<dyn PromptResponder>>,
    ) -> Self {
        Self {
            stdin: Arc::new(stdin),
            clipboard: Arc::new(clipboard),
            responder,
        }
    }

    /// Constructs sources from the real process (stdin, clipboard, prompt
    /// responder).
    ///
    /// Production `App::run` will call this at the process edge. The body is
    /// unimplemented in this workstream (`todo!()`).
    pub fn from_process() -> Self {
        todo!("ROB04-WS01 lands this signature only; real-process construction is later")
    }

    /// Stdin reader for this invocation.
    pub fn stdin(&self) -> &dyn StdinReader {
        self.stdin.as_ref()
    }

    /// Clipboard reader for this invocation.
    pub fn clipboard(&self) -> &dyn ClipboardReader {
        self.clipboard.as_ref()
    }

    /// Prompt responder for this invocation, if one was supplied.
    pub fn responder(&self) -> Option<&dyn PromptResponder> {
        self.responder.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::{MockClipboard, MockStdin};

    /// Compile-time probe: if `T` implements `Copy`, both `AmbiguousIfImpl`
    /// impls apply and `<Probe<T> as AmbiguousIfImpl<_>>::check` is ambiguous.
    fn assert_not_copy<T>() {
        struct Probe<U>(std::marker::PhantomData<U>);
        trait AmbiguousIfImpl<A> {
            fn check() {}
        }
        impl<U> AmbiguousIfImpl<()> for Probe<U> {}
        impl<U: Copy> AmbiguousIfImpl<u8> for Probe<U> {}
        let _ = <Probe<T> as AmbiguousIfImpl<_>>::check;
    }

    fn sample() -> InputSources {
        InputSources::new(MockStdin::piped("hello"), MockClipboard::empty(), None)
    }

    #[test]
    fn input_sources_constructs_from_explicit_readers() {
        let sources = sample();
        assert!(!sources.stdin().is_terminal());
        assert_eq!(sources.stdin().read_to_string().unwrap(), "hello");
        assert_eq!(sources.clipboard().read().unwrap(), None);
        assert!(sources.responder().is_none());
    }

    #[test]
    fn input_sources_is_not_copy() {
        assert_not_copy::<InputSources>();
        let sources = sample();
        let moved = sources;
        assert!(!moved.stdin().is_terminal());
    }
}
