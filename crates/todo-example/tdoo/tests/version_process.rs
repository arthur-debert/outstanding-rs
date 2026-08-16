//! Runs `tdoo --version` the way a user does — as a real process.
//!
//! The harness proves the builder setting reaches the parse; only the
//! compiled binary proves the answer arrives on stdout with status 0.

use std::process::Command;

#[test]
fn the_compiled_binary_prints_its_version_and_succeeds() {
    let store = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tdoo"))
        // The binary loads its store before parsing; keep that off the
        // developer's real todo file.
        .env("TODO_FILE", store.path().join("todos.json"))
        .arg("--version")
        .output()
        .unwrap();

    assert!(output.status.success(), "tdoo --version must exit 0");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("tdoo {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(
        output.stderr.is_empty(),
        "a version display writes nothing to stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
