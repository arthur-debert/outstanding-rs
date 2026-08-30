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

fn run_with_artifact_path(
    binary: &PathBuf,
    args: &[&str],
    artifact_path: &std::path::Path,
) -> Output {
    Command::new(binary)
        .args(args)
        .env("STANDOUT_FIXTURE_ARTIFACT_PATH", artifact_path)
        .output()
        .unwrap()
}

#[test]
fn real_process_routes_artifact_bytes_and_reports_to_separate_channels() {
    let binary = fixture_binary();
    let tempdir = tempfile::tempdir().unwrap();

    let to_file = tempdir.path().join("artifact.bin");
    let file_run = run_with_artifact_path(&binary, &["artifact"], &to_file);
    assert_eq!(file_run.status.code(), Some(0));
    assert_eq!(std::fs::read(&to_file).unwrap(), [0, 1, 2]);
    assert_eq!(
        String::from_utf8_lossy(&file_run.stdout),
        format!("wrote 3 entries to {}\n", to_file.display())
    );
    assert!(file_run.stderr.is_empty());

    let override_path = tempdir.path().join("override.bin");
    let override_run = run_with_artifact_path(
        &binary,
        &[
            "--output-file-path",
            override_path.to_str().unwrap(),
            "artifact",
        ],
        &to_file,
    );
    assert_eq!(override_run.status.code(), Some(0));
    assert_eq!(std::fs::read(&override_path).unwrap(), [0, 1, 2]);
    assert_eq!(
        String::from_utf8_lossy(&override_run.stdout),
        format!("wrote 3 entries to {}\n", override_path.display())
    );

    let stdout_run = run_with_artifact_path(&binary, &["artifact-stdout"], &to_file);
    assert_eq!(stdout_run.status.code(), Some(0));
    assert_eq!(stdout_run.stdout, [0, 1, 2]);
    assert_eq!(
        String::from_utf8_lossy(&stdout_run.stderr),
        "wrote 3 entries to -\n"
    );

    let no_destination = run_with_artifact_path(&binary, &["artifact-no-destination"], &to_file);
    assert_eq!(no_destination.status.code(), Some(1));
    assert!(no_destination.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&no_destination.stderr);
    assert!(stderr.contains("no destination selected"));
    assert!(!stderr.contains("wrote 3 entries"));

    let unwritable = tempdir.path().join("missing").join("artifact.bin");
    let write_failure = run_with_artifact_path(&binary, &["artifact"], &unwritable);
    assert_eq!(write_failure.status.code(), Some(1));
    assert!(write_failure.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&write_failure.stderr);
    assert!(stderr.contains("Error writing artifact"));
    assert!(!stderr.contains("wrote 3 entries"));
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
fn real_process_accepts_broken_text_stdout_but_reports_binary_stdout() {
    let binary = fixture_binary();
    let mut text_child = Command::new(&binary)
        .arg("huge")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(text_child.stdout.take());
    let text_output = text_child.wait_with_output().unwrap();
    assert_eq!(text_output.status.code(), Some(0));
    assert!(text_output.stderr.is_empty());

    let mut binary_child = Command::new(&binary)
        .arg("binary-huge")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(binary_child.stdout.take());
    let binary_output = binary_child.wait_with_output().unwrap();
    assert_eq!(binary_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&binary_output.stderr).contains("Error writing"),
        "stderr: {}",
        String::from_utf8_lossy(&binary_output.stderr)
    );
}
