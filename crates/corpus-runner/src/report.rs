//! The run report: the durable artifact a corpus run leaves behind.
//!
//! `schema_version` 1 — the shape is a recorded decision (see
//! `corpus/README.md`); an ADR may formalize it later. Objective results
//! (acceptance, invariants) and the agent's self-assessment (questionnaire)
//! are deliberately separate sections, and the `pins` block is what makes two
//! runs comparable: same spec hash + same framework version + same docs
//! commit means the same experiment.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// The current report schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Everything one corpus run durably records.
#[derive(Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub schema_version: u32,
    /// Directory-name identity of the run, e.g. `smoke-1755500000`.
    pub run_id: String,
    pub archetype: ArchetypeStamp,
    pub pins: Pins,
    pub blindness: Blindness,
    pub session: SessionReport,
    pub acceptance: AcceptanceReport,
    /// ROB01 invariant-matrix cells, one per (command × check).
    pub invariants: Vec<InvariantCell>,
    pub questionnaire: QuestionnaireReport,
}

/// Which archetype ran, pinned by content rather than by name alone.
#[derive(Debug, Serialize, Deserialize)]
pub struct ArchetypeStamp {
    pub name: String,
    /// sha256 (hex) of the exact spec text provisioned into the workspace.
    pub spec_sha256: String,
}

/// The versions that make runs comparable and reproducible.
#[derive(Debug, Serialize, Deserialize)]
pub struct Pins {
    /// The crates.io framework version the blind scaffold pinned (`=x.y.z`).
    pub framework_version: String,
    /// Git commit of the checkout the docs snapshot was copied from
    /// (`unknown` when the docs directory is not inside a git checkout).
    pub docs_commit: String,
    /// sha256 (hex) over the provisioned docs snapshot's actual bytes
    /// (sorted relative paths + contents) — the content-true pin that
    /// `docs_commit` alone cannot give when the source tree is dirty.
    pub docs_sha256: String,
    /// Semantic fingerprint of the exit questionnaire definition.
    pub questionnaire_fingerprint: String,
}

/// What the run did — and what the agent says it did — about blindness.
#[derive(Debug, Serialize, Deserialize)]
pub struct Blindness {
    /// One-line statement of the protocol this run enforced.
    pub policy: String,
    /// The only environment variables the agent process inherited.
    pub env_allowlist: Vec<String>,
    /// True when provisioning excluded framework source (always, today;
    /// recorded so a future compromised mode is visible in the report).
    pub framework_source_excluded: bool,
    /// The agent's own answer: which provided docs it consulted.
    pub agent_reported_docs: Option<String>,
    /// The agent's own answer: what it relied on beyond the provided docs.
    pub agent_reported_external_sources: Option<String>,
}

/// Instrumentation of the implementation session.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionReport {
    /// The shell command the session ran (the agent seam).
    pub agent_cmd: String,
    pub wall_seconds: f64,
    /// The agent process exit code; `None` when killed by a signal.
    pub exit_code: Option<i32>,
    /// True when the session hit its deadline and was killed.
    pub timed_out: bool,
    /// How many times the agent command was invoked for this run.
    pub attempts: u32,
    /// Conversation turns, when the transcript is Claude Code stream-json.
    pub turns: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// Path of the session transcript, relative to the run directory.
    pub transcript: String,
}

/// Objective results: did the produced app build, and did the pre-written
/// checks pass against its binary.
#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptanceReport {
    pub built: bool,
    /// Build stderr tail when the build failed.
    pub build_detail: Option<String>,
    pub checks: Vec<CheckResult>,
}

impl AcceptanceReport {
    /// A report for a workspace whose app never built: no checks ran.
    pub fn build_failed(detail: String) -> Self {
        Self {
            built: false,
            build_detail: Some(detail),
            checks: Vec::new(),
        }
    }
}

/// One acceptance check's outcome.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    /// Why it failed; `None` on a pass.
    pub detail: Option<String>,
}

/// One invariant-matrix cell's outcome.
#[derive(Debug, Serialize, Deserialize)]
pub struct InvariantCell {
    /// The command words the cell ran, joined by spaces (e.g. `list`).
    pub command: String,
    /// Which invariant the cell asserts (e.g. `text: no unresolved tags`).
    pub check: String,
    pub passed: bool,
    pub detail: Option<String>,
}

/// The collected exit questionnaire: the agent's self-assessment.
#[derive(Debug, Serialize, Deserialize)]
pub struct QuestionnaireReport {
    /// True when the sheet parsed and decoded cleanly.
    pub collected: bool,
    /// Parse/validation diagnostics when it did not.
    pub diagnostics: Vec<String>,
    /// Decoded answers keyed by stable field id.
    pub answers: BTreeMap<String, String>,
}

impl RunReport {
    /// Writes the report as pretty JSON to `path`.
    pub fn write(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json + "\n")
            .with_context(|| format!("writing report to {}", path.display()))
    }
}
