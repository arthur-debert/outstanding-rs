use super::ExitStatus;
use crate::artifact::Artifact;
use crate::results::NoEvents;
use serde::Serialize;
#[derive(Debug)]
#[non_exhaustive]
pub enum Output<T: Serialize> {
    Render(T),
    Silent,
    Binary {
        data: Vec<u8>,
        filename: String,
    },
    Artifact(Artifact<T>),
    /// Emitted as `output` alone would be; the process exits with `status`.
    WithStatus {
        output: Box<Output<T>>,
        status: ExitStatus,
    },
}
impl<T: Serialize> Output<T> {
    /// A signal beside the result, never a failure; a later call replaces the earlier status.
    pub fn with_exit_status(self, status: ExitStatus) -> Self {
        let (output, _) = self.split_exit_status();
        Output::WithStatus {
            output: Box::new(output),
            status,
        }
    }
    pub fn split_exit_status(self) -> (Self, Option<ExitStatus>) {
        match self {
            Output::WithStatus { output, status } => (output.split_exit_status().0, Some(status)),
            other => (other, None),
        }
    }
    pub fn exit_status(&self) -> ExitStatus {
        match self {
            Output::WithStatus { status, .. } => *status,
            _ => ExitStatus::SUCCESS,
        }
    }
    pub fn map_render(self, f: impl FnOnce(T) -> T) -> Self {
        match self {
            Output::Render(data) => Output::Render(f(data)),
            Output::WithStatus { output, status } => Output::WithStatus {
                output: Box::new(output.map_render(f)),
                status,
            },
            other => other,
        }
    }
    fn declared(&self) -> &Self {
        match self {
            Output::WithStatus { output, .. } => output.declared(),
            other => other,
        }
    }
    pub fn is_render(&self) -> bool {
        matches!(self.declared(), Output::Render(_))
    }
    pub fn is_silent(&self) -> bool {
        matches!(self.declared(), Output::Silent)
    }
    pub fn is_binary(&self) -> bool {
        matches!(self.declared(), Output::Binary { .. })
    }
    pub fn is_artifact(&self) -> bool {
        matches!(self.declared(), Output::Artifact(_))
    }
}
pub type HandlerResult<T> = Result<Output<T>, anyhow::Error>;

/// What a command that declares events returns once the events are done: they
/// already carried the run's results, so a payload has nowhere left to go.
#[derive(Debug)]
#[non_exhaustive]
pub enum Summary<T: Serialize> {
    Render(T),
    Silent,
    /// Emitted as `summary` alone would be; the process exits with `status`.
    WithStatus {
        summary: Box<Summary<T>>,
        status: ExitStatus,
    },
}

impl<T: Serialize> Summary<T> {
    /// A signal beside the result, never a failure; a later call replaces the earlier status.
    pub fn with_exit_status(self, status: ExitStatus) -> Self {
        let (summary, _) = self.split_exit_status();
        Summary::WithStatus {
            summary: Box::new(summary),
            status,
        }
    }

    pub fn split_exit_status(self) -> (Self, Option<ExitStatus>) {
        match self {
            Summary::WithStatus { summary, status } => {
                (summary.split_exit_status().0, Some(status))
            }
            other => (other, None),
        }
    }

    pub fn exit_status(&self) -> ExitStatus {
        match self {
            Summary::WithStatus { status, .. } => *status,
            _ => ExitStatus::SUCCESS,
        }
    }

    pub fn map_render(self, f: impl FnOnce(T) -> T) -> Self {
        match self {
            Summary::Render(data) => Summary::Render(f(data)),
            Summary::WithStatus { summary, status } => Summary::WithStatus {
                summary: Box::new(summary.map_render(f)),
                status,
            },
            other => other,
        }
    }

    fn declared(&self) -> &Self {
        match self {
            Summary::WithStatus { summary, .. } => summary.declared(),
            other => other,
        }
    }

    pub fn is_render(&self) -> bool {
        matches!(self.declared(), Summary::Render(_))
    }

    pub fn is_silent(&self) -> bool {
        matches!(self.declared(), Summary::Silent)
    }
}

impl<T: Serialize> From<Summary<T>> for Output<T> {
    fn from(summary: Summary<T>) -> Self {
        match summary {
            Summary::Render(data) => Output::Render(data),
            Summary::Silent => Output::Silent,
            Summary::WithStatus { summary, status } => Output::WithStatus {
                output: Box::new(Output::from(*summary)),
                status,
            },
        }
    }
}

pub type SummaryResult<T> = Result<Summary<T>, anyhow::Error>;

#[diagnostic::on_unimplemented(
    message = "a command that declares events returns a `Summary`, not an `Output`",
    note = "the events carried the run's results already, so a summary is `Render` or `Silent`"
)]
pub trait IntoSummaryResult<T: Serialize> {
    fn into_summary_result(self) -> SummaryResult<T>;
}

#[diagnostic::do_not_recommend]
impl<T, E> IntoSummaryResult<T> for Result<T, E>
where
    T: Serialize,
    E: Into<anyhow::Error>,
{
    fn into_summary_result(self) -> SummaryResult<T> {
        self.map(Summary::Render).map_err(Into::into)
    }
}

impl<T, E> IntoSummaryResult<T> for Result<Summary<T>, E>
where
    T: Serialize,
    E: Into<anyhow::Error>,
{
    fn into_summary_result(self) -> SummaryResult<T> {
        self.map_err(Into::into)
    }
}

mod outcome {
    pub trait Sealed {}
}

/// What `Handler::handle` may return, tied to the command's event type:
/// `Output<T>` is an outcome only for `NoEvents`, so a command that declares
/// events has `Summary<T>` and no payload variant to return.
#[diagnostic::on_unimplemented(
    message = "a command that declares events returns `Summary<{T}>`, not `Output<{T}>`",
    note = "the events carried the run's results already, so a summary is `Render` or `Silent`"
)]
pub trait HandlerOutcome<T: Serialize, E: Serialize + 'static>: outcome::Sealed {
    fn into_output(self) -> Output<T>;
}

impl<T: Serialize> outcome::Sealed for Output<T> {}

impl<T: Serialize> outcome::Sealed for Summary<T> {}

impl<T: Serialize> HandlerOutcome<T, NoEvents> for Output<T> {
    fn into_output(self) -> Output<T> {
        self
    }
}

impl<T: Serialize, E: Serialize + 'static> HandlerOutcome<T, E> for Summary<T> {
    fn into_output(self) -> Output<T> {
        Output::from(self)
    }
}

pub trait IntoHandlerResult<T: Serialize> {
    fn into_handler_result(self) -> HandlerResult<T>;
}
impl<T, E> IntoHandlerResult<T> for Result<T, E>
where
    T: Serialize,
    E: Into<anyhow::Error>,
{
    fn into_handler_result(self) -> HandlerResult<T> {
        self.map(Output::Render).map_err(Into::into)
    }
}
impl<T: Serialize> IntoHandlerResult<T> for HandlerResult<T> {
    fn into_handler_result(self) -> HandlerResult<T> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, thiserror::Error)]
    #[error("the store refused")]
    struct StoreRefused;
    #[test]
    fn a_summary_carrying_an_error_of_its_own_converts_to_a_summary_result() {
        let rendered: Result<Summary<u8>, StoreRefused> = Ok(Summary::Render(7));
        assert!(matches!(
            rendered.into_summary_result(),
            Ok(Summary::Render(7))
        ));

        let refused: Result<Summary<u8>, StoreRefused> = Err(StoreRefused);
        assert_eq!(
            refused.into_summary_result().unwrap_err().to_string(),
            "the store refused"
        );
    }
    #[test]
    fn test_output_render() {
        let output: Output<String> = Output::Render("success".into());
        assert!(output.is_render());
        assert!(!output.is_silent());
        assert!(!output.is_binary());
    }
    #[test]
    fn test_output_silent() {
        let output: Output<String> = Output::Silent;
        assert!(!output.is_render());
        assert!(output.is_silent());
        assert!(!output.is_binary());
    }
    #[test]
    fn a_declared_status_rides_beside_the_output_and_the_last_one_wins() {
        let plain: Output<String> = Output::Render("found nothing".into());
        assert_eq!(plain.exit_status(), ExitStatus::SUCCESS);
        assert_eq!(plain.split_exit_status().1, None);

        let signalled = Output::Render(String::from("changes"))
            .with_exit_status(ExitStatus::from(3))
            .with_exit_status(ExitStatus::from(2));
        assert_eq!(signalled.exit_status(), ExitStatus::from(2));
        assert!(signalled.is_render());
        assert!(!signalled.is_silent());

        let stamped = signalled.map_render(|text| format!("{text}!"));
        let (output, status) = stamped.split_exit_status();
        assert_eq!(status, Some(ExitStatus::from(2)));
        assert!(matches!(output, Output::Render(ref text) if text == "changes!"));

        let silent: Output<()> = Output::Silent.with_exit_status(ExitStatus::from(4));
        assert!(silent.is_silent());
        assert_eq!(silent.split_exit_status().1, Some(ExitStatus::from(4)));
    }
    #[test]
    fn test_output_binary() {
        let output: Output<String> = Output::Binary {
            data: vec![0x25, 0x50, 0x44, 0x46],
            filename: "report.pdf".into(),
        };
        assert!(!output.is_render());
        assert!(!output.is_silent());
        assert!(output.is_binary());
    }
    #[test]
    fn test_into_handler_result_from_result_ok() {
        use super::IntoHandlerResult;
        let result: Result<String, anyhow::Error> = Ok("hello".to_string());
        let handler_result = result.into_handler_result();
        assert!(handler_result.is_ok());
        match handler_result.unwrap() {
            Output::Render(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_into_handler_result_from_result_err() {
        use super::IntoHandlerResult;
        let result: Result<String, anyhow::Error> = Err(anyhow::anyhow!("test error"));
        let handler_result = result.into_handler_result();
        assert!(handler_result.is_err());
        assert!(handler_result
            .unwrap_err()
            .to_string()
            .contains("test error"));
    }
    #[test]
    fn test_into_handler_result_passthrough_render() {
        use super::IntoHandlerResult;
        let handler_result: HandlerResult<String> = Ok(Output::Render("hello".to_string()));
        let result = handler_result.into_handler_result();
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_into_handler_result_passthrough_silent() {
        use super::IntoHandlerResult;
        let handler_result: HandlerResult<String> = Ok(Output::Silent);
        let result = handler_result.into_handler_result();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Output::Silent));
    }
    #[test]
    fn test_into_handler_result_passthrough_binary() {
        use super::IntoHandlerResult;
        let handler_result: HandlerResult<String> = Ok(Output::Binary {
            data: vec![1, 2, 3],
            filename: "test.bin".to_string(),
        });
        let result = handler_result.into_handler_result();
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Binary { data, filename } => {
                assert_eq!(data, vec![1, 2, 3]);
                assert_eq!(filename, "test.bin");
            }
            _ => panic!("Expected Output::Binary"),
        }
    }
    #[test]
    fn a_summary_becomes_the_output_the_presentation_pipeline_consumes() {
        assert!(Output::from(Summary::Render("done".to_string())).is_render());
        assert!(Output::from(Summary::<String>::Silent).is_silent());

        let carried =
            Output::from(Summary::Render("done".to_string()).with_exit_status(ExitStatus::from(2)));
        assert_eq!(carried.exit_status(), ExitStatus::from(2));
        assert!(carried.is_render());
    }
    #[test]
    fn a_later_exit_status_on_a_summary_replaces_the_earlier_one() {
        let summary = Summary::<String>::Silent
            .with_exit_status(ExitStatus::from(2))
            .with_exit_status(ExitStatus::from(3));
        let (declared, status) = summary.split_exit_status();
        assert_eq!(status, Some(ExitStatus::from(3)));
        assert!(declared.is_silent());
    }
    #[test]
    fn a_plain_value_from_an_emitting_closure_becomes_a_rendered_summary() {
        use super::IntoSummaryResult;
        let summary = Ok::<_, anyhow::Error>("hello".to_string())
            .into_summary_result()
            .unwrap();
        assert!(matches!(summary, Summary::Render(ref s) if s == "hello"));
    }
}
