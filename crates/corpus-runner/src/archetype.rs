//! Archetype loading: a spec plus its pre-written acceptance suite.
//!
//! An archetype is a directory under `corpus/archetypes/<name>/` holding
//! `spec.md` (the agent-facing behavioral spec) and `acceptance.toml` —
//! authored before any implementation, so "did it work" is never judged by
//! the implementer. Two acceptance schemas exist (`corpus/README.md`):
//!
//! - The **roster case schema** (`schema = 1`, `[[case]]` tables): black-box
//!   cases with full run semantics — scrubbed baseline env, sandbox files,
//!   pty attachment, scripted stdin, per-case timeout — and the documented
//!   assertion vocabulary, including `expected = "fail"` gap markers. The
//!   binary name is the archetype name (the roster's naming rule).
//! - The **runner check schema** (`binary` + `[[check]]`): the walking
//!   skeleton's own simpler vocabulary, used by the `smoke` archetype (whose
//!   binary name deliberately differs from its directory name).
//!
//! Both may carry an `[invariants]` table naming the commands the ROB01
//! invariant matrix sweeps.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context};
use serde::Deserialize;

use crate::digest;

/// One archetype as loaded from disk.
#[derive(Debug)]
pub struct Archetype {
    pub name: String,
    /// The exact spec text the agent will receive.
    pub spec: String,
    pub suite: Suite,
    acceptance_sha256: String,
}

/// The acceptance suite, in whichever schema the archetype carries.
#[derive(Debug)]
pub enum Suite {
    /// The runner check schema (`binary` + `[[check]]`) — the smoke path.
    Checks(ChecksConfig),
    /// The roster case schema (`schema = 1`, `[[case]]`).
    Cases(CaseSuite),
}

/// The runner check schema: what to run against the produced binary.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecksConfig {
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

/// Declarative ROB01 matrix plan. The global axes define the stable planned
/// cells; each command declares only whether its output is
/// framework-rendered or intentionally opaque bytes — every command runs on
/// every global axis combination.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invariants {
    #[serde(default = "all_modes")]
    pub modes: Vec<InvariantMode>,
    #[serde(default = "all_colors")]
    pub colors: Vec<ColorState>,
    #[serde(rename = "theme", default = "default_themes")]
    pub themes: Vec<InvariantTheme>,
    #[serde(rename = "command", default)]
    pub commands: Vec<InvariantCommand>,
}

impl Default for Invariants {
    fn default() -> Self {
        Self {
            modes: all_modes(),
            colors: all_colors(),
            themes: default_themes(),
            commands: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InvariantMode {
    Text,
    Term,
    Json,
}

impl InvariantMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Term => "term",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ColorState {
    Off,
    On,
}

impl ColorState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InvariantContract {
    Rendered,
    OpaqueBytes,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvariantTheme {
    pub name: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvariantCommand {
    pub argv: Vec<String>,
    pub contract: InvariantContract,
}

fn all_modes() -> Vec<InvariantMode> {
    vec![
        InvariantMode::Text,
        InvariantMode::Term,
        InvariantMode::Json,
    ]
}

fn all_colors() -> Vec<ColorState> {
    vec![ColorState::Off, ColorState::On]
}

fn default_themes() -> Vec<InvariantTheme> {
    vec![InvariantTheme {
        name: "application".to_string(),
        env: BTreeMap::new(),
    }]
}

/// The roster case schema (`corpus/README.md`, "Acceptance case format").
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseSuite {
    /// Schema version; only 1 exists.
    pub schema: u32,
    /// Must match the directory name; doubles as the binary name.
    pub archetype: String,
    #[serde(rename = "case")]
    pub cases: Vec<Case>,
    #[serde(default)]
    pub invariants: Invariants,
}

/// One roster acceptance case.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub name: String,
    /// Optional milestone/topic grouping, echoed into the report.
    pub group: Option<String>,
    /// One line naming the interaction under test.
    pub stresses: String,
    pub expected: Expected,
    /// The epic that closes the gap; required when `expected = "fail"`.
    pub gap: Option<String>,
    /// Why the case fails today; required when `expected = "fail"`.
    pub reason: Option<String>,
    pub run: CaseRun,
    pub expect: CaseExpect,
}

/// Whether the case is expected to pass, or is specced past current
/// capability (`fail`, with `gap`/`reason` naming the parity signal).
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Expected {
    Pass,
    Fail,
}

/// How to run the binary for one case (`corpus/README.md`, Run semantics).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseRun {
    /// Arguments after the binary name; may be empty (a naked invocation).
    pub argv: Vec<String>,
    /// Explicit env on top of the scrubbed baseline.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Streams attached to a pty; all others are pipes.
    #[serde(default)]
    pub tty: Vec<TtyStream>,
    /// Scripted input; omitted means stdin is piped and already at EOF.
    pub stdin: Option<String>,
    /// Working directory, relative to the sandbox root (default `.`).
    pub cwd: Option<String>,
    /// Hard bound; exceeding it fails the case.
    pub timeout_seconds: u64,
    /// Sandbox files created before the run, keyed by relative path.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

/// A standard stream a case may attach to the pty.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TtyStream {
    Stdin,
    Stdout,
    Stderr,
}

/// The case's assertions (`corpus/README.md`, Assertion vocabulary).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseExpect {
    pub exit_code: Option<i32>,
    /// Exact stream contents, LF-normalized.
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    /// stdout parses as JSON semantically equal to this JSON string.
    pub stdout_json: Option<String>,
    #[serde(default)]
    pub stdout_contains: Vec<String>,
    #[serde(default)]
    pub stderr_contains: Vec<String>,
    #[serde(default)]
    pub stdout_not_contains: Vec<String>,
    #[serde(default)]
    pub stderr_not_contains: Vec<String>,
    /// Each suffix must terminate exactly one non-empty stdout line. This is
    /// the answer-sheet assertion: tags cannot be duplicated or collapsed
    /// into prose and still pass.
    #[serde(default)]
    pub stdout_lines_end_with_once: Vec<String>,
}

impl CaseExpect {
    /// True when the case asserts nothing — a schema error at load time.
    fn is_empty(&self) -> bool {
        self.exit_code.is_none()
            && self.stdout.is_none()
            && self.stderr.is_none()
            && self.stdout_json.is_none()
            && self.stdout_contains.is_empty()
            && self.stderr_contains.is_empty()
            && self.stdout_not_contains.is_empty()
            && self.stderr_not_contains.is_empty()
            && self.stdout_lines_end_with_once.is_empty()
    }
}

impl Archetype {
    /// Loads `archetypes_dir/<name>/{spec.md,acceptance.toml}`, detecting
    /// which acceptance schema the file speaks by its tables.
    pub fn load(archetypes_dir: &Path, name: &str) -> anyhow::Result<Self> {
        let dir = archetypes_dir.join(name);
        let spec_path = dir.join("spec.md");
        let spec = std::fs::read_to_string(&spec_path)
            .with_context(|| format!("reading archetype spec {}", spec_path.display()))?;
        let acceptance_path = dir.join("acceptance.toml");
        let acceptance_text = std::fs::read_to_string(&acceptance_path)
            .with_context(|| format!("reading {}", acceptance_path.display()))?;
        let value: toml::Value = acceptance_text
            .parse()
            .with_context(|| format!("parsing {}", acceptance_path.display()))?;
        let suite = if value.get("case").is_some() || value.get("schema").is_some() {
            let suite: CaseSuite = value
                .try_into()
                .with_context(|| format!("parsing {}", acceptance_path.display()))?;
            validate_case_suite(&suite, name, &acceptance_path)?;
            Suite::Cases(suite)
        } else {
            Suite::Checks(
                value
                    .try_into()
                    .with_context(|| format!("parsing {}", acceptance_path.display()))?,
            )
        };
        Ok(Self {
            name: name.to_string(),
            spec,
            suite,
            acceptance_sha256: digest::sha256_hex(&acceptance_text),
        })
    }

    /// The binary the produced app must build: the roster rule is that
    /// archetype names double as binary names; the check schema names its
    /// binary explicitly (smoke's `smoketable`).
    pub fn binary(&self) -> &str {
        match &self.suite {
            Suite::Checks(config) => &config.binary,
            Suite::Cases(_) => &self.name,
        }
    }

    /// The commands the ROB01 invariant matrix sweeps.
    pub fn invariants(&self) -> &Invariants {
        match &self.suite {
            Suite::Checks(config) => &config.invariants,
            Suite::Cases(suite) => &suite.invariants,
        }
    }

    /// sha256 (hex) of the spec text, pinning the run to spec content.
    pub fn spec_sha256(&self) -> String {
        digest::sha256_hex(&self.spec)
    }

    pub fn acceptance_sha256(&self) -> &str {
        &self.acceptance_sha256
    }
}

/// The semantic rules the case schema carries beyond its shape: version 1,
/// archetype/directory agreement, a positive timeout, at least one assertion
/// per case, and `gap`/`reason` exactly on expected-fail cases.
fn validate_case_suite(suite: &CaseSuite, name: &str, path: &Path) -> anyhow::Result<()> {
    if suite.schema != 1 {
        bail!(
            "{}: unknown schema version {}",
            path.display(),
            suite.schema
        );
    }
    if suite.archetype != name {
        bail!(
            "{}: archetype field {:?} does not match directory name {:?}",
            path.display(),
            suite.archetype,
            name
        );
    }
    for case in &suite.cases {
        if case.run.timeout_seconds == 0 {
            bail!(
                "{}: case {:?} must carry a positive timeout_seconds",
                path.display(),
                case.name
            );
        }
        if case.expect.is_empty() {
            bail!("{}: case {:?} asserts nothing", path.display(), case.name);
        }
        let is_gap = case.expected == Expected::Fail;
        if is_gap != (case.gap.is_some() && case.reason.is_some()) {
            bail!(
                "{}: case {:?} must carry gap+reason exactly when expected = \"fail\"",
                path.display(),
                case.name
            );
        }
    }
    Ok(())
}
