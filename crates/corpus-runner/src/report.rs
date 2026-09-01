use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub schema_version: u32,
    pub run_id: String,
    pub archetype: ArchetypeStamp,
    pub pins: Pins,
    pub evaluation: EvaluationStamp,
    pub blindness: Blindness,
    pub session: SessionReport,
    pub provenance: AgentProvenance,
    pub acceptance: AcceptanceReport,
    pub invariants: Vec<InvariantCell>,
    pub questionnaire: QuestionnaireReport,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluationStamp {
    pub origin: String,
    pub isolation: IsolationRecord,
    pub binary_sha256: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IsolationRecord {
    pub backend: String,
    pub filesystem: String,
    pub network: NetworkEnforcement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkEnforcement {
    AllowedByPolicy,
    Denied,
    DenialRequestedButUnsupported,
    NotEnforced,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArchetypeStamp {
    pub name: String,
    pub spec_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Pins {
    pub framework_version: String,
    pub docs_commit: String,
    pub docs_sha256: String,
    pub acceptance_sha256: String,
    pub questionnaire_fingerprint: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Blindness {
    pub policy: String,
    pub env_allowlist: Vec<String>,
    pub framework_source_excluded: bool,
    pub isolation: IsolationRecord,
    pub credential_exceptions: Vec<String>,
    pub agent_reported_docs: Option<String>,
    pub agent_reported_external_sources: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionReport {
    pub agent_cmd: String,
    pub wall_seconds: f64,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub turns: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub transcript: String,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProvenance {
    pub backend: Option<String>,
    pub executable_version: Option<String>,
    pub model_requested: Option<String>,
    pub model_observed: Option<String>,
    pub prompt: Option<String>,
    pub settings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptanceReport {
    pub built: bool,
    pub build_detail: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cases: Vec<CaseResult>,
}

impl AcceptanceReport {
    pub fn build_failed(detail: String) -> Self {
        Self {
            built: false,
            build_detail: Some(detail),
            cases: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaseResult {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub stresses: String,
    pub expected: String,
    pub outcome: CaseOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaseOutcome {
    Pass,
    Fail,
    ExpectedFail,
    UnexpectedPass,
}

impl CaseOutcome {
    pub fn is_expected(self) -> bool {
        matches!(self, CaseOutcome::Pass | CaseOutcome::ExpectedFail)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InvariantCell {
    pub command: String,
    pub mode: String,
    pub color: String,
    pub theme: String,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct QuestionnaireReport {
    pub collected: bool,
    pub diagnostics: Vec<String>,
    pub answers: BTreeMap<String, String>,
}

impl RunReport {
    pub fn write(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json + "\n")
            .with_context(|| format!("writing report to {}", path.display()))
    }
}

pub const HISTORICAL_SCHEMA_MIN: u32 = 2;

#[derive(Debug, Deserialize)]
pub struct HistoricalRun {
    pub schema_version: u32,
    pub run_id: String,
    pub archetype: HistoricalArchetype,
    pub pins: Pins,
    pub blindness: HistoricalBlindness,
    pub session: SessionReport,
    #[serde(default)]
    pub provenance: Option<AgentProvenance>,
    pub questionnaire: QuestionnaireReport,
}

#[derive(Debug, Deserialize)]
pub struct HistoricalArchetype {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct HistoricalBlindness {
    pub env_allowlist: Vec<String>,
    #[serde(default)]
    pub agent_reported_docs: Option<String>,
    #[serde(default)]
    pub agent_reported_external_sources: Option<String>,
}
