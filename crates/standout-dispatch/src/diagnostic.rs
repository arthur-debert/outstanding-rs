//! The diagnostic document: the one shape a failure takes on stdout under a
//! structured output mode.
//!
//! A handler returns a [`Diagnostic`] as its error when it has a `detail` or a
//! source `range` to report; any other error type reaches the document with
//! its `Display` text as `summary` and an empty `detail`. `kind` is the
//! [`DiagnosticKind`] projected from the [`RunErrorKind`] the framework
//! assigned when the error crossed the dispatch boundary, so a value a handler
//! constructs carries a placeholder that the framework overwrites. The wire
//! vocabulary is fixed: every `FinalWrite` payload is the one `final-write`,
//! while hook phases stay distinct. `framework` is the one kind outside that
//! projection: a `severity: warning` entry the framework raises on its own
//! account under `ndjson`, which no run failure classifies.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::contract::ContractSurface;
use crate::handler::RunErrorKind;
use crate::hooks::HookPhase;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    #[serde(rename = "type", with = "document_type")]
    document_type: (),
    schema_version: u32,
    pub severity: Severity,
    pub kind: DiagnosticKind,
    pub summary: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<DiagnosticRange>,
}

impl ContractSurface for Diagnostic {
    const SCHEMA_VERSION: u32 = 1;
}

impl Diagnostic {
    pub fn error(summary: impl Into<String>) -> Self {
        Self::new(Severity::Error, summary)
    }

    pub fn warning(summary: impl Into<String>) -> Self {
        Self::new(Severity::Warning, summary)
    }

    fn new(severity: Severity, summary: impl Into<String>) -> Self {
        Self {
            document_type: (),
            schema_version: Self::SCHEMA_VERSION,
            severity,
            kind: DiagnosticKind::Handler,
            summary: summary.into(),
            detail: String::new(),
            range: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn range(mut self, filename: impl Into<String>, line: u64, column: u64) -> Self {
        self.range = Some(DiagnosticRange {
            filename: filename.into(),
            start: DiagnosticPosition { line, column },
        });
        self
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(range) = &self.range {
            write!(
                f,
                "{}:{}:{}: ",
                range.filename, range.start.line, range.start.column
            )?;
        }
        f.write_str(&self.summary)?;
        if !self.detail.is_empty() {
            write!(f, "\n{}", self.detail)?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticKind {
    ClapUsage,
    DefaultCommand,
    Handler,
    HookPreDispatch,
    HookPostDispatch,
    HookPostOutput,
    Render,
    FinalWrite,
    External,
    App,
    Config,
    Framework,
}

impl From<RunErrorKind> for DiagnosticKind {
    fn from(kind: RunErrorKind) -> Self {
        match kind {
            RunErrorKind::ClapUsage => Self::ClapUsage,
            RunErrorKind::DefaultCommand => Self::DefaultCommand,
            RunErrorKind::Handler => Self::Handler,
            RunErrorKind::Hook(HookPhase::PreDispatch) => Self::HookPreDispatch,
            RunErrorKind::Hook(HookPhase::PostDispatch) => Self::HookPostDispatch,
            RunErrorKind::Hook(HookPhase::PostOutput) => Self::HookPostOutput,
            RunErrorKind::Render => Self::Render,
            RunErrorKind::FinalWrite(_) => Self::FinalWrite,
            RunErrorKind::External => Self::External,
            RunErrorKind::App => Self::App,
            RunErrorKind::Config => Self::Config,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRange {
    pub filename: String,
    pub start: DiagnosticPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticPosition {
    pub line: u64,
    pub column: u64,
}

mod document_type {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    const TAG: &str = "diagnostic";

    pub fn serialize<S: Serializer>(_: &(), serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(TAG)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
        let tag = String::deserialize(deserializer)?;
        if tag == TAG {
            Ok(())
        } else {
            Err(D::Error::custom(format!(
                "expected a \"{TAG}\" document, found type {tag:?}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::OutputKind;

    #[test]
    fn a_ranged_diagnostic_serializes_flat_with_the_fixed_type_tag() {
        let diagnostic = Diagnostic::error("config line 2 does not parse")
            .detail("expected `resource <name> <state>`")
            .range("main.tfl", 2, 1);
        let json = serde_json::to_value(&diagnostic).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "diagnostic",
                "schema_version": 1,
                "severity": "error",
                "kind": "handler",
                "summary": "config line 2 does not parse",
                "detail": "expected `resource <name> <state>`",
                "range": { "filename": "main.tfl", "start": { "line": 2, "column": 1 } },
            })
        );
        let back: Diagnostic = serde_json::from_value(json).unwrap();
        assert_eq!(back, diagnostic);
    }

    #[test]
    fn an_unranged_diagnostic_omits_the_range_key() {
        let mut diagnostic = Diagnostic::warning("soft");
        diagnostic.kind = DiagnosticKind::HookPostOutput;
        let json = serde_json::to_string(&diagnostic).unwrap();
        assert_eq!(
            json,
            r#"{"type":"diagnostic","schema_version":1,"severity":"warning","kind":"hook-post-output","summary":"soft","detail":""}"#
        );
        assert_eq!(
            serde_json::from_str::<Diagnostic>(&json).unwrap(),
            diagnostic
        );
    }

    #[test]
    fn every_run_error_kind_projects_onto_the_fixed_wire_vocabulary() {
        let expected = [
            (RunErrorKind::ClapUsage, "clap-usage"),
            (RunErrorKind::DefaultCommand, "default-command"),
            (RunErrorKind::Handler, "handler"),
            (
                RunErrorKind::Hook(HookPhase::PreDispatch),
                "hook-pre-dispatch",
            ),
            (
                RunErrorKind::Hook(HookPhase::PostDispatch),
                "hook-post-dispatch",
            ),
            (
                RunErrorKind::Hook(HookPhase::PostOutput),
                "hook-post-output",
            ),
            (RunErrorKind::Render, "render"),
            (RunErrorKind::FinalWrite(OutputKind::Text), "final-write"),
            (RunErrorKind::FinalWrite(OutputKind::Binary), "final-write"),
            (
                RunErrorKind::FinalWrite(OutputKind::Artifact),
                "final-write",
            ),
            (RunErrorKind::External, "external"),
            (RunErrorKind::App, "app"),
            (RunErrorKind::Config, "config"),
        ];
        for (kind, name) in expected {
            let wire = DiagnosticKind::from(kind);
            assert_eq!(serde_json::to_value(wire).unwrap(), name, "{kind:?}");
            assert_eq!(
                serde_json::from_value::<DiagnosticKind>(name.into()).unwrap(),
                wire
            );
        }
        assert!(serde_json::from_value::<DiagnosticKind>("final-write-text".into()).is_err());
    }

    #[test]
    fn a_document_of_another_type_is_refused() {
        let error = serde_json::from_str::<Diagnostic>(
            r#"{"type":"result","schema_version":1,"severity":"error","kind":"handler","summary":"","detail":""}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("\"diagnostic\""), "{error}");
    }

    #[test]
    fn display_is_the_human_prose_form() {
        assert_eq!(Diagnostic::error("boom").to_string(), "boom");
        assert_eq!(
            Diagnostic::error("boom")
                .detail("why")
                .range("a.cfg", 3, 7)
                .to_string(),
            "a.cfg:3:7: boom\nwhy"
        );
    }
}
