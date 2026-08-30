use crate::cli::handler::{DispatchResult, RunError, RunErrorKind, RunOutput};
use crate::OutputMode;

#[derive(Debug)]
pub struct CompletedRun {
    inner: DispatchResult,
    warnings: Vec<String>,
    output_mode: OutputMode,
}

impl CompletedRun {
    pub fn from_dispatch(
        inner: DispatchResult,
        warnings: Vec<String>,
        output_mode: OutputMode,
    ) -> Self {
        Self {
            inner,
            warnings,
            output_mode,
        }
    }

    pub fn outcome(&self) -> &DispatchResult {
        &self.inner
    }

    pub fn into_outcome(self) -> DispatchResult {
        self.inner
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn output_mode(&self) -> OutputMode {
        self.output_mode
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
    PagedHelp(String),
    Error(clap::Error),
}

#[derive(Debug)]
pub(crate) enum HelpDisplay {
    Rendered { text: String, paged: bool },
    Clap(clap::Error),
    RenderFailed(clap::Error),
}

impl From<HelpDisplay> for HelpResult {
    fn from(display: HelpDisplay) -> Self {
        match display {
            HelpDisplay::Rendered { text, paged: true } => HelpResult::PagedHelp(text),
            HelpDisplay::Rendered { text, paged: false } => HelpResult::Help(text),
            HelpDisplay::Clap(e) | HelpDisplay::RenderFailed(e) => HelpResult::Error(e),
        }
    }
}

impl From<HelpDisplay> for DispatchResult {
    fn from(display: HelpDisplay) -> Self {
        match display {
            HelpDisplay::Rendered { text, paged } => DispatchResult::Handled(if paged {
                RunOutput::paged_help(text)
            } else {
                RunOutput::clap_help(text)
            }),
            HelpDisplay::Clap(e) if e.use_stderr() => {
                DispatchResult::Error(RunError::new(e.to_string(), RunErrorKind::ClapUsage))
            }
            HelpDisplay::Clap(e) => DispatchResult::Handled(RunOutput::clap_help(e.to_string())),
            HelpDisplay::RenderFailed(e) => {
                DispatchResult::Error(RunError::new(e.to_string(), RunErrorKind::Render))
            }
        }
    }
}
