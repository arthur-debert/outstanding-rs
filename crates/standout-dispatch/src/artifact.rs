//! Compound artifacts: owned bytes plus an application-owned semantic report.
//!
//! # Why this exists
//!
//! A command that produces a file — an export, a bundle, a database dump —
//! has to keep four things apart:
//!
//! 1. the artifact bytes, which must survive byte-for-byte;
//! 2. metadata about where the bytes would like to land;
//! 3. application-owned facts about the operation (counts, warnings,
//!    partial-success notes);
//! 4. the success report, which can only be truthful once the concrete
//!    destination is known.
//!
//! [`Output::Binary`](crate::Output::Binary) carries (1) and a filename hint,
//! but has nowhere to put (3) and renders nothing after the write. [`Artifact`]
//! carries all four: the application produces exact bytes and domain facts, and
//! the framework selects the destination, performs the final write, and only
//! then renders the report enriched with an [`ArtifactReceipt`] naming the
//! destination that actually completed.
//!
//! # Ownership boundary
//!
//! | Concern | Owner |
//! |---------|-------|
//! | Artifact bytes | Application |
//! | Suggested destination | Application (a *suggestion*) |
//! | Semantic report / warning taxonomy | Application |
//! | Destination selection | Framework |
//! | The final write and its failure | Framework |
//! | Receipt (completed destination) | Framework |
//!
//! # Destination policy
//!
//! The framework selects the destination deterministically, in this order:
//!
//! 1. an explicit output-file override (`--output-file-path`, when the app
//!    enabled that flag);
//! 2. the artifact's suggested destination, if the application opted in with
//!    [`Artifact::suggest_destination`];
//! 3. stdout, if the application opted in with [`Artifact::allow_stdout`].
//!
//! If none applies the run fails with
//! [`RunErrorKind::FinalWrite(OutputKind::Artifact)`](crate::RunErrorKind::FinalWrite)
//! rather than inventing a file or silently discarding the bytes. Every step
//! shares that single framework-owned failure path.
//!
//! Note that a suggested destination on `Output::Binary` does *not* authorize a
//! write: only [`Artifact::suggest_destination`] does. The opt-in is the
//! difference between the two shapes.
//!
//! # Report channel
//!
//! Mixing the report into the artifact bytes would corrupt them, so the channel
//! follows the destination:
//!
//! | Artifact destination | Report goes to |
//! |----------------------|----------------|
//! | File | stdout |
//! | Stdout | stderr (the diagnostic channel) |
//!
//! # Example
//!
//! ```rust
//! use serde::Serialize;
//! use standout_dispatch::{Artifact, HandlerResult, Output};
//!
//! #[derive(Serialize)]
//! struct ExportReport {
//!     entries: usize,
//!     warnings: Vec<String>,
//! }
//!
//! fn export() -> HandlerResult<ExportReport> {
//!     let bytes = b"id,title\n1,buy milk\n".to_vec();
//!     Ok(Output::Artifact(
//!         Artifact::new(bytes)
//!             .suggest_destination("export.csv")
//!             .with_report(ExportReport {
//!                 entries: 1,
//!                 warnings: vec!["1 entry had no due date".into()],
//!             }),
//!     ))
//! }
//! ```

use std::path::{Path, PathBuf};

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// The label used for the stdout destination in serialized receipts.
const STDOUT_LABEL: &str = "-";

/// A destination the framework can complete an artifact write to.
///
/// Produced by the framework's destination policy; applications only ever
/// *suggest* a destination (see [`Artifact::suggest_destination`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactDestination {
    /// The bytes go to standard output.
    Stdout,
    /// The bytes go to this file.
    File(PathBuf),
}

impl ArtifactDestination {
    /// Returns the file path, or `None` for stdout.
    pub fn path(&self) -> Option<&Path> {
        match self {
            ArtifactDestination::Stdout => None,
            ArtifactDestination::File(path) => Some(path),
        }
    }

    /// Returns true if this destination is standard output.
    pub fn is_stdout(&self) -> bool {
        matches!(self, ArtifactDestination::Stdout)
    }

    /// Returns the display label: the path, or `-` for stdout.
    ///
    /// This is the value templates see as `{{ receipt.destination }}`.
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

/// The framework's record of a completed artifact write.
///
/// A receipt only exists once the write succeeded, so its destination is a
/// fact, not an intention. It is serialized into the report envelope under
/// `receipt` as:
///
/// ```json
/// { "destination": "/tmp/export.csv", "stdout": false, "byte_count": 20 }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactReceipt {
    destination: ArtifactDestination,
    byte_count: usize,
}

impl ArtifactReceipt {
    /// Records a completed write of `byte_count` bytes to `destination`.
    ///
    /// Framework-owned: applications receive receipts, they do not mint them.
    pub fn new(destination: ArtifactDestination, byte_count: usize) -> Self {
        Self {
            destination,
            byte_count,
        }
    }

    /// Returns the destination the write completed to.
    pub fn destination(&self) -> &ArtifactDestination {
        &self.destination
    }

    /// Returns the written file's path, or `None` if the bytes went to stdout.
    pub fn path(&self) -> Option<&Path> {
        self.destination.path()
    }

    /// Returns true if the bytes went to standard output.
    pub fn is_stdout(&self) -> bool {
        self.destination.is_stdout()
    }

    /// Returns the number of artifact bytes the write covered.
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

/// Owned artifact bytes with an optional destination suggestion and report.
///
/// Returned from a handler as [`Output::Artifact`](crate::Output::Artifact).
/// See the [module docs](self) for the destination policy and report channel.
///
/// Bytes are owned: streaming is deliberately not part of this contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact<T> {
    bytes: Vec<u8>,
    suggested_destination: Option<PathBuf>,
    stdout_fallback: bool,
    report: Option<T>,
}

impl<T> Artifact<T> {
    /// Creates an artifact from owned bytes, with no destination suggestion,
    /// no stdout fallback, and no report.
    ///
    /// Such an artifact is only writable under an explicit output-file
    /// override; without one the run fails rather than guessing a destination.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            suggested_destination: None,
            stdout_fallback: false,
            report: None,
        }
    }

    /// Suggests where the bytes should land, and authorizes the framework to
    /// write there when no explicit override is given.
    ///
    /// This opt-in is what separates an artifact from `Output::Binary`: a
    /// filename on binary output is a hint to the caller, never a license to
    /// touch the filesystem.
    pub fn suggest_destination(mut self, destination: impl Into<PathBuf>) -> Self {
        self.suggested_destination = Some(destination.into());
        self
    }

    /// Authorizes stdout as the last-resort destination.
    ///
    /// Without this, an artifact with no override and no suggestion is a typed
    /// failure. With it, the report is routed to stderr so it can never
    /// contaminate the bytes.
    pub fn allow_stdout(mut self) -> Self {
        self.stdout_fallback = true;
        self
    }

    /// Attaches the application-owned semantic report rendered after the write.
    ///
    /// The report is the application's own type: counts, warnings, partial
    /// success — whatever taxonomy it owns. Standout only transports it.
    pub fn with_report(mut self, report: T) -> Self {
        self.report = Some(report);
        self
    }

    /// Returns the artifact bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the suggested destination, if the application opted in.
    pub fn suggested_destination(&self) -> Option<&Path> {
        self.suggested_destination.as_deref()
    }

    /// Returns true if the application authorized the stdout fallback.
    pub fn stdout_allowed(&self) -> bool {
        self.stdout_fallback
    }

    /// Returns the semantic report, if any.
    pub fn report(&self) -> Option<&T> {
        self.report.as_ref()
    }

    /// Splits the artifact into its bytes, suggestion, stdout opt-in, and report.
    pub fn into_parts(self) -> (Vec<u8>, Option<PathBuf>, bool, Option<T>) {
        (
            self.bytes,
            self.suggested_destination,
            self.stdout_fallback,
            self.report,
        )
    }
}

/// A completed artifact run: the framework's outcome for [`Output::Artifact`](crate::Output::Artifact).
///
/// Carried by [`DispatchResult::Artifact`](crate::DispatchResult::Artifact). The receipt
/// is authoritative for a file destination — the bytes are already on disk. For
/// the stdout destination the byte write is deferred to the framework's stdout
/// writer, which is why `run()` can still report a typed stdout write failure.
#[derive(Debug, Clone)]
pub struct ArtifactRun {
    bytes: Vec<u8>,
    suggested_destination: Option<PathBuf>,
    receipt: ArtifactReceipt,
    report: Option<String>,
}

impl ArtifactRun {
    /// Records a completed artifact run.
    ///
    /// Framework-owned: `report` is the already-rendered (or serialized)
    /// success report, which the framework only produces after the write.
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

    /// Returns the artifact bytes, byte-for-byte as the handler produced them.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns what the application suggested, which may differ from where the
    /// bytes actually went.
    pub fn suggested_destination(&self) -> Option<&Path> {
        self.suggested_destination.as_deref()
    }

    /// Returns the framework's receipt for the write.
    pub fn receipt(&self) -> &ArtifactReceipt {
        &self.receipt
    }

    /// Returns the destination the framework selected and completed.
    pub fn destination(&self) -> &ArtifactDestination {
        self.receipt.destination()
    }

    /// Returns the rendered success report, or `None` if the artifact carried
    /// no report.
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
