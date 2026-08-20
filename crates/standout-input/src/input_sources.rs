//! Explicit stdin, clipboard, and prompt-responder for one invocation.
//!
//! Production `App::run` constructs [`InputSources`] from the real process;
//! tests put mocks in the same type. It is owned and not `Copy` (readers, not
//! scalars). It is not bundled with destination properties into a combined
//! run-environment type.

use std::fmt;
use std::sync::Arc;

use crate::env::{ClipboardReader, RealClipboard, RealStdin, StdinReader};
use crate::responder::PromptResponder;

/// Stdin, clipboard, and prompt-responder used for one invocation.
///
/// Constructed from the real process in production ([`from_process`](Self::from_process));
/// constructed with mocks by tests ([`new`](Self::new), [`with_stdin`](Self::with_stdin)
/// and siblings). Passed into `App` next to target properties as a second
/// argument, not stored on `App`. Input collection takes this value as an
/// argument; it does not read process-global default readers.
#[derive(Clone)]
pub struct InputSources {
    stdin: Arc<dyn StdinReader>,
    clipboard: Arc<dyn ClipboardReader>,
    responder: Option<Arc<dyn PromptResponder>>,
}

impl fmt::Debug for InputSources {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputSources")
            .field("has_responder", &self.responder.is_some())
            .finish_non_exhaustive()
    }
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

    /// Constructs sources from the real process (stdin, clipboard, no prompt
    /// responder).
    ///
    /// Production `App::run` calls this at the process edge. Interactive
    /// prompts then use the real backend; tests that need a scripted responder
    /// call [`with_responder`](Self::with_responder) on a value they construct.
    pub fn from_process() -> Self {
        Self::new(RealStdin, RealClipboard, None)
    }

    /// Replaces the stdin reader.
    pub fn with_stdin(mut self, stdin: impl StdinReader + 'static) -> Self {
        self.stdin = Arc::new(stdin);
        self
    }

    /// Replaces the clipboard reader.
    pub fn with_clipboard(mut self, clipboard: impl ClipboardReader + 'static) -> Self {
        self.clipboard = Arc::new(clipboard);
        self
    }

    /// Sets the prompt responder used by `.prompt_from`, `resolve_from`, and
    /// framework interactive collection.
    ///
    /// Standalone `.prompt()` builds [`InputSources::from_process`] and never
    /// sees this responder. Harness tests call `.prompt_from(ctx.input_sources())`
    /// (or resolve chains against those sources).
    pub fn with_responder(mut self, responder: Arc<dyn PromptResponder>) -> Self {
        self.responder = Some(responder);
        self
    }

    /// Stdin reader for this invocation.
    pub fn stdin(&self) -> &dyn StdinReader {
        self.stdin.as_ref()
    }

    /// Shared stdin reader handle, for binding collectors at resolve time.
    pub fn stdin_arc(&self) -> Arc<dyn StdinReader> {
        Arc::clone(&self.stdin)
    }

    /// Clipboard reader for this invocation.
    pub fn clipboard(&self) -> &dyn ClipboardReader {
        self.clipboard.as_ref()
    }

    /// Shared clipboard reader handle, for binding collectors at resolve time.
    pub fn clipboard_arc(&self) -> Arc<dyn ClipboardReader> {
        Arc::clone(&self.clipboard)
    }

    /// Prompt responder for this invocation, if one was supplied.
    pub fn responder(&self) -> Option<&dyn PromptResponder> {
        self.responder.as_deref()
    }

    /// Shared prompt-responder handle, if one was supplied.
    pub fn responder_arc(&self) -> Option<Arc<dyn PromptResponder>> {
        self.responder.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::{MockClipboard, MockStdin};

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
        // Probe the concrete type. A generic helper would pick the unconstrained
        // impl while type-checking the helper and stay green if `InputSources`
        // later gained `Copy`.
        struct Probe<U>(std::marker::PhantomData<U>);
        trait AmbiguousIfImpl<A> {
            fn check() {}
        }
        impl<U> AmbiguousIfImpl<()> for Probe<U> {}
        impl<U: Copy> AmbiguousIfImpl<u8> for Probe<U> {}
        let _ = <Probe<InputSources> as AmbiguousIfImpl<_>>::check;

        let sources = sample();
        let moved = sources;
        assert!(!moved.stdin().is_terminal());
    }

    #[test]
    fn input_sources_is_clone() {
        let sources = sample();
        let cloned = sources.clone();
        assert_eq!(cloned.stdin().read_to_string().unwrap(), "hello");
        assert_eq!(sources.stdin().read_to_string().unwrap(), "hello");
    }

    #[test]
    fn input_sources_debug_is_structural() {
        let sources = sample();
        let debug = format!("{sources:?}");
        assert!(debug.contains("InputSources"));
        assert!(debug.contains("has_responder: false"));
    }

    #[test]
    fn from_process_constructs_production_sources() {
        let sources = InputSources::from_process();
        let _ = sources.stdin().is_terminal();
        assert!(sources.responder().is_none());
    }

    #[test]
    fn builders_replace_readers() {
        let sources = InputSources::from_process()
            .with_stdin(MockStdin::piped("in"))
            .with_clipboard(MockClipboard::with_content("clip"));
        assert_eq!(sources.stdin().read_to_string().unwrap(), "in");
        assert_eq!(
            sources.clipboard().read().unwrap(),
            Some("clip".to_string())
        );
    }
}
