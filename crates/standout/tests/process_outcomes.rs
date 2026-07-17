use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

fn fixture_binary() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            let status = Command::new(env!("CARGO"))
                .current_dir(&workspace)
                .args([
                    "build",
                    "--quiet",
                    "-p",
                    "standout",
                    "--example",
                    "outcome_fixture",
                ])
                .status()
                .unwrap();
            assert!(status.success());

            let target_dir = std::env::var_os("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .map(|path| {
                    if path.is_absolute() {
                        path
                    } else {
                        workspace.join(path)
                    }
                })
                .unwrap_or_else(|| workspace.join("target"));
            target_dir.join(format!(
                "debug/examples/outcome_fixture{}",
                std::env::consts::EXE_SUFFIX
            ))
        })
        .clone()
}

fn run(binary: &PathBuf, args: &[&str]) -> Output {
    Command::new(binary).args(args).output().unwrap()
}

#[test]
fn real_process_status_and_stream_matrix() {
    let binary = fixture_binary();

    let help = run(&binary, &["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));
    assert!(help.stderr.is_empty());

    let version = run(&binary, &["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&version.stdout).contains("1.2.3"));
    assert!(version.stderr.is_empty());

    let usage = run(&binary, &["--unknown"]);
    assert_eq!(usage.status.code(), Some(2));
    assert!(usage.stdout.is_empty());
    assert!(String::from_utf8_lossy(&usage.stderr).contains("unexpected argument"));

    let success = run(&binary, &["ok"]);
    assert_eq!(success.status.code(), Some(0));
    assert_eq!(success.stdout, b"ok\n");
    assert!(success.stderr.is_empty());

    let handler = run(&binary, &["fail"]);
    assert_eq!(handler.status.code(), Some(1));
    assert!(handler.stdout.is_empty());
    assert!(String::from_utf8_lossy(&handler.stderr).contains("fixture handler failed"));

    let external = run(&binary, &["external"]);
    assert_eq!(external.status.code(), Some(128));
    assert!(external.stdout.is_empty());
    assert_eq!(external.stderr, b"fatal: external fixture failed");

    let external_pre = run(&binary, &["external-pre"]);
    assert_eq!(external_pre.status.code(), Some(128));
    assert!(external_pre.stdout.is_empty());
    assert_eq!(external_pre.stderr, b"fatal: pre-dispatch fixture failed");

    let silent = run(&binary, &["silent"]);
    assert_eq!(silent.status.code(), Some(0));
    assert!(silent.stdout.is_empty());
    assert!(silent.stderr.is_empty());

    let binary_output = run(&binary, &["binary"]);
    assert_eq!(binary_output.status.code(), Some(0));
    assert_eq!(binary_output.stdout, [0, 1, 2]);
    assert!(binary_output.stderr.is_empty());

    let no_match = run(&binary, &[]);
    assert_eq!(no_match.status.code(), Some(0));
    assert!(no_match.stdout.is_empty());
    assert!(no_match.stderr.is_empty());

    let warning_success = run(&binary, &["warn-ok"]);
    assert_eq!(warning_success.status.code(), Some(0));
    assert_eq!(warning_success.stdout, b"ok\n");
    assert!(String::from_utf8_lossy(&warning_success.stderr).contains("fixture warning"));

    let warning_failure = run(&binary, &["warn-fail"]);
    assert_eq!(warning_failure.status.code(), Some(1));
    assert!(warning_failure.stdout.is_empty());
    let warning_stderr = String::from_utf8_lossy(&warning_failure.stderr);
    assert!(warning_stderr.contains("fixture handler failed"));
    assert!(warning_stderr.contains("fixture warning"));

    let tempdir = tempfile::tempdir().unwrap();
    let output_file = tempdir.path().join("out.txt");
    let file_success = run(
        &binary,
        &["--output-file-path", output_file.to_str().unwrap(), "ok"],
    );
    assert_eq!(file_success.status.code(), Some(0));
    assert!(file_success.stdout.is_empty());
    assert!(file_success.stderr.is_empty());
    assert_eq!(std::fs::read_to_string(output_file).unwrap(), "ok");

    let file_failure = run(
        &binary,
        &["--output-file-path", tempdir.path().to_str().unwrap(), "ok"],
    );
    assert_eq!(file_failure.status.code(), Some(1));
    assert!(file_failure.stdout.is_empty());
    assert!(String::from_utf8_lossy(&file_failure.stderr).contains("Error writing output"));
}

#[test]
fn real_process_reports_broken_text_and_binary_stdout() {
    let binary = fixture_binary();
    for command in ["huge", "binary-huge"] {
        let mut child = Command::new(&binary)
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(1), "command: {command}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Error writing"),
            "command: {command}, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
