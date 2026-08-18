//! The run report: the durable artifact a corpus run leaves behind.
//!
//! `schema_version` 3 — the shape is a recorded decision (see
//! `corpus/README.md`); an ADR may formalize it later. Objective results
//! (acceptance, invariants) and the agent's self-assessment (questionnaire)
//! are deliberately separate sections, and the `pins` block is what makes two
//! runs comparable: same spec hash + same framework version + same docs
//! commit means the same experiment. Version 3 is one bump carrying every
//! shape change over version 2: it replaced the single `isolation_backend`
//! word with the per-capability [`IsolationRecord`], dropped the
//! producerless `session.attempts` counter, and removed the retired check
//! schema's parallel `checks` vector. Committed v2 reports (the preserved
//! pilot evidence) still deserialize: the historical path accepts
//! [`HISTORICAL_SCHEMA_MIN`]`..=`[`SCHEMA_VERSION`], and retired keys are
//! ignored where present.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// The current report schema version.
pub const SCHEMA_VERSION: u32 = 3;

/// Everything one corpus run durably records.
#[derive(Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub schema_version: u32,
    /// Directory-name identity of the run, e.g. `smoke-1755500000`.
    pub run_id: String,
    pub archetype: ArchetypeStamp,
    pub pins: Pins,
    pub evaluation: EvaluationStamp,
    pub blindness: Blindness,
    pub session: SessionReport,
    pub acceptance: AcceptanceReport,
    /// ROB01 invariant-matrix cells, one per (command × check).
    pub invariants: Vec<InvariantCell>,
    pub questionnaire: QuestionnaireReport,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluationStamp {
    /// `full-run` when one runner invocation owned every phase, or
    /// `isolated-re-evaluation` for preserved historical workspaces.
    pub origin: String,
    /// The check-phase boundary the objective results ran under. Its policy
    /// requests a network denial; the record states whether the backend
    /// actually enforced it.
    pub isolation: IsolationRecord,
    /// sha256 of the exact produced executable evaluated, when one existed.
    pub binary_sha256: Option<String>,
}

/// What the isolation boundary actually enforced, per capability. One
/// backend word overstates parity — Linux Landlock (ABI v1) cannot enforce
/// a network policy at all, and its default-deny filesystem model differs
/// from Seatbelt's allow-default-with-denied-roots — so reports record each
/// capability distinctly and never read stronger than what was enforced.
/// The network state is derived from the phase policy that was actually
/// applied, never from the backend's abilities alone.
#[derive(Debug, Serialize, Deserialize)]
pub struct IsolationRecord {
    /// The kernel mechanism, e.g. `macos-seatbelt` or `linux-landlock`.
    pub backend: String,
    /// The filesystem enforcement model this backend applied.
    pub filesystem: String,
    /// How the phase policy's network request landed on this backend.
    pub network: NetworkEnforcement,
}

/// What actually happened at the network boundary of one phase policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkEnforcement {
    /// The phase policy did not request a network denial: access was allowed
    /// deliberately (the agent and build phases fetch crates.io).
    AllowedByPolicy,
    /// The policy requested a denial and the backend enforced it (Seatbelt).
    Denied,
    /// The policy requested a denial the backend cannot enforce — Landlock
    /// ABI v1 is filesystem-only — so the gap is recorded loudly instead
    /// (and warned about at apply time).
    DenialRequestedButUnsupported,
    /// No kernel boundary made any network claim (historical agent sessions,
    /// hosts without a supported backend).
    NotEnforced,
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
    /// sha256 of the exact acceptance.toml evaluated by the runner.
    pub acceptance_sha256: String,
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
    /// Kernel-enforced boundary for the agent session and the scaffold
    /// build (network is allowed by policy there — both phases fetch
    /// crates.io); the check boundary's record lives in `evaluation`.
    pub isolation: IsolationRecord,
    /// Any credential intentionally admitted to the agent boundary. Empty is
    /// the only safe default; generated builds share the agent's sandbox.
    pub credential_exceptions: Vec<String>,
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
    /// Conversation turns, when the transcript is Claude Code stream-json.
    pub turns: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// Path of the session transcript, relative to the run directory.
    pub transcript: String,
}

/// Objective results: did the produced app build, and did the pre-written
/// suite's cases pass against its binary.
#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptanceReport {
    pub built: bool,
    /// Build stderr tail when the build failed.
    pub build_detail: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cases: Vec<CaseResult>,
}

impl AcceptanceReport {
    /// A report for a workspace whose app never built: nothing ran.
    pub fn build_failed(detail: String) -> Self {
        Self {
            built: false,
            build_detail: Some(detail),
            cases: Vec::new(),
        }
    }
}

/// One roster acceptance case's outcome, carrying the case's own metadata
/// so the report (and the scorecard built from it) reads without the
/// acceptance.toml beside it.
#[derive(Debug, Serialize, Deserialize)]
pub struct CaseResult {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// The interaction the case stresses, from the suite.
    pub stresses: String,
    /// The authored expectation: `pass`, or `fail` for gap cases.
    pub expected: String,
    pub outcome: CaseOutcome,
    /// The parity epic a gap case signals for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<String>,
    /// Why a gap case fails today, from the suite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Failure detail with the observed streams; `None` on a clean pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// How a roster case landed once its `expected` marker is applied:
/// expected-fail is the gap cases' steady state, and an unexpected pass is
/// news (a gap silently closed), never celebrated silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaseOutcome {
    Pass,
    Fail,
    ExpectedFail,
    UnexpectedPass,
}

impl CaseOutcome {
    /// True for the outcomes that leave the suite healthy: a pass, or a gap
    /// case failing exactly as authored.
    pub fn is_expected(self) -> bool {
        matches!(self, CaseOutcome::Pass | CaseOutcome::ExpectedFail)
    }
}

/// One invariant-matrix cell's outcome.
#[derive(Debug, Serialize, Deserialize)]
pub struct InvariantCell {
    /// The command words the cell ran, joined by spaces (e.g. `list`).
    pub command: String,
    pub mode: String,
    pub color: String,
    pub theme: String,
    /// Which invariant the cell asserts (e.g. `text: no unresolved tags`).
    pub check: String,
    pub status: InvariantStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InvariantStatus {
    Pass,
    Fail,
    NotRun,
    NotApplicable,
}

impl InvariantStatus {
    pub fn passed(self) -> bool {
        self == Self::Pass
    }
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

/// The oldest report schema the typed historical path accepts. Version 2 is
/// the schema the preserved pilot evidence was written in and the oldest
/// shape [`HistoricalRun`] matches; earlier or unknown future versions are
/// refused by `reevaluate` rather than silently rewritten as the current
/// schema.
pub const HISTORICAL_SCHEMA_MIN: u32 = 2;

/// The slice of a historical report that re-evaluation preserves: identity,
/// pins, session instrumentation, questionnaire, and the agent's own
/// blindness answers. Preservation is verbatim except
/// `pins.acceptance_sha256`, which re-evaluation recomputes from the loaded
/// suite. Everything re-evaluation regenerates (evaluation, acceptance,
/// invariants, the enforced-blindness record) is absent, so any report in
/// the supported range ([`HISTORICAL_SCHEMA_MIN`]`..=`[`SCHEMA_VERSION`])
/// loads typed — the source's `schema_version` is required and validated by
/// `reevaluate`, unknown or retired keys are ignored, and an off-shape file
/// fails with a serde diagnostic naming the field, never a panic.
#[derive(Debug, Deserialize)]
pub struct HistoricalRun {
    pub schema_version: u32,
    pub run_id: String,
    pub archetype: HistoricalArchetype,
    pub pins: Pins,
    pub blindness: HistoricalBlindness,
    pub session: SessionReport,
    pub questionnaire: QuestionnaireReport,
}

/// The archetype identity a historical report claims (its hash pins are
/// recomputed from the current archetype on re-evaluation).
#[derive(Debug, Deserialize)]
pub struct HistoricalArchetype {
    pub name: String,
}

/// The preserved blindness facts: the historical env allowlist and the
/// agent's self-reported sources. The enforcement half is rewritten because
/// re-evaluation cannot vouch for a boundary it never installed.
#[derive(Debug, Deserialize)]
pub struct HistoricalBlindness {
    pub env_allowlist: Vec<String>,
    #[serde(default)]
    pub agent_reported_docs: Option<String>,
    #[serde(default)]
    pub agent_reported_external_sources: Option<String>,
}
