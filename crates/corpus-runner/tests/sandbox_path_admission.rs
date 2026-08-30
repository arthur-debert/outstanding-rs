// Regression coverage for PATH-directory admission (`sandbox::system_read_roots`):
// directories prepended to PATH become admitted read roots under the
// enforced sandbox, on both backends. Runs alone in its own test binary
// since prepending to PATH is process-wide state.

#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::time::Duration;

use corpus_runner::{session, workspace};

#[test]
fn sandboxed_agent_can_read_a_file_staged_in_a_path_admitted_directory() {
    let dir = tempfile::tempdir().unwrap();
    let tools_dir = dir.path().join("tools");
    fs::create_dir_all(&tools_dir).unwrap();
    let staged = tools_dir.join("solution.txt");
    fs::write(&staged, "canned solution\n").unwrap();

    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            tools_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );

    let isolation = workspace::Isolation::new(
        dir.path(),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    )
    .unwrap();

    let report = session::run_agent(
        dir.path(),
        &isolation,
        &format!("cat \"{}\"", staged.display()),
        &dir.path().join("t.jsonl"),
        Duration::from_secs(60),
    )
    .unwrap();

    assert_eq!(
        report.exit_code,
        Some(0),
        "sandboxed agent could not read {} staged under a PATH-admitted directory",
        staged.display()
    );
}
