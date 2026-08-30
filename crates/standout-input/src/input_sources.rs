use std::fmt;
use std::sync::Arc;

use crate::env::{ClipboardReader, RealClipboard, RealStdin, StdinReader};
use crate::responder::PromptResponder;

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

    pub fn from_process() -> Self {
        Self::new(RealStdin, RealClipboard, None)
    }

    pub fn with_stdin(mut self, stdin: impl StdinReader + 'static) -> Self {
        self.stdin = Arc::new(stdin);
        self
    }

    pub fn with_clipboard(mut self, clipboard: impl ClipboardReader + 'static) -> Self {
        self.clipboard = Arc::new(clipboard);
        self
    }

    pub fn with_responder(mut self, responder: Arc<dyn PromptResponder>) -> Self {
        self.responder = Some(responder);
        self
    }

    pub fn stdin(&self) -> &dyn StdinReader {
        self.stdin.as_ref()
    }

    pub fn stdin_arc(&self) -> Arc<dyn StdinReader> {
        Arc::clone(&self.stdin)
    }

    pub fn clipboard(&self) -> &dyn ClipboardReader {
        self.clipboard.as_ref()
    }

    pub fn clipboard_arc(&self) -> Arc<dyn ClipboardReader> {
        Arc::clone(&self.clipboard)
    }

    pub fn responder(&self) -> Option<&dyn PromptResponder> {
        self.responder.as_deref()
    }

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
