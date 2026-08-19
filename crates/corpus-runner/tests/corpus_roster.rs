//! Structural validation of the downstream-corpus archetype roster.
//!
//! The roster under `corpus/archetypes/` (see `corpus/README.md` for the
//! formats) is declarative data consumed by the corpus runner, so nothing
//! compiles it and a typo would otherwise surface only mid-pilot-run. This
//! suite is the compile step: every roster archetype must carry its three
//! files, every acceptance suite must load through the runner's own parser
//! ([`CaseSuite::parse`] — the schema's single definition, so a suite cannot
//! pass this lint and fail the runner or vice versa), the manifests must
//! deserialize into the documented schema exactly (typed structs, unknown
//! keys rejected — the manifest types live here because the runner never
//! parses manifests), cross-references must resolve — manifest `cases` to
//! acceptance case names, expected-fail `gap`s to the manifest's `[gaps]`
//! table — and, the corpus's founding rule, no implementation may live
//! beside the specs (acceptance is written spec-first; blind agents
//! implement elsewhere). One directory is exempt from roster membership:
//! `smoke`, the harness's own manifest-less walking-skeleton archetype
//! (see `corpus/README.md`, Layout); the no-implementation rule still
//! covers it.
//!
//! It deliberately does NOT run any acceptance case: that is the runner's
//! job, against a produced binary.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use corpus_runner::archetype::CaseSuite;
use serde::Deserialize;

// --- the manifest schema, as types -------------------------------------------
//
// One struct per table in `corpus/README.md`. `deny_unknown_fields` makes the
// vocabulary closed: a misspelled key fails here, not mid-pilot-run. The
// acceptance case schema is NOT redefined here — it is the runner's
// `CaseSuite`, parsed and validated by the runner's own rules.

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
/// the corpus runner and carrying no `manifest.toml` — not a roster member
/// (`corpus/README.md`, Layout), though its acceptance suite speaks the
/// same case schema. Only roster membership is waived:
/// `no_implementation_lives_in_the_roster` walks the whole directory,
/// `smoke` included.
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

/// One archetype's acceptance suite through the runner's parser — shape and
/// semantic rules alike come from `CaseSuite::parse`, never a second,
/// test-local reimplementation. Returns the suite for cross-file checks.
fn load_acceptance(dir: &Path) -> CaseSuite {
    let path = dir.join("acceptance.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
    CaseSuite::parse(&text, dir_name(dir), &path)
        .unwrap_or_else(|e| panic!("{} is not a valid acceptance suite: {e:#}", path.display()))
}

fn case_names(suite: &CaseSuite) -> HashSet<&str> {
    suite.cases.iter().map(|c| c.name.as_str()).collect()
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

/// Issue #365: the method-coverage archetype must keep the three known-edge
/// families the ROB03 pilot did not independently rediscover. An agent
/// cannot pass this suite without requesting a missing template name,
/// rendering through both registration orders, and combining an incomplete
/// app theme with framework help at root and at a deep leaf.
#[test]
fn validity_pins_the_known_edge_families() {
    let present: HashSet<String> = archetype_dirs()
        .iter()
        .map(|d| dir_name(d).to_string())
        .collect();
    assert!(
        present.contains("validity"),
        "method-coverage archetype `validity` is missing from the roster"
    );

    let suite = load_acceptance(&archetypes_dir().join("validity"));
    let names = case_names(&suite);
    for case in [
        "show-registered-name",
        "show-mistyped-name",
        "show-missing-name",
        "late-registered-before-templates",
        "early-registered-after-templates",
        "root-h-term-color-on",
        "root-help-flag-term-color-on",
        "root-help-word-term-color-on",
        "leaf-h-term-color-on",
        "leaf-help-flag-term-color-on",
        "leaf-help-word-term-color-on",
    ] {
        assert!(
            names.contains(case),
            "validity must keep known-edge case `{case}`"
        );
    }
}

/// The issue-#324 criterion called out by name: formlike must pin the
/// bounded-time non-interactive failure path.
#[test]
fn formlike_pins_the_bounded_noninteractive_failure() {
    let suite = load_acceptance(&archetypes_dir().join("formlike"));
    assert!(
        case_names(&suite).contains("missing-required-answer-under-closed-stdin-fails-fast"),
        "formlike must keep its bounded-time non-interactive failure case"
    );
}
