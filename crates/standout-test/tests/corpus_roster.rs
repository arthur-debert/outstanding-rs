//! Structural validation of the downstream-corpus archetype roster.
//!
//! The roster under `corpus/archetypes/` (see `corpus/README.md` for the
//! formats) is declarative data consumed by the corpus runner, so nothing
//! compiles it and a typo would otherwise surface only mid-pilot-run. This
//! suite is the compile step: every archetype must carry its three files,
//! the acceptance suites and manifests must parse and use only the documented
//! vocabulary, cross-references must resolve, and — the corpus's founding
//! rule — no implementation may live beside the specs (acceptance is written
//! spec-first; blind agents implement elsewhere).
//!
//! It deliberately does NOT run any acceptance case: that is the WS01
//! runner's job, against a produced binary.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use toml::Value;

fn archetypes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/archetypes")
}

/// Every archetype directory, sorted for stable failure output.
fn archetype_dirs() -> Vec<PathBuf> {
    let root = archetypes_dir();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("corpus/archetypes must exist: {e}"))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "corpus/archetypes has no archetypes");
    dirs
}

fn dir_name(dir: &Path) -> &str {
    dir.file_name().unwrap().to_str().unwrap()
}

fn parse_toml(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
    // parse::<Value>() would parse a single TOML *value*; a file is a table.
    let table = text
        .parse::<toml::Table>()
        .unwrap_or_else(|e| panic!("{} must parse as TOML: {e}", path.display()));
    Value::Table(table)
}

fn str_field<'a>(value: &'a Value, key: &str, context: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{context}: missing string field `{key}`"))
}

/// The acceptance case names of one archetype, validating each case as it goes.
fn case_names(dir: &Path) -> HashSet<String> {
    let name = dir_name(dir);
    let doc = parse_toml(&dir.join("acceptance.toml"));
    assert_eq!(
        doc.get("schema").and_then(Value::as_integer),
        Some(1),
        "{name}: acceptance.toml must declare `schema = 1`"
    );
    assert_eq!(
        doc.get("archetype").and_then(Value::as_str),
        Some(name),
        "{name}: acceptance.toml `archetype` must match the directory name"
    );

    let cases = doc
        .get("case")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{name}: acceptance.toml has no [[case]] entries"));
    assert!(!cases.is_empty(), "{name}: empty acceptance suite");

    let mut names = HashSet::new();
    for case in cases {
        let case_name = str_field(case, "name", &format!("{name}: case"));
        let ctx = format!("{name}/{case_name}");
        assert!(
            names.insert(case_name.to_string()),
            "{ctx}: duplicate case name"
        );
        assert!(
            case_name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{ctx}: case names are kebab-case"
        );

        validate_expected_marker(case, &ctx);
        validate_run(case, &ctx);
        validate_expect(case, &ctx);
    }
    names
}

/// `expected` is `"pass"`, or `"fail"` carrying the gap that owns the failure.
fn validate_expected_marker(case: &Value, ctx: &str) {
    match str_field(case, "expected", ctx) {
        "pass" => {}
        "fail" => {
            assert!(
                !str_field(case, "gap", ctx).is_empty(),
                "{ctx}: expected-fail case must name its `gap`"
            );
            assert!(
                !str_field(case, "reason", ctx).is_empty(),
                "{ctx}: expected-fail case must carry a `reason`"
            );
        }
        other => panic!("{ctx}: `expected` must be \"pass\" or \"fail\", got {other:?}"),
    }
}

fn validate_run(case: &Value, ctx: &str) {
    let run = case
        .get("run")
        .and_then(Value::as_table)
        .unwrap_or_else(|| panic!("{ctx}: missing [case.run]"));

    let known = [
        "argv",
        "env",
        "tty",
        "stdin",
        "cwd",
        "timeout_seconds",
        "files",
    ];
    for key in run.keys() {
        assert!(
            known.contains(&key.as_str()),
            "{ctx}: unknown run key `{key}`"
        );
    }

    let argv = run
        .get("argv")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{ctx}: run.argv must be an array"));
    assert!(
        argv.iter().all(|a| a.as_str().is_some()),
        "{ctx}: run.argv must be strings"
    );

    let timeout = run
        .get("timeout_seconds")
        .and_then(Value::as_integer)
        .unwrap_or_else(|| {
            panic!("{ctx}: run.timeout_seconds is mandatory (it is the never-hang bound)")
        });
    assert!(timeout > 0, "{ctx}: timeout_seconds must be positive");

    if let Some(tty) = run.get("tty") {
        let streams: Vec<&str> = tty
            .as_array()
            .unwrap_or_else(|| panic!("{ctx}: run.tty must be an array"))
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            streams
                .iter()
                .all(|s| ["stdin", "stdout", "stderr"].contains(s)),
            "{ctx}: run.tty entries must be stdin/stdout/stderr"
        );
    }
}

fn validate_expect(case: &Value, ctx: &str) {
    let expect = case
        .get("expect")
        .and_then(Value::as_table)
        .unwrap_or_else(|| panic!("{ctx}: missing [case.expect]"));

    let known = [
        "exit_code",
        "stdout",
        "stderr",
        "stdout_json",
        "stdout_contains",
        "stderr_contains",
        "stdout_not_contains",
        "stderr_not_contains",
    ];
    for key in expect.keys() {
        assert!(
            known.contains(&key.as_str()),
            "{ctx}: unknown assertion `{key}` (vocabulary is fixed in corpus/README.md)"
        );
    }
    assert!(
        !expect.is_empty(),
        "{ctx}: at least one assertion is required"
    );

    // Exact-stdout and semantic-JSON assertions on the same stream contradict
    // each other's reason to exist; a case picks one.
    assert!(
        !(expect.contains_key("stdout") && expect.contains_key("stdout_json")),
        "{ctx}: use `stdout` (exact) or `stdout_json` (semantic), not both"
    );
    if let Some(json) = expect.get("stdout_json") {
        let text = json
            .as_str()
            .unwrap_or_else(|| panic!("{ctx}: stdout_json must be a string of JSON"));
        serde_json::from_str::<serde_json::Value>(text)
            .unwrap_or_else(|e| panic!("{ctx}: stdout_json is not valid JSON: {e}"));
    }
}

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
        case_names(&dir);
    }
}

#[test]
fn manifests_are_wellformed_and_case_references_resolve() {
    for dir in archetype_dirs() {
        let name = dir_name(&dir);
        let doc = parse_toml(&dir.join("manifest.toml"));
        let names = case_names(&dir);

        let archetype = doc
            .get("archetype")
            .unwrap_or_else(|| panic!("{name}: manifest missing [archetype]"));
        assert_eq!(
            archetype.get("name").and_then(Value::as_str),
            Some(name),
            "{name}: manifest [archetype].name must match the directory"
        );
        for key in ["survey", "summary", "status"] {
            str_field(archetype, key, &format!("{name}: [archetype]"));
        }

        let used = doc
            .get("features")
            .and_then(|f| f.get("used"))
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{name}: manifest missing [features] used"));
        assert!(!used.is_empty(), "{name}: manifest names no features");

        let interactions = doc
            .get("interactions")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{name}: manifest has no [[interactions]]"));
        assert!(
            !interactions.is_empty(),
            "{name}: the manifest must name stressed interactions, not just features"
        );
        for interaction in interactions {
            let id = str_field(interaction, "id", &format!("{name}: interaction"));
            let ctx = format!("{name}/interaction {id}");
            assert!(
                !str_field(interaction, "description", &ctx).is_empty(),
                "{ctx}: empty description"
            );
            let stresses = interaction
                .get("stresses")
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("{ctx}: missing `stresses` list"));
            assert!(
                stresses.len() >= 2,
                "{ctx}: an interaction stresses at least two features"
            );
            let cases = interaction
                .get("cases")
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("{ctx}: missing `cases` list"));
            for case in cases {
                let case = case.as_str().unwrap();
                assert!(
                    names.contains(case),
                    "{ctx}: references unknown acceptance case `{case}`"
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
    let names = case_names(&archetypes_dir().join("formlike"));
    assert!(
        names.contains("missing-required-answer-under-closed-stdin-fails-fast"),
        "formlike must keep its bounded-time non-interactive failure case"
    );
}
