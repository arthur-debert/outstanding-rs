use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::path::{Path, PathBuf};
const STDOUT_LABEL: &str = "-";
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactDestination {
    Stdout,
    File(PathBuf),
}
impl ArtifactDestination {
    pub fn path(&self) -> Option<&Path> {
        match self {
            ArtifactDestination::Stdout => None,
            ArtifactDestination::File(path) => Some(path),
        }
    }
    pub fn is_stdout(&self) -> bool {
        matches!(self, ArtifactDestination::Stdout)
    }
    pub fn label(&self) -> String {
        match self {
            ArtifactDestination::Stdout => STDOUT_LABEL.to_string(),
            ArtifactDestination::File(path) => path.display().to_string(),
        }
    }
}
impl Serialize for ArtifactDestination {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.label())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactReceipt {
    destination: ArtifactDestination,
    byte_count: usize,
}
impl ArtifactReceipt {
    pub fn new(destination: ArtifactDestination, byte_count: usize) -> Self {
        Self {
            destination,
            byte_count,
        }
    }
    pub fn destination(&self) -> &ArtifactDestination {
        &self.destination
    }
    pub fn path(&self) -> Option<&Path> {
        self.destination.path()
    }
    pub fn is_stdout(&self) -> bool {
        self.destination.is_stdout()
    }
    pub fn byte_count(&self) -> usize {
        self.byte_count
    }
}
impl Serialize for ArtifactReceipt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("ArtifactReceipt", 3)?;
        state.serialize_field("destination", &self.destination)?;
        state.serialize_field("stdout", &self.destination.is_stdout())?;
        state.serialize_field("byte_count", &self.byte_count)?;
        state.end()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact<T> {
    bytes: Vec<u8>,
    suggested_destination: Option<PathBuf>,
    stdout_fallback: bool,
    report: Option<T>,
}
impl<T> Artifact<T> {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            suggested_destination: None,
            stdout_fallback: false,
            report: None,
        }
    }
    pub fn suggest_destination(mut self, destination: impl Into<PathBuf>) -> Self {
        self.suggested_destination = Some(destination.into());
        self
    }
    pub fn allow_stdout(mut self) -> Self {
        self.stdout_fallback = true;
        self
    }
    pub fn with_report(mut self, report: T) -> Self {
        self.report = Some(report);
        self
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn suggested_destination(&self) -> Option<&Path> {
        self.suggested_destination.as_deref()
    }
    pub fn stdout_allowed(&self) -> bool {
        self.stdout_fallback
    }
    pub fn report(&self) -> Option<&T> {
        self.report.as_ref()
    }
    pub fn into_parts(self) -> (Vec<u8>, Option<PathBuf>, bool, Option<T>) {
        (
            self.bytes,
            self.suggested_destination,
            self.stdout_fallback,
            self.report,
        )
    }
}
#[derive(Debug, Clone)]
pub struct ArtifactRun {
    bytes: Vec<u8>,
    suggested_destination: Option<PathBuf>,
    receipt: ArtifactReceipt,
    report: Option<String>,
}
impl ArtifactRun {
    pub fn new(
        bytes: Vec<u8>,
        suggested_destination: Option<PathBuf>,
        receipt: ArtifactReceipt,
        report: Option<String>,
    ) -> Self {
        Self {
            bytes,
            suggested_destination,
            receipt,
            report,
        }
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn suggested_destination(&self) -> Option<&Path> {
        self.suggested_destination.as_deref()
    }
    pub fn receipt(&self) -> &ArtifactReceipt {
        &self.receipt
    }
    pub fn destination(&self) -> &ArtifactDestination {
        self.receipt.destination()
    }
    pub fn report(&self) -> Option<&str> {
        self.report.as_deref()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    #[derive(Serialize, Debug, PartialEq, Eq, Clone)]
    struct Report {
        entries: usize,
    }
    #[test]
    fn artifact_defaults_to_no_destination_authorization() {
        let artifact = Artifact::<Report>::new(vec![1, 2, 3]);
        assert_eq!(artifact.bytes(), &[1, 2, 3]);
        assert_eq!(artifact.suggested_destination(), None);
        assert!(!artifact.stdout_allowed());
        assert!(artifact.report().is_none());
    }
    #[test]
    fn artifact_builder_records_every_opt_in() {
        let artifact = Artifact::new(b"data".to_vec())
            .suggest_destination("out.bin")
            .allow_stdout()
            .with_report(Report { entries: 2 });
        assert_eq!(artifact.bytes(), b"data");
        assert_eq!(artifact.suggested_destination(), Some(Path::new("out.bin")));
        assert!(artifact.stdout_allowed());
        assert_eq!(artifact.report(), Some(&Report { entries: 2 }));
    }
    #[test]
    fn artifact_into_parts_round_trips() {
        let artifact = Artifact::new(vec![7u8])
            .suggest_destination("a.bin")
            .with_report(Report { entries: 1 });
        let (bytes, suggested, stdout, report) = artifact.into_parts();
        assert_eq!(bytes, vec![7u8]);
        assert_eq!(suggested, Some(PathBuf::from("a.bin")));
        assert!(!stdout);
        assert_eq!(report, Some(Report { entries: 1 }));
    }
    #[test]
    fn file_destination_exposes_path() {
        let dest = ArtifactDestination::File(PathBuf::from("/tmp/x.zip"));
        assert_eq!(dest.path(), Some(Path::new("/tmp/x.zip")));
        assert!(!dest.is_stdout());
        assert_eq!(dest.label(), "/tmp/x.zip");
    }
    #[test]
    fn stdout_destination_has_no_path_and_dash_label() {
        let dest = ArtifactDestination::Stdout;
        assert_eq!(dest.path(), None);
        assert!(dest.is_stdout());
        assert_eq!(dest.label(), "-");
    }
    #[test]
    fn file_receipt_serializes_destination_and_count() {
        let receipt = ArtifactReceipt::new(ArtifactDestination::File(PathBuf::from("/tmp/x")), 12);
        let value = serde_json::to_value(&receipt).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"destination": "/tmp/x", "stdout": false, "byte_count": 12})
        );
        assert_eq!(receipt.path(), Some(Path::new("/tmp/x")));
        assert_eq!(receipt.byte_count(), 12);
        assert!(!receipt.is_stdout());
    }
    #[test]
    fn stdout_receipt_serializes_dash_and_flag() {
        let receipt = ArtifactReceipt::new(ArtifactDestination::Stdout, 3);
        let value = serde_json::to_value(&receipt).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"destination": "-", "stdout": true, "byte_count": 3})
        );
        assert!(receipt.is_stdout());
    }
    #[test]
    fn artifact_run_exposes_bytes_suggestion_receipt_and_report() {
        let run = ArtifactRun::new(
            vec![1, 2],
            Some(PathBuf::from("s.bin")),
            ArtifactReceipt::new(ArtifactDestination::File(PathBuf::from("o.bin")), 2),
            Some("wrote 2 bytes".to_string()),
        );
        assert_eq!(run.bytes(), &[1, 2]);
        assert_eq!(run.suggested_destination(), Some(Path::new("s.bin")));
        assert_eq!(
            run.destination(),
            &ArtifactDestination::File(PathBuf::from("o.bin"))
        );
        assert_eq!(run.receipt().byte_count(), 2);
        assert_eq!(run.report(), Some("wrote 2 bytes"));
    }
    #[test]
    fn artifact_run_without_report_is_none() {
        let run = ArtifactRun::new(
            vec![],
            None,
            ArtifactReceipt::new(ArtifactDestination::Stdout, 0),
            None,
        );
        assert_eq!(run.report(), None);
        assert_eq!(run.suggested_destination(), None);
    }
}
