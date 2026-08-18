//! Archetype loading: a spec plus its pre-written acceptance suite.
//!
//! An archetype is a directory under `corpus/archetypes/<name>/` holding
//! `spec.md` (the agent-facing behavioral spec) and `acceptance.toml` —
//! authored before any implementation, so "did it work" is never judged by
//! the implementer. There is one acceptance schema (`corpus/README.md`): the
//! **roster case schema** (`schema = 1`, `[[case]]` tables) — black-box
//! cases with full run semantics — scrubbed baseline env, sandbox files,
//! pty attachment, scripted stdin, per-case timeout — and the documented
//! assertion vocabulary, including `expected = "fail"` gap markers. The
//! binary name is the archetype name (the roster's naming rule).
//!
//! A suite may carry an `[invariants]` table naming the commands the ROB01
//! invariant matrix sweeps.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// One archetype as loaded from disk.
#[derive(Debug)]
pub struct Archetype {
    pub name: String,
    pub dir: PathBuf,
    /// The exact spec text the agent will receive.
    pub spec: String,
    pub suite: CaseSuite,
    acceptance_sha256: String,
}

/// Declarative ROB01 matrix plan. The global axes define the stable planned
/// cells; each command then declares where it applies and whether its output
/// is framework-rendered or intentionally opaque bytes.
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
    /// Empty means every global mode is applicable.
    #[serde(default)]
    pub modes: Vec<InvariantMode>,
    /// Empty means every global color state is applicable.
    #[serde(default)]
    pub colors: Vec<ColorState>,
    /// Empty means every global theme is applicable.
    #[serde(default)]
    pub themes: Vec<String>,
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
///
/// This is the schema's single definition: the runner loads suites through
/// it, and the roster structural test (`tests/corpus_roster.rs`) parses the
/// committed roster through [`CaseSuite::parse`], so a suite cannot pass one
/// consumer and fail the other.
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
    /// Row-association groups: every value in a group must co-occur on one
    /// single stdout line (e.g. a star with *its* constellation and
    /// magnitude), which flat `stdout_contains` cannot express.
    #[serde(default)]
    pub stdout_row_contains: Vec<Vec<String>>,
    /// JSON row-association groups: stdout must parse as JSON and every
    /// value in a group must co-occur among the scalars of one single JSON
    /// array element (numbers match their decimal literal).
    #[serde(default)]
    pub stdout_json_rows: Vec<Vec<String>>,
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
            && self.stdout_row_contains.is_empty()
            && self.stdout_json_rows.is_empty()
            && self.stdout_not_contains.is_empty()
            && self.stderr_not_contains.is_empty()
            && self.stdout_lines_end_with_once.is_empty()
    }
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
        let suite = CaseSuite::parse(&acceptance_text, name, &acceptance_path)?;
        Ok(Self {
            name: name.to_string(),
            dir,
            spec,
            suite,
            acceptance_sha256: sha256(&acceptance_text),
        })
    }

    /// The binary the produced app must build: the roster rule is that
    /// archetype names double as binary names.
    pub fn binary(&self) -> &str {
        &self.name
    }

    /// The commands the ROB01 invariant matrix sweeps.
    pub fn invariants(&self) -> &Invariants {
        &self.suite.invariants
    }

    /// sha256 (hex) of the spec text, pinning the run to spec content.
    pub fn spec_sha256(&self) -> String {
        sha256(&self.spec)
    }

    pub fn acceptance_sha256(&self) -> &str {
        &self.acceptance_sha256
    }
}

fn sha256(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

impl CaseSuite {
    /// Parses and validates `text` as a case suite for archetype `name`
    /// (the owning directory name); `path` labels diagnostics. This is the
    /// one entry point every consumer shares — [`Archetype::load`] and the
    /// roster structural test alike.
    pub fn parse(text: &str, name: &str, path: &Path) -> anyhow::Result<Self> {
        let suite: CaseSuite =
            toml::from_str(text).with_context(|| format!("parsing {}", path.display()))?;
        validate_case_suite(&suite, name, path)?;
        Ok(suite)
    }
}

/// The semantic rules the case schema carries beyond its shape: version 1,
/// archetype/directory agreement, a non-empty suite of uniquely named
/// kebab-case cases, a non-empty `stresses` line, a positive timeout, at
/// least one assertion per case, no vacuous assertion elements (an empty
/// row group or an empty string in a substring/row/suffix list matches any
/// output), no exact-`stdout` + semantic-`stdout_json` contradiction,
/// `stdout_json` that parses as JSON, and non-empty `gap`/`reason` exactly
/// on expected-fail cases.
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
    if suite.cases.is_empty() {
        bail!("{}: empty acceptance suite", path.display());
    }
    let mut names = HashSet::new();
    for case in &suite.cases {
        if !names.insert(case.name.as_str()) {
            bail!("{}: duplicate case name {:?}", path.display(), case.name);
        }
        let kebab = !case.name.is_empty()
            && case
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !kebab {
            bail!(
                "{}: case {:?} — case names are kebab-case",
                path.display(),
                case.name
            );
        }
        if case.stresses.is_empty() {
            bail!(
                "{}: case {:?} — `stresses` must name the interaction under test",
                path.display(),
                case.name
            );
        }
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
        // A vacuous element asserts nothing while counting as an assertion:
        // an empty row group is satisfied by any output, and an empty
        // string is a substring of everything. Reject both at load time so
        // a malformed case cannot silently pass.
        for (key, rows) in [
            ("stdout_row_contains", &case.expect.stdout_row_contains),
            ("stdout_json_rows", &case.expect.stdout_json_rows),
        ] {
            if rows.iter().any(|row| row.is_empty()) {
                bail!(
                    "{}: case {:?} — {key} groups must be non-empty",
                    path.display(),
                    case.name
                );
            }
            if rows.iter().flatten().any(|cell| cell.is_empty()) {
                bail!(
                    "{}: case {:?} — {key} cells must be non-empty",
                    path.display(),
                    case.name
                );
            }
        }
        for (key, entries) in [
            ("stdout_contains", &case.expect.stdout_contains),
            ("stderr_contains", &case.expect.stderr_contains),
            ("stdout_not_contains", &case.expect.stdout_not_contains),
            ("stderr_not_contains", &case.expect.stderr_not_contains),
            (
                "stdout_lines_end_with_once",
                &case.expect.stdout_lines_end_with_once,
            ),
        ] {
            if entries.iter().any(|entry| entry.is_empty()) {
                bail!(
                    "{}: case {:?} — {key} entries must be non-empty",
                    path.display(),
                    case.name
                );
            }
        }
        // Exact-stdout and semantic-JSON assertions on the same stream
        // contradict each other's reason to exist; a case picks one.
        if case.expect.stdout.is_some() && case.expect.stdout_json.is_some() {
            bail!(
                "{}: case {:?} — use `stdout` (exact) or `stdout_json` (semantic), not both",
                path.display(),
                case.name
            );
        }
        if let Some(json) = &case.expect.stdout_json {
            serde_json::from_str::<serde_json::Value>(json).with_context(|| {
                format!(
                    "{}: case {:?} — stdout_json is not valid JSON",
                    path.display(),
                    case.name
                )
            })?;
        }
        match case.expected {
            Expected::Pass => {
                if case.gap.is_some() || case.reason.is_some() {
                    bail!(
                        "{}: case {:?} — `gap`/`reason` belong to expected-fail cases only",
                        path.display(),
                        case.name
                    );
                }
            }
            Expected::Fail => {
                let named = case.gap.as_deref().is_some_and(|g| !g.is_empty())
                    && case.reason.as_deref().is_some_and(|r| !r.is_empty());
                if !named {
                    bail!(
                        "{}: case {:?} must carry non-empty gap+reason when expected = \"fail\"",
                        path.display(),
                        case.name
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! One fixture per reconciled validation rule: the case-schema rules the
    //! runner and the roster structural test used to disagree on, now all
    //! enforced by the shared parser above.

    use super::*;

    /// A minimal valid suite; `extra` splices fields into the one case.
    fn suite(case: &str) -> String {
        format!("schema = 1\narchetype = \"fake\"\n{case}")
    }

    fn parse(text: &str) -> anyhow::Result<CaseSuite> {
        CaseSuite::parse(text, "fake", Path::new("acceptance.toml"))
    }

    const VALID_CASE: &str = r#"
[[case]]
name = "valid-case"
stresses = "validation"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
exit_code = 0
"#;

    #[test]
    fn minimal_valid_suite_parses() {
        let suite = parse(&suite(VALID_CASE)).unwrap();
        assert_eq!(suite.cases.len(), 1);
    }

    #[test]
    fn unknown_schema_version_is_rejected() {
        let text = format!("schema = 2\narchetype = \"fake\"\n{VALID_CASE}");
        let err = parse(&text).unwrap_err();
        assert!(
            err.to_string().contains("unknown schema version"),
            "{err:#}"
        );
    }

    #[test]
    fn archetype_directory_mismatch_is_rejected() {
        let text = format!("schema = 1\narchetype = \"other\"\n{VALID_CASE}");
        let err = parse(&text).unwrap_err();
        assert!(err.to_string().contains("does not match"), "{err:#}");
    }

    #[test]
    fn empty_suite_is_rejected() {
        // An absent `[[case]]` already fails shape-wise (`case` is a
        // required field); `case = []` is the shape-valid empty suite the
        // semantic rule must catch.
        let err = parse("schema = 1\narchetype = \"fake\"\ncase = []\n").unwrap_err();
        assert!(
            err.to_string().contains("empty acceptance suite"),
            "{err:#}"
        );
    }

    #[test]
    fn duplicate_case_names_are_rejected() {
        let err = parse(&suite(&format!("{VALID_CASE}{VALID_CASE}"))).unwrap_err();
        assert!(err.to_string().contains("duplicate case name"), "{err:#}");
    }

    #[test]
    fn non_kebab_case_names_are_rejected() {
        let err = parse(&suite(&VALID_CASE.replace("valid-case", "Valid_Case"))).unwrap_err();
        assert!(err.to_string().contains("kebab-case"), "{err:#}");
    }

    #[test]
    fn empty_stresses_is_rejected() {
        let err = parse(&suite(&VALID_CASE.replace("\"validation\"", "\"\""))).unwrap_err();
        assert!(err.to_string().contains("stresses"), "{err:#}");
    }

    #[test]
    fn zero_timeout_is_rejected() {
        let err = parse(&suite(
            &VALID_CASE.replace("timeout_seconds = 5", "timeout_seconds = 0"),
        ))
        .unwrap_err();
        assert!(err.to_string().contains("positive timeout"), "{err:#}");
    }

    #[test]
    fn assertion_free_case_is_rejected() {
        let err = parse(&suite(&VALID_CASE.replace("exit_code = 0", ""))).unwrap_err();
        assert!(err.to_string().contains("asserts nothing"), "{err:#}");
    }

    /// An empty assertion list is no assertion: presence of the key alone
    /// must not satisfy the at-least-one-assertion rule.
    #[test]
    fn empty_assertion_lists_do_not_count() {
        for key in ["stdout_contains", "stdout_row_contains", "stdout_json_rows"] {
            let err = parse(&suite(
                &VALID_CASE.replace("exit_code = 0", &format!("{key} = []")),
            ))
            .unwrap_err();
            assert!(err.to_string().contains("asserts nothing"), "{err:#}");
        }
    }

    /// `[[]]` counts as a present assertion but is satisfied by any
    /// output; the vacuous-element rule must reject it.
    #[test]
    fn empty_row_groups_are_rejected() {
        for key in ["stdout_row_contains", "stdout_json_rows"] {
            let err = parse(&suite(
                &VALID_CASE.replace("exit_code = 0", &format!("{key} = [[]]")),
            ))
            .unwrap_err();
            assert!(
                err.to_string().contains("groups must be non-empty"),
                "{err:#}"
            );
        }
    }

    /// An empty cell is a substring of every line (and matching an empty
    /// scalar is `stdout_json` territory); the rule rejects it.
    #[test]
    fn empty_row_cells_are_rejected() {
        for key in ["stdout_row_contains", "stdout_json_rows"] {
            let err = parse(&suite(
                &VALID_CASE.replace("exit_code = 0", &format!("{key} = [[\"\"]]")),
            ))
            .unwrap_err();
            assert!(
                err.to_string().contains("cells must be non-empty"),
                "{err:#}"
            );
        }
    }

    /// The empty string is a substring (and suffix) of everything, so an
    /// empty entry in any flat assertion list is equally vacuous.
    #[test]
    fn empty_list_entries_are_rejected() {
        for key in [
            "stdout_contains",
            "stderr_contains",
            "stdout_not_contains",
            "stderr_not_contains",
            "stdout_lines_end_with_once",
        ] {
            let err = parse(&suite(
                &VALID_CASE.replace("exit_code = 0", &format!("{key} = [\"\"]")),
            ))
            .unwrap_err();
            assert!(
                err.to_string().contains("entries must be non-empty"),
                "{err:#}"
            );
        }
    }

    /// The two row-association kinds ported from the retired check schema
    /// parse and each satisfies the at-least-one-assertion rule alone.
    #[test]
    fn row_association_assertions_parse_and_count() {
        for key in ["stdout_row_contains", "stdout_json_rows"] {
            let suite = parse(&suite(&VALID_CASE.replace(
                "exit_code = 0",
                &format!("{key} = [[\"Aldebaran\", \"Taurus\", \"0.86\"]]"),
            )))
            .unwrap();
            assert_eq!(suite.cases.len(), 1);
        }
    }

    #[test]
    fn exact_stdout_and_semantic_json_together_are_rejected() {
        let err = parse(&suite(
            &VALID_CASE.replace("exit_code = 0", "stdout = \"x\"\nstdout_json = '{}'"),
        ))
        .unwrap_err();
        assert!(err.to_string().contains("not both"), "{err:#}");
    }

    #[test]
    fn malformed_stdout_json_is_rejected() {
        let err = parse(&suite(
            &VALID_CASE.replace("exit_code = 0", "stdout_json = 'not json'"),
        ))
        .unwrap_err();
        assert!(err.to_string().contains("not valid JSON"), "{err:#}");
    }

    #[test]
    fn gap_or_reason_on_expected_pass_is_rejected() {
        let err = parse(&suite(&VALID_CASE.replace(
            "expected = \"pass\"",
            "expected = \"pass\"\ngap = \"PAR01\"",
        )))
        .unwrap_err();
        assert!(
            err.to_string().contains("expected-fail cases only"),
            "{err:#}"
        );
    }

    #[test]
    fn expected_fail_without_gap_and_reason_is_rejected() {
        let err = parse(&suite(&VALID_CASE.replace(
            "expected = \"pass\"",
            "expected = \"fail\"\ngap = \"PAR01\"",
        )))
        .unwrap_err();
        assert!(err.to_string().contains("gap+reason"), "{err:#}");
    }

    #[test]
    fn empty_gap_or_reason_on_expected_fail_is_rejected() {
        let err = parse(&suite(&VALID_CASE.replace(
            "expected = \"pass\"",
            "expected = \"fail\"\ngap = \"PAR01\"\nreason = \"\"",
        )))
        .unwrap_err();
        assert!(err.to_string().contains("gap+reason"), "{err:#}");
    }

    /// The documented default: `[invariants]` keys may be omitted
    /// individually — omitted axes mean the full global plan, not an error.
    #[test]
    fn invariants_keys_default_individually() {
        let suite = parse(&suite(&format!(
            "{VALID_CASE}\n[invariants]\n[[invariants.command]]\nargv = [\"log\"]\ncontract = \"rendered\"\n"
        )))
        .unwrap();
        assert_eq!(suite.invariants.modes.len(), 3);
        assert_eq!(suite.invariants.colors.len(), 2);
        assert_eq!(suite.invariants.themes.len(), 1);
        assert_eq!(suite.invariants.commands.len(), 1);
    }
}
