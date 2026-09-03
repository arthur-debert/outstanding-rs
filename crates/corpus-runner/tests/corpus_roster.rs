// Structural validation of the archetype roster (`corpus/archetypes/`,
// schema in `corpus/README.md`). The manifest schema itself
// (`corpus_runner::manifest`) is the one owner of its shape; this file only
// cross-references a manifest against its acceptance suite.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use corpus_runner::archetype::CaseSuite;
use corpus_runner::manifest::{Manifest, Status};

fn archetypes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/archetypes")
}

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
        let manifest = Manifest::load(&archetypes_dir(), name)
            .unwrap_or_else(|e| panic!("{name}: manifest.toml is invalid: {e:#}"));
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

        let gaps = &manifest.gaps;
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
        "show-registered-name-term-color-on",
        "show-mistyped-name",
        "show-missing-name",
        "early-registered-before-templates",
        "late-registered-after-templates",
        "root-h-text-color-off",
        "root-help-flag-text-color-off",
        "root-help-word-text-color-off",
        "root-h-term-color-on",
        "root-help-flag-term-color-on",
        "root-help-word-term-color-on",
        "leaf-h-text-color-off",
        "leaf-help-flag-text-color-off",
        "leaf-help-word-text-color-off",
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

#[test]
fn formlike_pins_the_bounded_noninteractive_failure() {
    let suite = load_acceptance(&archetypes_dir().join("formlike"));
    assert!(
        case_names(&suite).contains("missing-required-answer-under-closed-stdin-fails-fast"),
        "formlike must keep its bounded-time non-interactive failure case"
    );
}
