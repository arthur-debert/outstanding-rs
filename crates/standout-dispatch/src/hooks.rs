use crate::handler::{AppFailure, CommandContext, ExternalFailure};
use clap::ArgMatches;
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;
use thiserror::Error;
#[derive(Debug, Clone)]
pub struct TextOutput {
    pub formatted: String,
    pub raw: String,
}
impl TextOutput {
    pub fn new(formatted: String, raw: String) -> Self {
        Self { formatted, raw }
    }
    pub fn plain(text: String) -> Self {
        Self {
            formatted: text.clone(),
            raw: text,
        }
    }
}
#[derive(Debug, Clone)]
pub struct ArtifactOutput {
    pub bytes: Vec<u8>,
    pub suggested_destination: Option<PathBuf>,
    pub stdout_allowed: bool,
    pub report: Option<standout_types::RenderData>,
}
#[derive(Debug, Clone)]
pub enum RenderedOutput {
    Text(TextOutput),
    Binary(Vec<u8>, String),
    Artifact(ArtifactOutput),
    Silent,
}
impl RenderedOutput {
    pub fn is_text(&self) -> bool {
        matches!(self, RenderedOutput::Text(_))
    }
    pub fn is_binary(&self) -> bool {
        matches!(self, RenderedOutput::Binary(_, _))
    }
    pub fn is_artifact(&self) -> bool {
        matches!(self, RenderedOutput::Artifact(_))
    }
    pub fn is_silent(&self) -> bool {
        matches!(self, RenderedOutput::Silent)
    }
    pub fn as_text(&self) -> Option<&str> {
        match self {
            RenderedOutput::Text(t) => Some(&t.formatted),
            _ => None,
        }
    }
    pub fn as_raw_text(&self) -> Option<&str> {
        match self {
            RenderedOutput::Text(t) => Some(&t.raw),
            _ => None,
        }
    }
    pub fn as_text_output(&self) -> Option<&TextOutput> {
        match self {
            RenderedOutput::Text(t) => Some(t),
            _ => None,
        }
    }
    pub fn as_binary(&self) -> Option<(&[u8], &str)> {
        match self {
            RenderedOutput::Binary(bytes, filename) => Some((bytes, filename)),
            _ => None,
        }
    }
    pub fn as_artifact(&self) -> Option<&ArtifactOutput> {
        match self {
            RenderedOutput::Artifact(artifact) => Some(artifact),
            _ => None,
        }
    }
    pub fn as_artifact_mut(&mut self) -> Option<&mut ArtifactOutput> {
        match self {
            RenderedOutput::Artifact(artifact) => Some(artifact),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPhase {
    PreDispatch,
    PostDispatch,
    PostOutput,
}
impl fmt::Display for HookPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookPhase::PreDispatch => write!(f, "pre-dispatch"),
            HookPhase::PostDispatch => write!(f, "post-dispatch"),
            HookPhase::PostOutput => write!(f, "post-output"),
        }
    }
}
#[derive(Debug, Error)]
#[error("hook error ({phase}): {message}")]
pub struct HookError {
    pub message: String,
    pub phase: HookPhase,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}
impl HookError {
    pub fn pre_dispatch(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            phase: HookPhase::PreDispatch,
            source: None,
        }
    }
    pub fn pre_dispatch_external(failure: ExternalFailure) -> Self {
        Self {
            message: failure.diagnostic().to_owned(),
            phase: HookPhase::PreDispatch,
            source: Some(Box::new(failure)),
        }
    }
    pub fn pre_dispatch_app(failure: AppFailure) -> Self {
        Self {
            message: failure.diagnostic().to_owned(),
            phase: HookPhase::PreDispatch,
            source: Some(Box::new(failure)),
        }
    }
    pub fn post_dispatch(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            phase: HookPhase::PostDispatch,
            source: None,
        }
    }
    pub fn post_output(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            phase: HookPhase::PostOutput,
            source: None,
        }
    }
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        self.source = Some(source.into());
        self
    }
}
pub type PreDispatchFn = Rc<dyn Fn(&ArgMatches, &mut CommandContext) -> Result<(), HookError>>;
pub type PostDispatchFn = Rc<
    dyn Fn(
        &ArgMatches,
        &CommandContext,
        standout_types::RenderData,
    ) -> Result<standout_types::RenderData, HookError>,
>;
pub type PostOutputFn =
    Rc<dyn Fn(&ArgMatches, &CommandContext, RenderedOutput) -> Result<RenderedOutput, HookError>>;
#[derive(Clone, Default)]
pub struct Hooks {
    pre_dispatch: Vec<PreDispatchFn>,
    post_dispatch: Vec<PostDispatchFn>,
    post_output: Vec<PostOutputFn>,
}
impl Hooks {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.pre_dispatch.is_empty() && self.post_dispatch.is_empty() && self.post_output.is_empty()
    }
    pub fn has_phase(&self, phase: HookPhase) -> bool {
        match phase {
            HookPhase::PreDispatch => !self.pre_dispatch.is_empty(),
            HookPhase::PostDispatch => !self.post_dispatch.is_empty(),
            HookPhase::PostOutput => !self.post_output.is_empty(),
        }
    }
    pub fn phases(&self) -> impl Iterator<Item = HookPhase> + '_ {
        [
            HookPhase::PreDispatch,
            HookPhase::PostDispatch,
            HookPhase::PostOutput,
        ]
        .into_iter()
        .filter(|phase| self.has_phase(*phase))
    }
    pub fn append(mut self, mut other: Hooks) -> Self {
        self.pre_dispatch.append(&mut other.pre_dispatch);
        self.post_dispatch.append(&mut other.post_dispatch);
        self.post_output.append(&mut other.post_output);
        self
    }
    pub fn pre_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &mut CommandContext) -> Result<(), HookError> + 'static,
    {
        self.pre_dispatch.push(Rc::new(f));
        self
    }
    pub fn post_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(
                &ArgMatches,
                &CommandContext,
                standout_types::RenderData,
            ) -> Result<standout_types::RenderData, HookError>
            + 'static,
    {
        self.post_dispatch.push(Rc::new(f));
        self
    }
    pub fn post_output<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext, RenderedOutput) -> Result<RenderedOutput, HookError>
            + 'static,
    {
        self.post_output.push(Rc::new(f));
        self
    }
    pub fn run_pre_dispatch(
        &self,
        matches: &ArgMatches,
        ctx: &mut CommandContext,
    ) -> Result<(), HookError> {
        for hook in &self.pre_dispatch {
            hook(matches, ctx)?;
        }
        Ok(())
    }
    pub fn run_post_dispatch(
        &self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        data: standout_types::RenderData,
    ) -> Result<standout_types::RenderData, HookError> {
        let mut current = data;
        for hook in &self.post_dispatch {
            current = hook(matches, ctx, current)?;
        }
        Ok(current)
    }
    pub fn run_post_output(
        &self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        output: RenderedOutput,
    ) -> Result<RenderedOutput, HookError> {
        let mut current = output;
        for hook in &self.post_output {
            current = hook(matches, ctx, current)?;
        }
        Ok(current)
    }
}
impl fmt::Debug for Hooks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hooks")
            .field("pre_dispatch_count", &self.pre_dispatch.len())
            .field("post_dispatch_count", &self.post_dispatch.len())
            .field("post_output_count", &self.post_output.len())
            .finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn test_context() -> CommandContext {
        CommandContext::new(
            vec!["test".into()],
            std::rc::Rc::new(crate::Extensions::new()),
        )
    }
    fn test_matches() -> ArgMatches {
        clap::Command::new("test").get_matches_from(vec!["test"])
    }
    #[test]
    fn test_rendered_output_variants() {
        let text = RenderedOutput::Text(TextOutput::new("formatted".into(), "raw".into()));
        assert!(text.is_text());
        assert!(!text.is_binary());
        assert!(!text.is_silent());
        assert_eq!(text.as_text(), Some("formatted"));
        assert_eq!(text.as_raw_text(), Some("raw"));
        let plain = RenderedOutput::Text(TextOutput::plain("hello".into()));
        assert_eq!(plain.as_text(), Some("hello"));
        assert_eq!(plain.as_raw_text(), Some("hello"));
        let binary = RenderedOutput::Binary(vec![1, 2, 3], "file.bin".into());
        assert!(!binary.is_text());
        assert!(binary.is_binary());
        assert_eq!(binary.as_binary(), Some((&[1u8, 2, 3][..], "file.bin")));
        let silent = RenderedOutput::Silent;
        assert!(silent.is_silent());
    }
    #[test]
    fn test_hook_error_creation() {
        let err = HookError::pre_dispatch("test error");
        assert_eq!(err.phase, HookPhase::PreDispatch);
        assert_eq!(err.message, "test error");
    }
    #[test]
    fn test_hooks_empty() {
        let hooks = Hooks::new();
        assert!(hooks.is_empty());
    }
    #[test]
    fn test_hooks_report_registered_phases() {
        let hooks = Hooks::new()
            .pre_dispatch(|_, _| Ok(()))
            .post_output(|_, _, output| Ok(output));
        let phases: Vec<_> = hooks.phases().collect();
        assert_eq!(phases, vec![HookPhase::PreDispatch, HookPhase::PostOutput]);
        assert!(hooks.has_phase(HookPhase::PreDispatch));
        assert!(!hooks.has_phase(HookPhase::PostDispatch));
    }
    #[test]
    fn test_hooks_append_preserves_phase_order() {
        use std::cell::RefCell;
        let calls = Rc::new(RefCell::new(Vec::new()));
        let first_calls = calls.clone();
        let second_calls = calls.clone();
        let hooks = Hooks::new()
            .pre_dispatch(move |_, _| {
                first_calls.borrow_mut().push("first");
                Ok(())
            })
            .append(Hooks::new().pre_dispatch(move |_, _| {
                second_calls.borrow_mut().push("second");
                Ok(())
            }));
        let mut ctx = test_context();
        let matches = test_matches();
        hooks.run_pre_dispatch(&matches, &mut ctx).unwrap();
        assert_eq!(&*calls.borrow(), &["first", "second"]);
    }
    #[test]
    fn test_pre_dispatch_success() {
        use std::cell::Cell;
        use std::rc::Rc;
        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();
        let hooks = Hooks::new().pre_dispatch(move |_, _| {
            called_clone.set(true);
            Ok(())
        });
        let mut ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_pre_dispatch(&matches, &mut ctx);
        assert!(result.is_ok());
        assert!(called.get());
    }
    #[test]
    fn test_pre_dispatch_error_aborts() {
        let hooks = Hooks::new()
            .pre_dispatch(|_, _| Err(HookError::pre_dispatch("first fails")))
            .pre_dispatch(|_, _| panic!("should not be called"));
        let mut ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_pre_dispatch(&matches, &mut ctx);
        assert!(result.is_err());
    }
    #[test]
    fn test_pre_dispatch_injects_extensions() {
        struct TestState {
            value: i32,
        }
        let hooks = Hooks::new().pre_dispatch(|_, ctx| {
            ctx.extensions.insert(TestState { value: 42 });
            Ok(())
        });
        let mut ctx = test_context();
        let matches = test_matches();
        assert!(!ctx.extensions.contains::<TestState>());
        hooks.run_pre_dispatch(&matches, &mut ctx).unwrap();
        let state = ctx.extensions.get::<TestState>().unwrap();
        assert_eq!(state.value, 42);
    }
    #[test]
    fn test_pre_dispatch_multiple_hooks_share_context() {
        struct Counter {
            count: i32,
        }
        let hooks = Hooks::new()
            .pre_dispatch(|_, ctx| {
                ctx.extensions.insert(Counter { count: 1 });
                Ok(())
            })
            .pre_dispatch(|_, ctx| {
                if let Some(counter) = ctx.extensions.get_mut::<Counter>() {
                    counter.count += 10;
                }
                Ok(())
            });
        let mut ctx = test_context();
        let matches = test_matches();
        hooks.run_pre_dispatch(&matches, &mut ctx).unwrap();
        let counter = ctx.extensions.get::<Counter>().unwrap();
        assert_eq!(counter.count, 11);
    }
    #[test]
    fn test_post_dispatch_transformation() {
        use crate::test_data as json;
        let hooks = Hooks::new().post_dispatch(|_, _, mut data| {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("modified".into(), json!(true));
            }
            Ok(data)
        });
        let ctx = test_context();
        let matches = test_matches();
        let data = json!({"value": 42});
        let result = hooks.run_post_dispatch(&matches, &ctx, data);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output["value"], 42);
        assert_eq!(output["modified"], true);
    }
    #[test]
    fn test_post_output_transformation() {
        let hooks = Hooks::new().post_output(|_, _, output| {
            if let RenderedOutput::Text(text_output) = output {
                Ok(RenderedOutput::Text(TextOutput::new(
                    text_output.formatted.to_uppercase(),
                    text_output.raw.to_uppercase(),
                )))
            } else {
                Ok(output)
            }
        });
        let ctx = test_context();
        let matches = test_matches();
        let input = RenderedOutput::Text(TextOutput::plain("hello".into()));
        let result = hooks.run_post_output(&matches, &ctx, input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_text(), Some("HELLO"));
    }
}
