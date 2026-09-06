use std::sync::Arc;

use crate::cli::handler::{
    Delivery, DispatchResult, ExitStatus, RunError, RunErrorKind, RunOutput, RunRecorder,
};
use crate::{ColorPolicy, Representation};

#[must_use = "exit the process with `status`, or otherwise act on the outcome"]
#[derive(Debug, Clone)]
pub struct ProcessOutcome {
    pub handled: bool,
    pub status: ExitStatus,
    pub final_write_failure: Option<RunError>,
}

#[derive(Debug)]
pub struct CompletedRun {
    inner: DispatchResult,
    warnings: Vec<String>,
    output_mode: Representation,
    color_policy: ColorPolicy,
    results: Vec<serde_json::Value>,
    delivery: Delivery,
    entries: String,
}

impl CompletedRun {
    pub(crate) fn from_dispatch(
        inner: DispatchResult,
        warnings: Vec<String>,
        output_mode: Representation,
        color_policy: ColorPolicy,
        recorder: &RunRecorder,
    ) -> Self {
        Self {
            inner,
            warnings,
            output_mode,
            color_policy,
            results: recorder
                .records()
                .iter()
                .map(standout_render::RenderData::to_json)
                .collect(),
            delivery: recorder.delivery(),
            entries: String::new(),
        }
    }

    pub(crate) fn with_entries(mut self, entries: String) -> Self {
        self.entries = entries;
        self
    }

    pub fn outcome(&self) -> &DispatchResult {
        &self.inner
    }

    /// The event lines `run_with` and `dispatch` capture as the handler emits
    /// them, newlines included.
    pub fn entries(&self) -> &str {
        &self.entries
    }

    pub fn into_outcome(self) -> DispatchResult {
        self.inner
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn output_mode(&self) -> Representation {
        self.output_mode
    }

    pub fn color_policy(&self) -> ColorPolicy {
        self.color_policy
    }

    /// The run's result values as data, whatever representation it selected.
    pub fn results(&self) -> &[serde_json::Value] {
        &self.results
    }

    /// Where the rendered bytes went.
    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }
}

impl std::ops::Deref for CompletedRun {
    type Target = DispatchResult;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Debug)]
pub enum HelpResult {
    Matches(clap::ArgMatches),
    Help(String),
    Error(clap::Error),
}

#[derive(Debug)]
pub(crate) enum HelpDisplay {
    Rendered { text: String },
    Clap(clap::Error),
    RenderFailed(clap::Error),
}

impl From<HelpDisplay> for HelpResult {
    fn from(display: HelpDisplay) -> Self {
        match display {
            HelpDisplay::Rendered { text } => HelpResult::Help(text),
            HelpDisplay::Clap(e) | HelpDisplay::RenderFailed(e) => HelpResult::Error(e),
        }
    }
}

impl From<HelpDisplay> for DispatchResult {
    fn from(display: HelpDisplay) -> Self {
        match display {
            HelpDisplay::Rendered { text } => DispatchResult::Handled(RunOutput::clap_help(text)),
            HelpDisplay::Clap(e) if e.use_stderr() => {
                DispatchResult::Error(RunError::new(e.to_string(), RunErrorKind::ClapUsage))
            }
            HelpDisplay::Clap(e) => DispatchResult::Handled(RunOutput::clap_help(e.to_string())),
            HelpDisplay::RenderFailed(e) => {
                DispatchResult::Error(RunError::render(e.to_string(), Arc::new(e)))
            }
        }
    }
}
