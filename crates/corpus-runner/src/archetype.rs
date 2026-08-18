//! Archetype loading: a spec plus its pre-written acceptance suite.
//!
//! An archetype is a directory under `corpus/archetypes/<name>/` holding
//! `spec.md` (the agent-facing behavioral spec) and `acceptance.toml` (the
//! binary name, black-box checks, and invariant-matrix commands — authored
//! before any implementation, so "did it work" is never judged by the
//! implementer).

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// One archetype as loaded from disk.
#[derive(Debug)]
pub struct Archetype {
    pub name: String,
    pub dir: PathBuf,
    /// The exact spec text the agent will receive.
    pub spec: String,
    pub acceptance: AcceptanceConfig,
}

/// `acceptance.toml`: what to run against the produced binary.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceConfig {
    /// The binary the scaffold's package produces.
    pub binary: String,
    #[serde(rename = "check", default)]
    pub checks: Vec<Check>,
    #[serde(default)]
    pub invariants: Invariants,
}

/// One black-box acceptance check.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub name: String,
    pub args: Vec<String>,
    /// Expected exit code (default 0).
    #[serde(default)]
    pub expect_exit: i32,
    /// Substrings that must each appear on stdout.
    #[serde(default)]
    pub stdout_contains: Vec<String>,
    /// Row-association groups: every value in a group must co-occur on one
    /// single stdout line (e.g. a star with *its* constellation and
    /// magnitude), which flat `stdout_contains` cannot express.
    #[serde(default)]
    pub stdout_row_contains: Vec<Vec<String>>,
    /// When true, stdout must parse as JSON.
    #[serde(default)]
    pub stdout_is_json: bool,
    /// JSON row-association groups: stdout must parse as JSON and every
    /// value in a group must co-occur among the scalars of one single JSON
    /// array element (numbers match their decimal literal).
    #[serde(default)]
    pub stdout_json_rows: Vec<Vec<String>>,
}

/// Which commands the ROB01 invariant matrix exercises.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invariants {
    /// Command word-lists; each runs across the output-mode matrix.
    #[serde(default)]
    pub commands: Vec<Vec<String>>,
}

impl Archetype {
    /// Loads `archetypes_dir/<name>/{spec.md,acceptance.toml}`.
    pub fn load(archetypes_dir: &Path, name: &str) -> anyhow::Result<Self> {
        let dir = archetypes_dir.join(name);
        let spec_path = dir.join("spec.md");
        let spec = std::fs::read_to_string(&spec_path)
            .with_context(|| format!("reading archetype spec {}", spec_path.display()))?;
        let acceptance_path = dir.join("acceptance.toml");
        let acceptance_text = std::fs::read_to_string(&acceptance_path)
            .with_context(|| format!("reading {}", acceptance_path.display()))?;
        let acceptance: AcceptanceConfig = toml::from_str(&acceptance_text)
            .with_context(|| format!("parsing {}", acceptance_path.display()))?;
        Ok(Self {
            name: name.to_string(),
            dir,
            spec,
            acceptance,
        })
    }

    /// sha256 (hex) of the spec text, pinning the run to spec content.
    pub fn spec_sha256(&self) -> String {
        let digest = Sha256::digest(self.spec.as_bytes());
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }
}
