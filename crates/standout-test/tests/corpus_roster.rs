//! Structural validation of the downstream-corpus archetype roster.
//!
//! The roster under `corpus/archetypes/` (see `corpus/README.md` for the
//! formats) is declarative data consumed by the corpus runner, so nothing
//! compiles it and a typo would otherwise surface only mid-pilot-run. This
//! suite is the compile step: every roster archetype must carry its three
//! files, the acceptance suites and manifests must deserialize into the
//! documented schema exactly (typed structs, unknown keys rejected, every
//! field's shape checked), cross-references must resolve — manifest `cases`
//! to acceptance case names, expected-fail `gap`s to the manifest's `[gaps]`
//! table — and, the corpus's founding rule, no implementation may live
//! beside the specs (acceptance is written spec-first; blind agents
//! implement elsewhere). One directory is exempt from roster membership:
//! `smoke`, the harness's own walking-skeleton archetype in the runner's
//! check schema (see `corpus/README.md`, Layout); the no-implementation
//! rule still covers it.
//!
//! It deliberately does NOT run any acceptance case: that is the WS01
//! runner's job, against a produced binary.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

// --- the documented schema, as types ----------------------------------------
//
// One struct per table in `corpus/README.md`. `deny_unknown_fields` makes the
// vocabulary closed: a misspelled key fails here, not mid-pilot-run. Fields
// whose values carry extra rules (positive timeout, resolvable references,
// pass/fail coupling) get semantic checks on top, below.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptanceDoc {
    schema: i64,
    archetype: String,
    #[serde(rename = "case")]
    cases: Vec<AcceptanceCase>,
    // Optional: the read-only commands the runner's ROB01 invariant matrix
    // sweeps across output modes (WS04's execution addition to the schema).
    #[allow(dead_code)] // shape-checked by the typed parse; the runner consumes it
    invariants: Option<InvariantsDoc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvariantsDoc {
    #[allow(dead_code)]
    modes: Vec<InvariantMode>,
    #[allow(dead_code)]
    colors: Vec<ColorState>,
    #[serde(rename = "theme")]
    #[allow(dead_code)]
    themes: Vec<InvariantTheme>,
    #[serde(rename = "command")]
    #[allow(dead_code)]
    commands: Vec<InvariantCommand>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum InvariantMode {
    Text,
    Term,
    Json,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ColorState {
    Off,
    On,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvariantTheme {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    env: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvariantCommand {
    #[allow(dead_code)] // an empty word-list is the naked invocation
    argv: Vec<String>,
    #[allow(dead_code)]
    contract: InvariantContract,
    #[allow(dead_code)]
    modes: Option<Vec<InvariantMode>>,
    #[allow(dead_code)]
    colors: Option<Vec<ColorState>>,
    #[allow(dead_code)]
    themes: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum InvariantContract {
    Rendered,
    OpaqueBytes,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptanceCase {
    name: String,
    #[allow(dead_code)] // grouping is for humans and runner reports, not checks
    group: Option<String>,
    stresses: String,
    expected: Expected,
    gap: Option<String>,
    reason: Option<String>,
    run: Run,
    expect: Expect,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Expected {
    Pass,
    Fail,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Run {
    // Required by the schema; may be empty (a naked default-command line).
    #[allow(dead_code)]
    argv: Vec<String>,
    #[allow(dead_code)]
    env: Option<BTreeMap<String, String>>,
    #[allow(dead_code)]
    tty: Option<Vec<TtyStream>>,
    #[allow(dead_code)]
    stdin: Option<String>,
    #[allow(dead_code)]
    cwd: Option<String>,
    // Mandatory: it is the never-hang bound.
    timeout_seconds: i64,
    #[allow(dead_code)]
    files: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum TtyStream {
    Stdin,
    Stdout,
    Stderr,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    exit_code: Option<i64>,
    stdout: Option<String>,
    stderr: Option<String>,
    stdout_json: Option<String>,
    stdout_contains: Option<Vec<String>>,
    stderr_contains: Option<Vec<String>>,
    stdout_not_contains: Option<Vec<String>>,
    stderr_not_contains: Option<Vec<String>>,
    stdout_lines_end_with_once: Option<Vec<String>>,
}

impl Expect {
    fn assertion_count(&self) -> usize {
        [
            self.exit_code.is_some(),
            self.stdout.is_some(),
            self.stderr.is_some(),
            self.stdout_json.is_some(),
            self.stdout_contains.is_some(),
            self.stderr_contains.is_some(),
            self.stdout_not_contains.is_some(),
            self.stderr_not_contains.is_some(),
            self.stdout_lines_end_with_once.is_some(),
        ]
        .iter()
        .filter(|present| **present)
        .count()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDoc {
    archetype: ManifestArchetype,
    features: Features,
    interactions: Vec<Interaction>,
    gaps: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestArchetype {
    name: String,
    #[allow(dead_code)]
    survey: String,
    #[allow(dead_code)]
    summary: String,
    status: Status,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
enum Status {
    #[serde(rename = "in-capability")]
    InCapability,
    #[serde(rename = "partially-past-capability")]
    PartiallyPastCapability,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Features {
    used: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Interaction {
    id: String,
    stresses: Vec<String>,
    description: String,
    cases: Vec<String>,
}

// --- loading ----------------------------------------------------------------

fn archetypes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/archetypes")
}

/// Every roster archetype directory, sorted for stable failure output.
///
/// `smoke` is exempt by name: it is the harness's own walking-skeleton
/// archetype (spec: "the harness itself gets a smoke archetype"), owned by
/// the corpus runner and written in the runner's check schema — not a
/// roster member (`corpus/README.md`, Layout). Only roster membership is
/// waived: `no_implementation_lives_in_the_roster` walks the whole
/// directory, `smoke` included.
fn archetype_dirs() -> Vec<PathBuf> {
    let root = archetypes_dir();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("corpus/archetypes must exist: {e}"))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.is_dir() && dir_name(p) != "smoke")
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "corpus/archetypes has no archetypes");
    dirs
}

fn dir_name(dir: &Path) -> &str {
    dir.file_name().unwrap().to_str().unwrap()
}

/// Typed parse: the file must match the documented schema exactly. Serde's
/// error carries the offending key/type and its TOML position.
fn parse<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| {
        panic!(
            "{} does not match the schema in corpus/README.md: {e}",
            path.display()
        )
    })
}

/// One archetype's acceptance suite: typed parse plus the semantic rules the
/// types cannot carry. Returns the doc for cross-file checks.
fn load_acceptance(dir: &Path) -> AcceptanceDoc {
    let name = dir_name(dir);
    let doc: AcceptanceDoc = parse(&dir.join("acceptance.toml"));
    assert_eq!(
        doc.schema, 1,
        "{name}: acceptance.toml must declare `schema = 1`"
    );
    assert_eq!(
        doc.archetype, name,
        "{name}: acceptance.toml `archetype` must match the directory name"
    );
    assert!(!doc.cases.is_empty(), "{name}: empty acceptance suite");

    let mut names = HashSet::new();
    for case in &doc.cases {
        let ctx = format!("{name}/{}", case.name);
        assert!(
            names.insert(case.name.clone()),
            "{ctx}: duplicate case name"
        );
        assert!(
            case.name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{ctx}: case names are kebab-case"
        );
        assert!(
            !case.stresses.is_empty(),
            "{ctx}: `stresses` must name the interaction under test"
        );
        validate_expected_marker(case, &ctx);
        validate_run(&case.run, &ctx);
        validate_expect(&case.expect, &ctx);
    }
    doc
}

fn case_names(doc: &AcceptanceDoc) -> HashSet<&str> {
    doc.cases.iter().map(|c| c.name.as_str()).collect()
}

/// `expected = "pass"` stands alone; `"fail"` carries the gap that owns the
/// failure and the reason it fails today (the gap resolves against the
/// manifest's `[gaps]` table in the cross-file check below).
fn validate_expected_marker(case: &AcceptanceCase, ctx: &str) {
    match case.expected {
        Expected::Pass => {
            assert!(
                case.gap.is_none() && case.reason.is_none(),
                "{ctx}: `gap`/`reason` belong to expected-fail cases only"
            );
        }
        Expected::Fail => {
            assert!(
                case.gap.as_deref().is_some_and(|g| !g.is_empty()),
                "{ctx}: expected-fail case must name its `gap`"
            );
            assert!(
                case.reason.as_deref().is_some_and(|r| !r.is_empty()),
                "{ctx}: expected-fail case must carry a `reason`"
            );
        }
    }
}

fn validate_run(run: &Run, ctx: &str) {
    assert!(
        run.timeout_seconds > 0,
        "{ctx}: timeout_seconds must be positive (it is the never-hang bound)"
    );
}

fn validate_expect(expect: &Expect, ctx: &str) {
    assert!(
        expect.assertion_count() > 0,
        "{ctx}: at least one assertion is required"
    );
    // Exact-stdout and semantic-JSON assertions on the same stream contradict
    // each other's reason to exist; a case picks one.
    assert!(
        !(expect.stdout.is_some() && expect.stdout_json.is_some()),
        "{ctx}: use `stdout` (exact) or `stdout_json` (semantic), not both"
    );
    if let Some(json) = &expect.stdout_json {
        serde_json::from_str::<serde_json::Value>(json)
            .unwrap_or_else(|e| panic!("{ctx}: stdout_json is not valid JSON: {e}"));
    }
}

// --- the tests --------------------------------------------------------------

#[test]
fn every_archetype_carries_spec_manifest_and_acceptance() {
    for dir in archetype_dirs() {
        for file in ["spec.md", "manifest.toml", "acceptance.toml"] {
            assert!(
                dir.join(file).is_file(),
                "{}: missing {file}",
                dir_name(&dir)
            );
        }
    }
}

#[test]
fn acceptance_suites_are_wellformed() {
    for dir in archetype_dirs() {
        load_acceptance(&dir);
    }
}

#[test]
fn manifests_are_wellformed_and_cross_references_resolve() {
    for dir in archetype_dirs() {
        let name = dir_name(&dir);
        let manifest: ManifestDoc = parse(&dir.join("manifest.toml"));
        let acceptance = load_acceptance(&dir);
        let names = case_names(&acceptance);

        assert_eq!(
            manifest.archetype.name, name,
            "{name}: manifest [archetype].name must match the directory"
        );
        assert!(
            !manifest.features.used.is_empty(),
            "{name}: manifest names no features"
        );
        assert!(
            !manifest.interactions.is_empty(),
            "{name}: the manifest must name stressed interactions, not just features"
        );
        for interaction in &manifest.interactions {
            let ctx = format!("{name}/interaction {}", interaction.id);
            assert!(
                !interaction.description.is_empty(),
                "{ctx}: empty description"
            );
            assert!(
                interaction.stresses.len() >= 2,
                "{ctx}: an interaction stresses at least two features"
            );
            for case in &interaction.cases {
                assert!(
                    names.contains(case.as_str()),
                    "{ctx}: references unknown acceptance case `{case}`"
                );
            }
        }

        // The gaps table exists exactly when the archetype is specced past
        // capability, and every expected-fail case's `gap` resolves into it —
        // a typo here would attribute runner results to a nonexistent epic.
        let gaps = manifest.gaps.unwrap_or_default();
        match manifest.archetype.status {
            Status::PartiallyPastCapability => {
                assert!(
                    !gaps.is_empty(),
                    "{name}: partially-past-capability requires a non-empty [gaps] table"
                );
            }
            Status::InCapability => {
                assert!(
                    gaps.is_empty(),
                    "{name}: [gaps] is only for partially-past-capability archetypes"
                );
            }
        }
        for case in &acceptance.cases {
            if let Some(gap) = &case.gap {
                assert!(
                    gaps.contains_key(gap),
                    "{name}/{}: gap `{gap}` is not in the manifest's [gaps] table",
                    case.name
                );
            }
        }
    }
}

/// Spec-first is only credible if it is checkable: the roster holds specs and
/// suites, never the archetype implementations blind agents will produce.
#[test]
fn no_implementation_lives_in_the_roster() {
    fn walk(dir: &Path, offenders: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, offenders);
            } else {
                let file = path.file_name().unwrap().to_str().unwrap();
                let implementation = file == "Cargo.toml"
                    || file == "Cargo.lock"
                    || Path::new(file).extension().is_some_and(|e| e == "rs");
                if implementation {
                    offenders.push(path);
                }
            }
        }
    }
    let mut offenders = Vec::new();
    walk(&archetypes_dir(), &mut offenders);
    assert!(
        offenders.is_empty(),
        "implementation files inside corpus/archetypes (acceptance is spec-first): {offenders:?}"
    );
}

#[test]
fn pilot_roster_is_complete() {
    let present: HashSet<String> = archetype_dirs()
        .iter()
        .map(|d| dir_name(d).to_string())
        .collect();
    for archetype in ["gitlike", "systemdlike", "formlike", "ghlike"] {
        assert!(
            present.contains(archetype),
            "pilot archetype `{archetype}` is missing from the roster"
        );
    }
}

/// The issue-#324 criterion called out by name: formlike must pin the
/// bounded-time non-interactive failure path.
#[test]
fn formlike_pins_the_bounded_noninteractive_failure() {
    let doc = load_acceptance(&archetypes_dir().join("formlike"));
    assert!(
        case_names(&doc).contains("missing-required-answer-under-closed-stdin-fails-fast"),
        "formlike must keep its bounded-time non-interactive failure case"
    );
}
