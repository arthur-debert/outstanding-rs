mod context;
mod errors;
mod output;
mod run;

pub use context::{CommandContext, Extensions};
pub use errors::{
    AppFailure, ExitStatus, ExternalFailure, InvalidAppStatus, InvalidExternalStatus, OutputKind,
    RunError, RunErrorKind,
};
pub use output::{
    HandlerOutcome, HandlerResult, IntoHandlerResult, IntoSummaryResult, Output, Summary,
    SummaryResult,
};
pub use run::{DispatchResult, RunOutput, SuccessKind};

use crate::results::{NoEvents, Results};
use crate::verify::ExpectedArg;
use clap::ArgMatches;
use serde::Serialize;
pub trait Handler {
    /// The type of the values the command produces while it runs, or
    /// [`NoEvents`] for a command that produces none, which is what
    /// [`emits_events`](crate::emits_events) reads.
    type Event: Serialize + 'static;
    type Output: Serialize;
    /// [`Output`] for a batch command, [`Summary`] for one that declares
    /// events: [`HandlerOutcome`] admits the first only under [`NoEvents`].
    type Outcome: HandlerOutcome<Self::Output, Self::Event>;
    fn handle(
        &mut self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        results: &mut Results<Self::Event>,
    ) -> Result<Self::Outcome, anyhow::Error>;
    fn expected_args(&self) -> Vec<ExpectedArg> {
        Vec::new()
    }
}
pub struct FnHandler<F, T, R = HandlerResult<T>>
where
    T: Serialize,
{
    f: F,
    _phantom: std::marker::PhantomData<fn() -> (T, R)>,
}
impl<F, T, R> FnHandler<F, T, R>
where
    F: FnMut(&ArgMatches, &CommandContext) -> R,
    R: IntoHandlerResult<T>,
    T: Serialize,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}
impl<F, T, R> Handler for FnHandler<F, T, R>
where
    F: FnMut(&ArgMatches, &CommandContext) -> R,
    R: IntoHandlerResult<T>,
    T: Serialize,
{
    type Event = NoEvents;
    type Output = T;
    type Outcome = Output<T>;
    fn handle(
        &mut self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        _results: &mut Results<NoEvents>,
    ) -> HandlerResult<T> {
        (self.f)(matches, ctx).into_handler_result()
    }
}
/// The adapter behind a three-argument closure, whose third parameter is the
/// command's typed results channel, so `Event` is the closure's event type.
pub struct EventsFnHandler<F, E, T, R = SummaryResult<T>>
where
    E: Serialize + 'static,
    T: Serialize,
{
    f: F,
    _event: std::marker::PhantomData<fn(E)>,
    _phantom: std::marker::PhantomData<fn() -> (T, R)>,
}
impl<F, E, T, R> EventsFnHandler<F, E, T, R>
where
    F: FnMut(&ArgMatches, &CommandContext, &mut Results<E>) -> R,
    R: IntoSummaryResult<T>,
    E: Serialize + 'static,
    T: Serialize,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _event: std::marker::PhantomData,
            _phantom: std::marker::PhantomData,
        }
    }
}
impl<F, E, T, R> Handler for EventsFnHandler<F, E, T, R>
where
    F: FnMut(&ArgMatches, &CommandContext, &mut Results<E>) -> R,
    R: IntoSummaryResult<T>,
    E: Serialize + 'static,
    T: Serialize,
{
    type Event = E;
    type Output = T;
    type Outcome = Summary<T>;
    fn handle(
        &mut self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        results: &mut Results<E>,
    ) -> SummaryResult<T> {
        (self.f)(matches, ctx, results).into_summary_result()
    }
}
pub struct SimpleFnHandler<F, T, R = HandlerResult<T>>
where
    T: Serialize,
{
    f: F,
    _phantom: std::marker::PhantomData<fn() -> (T, R)>,
}
impl<F, T, R> SimpleFnHandler<F, T, R>
where
    F: FnMut(&ArgMatches) -> R,
    R: IntoHandlerResult<T>,
    T: Serialize,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}
impl<F, T, R> Handler for SimpleFnHandler<F, T, R>
where
    F: FnMut(&ArgMatches) -> R,
    R: IntoHandlerResult<T>,
    T: Serialize,
{
    type Event = NoEvents;
    type Output = T;
    type Outcome = Output<T>;
    fn handle(
        &mut self,
        matches: &ArgMatches,
        _ctx: &CommandContext,
        _results: &mut Results<NoEvents>,
    ) -> HandlerResult<T> {
        (self.f)(matches).into_handler_result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn test_fn_handler() {
        let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            Ok(Output::Render(json!({"status": "ok"})))
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
    }
    #[test]
    fn test_fn_handler_mutation() {
        let mut counter = 0u32;
        let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            counter += 1;
            Ok(Output::Render(counter))
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let _ = handler.handle(&matches, &ctx, &mut Results::discarding());
        let _ = handler.handle(&matches, &ctx, &mut Results::discarding());
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        if let Ok(Output::Render(count)) = result {
            assert_eq!(count, 3);
        }
    }
    #[test]
    fn test_fn_handler_with_auto_wrap() {
        let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            Ok::<_, anyhow::Error>("auto-wrapped".to_string())
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(s) => assert_eq!(s, "auto-wrapped"),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_fn_handler_with_explicit_output() {
        let mut handler =
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::<()>::Silent));
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Output::Silent));
    }
    #[test]
    fn test_fn_handler_with_custom_error_type() {
        #[derive(Debug)]
        struct CustomError(String);
        impl std::fmt::Display for CustomError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "CustomError: {}", self.0)
            }
        }
        impl std::error::Error for CustomError {}
        let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            Err::<String, CustomError>(CustomError("oops".to_string()))
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("CustomError: oops"));
    }
    #[test]
    fn test_simple_fn_handler_basic() {
        use super::SimpleFnHandler;
        let mut handler = SimpleFnHandler::new(|_m: &ArgMatches| {
            Ok::<_, anyhow::Error>("no context needed".to_string())
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(s) => assert_eq!(s, "no context needed"),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_simple_fn_handler_with_args() {
        use super::SimpleFnHandler;
        let mut handler = SimpleFnHandler::new(|m: &ArgMatches| {
            let verbose = m.get_flag("verbose");
            Ok::<_, anyhow::Error>(verbose)
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test")
            .arg(
                clap::Arg::new("verbose")
                    .short('v')
                    .action(clap::ArgAction::SetTrue),
            )
            .get_matches_from(vec!["test", "-v"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(v) => assert!(v),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_simple_fn_handler_explicit_output() {
        use super::SimpleFnHandler;
        let mut handler = SimpleFnHandler::new(|_m: &ArgMatches| Ok(Output::<()>::Silent));
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Output::Silent));
    }
    #[test]
    fn test_simple_fn_handler_error() {
        use super::SimpleFnHandler;
        let mut handler = SimpleFnHandler::new(|_m: &ArgMatches| {
            Err::<String, _>(anyhow::anyhow!("simple error"))
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("simple error"));
    }
    #[test]
    fn test_simple_fn_handler_mutation() {
        use super::SimpleFnHandler;
        let mut counter = 0u32;
        let mut handler = SimpleFnHandler::new(|_m: &ArgMatches| {
            counter += 1;
            Ok::<_, anyhow::Error>(counter)
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let _ = handler.handle(&matches, &ctx, &mut Results::discarding());
        let _ = handler.handle(&matches, &ctx, &mut Results::discarding());
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(n) => assert_eq!(n, 3),
            _ => panic!("Expected Output::Render"),
        }
    }
}
