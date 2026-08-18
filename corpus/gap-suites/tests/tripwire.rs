//! The closed-gap tripwire. Without a produced binary the gap suites
//! short-circuit as expected-fail, so their green in CI carries no signal by
//! itself; these tests are what keep that silence honest. The ledger test
//! pins the tripwire inventory (`gaps.toml`) to the suites' actual
//! `expect_gap` call sites, so a tripwire cannot be removed, added, or
//! declared closed without the ledger and the suite agreeing in the same
//! change — closing an epic without promoting its suite is a red test, as is
//! promoting without recording the closure. The simulation tests exercise
//! the loud path itself: a gap assertion that passes against a produced
//! binary must fail the test run, visibly, in exactly the way a silently
//! closed gap would.

use std::fs;
use std::path::{Path, PathBuf};

/// The textual shape of an armed tripwire in a suite source file. This file
/// calls the wrapper too, so the ledger sweep skips `tripwire.rs` by name.
const NEEDLE: &str = "expect_gap(";

fn suite_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Counts `expect_gap(` call sites in one suite source file.
fn armed_count(source: &str) -> usize {
    source.matches(NEEDLE).count()
}

#[test]
fn gap_ledger_matches_the_armed_suites() {
    let root = suite_root();
    let ledger: toml::Table = fs::read_to_string(root.join("gaps.toml"))
        .expect("gaps.toml must exist beside Cargo.toml")
        .parse()
        .expect("gaps.toml must parse as TOML");
    let gates = ledger["gate"].as_array().expect("gaps.toml lists [[gate]]");
    assert!(!gates.is_empty(), "the ledger cannot be empty");

    let mut listed = Vec::new();
    for gate in gates {
        let name = gate["name"].as_str().expect("gate.name is a string");
        let tests = gate["tests"].as_str().expect("gate.tests is a string");
        let status = gate["status"].as_str().expect("gate.status is a string");
        let armed = gate["armed"].as_integer().expect("gate.armed is a count") as usize;
        assert!(
            gate["env"].as_str().is_some() && gate["epic"].as_str().is_some(),
            "gate {name}: env and epic must name the binary variable and owning epic"
        );
        listed.push(tests.to_string());

        let source = fs::read_to_string(root.join(tests))
            .unwrap_or_else(|err| panic!("gate {name}: ledger names {tests}, unreadable: {err}"));
        let found = armed_count(&source);
        match status {
            "open" => {
                assert!(
                    found > 0,
                    "gate {name}: status is open but {tests} carries no expected-fail \
                     assertions — if the gap closed, promote the suite AND flip this \
                     gate to closed in gaps.toml"
                );
                assert_eq!(
                    found, armed,
                    "gate {name}: gaps.toml arms {armed} assertions but {tests} \
                     carries {found} — update the ledger in the same change that \
                     changed the suite"
                );
            }
            "closed" => {
                assert_eq!(
                    found, 0,
                    "gate {name}: status is closed but {tests} still wraps {found} \
                     assertions as expected-fail — promote them to plain requirements \
                     (see README) in the same change that closes the gate"
                );
                assert_eq!(armed, 0, "gate {name}: a closed gate arms nothing");
            }
            other => panic!("gate {name}: unknown status {other:?} (open|closed)"),
        }
    }

    // Every suite carrying tripwires is on the ledger: a new gap suite must
    // register its gate before it can ride the expected-fail machinery.
    for entry in fs::read_dir(root.join("tests")).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().is_some_and(|f| f == "tripwire.rs") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        if armed_count(&source) > 0 {
            let relative = format!("tests/{}", path.file_name().unwrap().to_string_lossy());
            assert!(
                listed.contains(&relative),
                "{relative} carries expected-fail assertions but has no [[gate]] \
                 entry in gaps.toml"
            );
        }
    }
}

/// The simulation the cleanup Spec asks for: point the machinery at a binary
/// that passes a gap case and show the visible failure. The "gap case" here
/// is a real black-box assertion (spawn the binary, check its stream) whose
/// behavior the simulated binary already has — exactly a silently closed
/// gap — and the expected-fail wrapper must turn that into a loud test
/// failure, not a quiet pass.
#[cfg(unix)]
#[test]
fn a_gap_case_that_passes_fails_loudly() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("simulated");
    fs::write(&binary, "#!/bin/sh\necho gap-behavior-present\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    // nextest runs each test in its own process, so this cannot race another
    // test's environment.
    std::env::set_var("CORPUS_TRIPWIRE_SIM_BIN", &binary);

    let outcome = std::panic::catch_unwind(|| {
        corpus_gap_suites::expect_gap(
            "tripwire/simulation",
            "CORPUS_TRIPWIRE_SIM_BIN",
            "simulated capability, closed on purpose",
            |binary| {
                let dir = tempfile::tempdir().unwrap();
                let out = corpus_gap_suites::run(binary, &[], dir.path())?;
                if out.stdout.contains("gap-behavior-present") && out.code == Some(0) {
                    Ok(())
                } else {
                    Err(format!("capability absent: {:?}", out.stdout))
                }
            },
        );
    });

    let panic = outcome.expect_err("a passing gap case must fail the test");
    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_else(|| "non-string panic".into());
    assert!(
        message.contains("UNEXPECTED PASS"),
        "the failure must name the unexpected pass: {message}"
    );
    assert!(
        message.contains("tripwire/simulation"),
        "the failure must carry its gate: {message}"
    );
}

/// The steady state stays quiet on both sides of the tripwire: no produced
/// binary short-circuits, and a binary the assertion still rejects is an
/// open gap — neither may fail the run.
#[cfg(unix)]
#[test]
fn open_gaps_and_missing_binaries_stay_expected_fail() {
    use std::os::unix::fs::PermissionsExt;

    std::env::remove_var("CORPUS_TRIPWIRE_SIM_BIN");
    corpus_gap_suites::expect_gap(
        "tripwire/simulation",
        "CORPUS_TRIPWIRE_SIM_BIN",
        "no binary produced",
        |_| panic!("must short-circuit before running the assertion"),
    );

    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("simulated");
    fs::write(&binary, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("CORPUS_TRIPWIRE_SIM_BIN", &binary);
    corpus_gap_suites::expect_gap(
        "tripwire/simulation",
        "CORPUS_TRIPWIRE_SIM_BIN",
        "capability still missing",
        |_| Err("gap open".into()),
    );
}
