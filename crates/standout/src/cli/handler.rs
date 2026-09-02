pub use standout_dispatch::{
    AppFailure, Artifact, ArtifactDestination, ArtifactReceipt, ArtifactRun, CommandContext,
    Diagnostic, DiagnosticKind, DiagnosticPosition, DiagnosticRange, DispatchResult, EntryStream,
    ExitStatus, Extensions, ExternalFailure, FnHandler, Handler, HandlerResult, InvalidAppStatus,
    InvalidExternalStatus, Output, OutputKind, RunError, RunErrorKind, RunOutput, Severity,
    StreamCapture, StreamError, StreamSink, SuccessKind,
};

use standout_input::{InputSourceKind, Inputs, MissingInput};

use crate::cli::questionnaire::QUESTIONNAIRE_INPUT_NAME;

pub trait CommandContextInput {
    fn input<T: 'static>(&self, name: &str) -> Result<&T, MissingInput>;

    fn questionnaire<T: 'static>(&self) -> Result<&T, MissingInput>;

    fn input_source(&self, name: &str) -> Option<InputSourceKind>;

    fn inputs(&self) -> Option<&Inputs>;

    fn input_sources(&self) -> &standout_input::InputSources;

    fn warn(&self, message: impl Into<String>);
}

impl CommandContextInput for CommandContext {
    fn input<T: 'static>(&self, name: &str) -> Result<&T, MissingInput> {
        match self.extensions.get::<Inputs>() {
            Some(bag) => bag.get_required::<T>(name),
            None => Err(MissingInput::NotRegistered {
                name: name.to_string(),
            }),
        }
    }

    fn questionnaire<T: 'static>(&self) -> Result<&T, MissingInput> {
        self.input(QUESTIONNAIRE_INPUT_NAME)
    }

    fn input_source(&self, name: &str) -> Option<InputSourceKind> {
        self.extensions.get::<Inputs>()?.source_of(name)
    }

    fn inputs(&self) -> Option<&Inputs> {
        self.extensions.get::<Inputs>()
    }

    fn input_sources(&self) -> &standout_input::InputSources {
        self.extensions
            .get::<standout_input::InputSources>()
            .expect("InputSources are inserted at the run/dispatch edge")
    }

    fn warn(&self, message: impl Into<String>) {
        if let Some(buffer) = self
            .extensions
            .get::<standout_render::warnings::WarningBuffer>()
        {
            buffer.push(message);
        }
    }
}
