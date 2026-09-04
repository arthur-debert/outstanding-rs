//! Archetype loading: a directory under `corpus/archetypes/<name>/` holding
//! `spec.md` and `acceptance.toml`. See `corpus/README.md` for the roster
//! case schema (`schema = 1`, `[[case]]` tables).

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{bail, Context};
use serde::Deserialize;

use crate::digest;
use crate::manifest::{GapEntry, Manifest};

#[derive(Debug)]
pub struct Archetype {
    pub name: String,
    pub spec: String,
    pub suite: CaseSuite,
    pub gaps: BTreeMap<String, GapEntry>,
    acceptance_sha256: String,
}

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

/// A presentation cell of the matrix, not a `--output` value: the human page
/// plain, the same page with escape sequences, and the JSON encoding. Since
/// TERM01 the first two are `--color never`/`always` on the human
/// representation, which the flag cannot name.
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

    pub fn argv(self) -> &'static [&'static str] {
        match self {
            Self::Text => &["--color", "never"],
            Self::Term => &["--color", "always"],
            Self::Json => &["--output", "json"],
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
    Either,
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
    #[serde(default = "default_true")]
    pub equal_across_modes: bool,
}

fn default_true() -> bool {
    true
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseSuite {
    pub schema: u32,
    pub archetype: String,
    #[serde(rename = "case")]
    pub cases: Vec<Case>,
    #[serde(default)]
    pub invariants: Invariants,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub name: String,
    pub group: Option<String>,
    pub stresses: String,
    pub expected: Expected,
    pub gap: Option<String>,
    pub reason: Option<String>,
    pub run: CaseRun,
    pub expect: CaseExpect,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Expected {
    Pass,
    Fail,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseRun {
    pub argv: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub tty: Vec<TtyStream>,
    pub stdin: Option<String>,
    pub cwd: Option<String>,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TtyStream {
    Stdin,
    Stdout,
    Stderr,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseExpect {
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_json: Option<String>,
    pub stdout_json_subset: Option<String>,
    #[serde(default)]
    pub stdout_contains: Vec<String>,
    #[serde(default)]
    pub stderr_contains: Vec<String>,
    #[serde(default)]
    pub stdout_row_contains: Vec<Vec<String>>,
    #[serde(default)]
    pub stdout_json_rows: Vec<Vec<String>>,
    #[serde(default)]
    pub stdout_not_contains: Vec<String>,
    #[serde(default)]
    pub stderr_not_contains: Vec<String>,
    #[serde(default)]
    pub stdout_lines_end_with_once: Vec<String>,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub files_absent: Vec<String>,
}

impl CaseExpect {
    fn is_empty(&self) -> bool {
        self.exit_code.is_none()
            && self.stdout.is_none()
            && self.stderr.is_none()
            && self.stdout_json.is_none()
            && self.stdout_json_subset.is_none()
            && self.stdout_contains.is_empty()
            && self.stderr_contains.is_empty()
            && self.stdout_row_contains.is_empty()
            && self.stdout_json_rows.is_empty()
            && self.stdout_not_contains.is_empty()
            && self.stderr_not_contains.is_empty()
            && self.stdout_lines_end_with_once.is_empty()
            && self.files.is_empty()
            && self.files_absent.is_empty()
    }
}

impl Archetype {
    pub fn load(archetypes_dir: &Path, name: &str) -> anyhow::Result<Self> {
        let dir = archetypes_dir.join(name);
        let spec_path = dir.join("spec.md");
        let spec = std::fs::read_to_string(&spec_path)
            .with_context(|| format!("reading archetype spec {}", spec_path.display()))?;
        let acceptance_path = dir.join("acceptance.toml");
        let acceptance_text = std::fs::read_to_string(&acceptance_path)
            .with_context(|| format!("reading {}", acceptance_path.display()))?;
        let suite = CaseSuite::parse(&acceptance_text, name, &acceptance_path)?;
        let gaps = Manifest::load_optional(archetypes_dir, name)?
            .map(|manifest| manifest.gaps)
            .unwrap_or_default();
        for case in &suite.cases {
            if let Some(gap) = case.gap.as_deref() {
                if !gaps.contains_key(gap) {
                    bail!(
                        "{}: case {:?} names gap {gap:?}, which is not a manifest [gaps] entry",
                        acceptance_path.display(),
                        case.name
                    );
                }
            }
        }
        Ok(Self {
            name: name.to_string(),
            spec,
            suite,
            gaps,
            acceptance_sha256: digest::sha256_hex(&acceptance_text),
        })
    }

    pub fn gap_evidence(&self, gap: &str) -> Option<crate::manifest::Evidence<'_>> {
        self.gaps.get(gap).and_then(GapEntry::evidence)
    }

    pub fn binary(&self) -> &str {
        &self.name
    }

    pub fn invariants(&self) -> &Invariants {
        &self.suite.invariants
    }

    pub fn spec_sha256(&self) -> String {
        digest::sha256_hex(&self.spec)
    }

    pub fn acceptance_sha256(&self) -> &str {
        &self.acceptance_sha256
    }
}

impl CaseSuite {
    pub fn parse(text: &str, name: &str, path: &Path) -> anyhow::Result<Self> {
        let suite: CaseSuite =
            toml::from_str(text).with_context(|| format!("parsing {}", path.display()))?;
        validate_case_suite(&suite, name, path)?;
        Ok(suite)
    }
}

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
            ("files_absent", &case.expect.files_absent),
        ] {
            if entries.iter().any(|entry| entry.is_empty()) {
                bail!(
                    "{}: case {:?} — {key} entries must be non-empty",
                    path.display(),
                    case.name
                );
            }
        }
        if case.expect.files.keys().any(|key| key.is_empty()) {
            bail!(
                "{}: case {:?} — files keys must be non-empty",
                path.display(),
                case.name
            );
        }
        for (key, semantic) in [
            ("stdout_json", case.expect.stdout_json.is_some()),
            (
                "stdout_json_subset",
                case.expect.stdout_json_subset.is_some(),
            ),
        ] {
            if case.expect.stdout.is_some() && semantic {
                bail!(
                    "{}: case {:?} — use `stdout` (exact) or `{key}` (semantic), not both",
                    path.display(),
                    case.name
                );
            }
        }
        for (key, json) in [
            ("stdout_json", &case.expect.stdout_json),
            ("stdout_json_subset", &case.expect.stdout_json_subset),
        ] {
            let Some(json) = json else { continue };
            serde_json::from_str::<serde_json::Value>(json).with_context(|| {
                format!(
                    "{}: case {:?} — {key} is not valid JSON",
                    path.display(),
                    case.name
                )
            })?;
        }
        match case.expected {
            Expected::Pass => {
                if case.reason.is_some() {
                    bail!(
                        "{}: case {:?} — `reason` explains an expected failure; it does not belong on an expected = \"pass\" case",
                        path.display(),
                        case.name
                    );
                }
                if case.gap.as_deref().is_some_and(str::is_empty) {
                    bail!(
                        "{}: case {:?} — `gap` must be non-empty when present",
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
    use super::*;

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

    #[test]
    fn empty_list_entries_are_rejected() {
        for key in [
            "stdout_contains",
            "stderr_contains",
            "stdout_not_contains",
            "stderr_not_contains",
            "stdout_lines_end_with_once",
            "files_absent",
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
        for key in ["stdout_json", "stdout_json_subset"] {
            let err = parse(&suite(
                &VALID_CASE.replace("exit_code = 0", &format!("stdout = \"x\"\n{key} = '{{}}'")),
            ))
            .unwrap_err();
            assert!(err.to_string().contains("not both"), "{err:#}");
        }
    }

    #[test]
    fn malformed_stdout_json_is_rejected() {
        for key in ["stdout_json", "stdout_json_subset"] {
            let err = parse(&suite(
                &VALID_CASE.replace("exit_code = 0", &format!("{key} = 'not json'")),
            ))
            .unwrap_err();
            assert!(err.to_string().contains("not valid JSON"), "{err:#}");
        }
    }

    #[test]
    fn stdout_json_subset_counts_as_an_assertion() {
        let suite = parse(&suite(
            &VALID_CASE.replace("exit_code = 0", "stdout_json_subset = '{\"a\":1}'"),
        ))
        .unwrap();
        assert_eq!(suite.cases.len(), 1);
    }

    #[test]
    fn files_and_files_absent_count_as_assertions() {
        let suite = parse(&suite(&VALID_CASE.replace(
            "exit_code = 0",
            "files_absent = [\"conf/staging\"]\n[case.expect.files]\n\"conf/default\" = \"a\\n\"",
        )))
        .unwrap();
        assert_eq!(suite.cases.len(), 1);
        let expect = &suite.cases[0].expect;
        assert_eq!(expect.files.get("conf/default"), Some(&"a\n".to_string()));
        assert_eq!(expect.files_absent, vec!["conf/staging".to_string()]);
    }

    #[test]
    fn empty_files_key_is_rejected() {
        let err = parse(&suite(
            &VALID_CASE.replace("exit_code = 0", "[case.expect.files]\n\"\" = \"a\""),
        ))
        .unwrap_err();
        assert!(err.to_string().contains("files keys"), "{err:#}");
    }

    #[test]
    fn gap_on_expected_pass_is_accepted() {
        let suite = parse(&suite(&VALID_CASE.replace(
            "expected = \"pass\"",
            "expected = \"pass\"\ngap = \"PAR01\"",
        )))
        .unwrap();
        assert_eq!(suite.cases[0].gap.as_deref(), Some("PAR01"));
    }

    #[test]
    fn empty_gap_on_expected_pass_is_rejected() {
        let err = parse(&suite(
            &VALID_CASE.replace("expected = \"pass\"", "expected = \"pass\"\ngap = \"\""),
        ))
        .unwrap_err();
        assert!(err.to_string().contains("must be non-empty"), "{err:#}");
    }

    #[test]
    fn reason_on_expected_pass_is_rejected() {
        let err = parse(&suite(&VALID_CASE.replace(
            "expected = \"pass\"",
            "expected = \"pass\"\ngap = \"PAR01\"\nreason = \"still tracked\"",
        )))
        .unwrap_err();
        assert!(
            err.to_string().contains("does not belong on an expected"),
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
        assert!(suite.invariants.commands[0].equal_across_modes);
    }

    #[test]
    fn contract_either_parses() {
        let suite = parse(&suite(&format!(
            "{VALID_CASE}\n[invariants]\n[[invariants.command]]\nargv = [\"build\"]\ncontract = \"either\"\n"
        )))
        .unwrap();
        assert_eq!(
            suite.invariants.commands[0].contract,
            InvariantContract::Either
        );
    }

    #[test]
    fn equal_across_modes_false_parses() {
        let suite = parse(&suite(&format!(
            "{VALID_CASE}\n[invariants]\n[[invariants.command]]\nargv = [\"config\", \"list\"]\ncontract = \"rendered\"\nequal_across_modes = false\n"
        )))
        .unwrap();
        assert!(!suite.invariants.commands[0].equal_across_modes);
    }

    fn load_with_manifest_gap(case_gap: &str, manifest_gaps: &str) -> anyhow::Result<Archetype> {
        let dir = tempfile::tempdir().unwrap();
        let archetype_dir = dir.path().join("fake");
        std::fs::create_dir_all(&archetype_dir).unwrap();
        std::fs::write(archetype_dir.join("spec.md"), "# fake\n").unwrap();
        std::fs::write(
            archetype_dir.join("acceptance.toml"),
            suite(&VALID_CASE.replace(
                "expected = \"pass\"",
                &format!("expected = \"pass\"\ngap = \"{case_gap}\""),
            )),
        )
        .unwrap();
        std::fs::write(
            archetype_dir.join("manifest.toml"),
            format!(
                "interactions = []\n\n\
                 [archetype]\n\
                 name = \"fake\"\n\
                 survey = \"C1\"\n\
                 summary = \"one line\"\n\
                 status = \"partially-past-capability\"\n\n\
                 [features]\n\
                 used = []\n\n\
                 {manifest_gaps}"
            ),
        )
        .unwrap();
        Archetype::load(dir.path(), "fake")
    }

    #[test]
    fn a_case_naming_a_known_manifest_gap_loads() {
        let archetype =
            load_with_manifest_gap("PAR01", "[gaps]\nPAR01 = \"still tracked\"\n").unwrap();
        assert_eq!(archetype.suite.cases[0].gap.as_deref(), Some("PAR01"));
    }

    #[test]
    fn a_case_naming_a_gap_missing_from_the_manifest_is_rejected() {
        let err =
            load_with_manifest_gap("PAR99", "[gaps]\nPAR01 = \"still tracked\"\n").unwrap_err();
        assert!(
            err.to_string().contains("not a manifest [gaps] entry"),
            "{err:#}"
        );
    }

    #[test]
    fn a_case_naming_a_gap_with_no_manifest_gaps_table_is_rejected() {
        let err = load_with_manifest_gap("PAR01", "").unwrap_err();
        assert!(
            err.to_string().contains("not a manifest [gaps] entry"),
            "{err:#}"
        );
    }
}
