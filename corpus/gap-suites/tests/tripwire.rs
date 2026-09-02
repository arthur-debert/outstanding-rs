//! The closed-gap tripwire: the ledger test pins `gaps.toml` to the suites'
//! `expect_gap` call sites, and the simulation tests exercise the loud path
//! itself. `corpus/gap-suites/README.md` explains both.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// Serializes the simulation tests' `set_var` calls; each also uses its own variable name.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// This file calls the wrapper too, so the ledger sweep skips `tripwire.rs` by name.
const NEEDLE: &str = "expect_gap(";

fn suite_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

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

    // A new gap suite must register its gate before it can carry tripwires.
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

// A real black-box assertion against a binary that already has the behavior: a
// silently closed gap, manufactured on purpose.
#[cfg(unix)]
#[test]
fn a_gap_case_that_passes_fails_loudly() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("simulated");
    fs::write(&binary, "#!/bin/sh\necho gap-behavior-present\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("CORPUS_TRIPWIRE_SIM_PASS_BIN", &binary);

    let outcome = std::panic::catch_unwind(|| {
        corpus_gap_suites::expect_gap(
            "tripwire/simulation",
            "CORPUS_TRIPWIRE_SIM_PASS_BIN",
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

#[cfg(unix)]
#[test]
fn open_gaps_and_missing_binaries_stay_expected_fail() {
    use std::os::unix::fs::PermissionsExt;

    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // This name is set nowhere: a remove_var would race the other simulation test.
    corpus_gap_suites::expect_gap(
        "tripwire/simulation",
        "CORPUS_TRIPWIRE_SIM_NEVER_SET_BIN",
        "no binary produced",
        |_| panic!("must short-circuit before running the assertion"),
    );

    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("simulated");
    fs::write(&binary, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("CORPUS_TRIPWIRE_SIM_OPEN_BIN", &binary);
    corpus_gap_suites::expect_gap(
        "tripwire/simulation",
        "CORPUS_TRIPWIRE_SIM_OPEN_BIN",
        "capability still missing",
        |_| Err("gap open".into()),
    );
}
