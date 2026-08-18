//! Regression coverage for the PATH-directory admission mechanism the agent
//! phase's sandbox policy relies on (`sandbox::system_read_roots`):
//! directories prepended to PATH before a run become explicitly admitted
//! read roots on both the macOS Seatbelt and Linux Landlock backends, so
//! fixture bytes staged there (rather than relying on macOS's allow-default
//! fallback for anything not explicitly denied) are readable under the
//! enforced sandbox on both platforms. This is the mechanism
//! `walking_skeleton.rs` uses to stage its canned solution and agent script
//! outside the run workspace, whose name isn't known until `run()` claims
//! it, so nothing can be pre-staged into it directly.
//!
//! Runs alone in its own test binary: prepending to PATH is process-wide
//! state (see `common::install_fake_cargo`'s doc comment).

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

    // Prepending `tools_dir` to PATH is exactly what admits it: see
    // `sandbox::system_read_roots`'s PATH-directory handling.
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
