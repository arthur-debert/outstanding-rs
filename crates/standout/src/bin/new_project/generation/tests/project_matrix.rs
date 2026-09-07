use super::*;
use crate::new_project::publish::publish_project;
use crate::new_project::test_support::run_cargo;
use std::fs;
use std::process::Command;

#[test]
fn generated_project_matrix_formats_checks_tests_and_runs() {
    let dir = TempDir::new().unwrap();
    let mut message = sample_spec(dir.path());
    message.result_shape = ResultShape::Message;
    message.record_fields.clear();
    let rich = rich_questionnaire_spec(dir.path());
    let path_first = path_first_spec(dir.path());
    let file_only = file_only_spec(dir.path());
    let bool_first = single_input_spec(
        dir.path(),
        "bool-tool",
        CommandInput {
            name: "verbose".into(),
            value_type: InputValueType::Bool,
            cardinality: InputCardinality::Boolean,
            sources: vec![InputSource::Argument],
        },
    );
    let optional_first = single_input_spec(
        dir.path(),
        "optional-tool",
        CommandInput {
            name: "note".into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Optional,
            sources: vec![InputSource::Argument],
        },
    );
    let repeated_first = single_input_spec(
        dir.path(),
        "repeated-tool",
        CommandInput {
            name: "tag".into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Repeated,
            sources: vec![InputSource::Argument],
        },
    );

    for spec in [
        &message,
        &rich,
        &path_first,
        &file_only,
        &bool_first,
        &optional_first,
        &repeated_first,
    ] {
        publish_project(spec).unwrap();
        run_cargo(&spec.destination, ["fmt", "--check"]);
        run_cargo(&spec.destination, ["check", "--workspace"]);
        run_cargo(&spec.destination, ["test", "--workspace"]);
    }

    let file_readme =
        fs::read_to_string(file_only.destination.join("crates/file-tool/README.md")).unwrap();
    assert!(file_readme.contains("--document-file document-input.txt"));
    assert!(!file_readme.contains("--document VALUE"));
    assert!(
        file_readme.contains("Blank values for the required string input `document` are rejected")
    );
    let bool_readme =
        fs::read_to_string(bool_first.destination.join("crates/bool-tool/README.md")).unwrap();
    assert!(bool_readme.contains("inspect --verbose"));
    assert!(!bool_readme.contains("Blank values"));
    let path_readme =
        fs::read_to_string(path_first.destination.join("crates/config-tool/README.md")).unwrap();
    assert!(!path_readme.contains("Blank values"));
    let optional_readme = fs::read_to_string(
        optional_first
            .destination
            .join("crates/optional-tool/README.md"),
    )
    .unwrap();
    assert!(!optional_readme.contains("Blank values"));
    let repeated_readme = fs::read_to_string(
        repeated_first
            .destination
            .join("crates/repeated-tool/README.md"),
    )
    .unwrap();
    assert!(!repeated_readme.contains("Blank values"));

    let file_input = file_only.destination.join("document.txt");
    fs::write(&file_input, "File only").unwrap();
    let file_only_run = run_binary(
        &file_only.destination,
        [
            "run",
            "-q",
            "-p",
            "file-tool",
            "--",
            "inspect",
            "--document-file",
            file_input.to_str().unwrap(),
        ],
    );
    assert!(String::from_utf8(file_only_run.stdout)
        .unwrap()
        .contains("Processed File only"));

    let missing_file_run = Command::new("cargo")
        .current_dir(&file_only.destination)
        .args([
            "run",
            "-q",
            "-p",
            "file-tool",
            "--",
            "inspect",
            "--document-file",
            "absent-document.txt",
        ])
        .output()
        .unwrap();
    assert!(!missing_file_run.status.success());
    let missing_file_stderr = String::from_utf8(missing_file_run.stderr).unwrap();
    assert!(
        missing_file_stderr.contains("absent-document.txt"),
        "the unreadable path belongs in the diagnostic\nstderr:\n{missing_file_stderr}"
    );

    let message_human = run_binary(
        &message.destination,
        [
            "run",
            "-q",
            "-p",
            "hello-tool",
            "--",
            "greet",
            "--name",
            "Ada",
        ],
    );
    let stdout = String::from_utf8(message_human.stdout).unwrap();
    assert!(stdout.contains("Processed Ada"));

    let help = run_binary(
        &message.destination,
        ["run", "-q", "-p", "hello-tool", "--", "--help"],
    );
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("USAGE"), "unexpected help page:\n{help}");
    assert!(!help.contains("Usage:"), "unexpected help page:\n{help}");

    let bare = run_binary(
        &message.destination,
        ["run", "-q", "-p", "hello-tool", "--", "--name", "Ada"],
    );
    assert!(String::from_utf8(bare.stdout)
        .unwrap()
        .contains("Processed Ada"));

    let human = run_binary(
        &rich.destination,
        [
            "run",
            "-q",
            "-p",
            "inspect-tool",
            "--",
            "inspect",
            "--document",
            "Ada",
            "--verbose",
            "--tag",
            "alpha",
            "--tag",
            "beta",
            "--config",
            "settings.toml",
        ],
    );
    let stdout = String::from_utf8(human.stdout).unwrap();
    assert!(stdout.contains("Ada"));
    assert!(stdout.contains("Summary:"));
    assert!(stdout.contains("Echo: Ada"));

    let json = run_binary(
        &rich.destination,
        [
            "run",
            "-q",
            "-p",
            "inspect-tool",
            "--",
            "inspect",
            "--document",
            "Ada",
            "--output",
            "json",
        ],
    );
    let value = json_value(&json);
    assert_eq!(value["summary"], "Processed Ada");
    assert_eq!(value["count"], "3");
    assert_eq!(value["echo"], "Ada");

    let input_file = rich.destination.join("input.txt");
    fs::write(&input_file, "File Ada").unwrap();
    let file_json = run_binary(
        &rich.destination,
        [
            "run",
            "-q",
            "-p",
            "inspect-tool",
            "--",
            "inspect",
            "--document-file",
            input_file.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    let value = json_value(&file_json);
    assert_eq!(value["summary"], "Processed File Ada");
    assert_eq!(value["count"], "8");

    let stdin_json = run_binary_with_stdin(
        &rich.destination,
        [
            "run",
            "-q",
            "-p",
            "inspect-tool",
            "--",
            "inspect",
            "--output",
            "json",
        ],
        "Pipe Ada\n",
    );
    let value = json_value(&stdin_json);
    assert_eq!(value["summary"], "Processed Pipe Ada");
    assert_eq!(value["count"], "8");

    let precedence_json = run_binary(
        &rich.destination,
        [
            "run",
            "-q",
            "-p",
            "inspect-tool",
            "--",
            "inspect",
            "--document",
            "Arg Ada",
            "--document-file",
            input_file.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    let value = json_value(&precedence_json);
    assert_eq!(value["summary"], "Processed Arg Ada");
    assert_eq!(value["count"], "7");

    let invalid = Command::new("cargo")
        .current_dir(&rich.destination)
        .args([
            "run",
            "-q",
            "-p",
            "inspect-tool",
            "--",
            "inspect",
            "--document",
            "   ",
        ])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("document cannot be empty"));

    let missing = Command::new("cargo")
        .current_dir(&rich.destination)
        .args(["run", "-q", "-p", "inspect-tool", "--", "inspect"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    let missing = String::from_utf8_lossy(&missing.stderr);
    assert!(
        missing.contains("input `document`"),
        "unexpected stderr: {missing}"
    );
    assert!(
        missing.contains("No input provided"),
        "unexpected stderr: {missing}"
    );
}

fn run_binary<const N: usize>(cwd: &Path, args: [&str; N]) -> std::process::Output {
    let output = Command::new("cargo")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "binary run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_binary_with_stdin<const N: usize>(
    cwd: &Path,
    args: [&str; N],
    stdin: &str,
) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("cargo")
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "binary run with stdin failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn json_value(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap()
}
