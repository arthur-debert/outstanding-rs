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
    assert!(String::from_utf8_lossy(&help.stdout).contains("USAGE"));
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

    let signal = run(&binary, &["signal"]);
    assert_eq!(signal.status.code(), Some(2));
    assert_eq!(signal.stdout, b"changes\n");
    assert!(signal.stderr.is_empty());

    let signal_json = run(&binary, &["signal", "--output", "json"]);
    assert_eq!(signal_json.status.code(), Some(2));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&signal_json.stdout).unwrap(),
        serde_json::json!({ "message": "changes" })
    );
    assert!(signal_json.stderr.is_empty());

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
fn real_process_structured_failures_are_stdout_documents() {
    use standout::cli::{parse_diagnostic, DiagnosticKind, HookPhase, RunErrorKind};
    use standout::OutputMode;

    let binary = fixture_binary();
    let cases: [(&[&str], RunErrorKind, i32, &str); 6] = [
        (
            &["--output", "json", "--unknown"],
            RunErrorKind::ClapUsage,
            2,
            "unexpected argument '--unknown' found",
        ),
        (
            &["--unknown", "--output", "json"],
            RunErrorKind::ClapUsage,
            2,
            "unexpected argument '--unknown' found",
        ),
        (
            &["fail", "--output", "json"],
            RunErrorKind::Handler,
            1,
            "fixture handler failed",
        ),
        (
            &["hook-fail", "--output", "json"],
            RunErrorKind::Hook(HookPhase::PreDispatch),
            1,
            "fixture hook failed",
        ),
        (
            &["render-fail", "--output", "json"],
            RunErrorKind::Render,
            1,
            "key must be a string",
        ),
        (
            &["ranged", "--output", "json"],
            RunErrorKind::Handler,
            1,
            "config line 2 does not parse",
        ),
    ];
    for (args, kind, code, summary) in cases {
        let output = run(&binary, args);
        assert_eq!(output.status.code(), Some(code), "{args:?}");
        assert!(output.stderr.is_empty(), "{args:?}: {:?}", output.stderr);
        let stdout = String::from_utf8(output.stdout).unwrap();
        let diagnostic = parse_diagnostic(OutputMode::Json, &stdout)
            .unwrap_or_else(|e| panic!("{args:?}: {e}:\n{stdout}"));
        assert_eq!(diagnostic.kind, DiagnosticKind::from(kind), "{args:?}");
        assert!(diagnostic.summary.contains(summary), "{args:?}: {stdout}");
    }

    let ranged = run(&binary, &["ranged", "--output", "yaml"]);
    let ranged =
        parse_diagnostic(OutputMode::Yaml, &String::from_utf8(ranged.stdout).unwrap()).unwrap();
    assert_eq!(ranged.detail, "expected `resource <name> <state>`");
    assert_eq!(ranged.range.unwrap().start.line, 2);

    let csv = run(&binary, &["fail", "--output", "csv"]);
    assert_eq!(csv.status.code(), Some(1));
    assert!(csv.stderr.is_empty());
    assert_eq!(
        String::from_utf8(csv.stdout).unwrap(),
        "type,schema_version,severity,kind,summary,detail,range_filename,range_line,range_column\n\
         diagnostic,1,error,handler,fixture handler failed,,,,\n"
    );

    let warning_failure = run(&binary, &["warn-fail", "--output", "json"]);
    assert_eq!(warning_failure.status.code(), Some(1));
    let stdout = String::from_utf8(warning_failure.stdout).unwrap();
    assert!(
        parse_diagnostic(OutputMode::Json, &stdout).is_ok(),
        "{stdout}"
    );
    let stderr = String::from_utf8_lossy(&warning_failure.stderr);
    assert!(stderr.contains("fixture warning"), "{stderr}");
    assert!(!stderr.contains("fixture handler failed"), "{stderr}");

    let external = run(&binary, &["external", "--output", "json"]);
    assert_eq!(external.status.code(), Some(128));
    assert_eq!(external.stderr, b"fatal: external fixture failed");
    let external = parse_diagnostic(
        OutputMode::Json,
        &String::from_utf8(external.stdout).unwrap(),
    )
    .unwrap();
    assert_eq!(external.kind, DiagnosticKind::External);
    assert_eq!(external.detail, "fatal: external fixture failed");

    let malformed = run(&binary, &["--unknown", "--output", "jsn"]);
    assert_eq!(malformed.status.code(), Some(2));
    assert!(malformed.stdout.is_empty());
    assert!(String::from_utf8_lossy(&malformed.stderr).starts_with("error:"));

    let help = run(&binary, &["--help", "--output", "term-debug"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).contains("[header]USAGE[/header]"));
    assert!(help.stderr.is_empty());
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

struct EmittedRun {
    output: Output,
    handled: bool,
    status: u8,
}

/// Runs the fixture through `run_emitted`; the fixture writes the outcome it
/// was handed to a file after emission and exits with the reported status.
fn run_emitted(
    binary: &PathBuf,
    args: &[&str],
    artifact_path: Option<&std::path::Path>,
    close_stdout: bool,
) -> EmittedRun {
    let tempdir = tempfile::tempdir().unwrap();
    let outcome_path = tempdir.path().join("outcome");
    let mut command = Command::new(binary);
    command
        .args(args)
        .env("STANDOUT_FIXTURE_EDGE", "emitted")
        .env("STANDOUT_FIXTURE_OUTCOME_PATH", &outcome_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = artifact_path {
        command.env("STANDOUT_FIXTURE_ARTIFACT_PATH", path);
    }
    let mut child = command.spawn().unwrap();
    if close_stdout {
        drop(child.stdout.take());
    }
    let output = child.wait_with_output().unwrap();
    let outcome = std::fs::read_to_string(&outcome_path)
        .expect("run_emitted returned, so the caller's post-emission write happened");
    let (handled, status) = outcome
        .strip_prefix("handled=")
        .and_then(|rest| rest.split_once(" status="))
        .unwrap();
    EmittedRun {
        output,
        handled: handled.parse().unwrap(),
        status: status.parse().unwrap(),
    }
}

#[test]
fn emitted_edge_reports_the_outcome_run_exits_with() {
    let binary = fixture_binary();
    let tempdir = tempfile::tempdir().unwrap();
    let artifact_path = tempdir.path().join("artifact.bin");
    let unwritable = tempdir.path().join("missing").join("artifact.bin");
    let output_file = tempdir.path().join("out.txt");
    let output_file_arg = output_file.to_str().unwrap();
    let directory_arg = tempdir.path().to_str().unwrap();

    struct Case<'a> {
        args: &'a [&'a str],
        artifact_path: Option<&'a std::path::Path>,
        handled: bool,
        status: u8,
        stdout: &'a [u8],
        stderr_contains: &'a str,
    }

    let cases = [
        Case {
            args: &["ok"],
            artifact_path: None,
            handled: true,
            status: 0,
            stdout: b"ok\n",
            stderr_contains: "",
        },
        Case {
            args: &["fail"],
            artifact_path: None,
            handled: true,
            status: 1,
            stdout: b"",
            stderr_contains: "fixture handler failed",
        },
        Case {
            args: &["--unknown"],
            artifact_path: None,
            handled: true,
            status: 2,
            stdout: b"",
            stderr_contains: "unexpected argument",
        },
        Case {
            args: &["external"],
            artifact_path: None,
            handled: true,
            status: 128,
            stdout: b"",
            stderr_contains: "fatal: external fixture failed",
        },
        Case {
            args: &["binary"],
            artifact_path: None,
            handled: true,
            status: 0,
            stdout: &[0, 1, 2],
            stderr_contains: "",
        },
        Case {
            args: &["artifact-stdout"],
            artifact_path: None,
            handled: true,
            status: 0,
            stdout: &[0, 1, 2],
            stderr_contains: "wrote 3 entries to -",
        },
        Case {
            args: &["artifact"],
            artifact_path: Some(&unwritable),
            handled: true,
            status: 1,
            stdout: b"",
            stderr_contains: "Error writing artifact",
        },
        Case {
            args: &["warn-ok"],
            artifact_path: None,
            handled: true,
            status: 0,
            stdout: b"ok\n",
            stderr_contains: "fixture warning",
        },
        Case {
            args: &["warn-fail"],
            artifact_path: None,
            handled: true,
            status: 1,
            stdout: b"",
            stderr_contains: "fixture warning",
        },
        Case {
            args: &["silent"],
            artifact_path: None,
            handled: true,
            status: 0,
            stdout: b"",
            stderr_contains: "",
        },
        Case {
            args: &[],
            artifact_path: None,
            handled: false,
            status: 0,
            stdout: b"",
            stderr_contains: "",
        },
        Case {
            args: &["--output-file-path", output_file_arg, "ok"],
            artifact_path: None,
            handled: true,
            status: 0,
            stdout: b"",
            stderr_contains: "",
        },
        Case {
            args: &["--output-file-path", directory_arg, "ok"],
            artifact_path: None,
            handled: true,
            status: 1,
            stdout: b"",
            stderr_contains: "Error writing output",
        },
    ];

    for case in &cases {
        let run = run_emitted(&binary, case.args, case.artifact_path, false);
        let stderr = String::from_utf8_lossy(&run.output.stderr);
        let label = format!("{:?} (artifact path {:?})", case.args, case.artifact_path);
        assert_eq!(run.handled, case.handled, "{label}: handled");
        assert_eq!(run.status, case.status, "{label}: reported status");
        assert_eq!(
            run.output.status.code(),
            Some(i32::from(case.status)),
            "{label}: exit"
        );
        assert_eq!(run.output.stdout, case.stdout, "{label}: stdout");
        if case.stderr_contains.is_empty() {
            assert!(stderr.is_empty(), "{label}: stderr {stderr:?}");
        } else {
            assert!(
                stderr.contains(case.stderr_contains),
                "{label}: stderr {stderr:?}"
            );
        }
    }
    assert_eq!(std::fs::read_to_string(output_file).unwrap(), "ok");

    let artifact_run = run_emitted(&binary, &["artifact"], Some(&artifact_path), false);
    assert!(artifact_run.handled);
    assert_eq!(artifact_run.status, 0);
    assert_eq!(std::fs::read(&artifact_path).unwrap(), [0, 1, 2]);
    assert_eq!(
        String::from_utf8_lossy(&artifact_run.output.stdout),
        format!("wrote 3 entries to {}\n", artifact_path.display())
    );
    assert!(artifact_run.output.stderr.is_empty());
}

#[test]
fn emitted_edge_reports_closed_consumer_pipes_the_way_run_exits() {
    let binary = fixture_binary();

    let text = run_emitted(&binary, &["huge"], None, true);
    assert!(text.handled);
    assert_eq!(text.status, 0);
    assert_eq!(text.output.status.code(), Some(0));
    assert!(text.output.stderr.is_empty());

    let bytes = run_emitted(&binary, &["binary-huge"], None, true);
    assert!(bytes.handled);
    assert_eq!(bytes.status, 1);
    assert_eq!(bytes.output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&bytes.output.stderr).contains("Error writing"),
        "stderr: {}",
        String::from_utf8_lossy(&bytes.output.stderr)
    );
}
