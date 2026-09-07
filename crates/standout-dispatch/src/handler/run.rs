use super::{ExitStatus, RunError, RunErrorKind};
use crate::artifact::ArtifactRun;
use clap::ArgMatches;
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SuccessKind {
    Command,
    ClapHelp,
    ClapVersion,
}
#[derive(Debug, Clone)]
pub struct RunOutput {
    text: String,
    kind: SuccessKind,
    status: ExitStatus,
    warnings_included: bool,
}
impl RunOutput {
    pub fn command(text: impl Into<String>) -> Self {
        Self::new(text, SuccessKind::Command)
    }
    pub fn clap_help(text: impl Into<String>) -> Self {
        Self::new(text, SuccessKind::ClapHelp)
    }
    pub fn clap_version(text: impl Into<String>) -> Self {
        Self::new(text, SuccessKind::ClapVersion)
    }
    fn new(text: impl Into<String>, kind: SuccessKind) -> Self {
        Self {
            text: text.into(),
            kind,
            status: ExitStatus::SUCCESS,
            warnings_included: false,
        }
    }
    pub fn with_exit_status(mut self, status: ExitStatus) -> Self {
        self.status = status;
        self
    }
    /// Marks output whose document already carries the run's warning records,
    /// so the framework neither appends them nor renders them to stderr.
    pub fn with_warnings_included(mut self, included: bool) -> Self {
        self.warnings_included = included;
        self
    }
    pub const fn warnings_included(&self) -> bool {
        self.warnings_included
    }
    pub fn as_str(&self) -> &str {
        &self.text
    }
    pub const fn kind(&self) -> SuccessKind {
        self.kind
    }
    pub const fn exit_status(&self) -> ExitStatus {
        self.status
    }
    pub fn into_string(self) -> String {
        self.text
    }
}
impl std::ops::Deref for RunOutput {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
impl AsRef<str> for RunOutput {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for RunOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl PartialEq<str> for RunOutput {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl PartialEq<&str> for RunOutput {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}
impl PartialEq<String> for RunOutput {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}
impl From<String> for RunOutput {
    fn from(text: String) -> Self {
        Self::command(text)
    }
}
impl From<&str> for RunOutput {
    fn from(text: &str) -> Self {
        Self::command(text)
    }
}
impl From<RunOutput> for String {
    fn from(output: RunOutput) -> Self {
        output.into_string()
    }
}
#[derive(Debug)]
#[non_exhaustive]
pub enum DispatchResult {
    Handled(RunOutput),
    Binary(Vec<u8>, String),
    Artifact(ArtifactRun),
    Silent,
    Error(RunError),
    NoMatch(ArgMatches),
}
impl DispatchResult {
    pub fn is_handled(&self) -> bool {
        matches!(self, DispatchResult::Handled(_))
    }
    pub fn is_binary(&self) -> bool {
        matches!(self, DispatchResult::Binary(_, _))
    }
    pub fn is_artifact(&self) -> bool {
        matches!(self, DispatchResult::Artifact(_))
    }
    pub fn is_silent(&self) -> bool {
        matches!(self, DispatchResult::Silent)
    }
    pub fn is_error(&self) -> bool {
        matches!(self, DispatchResult::Error(_))
    }
    pub fn output(&self) -> Option<&str> {
        match self {
            DispatchResult::Handled(s) => Some(s),
            _ => None,
        }
    }
    pub fn error(&self) -> Option<&str> {
        match self {
            DispatchResult::Error(s) => Some(s),
            _ => None,
        }
    }
    pub fn success_kind(&self) -> Option<SuccessKind> {
        match self {
            DispatchResult::Handled(output) => Some(output.kind()),
            DispatchResult::Binary(_, _) | DispatchResult::Artifact(_) | DispatchResult::Silent => {
                Some(SuccessKind::Command)
            }
            _ => None,
        }
    }
    pub fn error_kind(&self) -> Option<RunErrorKind> {
        match self {
            DispatchResult::Error(error) => Some(error.kind()),
            _ => None,
        }
    }
    pub fn exit_status(&self) -> Option<ExitStatus> {
        match self {
            DispatchResult::Handled(output) => Some(output.exit_status()),
            DispatchResult::Binary(_, _) | DispatchResult::Artifact(_) | DispatchResult::Silent => {
                Some(ExitStatus::SUCCESS)
            }
            DispatchResult::Error(error) => Some(error.exit_status()),
            DispatchResult::NoMatch(_) => None,
        }
    }
    pub fn binary(&self) -> Option<(&[u8], &str)> {
        match self {
            DispatchResult::Binary(bytes, filename) => Some((bytes, filename)),
            _ => None,
        }
    }
    pub fn artifact(&self) -> Option<&ArtifactRun> {
        match self {
            DispatchResult::Artifact(run) => Some(run),
            _ => None,
        }
    }
    pub fn matches(&self) -> Option<&ArgMatches> {
        match self {
            DispatchResult::NoMatch(m) => Some(m),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_handled_run_reports_the_status_its_output_declared() {
        let handled = DispatchResult::Handled(
            RunOutput::command("plan").with_exit_status(ExitStatus::from(2)),
        );
        assert_eq!(handled.exit_status(), Some(ExitStatus::from(2)));
        assert_eq!(handled.success_kind(), Some(SuccessKind::Command));
        assert!(!handled.is_error());
        assert_eq!(
            DispatchResult::Handled(RunOutput::command("plan")).exit_status(),
            Some(ExitStatus::SUCCESS)
        );
    }
    #[test]
    fn test_run_result_handled() {
        let result = DispatchResult::Handled("output".into());
        assert!(result.is_handled());
        assert!(!result.is_binary());
        assert!(!result.is_silent());
        assert_eq!(result.output(), Some("output"));
        assert!(result.matches().is_none());
    }
    #[test]
    fn test_run_result_silent() {
        let result = DispatchResult::Silent;
        assert!(!result.is_handled());
        assert!(!result.is_binary());
        assert!(result.is_silent());
    }
    #[test]
    fn test_run_result_binary() {
        let bytes = vec![0x25, 0x50, 0x44, 0x46];
        let result = DispatchResult::Binary(bytes.clone(), "report.pdf".into());
        assert!(!result.is_handled());
        assert!(result.is_binary());
        assert!(!result.is_silent());
        let (data, filename) = result.binary().unwrap();
        assert_eq!(data, &bytes);
        assert_eq!(filename, "report.pdf");
    }
    #[test]
    fn test_run_result_no_match() {
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = DispatchResult::NoMatch(matches);
        assert!(!result.is_handled());
        assert!(!result.is_binary());
        assert!(result.matches().is_some());
    }
}
