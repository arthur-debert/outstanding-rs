//! Test injection for interactive prompts.
//!
//! Wizard / setup-helper / REPL flows that build on the `.prompt()` shortcut
//! on every interactive source ([`InquireText`](crate::InquireText),
//! [`InquireSelect`](crate::InquireSelect), [`TextPromptSource`](crate::TextPromptSource),
//! and friends) are otherwise untestable in process — the inquire backends
//! reach for raw stdin and the simple-prompts and editor sources need a TTY.
//!
//! [`PromptResponder`] is the test seam: every `.prompt()` call consults an
//! explicit responder from [`crate::InputSources`] first, and falls through
//! to the real backend only when none was supplied. Tests put a
//! [`ScriptedResponder`] on [`crate::InputSources`] with a queue of typed
//! [`PromptResponse`] values; production wizard code that does not pass
//! sources uses the real backend.
//!
//! # Why responses are typed by *kind*, not by message text
//!
//! For finite-choice prompts ([`Select`](PromptKind::Select),
//! [`MultiSelect`](PromptKind::MultiSelect), [`Confirm`](PromptKind::Confirm))
//! the response is the *position* (or boolean) — never the option's display
//! label. Renaming "Production" to "Live" doesn't break a test that picked
//! `Choice(2)`. Same for confirm: a test asserts on `true`/`false`, not on
//! the prompt copy.
//!
//! Open prompts ([`Text`](PromptKind::Text), [`Password`](PromptKind::Password),
//! [`Editor`](PromptKind::Editor)) take a `String`, since the value *is* the
//! free-form answer.
//!
//! See the "Testing Wizards" section in the
//! [Interactive Flows topic](../../docs/topics/interactive-flows.md) for a
//! full example.

use std::sync::Mutex;

/// The kind of prompt being responded to.
///
/// The interactive source passes its kind to the responder; the responder
/// returns a [`PromptResponse`]. A scripted responder uses the kind to
/// validate that the next queued response matches what the source actually
/// asked for, panicking with a descriptive message on mismatch (a wizard-
/// reorder bug surfaces at the test, not as a silent wrong-data assert
/// downstream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// Free-form text input ([`InquireText`](crate::InquireText),
    /// [`TextPromptSource`](crate::TextPromptSource)).
    Text,
    /// Masked password input ([`InquirePassword`](crate::InquirePassword)).
    Password,
    /// Editor-based multi-line input ([`EditorSource`](crate::EditorSource),
    /// [`InquireEditor`](crate::InquireEditor)).
    Editor,
    /// Yes/no ([`InquireConfirm`](crate::InquireConfirm),
    /// [`ConfirmPromptSource`](crate::ConfirmPromptSource)).
    Confirm,
    /// Single selection from a list ([`InquireSelect`](crate::InquireSelect)).
    Select,
    /// Multi-selection from a list ([`InquireMultiSelect`](crate::InquireMultiSelect)).
    MultiSelect,
}

impl std::fmt::Display for PromptKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Password => write!(f, "password"),
            Self::Editor => write!(f, "editor"),
            Self::Confirm => write!(f, "confirm"),
            Self::Select => write!(f, "select"),
            Self::MultiSelect => write!(f, "multi-select"),
        }
    }
}

/// Context the source passes to a [`PromptResponder`].
///
/// Includes everything a smart responder might want: the prompt kind, the
/// human-facing message (for diagnostic / advanced matching), and — for
/// finite-choice prompts — the size of the option list so a `Choice(i)`
/// response can be range-checked.
#[derive(Debug, Clone, Copy)]
pub struct PromptContext<'a> {
    /// What kind of prompt is asking.
    pub kind: PromptKind,
    /// The human-facing prompt message (e.g. `"Pack name:"`).
    ///
    /// Mostly useful for diagnostics in panic messages and for advanced
    /// responders that want to match on text. Position-based scripted
    /// responders don't need to consult it.
    pub message: &'a str,
    /// Size of the option list, for `Select` / `MultiSelect`. `None` for
    /// open prompts and confirm.
    pub options: Option<usize>,
}

/// A response a [`PromptResponder`] can return.
#[derive(Debug, Clone)]
pub enum PromptResponse {
    /// Free-form text answer for [`Text`](PromptKind::Text),
    /// [`Password`](PromptKind::Password), and
    /// [`Editor`](PromptKind::Editor) prompts.
    Text(String),
    /// Boolean answer for [`Confirm`](PromptKind::Confirm) prompts.
    Bool(bool),
    /// Index of the chosen option for [`Select`](PromptKind::Select) prompts.
    /// Must be `< options` or the source will panic.
    Choice(usize),
    /// Indices of the chosen options for [`MultiSelect`](PromptKind::MultiSelect).
    /// Each must be `< options`.
    Choices(Vec<usize>),
    /// Surface this prompt as user cancellation
    /// ([`InputError::PromptCancelled`](crate::InputError::PromptCancelled)).
    Cancel,
    /// Surface this prompt as "no input"
    /// ([`InputError::NoInput`](crate::InputError::NoInput)) — the same path
    /// the source takes when stdin is not a TTY.
    Skip,
}

impl PromptResponse {
    /// Convenience constructor for a text response.
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    /// Convenience constructor for a multi-select response.
    pub fn choices(indices: impl IntoIterator<Item = usize>) -> Self {
        Self::Choices(indices.into_iter().collect())
    }

    /// Returns the kind this response is *valid* for, if any. `Cancel` and
    /// `Skip` are always valid, so they return `None`.
    pub(crate) fn expected_kind(&self) -> Option<&'static [PromptKind]> {
        match self {
            Self::Text(_) => Some(&[PromptKind::Text, PromptKind::Password, PromptKind::Editor]),
            Self::Bool(_) => Some(&[PromptKind::Confirm]),
            Self::Choice(_) => Some(&[PromptKind::Select]),
            Self::Choices(_) => Some(&[PromptKind::MultiSelect]),
            Self::Cancel | Self::Skip => None,
        }
    }
}

/// Test seam for the `.prompt()` shortcut on interactive sources.
///
/// When a responder is present on [`crate::InputSources`], every `prompt()`
/// call that receives those sources routes through it instead of opening a
/// real prompt. Implement this trait for custom dispatch logic, or use the
/// bundled [`ScriptedResponder`].
pub trait PromptResponder: Send + Sync {
    /// Produce a response for the given prompt.
    fn respond(&self, ctx: PromptContext<'_>) -> PromptResponse;
}

/// A position-based scripted responder.
///
/// Built from a queue of [`PromptResponse`] values. Each call to
/// [`respond`](PromptResponder::respond) pops the next response and
/// validates that its kind is compatible with the prompt the source
/// actually asked for; if not, it panics with a message that names the
/// position, the prompt kind, and the response kind.
///
/// This makes wizard-reorder bugs surface as test failures at the offending
/// step rather than as silent wrong-data assertions later.
///
/// ```
/// use standout_input::{ScriptedResponder, PromptResponse};
///
/// let responder = ScriptedResponder::new([
///     PromptResponse::text("buy milk"),
///     PromptResponse::Bool(true),
///     PromptResponse::Choice(2),
/// ]);
/// ```
pub struct ScriptedResponder {
    queue: Mutex<std::collections::VecDeque<PromptResponse>>,
}

impl ScriptedResponder {
    /// Create a scripted responder from a sequence of responses.
    pub fn new(responses: impl IntoIterator<Item = PromptResponse>) -> Self {
        Self {
            queue: Mutex::new(responses.into_iter().collect()),
        }
    }

    /// Number of responses still queued.
    pub fn remaining(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
}

impl PromptResponder for ScriptedResponder {
    fn respond(&self, ctx: PromptContext<'_>) -> PromptResponse {
        let response = self.queue.lock().unwrap().pop_front().unwrap_or_else(|| {
            panic!(
                "ScriptedResponder ran out of responses; \
                 next prompt was a `{}` prompt with message {:?}",
                ctx.kind, ctx.message
            )
        });

        if let Some(allowed) = response.expected_kind() {
            if !allowed.contains(&ctx.kind) {
                panic!(
                    "ScriptedResponder kind mismatch: expected response for `{}` prompt \
                     ({:?}), but got {:?}",
                    ctx.kind, ctx.message, response
                );
            }
        }

        // Range-check by reference so we don't move the response we're
        // about to return.
        if let PromptResponse::Choice(i) = &response {
            let n = ctx.options.unwrap_or(0);
            assert!(
                *i < n,
                "ScriptedResponder: Choice({i}) is out of range for select prompt \
                 with {n} option(s) ({:?})",
                ctx.message
            );
        }
        if let PromptResponse::Choices(indices) = &response {
            let n = ctx.options.unwrap_or(0);
            for &i in indices {
                assert!(
                    i < n,
                    "ScriptedResponder: Choices contains {i}, out of range for \
                     multi-select prompt with {n} option(s) ({:?})",
                    ctx.message
                );
            }
        }

        response
    }
}

impl std::fmt::Debug for ScriptedResponder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptedResponder")
            .field("remaining", &self.remaining())
            .finish()
    }
}

/// Helper used by source `.prompt()` shortcuts that return a free-form
/// `String` (text / password / editor prompts).
///
/// If a responder is supplied, dispatches and maps `Text(s) -> Ok(s)`,
/// `Cancel -> PromptCancelled`, `Skip -> NoInput`. Returns `Ok(None)` (i.e.
/// "fall through to the real backend") when no responder is supplied, so
/// the caller can use the original `is_available` + `collect` path.
///
/// `Bool` / `Choice` / `Choices` responses against an open prompt panic
/// via `ScriptedResponder`'s validation in production tests.
#[cfg(any(feature = "editor", feature = "simple-prompts", feature = "inquire"))]
pub(crate) fn intercept_text(
    kind: PromptKind,
    message: &str,
    responder: Option<&dyn PromptResponder>,
) -> Result<Option<String>, crate::InputError> {
    let Some(responder) = responder else {
        return Ok(None);
    };
    let response = responder.respond(PromptContext {
        kind,
        message,
        options: None,
    });
    match response {
        PromptResponse::Text(s) => Ok(Some(s)),
        PromptResponse::Cancel => Err(crate::InputError::PromptCancelled),
        PromptResponse::Skip => Err(crate::InputError::NoInput),
        other => panic!(
            "PromptResponder returned {other:?} for a `{kind}` prompt; \
             expected Text / Cancel / Skip"
        ),
    }
}

/// Helper for `.prompt()` shortcuts that return a `bool`
/// ([`InquireConfirm`](crate::InquireConfirm),
/// [`ConfirmPromptSource`](crate::ConfirmPromptSource)).
#[cfg(any(feature = "simple-prompts", feature = "inquire"))]
pub(crate) fn intercept_bool(
    kind: PromptKind,
    message: &str,
    responder: Option<&dyn PromptResponder>,
) -> Result<Option<bool>, crate::InputError> {
    let Some(responder) = responder else {
        return Ok(None);
    };
    let response = responder.respond(PromptContext {
        kind,
        message,
        options: None,
    });
    match response {
        PromptResponse::Bool(b) => Ok(Some(b)),
        PromptResponse::Cancel => Err(crate::InputError::PromptCancelled),
        PromptResponse::Skip => Err(crate::InputError::NoInput),
        other => panic!(
            "PromptResponder returned {other:?} for a `{kind}` prompt; \
             expected Bool / Cancel / Skip"
        ),
    }
}

/// Helper for [`InquireSelect`](crate::InquireSelect)::prompt(). Returns
/// the selected *index* into the source's options vector; the caller
/// performs the `options[i].clone()` so the typed `T` flows out.
#[cfg(feature = "inquire")]
pub(crate) fn intercept_choice(
    message: &str,
    n: usize,
    responder: Option<&dyn PromptResponder>,
) -> Result<Option<usize>, crate::InputError> {
    let Some(responder) = responder else {
        return Ok(None);
    };
    let response = responder.respond(PromptContext {
        kind: PromptKind::Select,
        message,
        options: Some(n),
    });
    match response {
        PromptResponse::Choice(i) => {
            assert!(
                i < n,
                "PromptResponder returned Choice({i}) for select prompt with {n} option(s)"
            );
            Ok(Some(i))
        }
        PromptResponse::Cancel => Err(crate::InputError::PromptCancelled),
        PromptResponse::Skip => Err(crate::InputError::NoInput),
        other => panic!(
            "PromptResponder returned {other:?} for a `select` prompt; \
             expected Choice / Cancel / Skip"
        ),
    }
}

/// Helper for [`InquireMultiSelect`](crate::InquireMultiSelect)::prompt().
/// Returns the selected indices.
#[cfg(feature = "inquire")]
pub(crate) fn intercept_choices(
    message: &str,
    n: usize,
    responder: Option<&dyn PromptResponder>,
) -> Result<Option<Vec<usize>>, crate::InputError> {
    let Some(responder) = responder else {
        return Ok(None);
    };
    let response = responder.respond(PromptContext {
        kind: PromptKind::MultiSelect,
        message,
        options: Some(n),
    });
    match response {
        PromptResponse::Choices(indices) => {
            for &i in &indices {
                assert!(
                    i < n,
                    "PromptResponder returned Choices containing {i} for multi-select \
                     prompt with {n} option(s)"
                );
            }
            Ok(Some(indices))
        }
        PromptResponse::Cancel => Err(crate::InputError::PromptCancelled),
        PromptResponse::Skip => Err(crate::InputError::NoInput),
        other => panic!(
            "PromptResponder returned {other:?} for a `multi-select` prompt; \
             expected Choices / Cancel / Skip"
        ),
    }
}

/// Maps a `.prompt()` intercept result into [`InputCollector::collect`](crate::InputCollector::collect).
///
/// `Ok(Some(v))` is an answered value (`Break(Some(v))`). `Ok(None)` means no
/// responder was supplied, so the collector should use its real backend
/// (`Continue`). `Err(NoInput)` is a responder `Skip`, returned as
/// `Break(None)` so an input chain can try the next source instead of aborting.
#[cfg(any(feature = "editor", feature = "simple-prompts", feature = "inquire"))]
pub(crate) fn collect_intercept<T>(
    intercepted: Result<Option<T>, crate::InputError>,
) -> Result<std::ops::ControlFlow<Option<T>>, crate::InputError> {
    match intercepted {
        Ok(Some(value)) => Ok(std::ops::ControlFlow::Break(Some(value))),
        Ok(None) => Ok(std::ops::ControlFlow::Continue(())),
        Err(crate::InputError::NoInput) => Ok(std::ops::ControlFlow::Break(None)),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(kind: PromptKind, options: Option<usize>) -> PromptContext<'static> {
        PromptContext {
            kind,
            message: "test prompt",
            options,
        }
    }

    #[test]
    fn scripted_responder_returns_in_order() {
        let r = ScriptedResponder::new([
            PromptResponse::text("first"),
            PromptResponse::Bool(true),
            PromptResponse::Choice(1),
        ]);
        assert!(
            matches!(r.respond(ctx(PromptKind::Text, None)), PromptResponse::Text(s) if s == "first")
        );
        assert!(matches!(
            r.respond(ctx(PromptKind::Confirm, None)),
            PromptResponse::Bool(true)
        ));
        assert!(matches!(
            r.respond(ctx(PromptKind::Select, Some(3))),
            PromptResponse::Choice(1)
        ));
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn cancel_and_skip_are_kind_agnostic() {
        let r = ScriptedResponder::new([PromptResponse::Cancel, PromptResponse::Skip]);
        // Cancel is fine for any kind
        assert!(matches!(
            r.respond(ctx(PromptKind::Select, Some(2))),
            PromptResponse::Cancel
        ));
        // Skip too
        assert!(matches!(
            r.respond(ctx(PromptKind::Confirm, None)),
            PromptResponse::Skip
        ));
    }

    #[test]
    fn text_response_works_for_all_open_kinds() {
        let r = ScriptedResponder::new([
            PromptResponse::text("a"),
            PromptResponse::text("b"),
            PromptResponse::text("c"),
        ]);
        assert!(matches!(
            r.respond(ctx(PromptKind::Text, None)),
            PromptResponse::Text(_)
        ));
        assert!(matches!(
            r.respond(ctx(PromptKind::Password, None)),
            PromptResponse::Text(_)
        ));
        assert!(matches!(
            r.respond(ctx(PromptKind::Editor, None)),
            PromptResponse::Text(_)
        ));
    }

    #[test]
    #[should_panic(expected = "kind mismatch")]
    fn scripted_responder_panics_on_kind_mismatch() {
        let r = ScriptedResponder::new([PromptResponse::text("oops")]);
        // Confirm prompt with a Text response — wizard order changed and
        // the test should fail loudly here, not 3 lines later.
        let _ = r.respond(ctx(PromptKind::Confirm, None));
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn scripted_responder_panics_on_out_of_range_choice() {
        let r = ScriptedResponder::new([PromptResponse::Choice(5)]);
        let _ = r.respond(ctx(PromptKind::Select, Some(3)));
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn scripted_responder_panics_on_out_of_range_multiselect() {
        let r = ScriptedResponder::new([PromptResponse::choices([0, 7])]);
        let _ = r.respond(ctx(PromptKind::MultiSelect, Some(3)));
    }

    #[test]
    #[should_panic(expected = "ran out of responses")]
    fn scripted_responder_panics_when_exhausted() {
        let r = ScriptedResponder::new([PromptResponse::text("only")]);
        let _ = r.respond(ctx(PromptKind::Text, None));
        let _ = r.respond(ctx(PromptKind::Text, None));
    }
}
