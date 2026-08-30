use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Text,
    Password,
    Editor,
    Confirm,
    Select,
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

#[derive(Debug, Clone, Copy)]
pub struct PromptContext<'a> {
    pub kind: PromptKind,
    pub message: &'a str,
    pub options: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum PromptResponse {
    Text(String),
    Bool(bool),
    Choice(usize),
    Choices(Vec<usize>),
    Cancel,
    Skip,
}

impl PromptResponse {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    pub fn choices(indices: impl IntoIterator<Item = usize>) -> Self {
        Self::Choices(indices.into_iter().collect())
    }

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

pub trait PromptResponder: Send + Sync {
    fn respond(&self, ctx: PromptContext<'_>) -> PromptResponse;
}

pub struct ScriptedResponder {
    queue: Mutex<std::collections::VecDeque<PromptResponse>>,
}

impl ScriptedResponder {
    pub fn new(responses: impl IntoIterator<Item = PromptResponse>) -> Self {
        Self {
            queue: Mutex::new(responses.into_iter().collect()),
        }
    }

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
        assert!(matches!(
            r.respond(ctx(PromptKind::Select, Some(2))),
            PromptResponse::Cancel
        ));
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
