//! Real CLI flows for the bootstrap wizard's answer sheets: questionnaire
//! rendering to stdout and a file, successful and failing `--answers FILE`
//! and `--answers -` submissions through the review and confirmation gate,
//! attended and rejected terminal confirmation, missing-terminal failure,
//! automated `--yes` runs, stale fingerprints, accumulated batch errors, and
//! the no-partial-write guarantee.
//!
//! Every test runs the production `standout` binary in a fresh temporary
//! working directory without a real terminal. Confirmation never reads
//! stdin — stdin only ever carries an answer sheet — so every run pins the
//! framework's attended-terminal seam (`STANDOUT_QUESTIONNAIRE_TERMINAL`): to
//! `absent` by default (a regression that reaches the confirmation gate
//! fails fast instead of prompting the developer's terminal), or to a
//! scripted replies file for attended flows.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

/// The framework's attended-terminal test seam: `absent` simulates no
/// terminal; any other value names a file of scripted reply lines. Debug
/// builds only — these tests exercise the debug-profile binary; a release
/// binary never reads the variable.
const TERMINAL_SEAM_VAR: &str = "STANDOUT_QUESTIONNAIRE_TERMINAL";

/// Run the production wizard binary in `cwd` with `stdin` piped in and the
/// attended-terminal seam pinned to `terminal_seam`. A run that fails fast
/// (a missing answers file, a rejected sheet) may exit before reading
/// stdin; the broken pipe that write then reports is an expected outcome,
/// so the helper still returns the child's output for failure-mode
/// assertions.
fn run_seamed(
    cwd: &Path,
    args: &[&str],
    stdin: &str,
    terminal_seam: impl AsRef<std::ffi::OsStr>,
) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_standout"))
        .current_dir(cwd)
        .args(args)
        .env(TERMINAL_SEAM_VAR, terminal_seam)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Err(error) = child.stdin.as_mut().unwrap().write_all(stdin.as_bytes()) {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe,
            "writing the child's stdin failed: {error}"
        );
    }
    child.wait_with_output().unwrap()
}

/// [`run_seamed`] with no attended terminal — the default, so any run that
/// unexpectedly reaches the confirmation gate fails fast.
fn run_standout(cwd: &Path, args: &[&str], stdin: &str) -> Output {
    run_seamed(cwd, args, stdin, "absent")
}

/// [`run_seamed`] with an attended terminal scripted to answer `replies`
/// (one line per prompt). The replies file lives outside `cwd`, keeping
/// whole-directory no-partial-write assertions exact.
fn run_attended(cwd: &Path, args: &[&str], stdin: &str, replies: &str) -> Output {
    let terminal = TempDir::new().unwrap();
    let script = terminal.path().join("replies.txt");
    fs::write(&script, replies).unwrap();
    run_seamed(cwd, args, stdin, &script)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_in_order(text: &str, earlier: &str, later: &str) {
    let earlier_at = text
        .find(earlier)
        .unwrap_or_else(|| panic!("missing {earlier:?} in:\n{text}"));
    let later_at = text
        .find(later)
        .unwrap_or_else(|| panic!("missing {later:?} in:\n{text}"));
    assert!(
        earlier_at < later_at,
        "expected {earlier:?} before {later:?} in:\n{text}"
    );
}

/// The entries of `dir`, sorted, for whole-directory no-partial-write
/// assertions (a failed run must leave nothing behind — no destination and
/// no staging leftovers).
fn dir_entries(dir: &Path) -> Vec<String> {
    let mut entries: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    entries
}

/// Set `value` as the answer text below the first question line ending
/// with `<id:...>`, replacing a pre-filled default line when one is
/// rendered.
fn fill(sheet: &str, id: &str, value: &str) -> String {
    let tag = format!("<id:{id}>");
    let lines: Vec<&str> = sheet.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut found = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        out.push(line.to_string());
        i += 1;
        if !found && line.trim_end().ends_with(&tag) {
            found = true;
            // A non-blank line right below the question is a pre-filled
            // default: the answer replaces it.
            if lines.get(i).is_some_and(|next| !next.trim().is_empty()) {
                i += 1;
            }
            out.push(value.to_string());
        }
    }
    assert!(found, "answer sheet has no question line for {tag}");
    out.join("\n") + "\n"
}

/// A completed sheet for a minimal `hello-tool` project, produced by the
/// binary's own `questions` rendering.
fn completed_sheet(cwd: &Path) -> String {
    let rendered = run_standout(cwd, &["new-project", "questions"], "");
    assert!(rendered.status.success());
    let sheet = stdout(&rendered);
    let sheet = fill(&sheet, "project.name", "hello-tool");
    let sheet = fill(&sheet, "command.name", "greet");
    let sheet = fill(&sheet, "command.description", "Greet one value");
    fill(&sheet, "command.inputs.name", "name")
}

/// A valid completed sheet whose answer text contains a suspected tag
/// fragment. The parser accepts the answer but reports a warning so users
/// can catch accidentally mangled question tags.
fn completed_sheet_with_suspected_tag(cwd: &Path) -> String {
    let sheet = completed_sheet(cwd);
    fill(
        &sheet,
        "command.description",
        "Greet one <id:project.name> value",
    )
}

#[test]
fn questions_renders_a_deterministic_sheet_and_generates_nothing() {
    let dir = TempDir::new().unwrap();

    let first = run_standout(dir.path(), &["new-project", "questions"], "");
    let second = run_standout(dir.path(), &["new-project", "questions"], "");

    assert!(first.status.success());
    assert_eq!(stdout(&first), stdout(&second));
    let sheet = stdout(&first);
    assert!(sheet.starts_with("#! standout-answers 1\n"));
    assert!(sheet.contains("#! questionnaire: standout.new-project"));
    assert!(sheet.contains("<id:project.name>"));
    assert!(sheet.contains("<id:command.inputs.sources>"));
    assert!(dir_entries(dir.path()).is_empty());
}

#[test]
fn questions_stdout_broken_pipe_is_successful_early_termination() {
    let dir = TempDir::new().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_standout"))
        .current_dir(dir.path())
        .arg("new-project")
        .arg("questions")
        .env(TERMINAL_SEAM_VAR, "absent")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stderr(&output).is_empty());
    assert!(dir_entries(dir.path()).is_empty());
}

#[test]
fn questions_writes_the_same_sheet_to_a_named_file() {
    let dir = TempDir::new().unwrap();

    let to_stdout = run_standout(dir.path(), &["new-project", "questions"], "");
    let to_file = run_standout(
        dir.path(),
        &["new-project", "questions", "--file", "answers.txt"],
        "",
    );

    assert!(to_file.status.success());
    assert!(stdout(&to_file).is_empty());
    assert_eq!(
        fs::read_to_string(dir.path().join("answers.txt")).unwrap(),
        stdout(&to_stdout).trim_end_matches('\n').to_string() + "\n"
    );
    assert_eq!(dir_entries(dir.path()), ["answers.txt"]);
}

#[test]
fn answers_file_generates_after_review_and_attended_confirmation() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("answers.txt"), completed_sheet(dir.path())).unwrap();

    let output = run_attended(
        dir.path(),
        &["new-project", "--answers", "answers.txt"],
        "",
        "yes\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let transcript = stdout(&output);
    assert!(transcript.contains("Continue? Type 'yes' to continue:"));
    assert!(transcript.contains("Review"));
    assert!(transcript.contains("Created hello-tool"));
    assert_in_order(&transcript, "Review", "Continue? Type 'yes' to continue:");
    assert_in_order(
        &transcript,
        "Continue? Type 'yes' to continue:",
        "Created hello-tool",
    );
    let destination = dir.path().join("hello-tool");
    assert!(destination.join("Cargo.toml").is_file());
    assert!(destination.join("crates/hello-tool/src/main.rs").is_file());
    assert_eq!(dir_entries(dir.path()), ["answers.txt", "hello-tool"]);
}

#[test]
fn rejected_confirmation_leaves_the_destination_unwritten() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("answers.txt"), completed_sheet(dir.path())).unwrap();

    let output = run_attended(
        dir.path(),
        &["new-project", "--answers", "answers.txt"],
        "",
        "no\n",
    );

    assert!(!output.status.success());
    let transcript = stdout(&output);
    assert!(transcript.contains("Review"));
    assert!(transcript.contains("Continue? Type 'yes' to continue:"));
    assert!(!transcript.contains("Created hello-tool"));
    assert_in_order(&transcript, "Review", "Continue? Type 'yes' to continue:");
    assert!(stderr(&output).contains("confirmation declined; nothing was run"));
    assert_eq!(dir_entries(dir.path()), ["answers.txt"]);
}

#[test]
fn stdin_sheet_generates_after_review_and_attended_confirmation() {
    let dir = TempDir::new().unwrap();
    let sheet = completed_sheet(dir.path());

    let output = run_attended(
        dir.path(),
        &["new-project", "--answers", "-"],
        &sheet,
        "yes\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let transcript = stdout(&output);
    assert!(transcript.contains("Continue? Type 'yes' to continue:"));
    assert!(transcript.contains("Review"));
    assert!(transcript.contains("Created hello-tool"));
    assert_in_order(&transcript, "Review", "Continue? Type 'yes' to continue:");
    assert_in_order(
        &transcript,
        "Continue? Type 'yes' to continue:",
        "Created hello-tool",
    );
    assert!(dir.path().join("hello-tool/Cargo.toml").is_file());
    assert_eq!(dir_entries(dir.path()), ["hello-tool"]);
}

#[test]
fn stdin_sheet_rejected_on_the_terminal_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let sheet = completed_sheet(dir.path());

    let output = run_attended(
        dir.path(),
        &["new-project", "--answers", "-"],
        &sheet,
        "no\n",
    );

    assert!(!output.status.success());
    let transcript = stdout(&output);
    assert!(transcript.contains("Review"));
    assert!(transcript.contains("Continue? Type 'yes' to continue:"));
    assert!(!transcript.contains("Created hello-tool"));
    assert_in_order(&transcript, "Review", "Continue? Type 'yes' to continue:");
    assert!(stderr(&output).contains("confirmation declined; nothing was run"));
    assert!(dir_entries(dir.path()).is_empty());
}

#[test]
fn stdin_sheet_without_a_terminal_fails_before_publication_with_guidance() {
    let dir = TempDir::new().unwrap();
    let sheet = completed_sheet(dir.path());

    let output = run_standout(dir.path(), &["new-project", "--answers", "-"], &sheet);

    assert!(!output.status.success());
    assert!(stdout(&output).contains("Review"));
    let errors = stderr(&output);
    assert!(errors.contains("attended terminal"));
    assert!(errors.contains("--yes"));
    assert!(errors.contains("nothing was run"));
    assert!(dir_entries(dir.path()).is_empty());
}

#[test]
fn stdin_sheet_with_yes_generates_without_any_terminal() {
    let dir = TempDir::new().unwrap();
    let sheet = completed_sheet(dir.path());

    let output = run_standout(
        dir.path(),
        &["new-project", "--answers", "-", "--yes"],
        &sheet,
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let transcript = stdout(&output);
    assert!(transcript.contains("Review"));
    assert!(!transcript.contains("Continue?"));
    assert!(transcript.contains("Created hello-tool"));
    assert!(dir.path().join("hello-tool/Cargo.toml").is_file());
    assert_eq!(dir_entries(dir.path()), ["hello-tool"]);
}

#[test]
fn stdin_sheet_surfaces_parse_warnings_without_rejecting_submission() {
    let dir = TempDir::new().unwrap();
    let sheet = completed_sheet_with_suspected_tag(dir.path());

    let output = run_standout(
        dir.path(),
        &["new-project", "--answers", "-", "--yes"],
        &sheet,
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Created hello-tool"));
    let warnings = stderr(&output);
    assert!(warnings.contains("Warning"), "{warnings}");
    assert!(warnings.contains("answer sheet from stdin"), "{warnings}");
    assert!(warnings.contains("command.description"), "{warnings}");
    assert!(dir.path().join("hello-tool/Cargo.toml").is_file());
    assert_eq!(dir_entries(dir.path()), ["hello-tool"]);
}

#[test]
fn named_file_with_yes_generates_without_any_terminal() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("answers.txt"), completed_sheet(dir.path())).unwrap();

    let output = run_standout(
        dir.path(),
        &["new-project", "--answers", "answers.txt", "--yes"],
        "",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let transcript = stdout(&output);
    assert!(transcript.contains("Review"));
    assert!(!transcript.contains("Continue?"));
    assert!(transcript.contains("Created hello-tool"));
    assert_eq!(dir_entries(dir.path()), ["answers.txt", "hello-tool"]);
}

#[test]
fn named_file_surfaces_parse_warnings_without_rejecting_submission() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("answers.txt"),
        completed_sheet_with_suspected_tag(dir.path()),
    )
    .unwrap();

    let output = run_standout(
        dir.path(),
        &["new-project", "--answers", "answers.txt", "--yes"],
        "",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Created hello-tool"));
    let warnings = stderr(&output);
    assert!(warnings.contains("Warning"), "{warnings}");
    assert!(warnings.contains("answer sheet answers.txt"), "{warnings}");
    assert!(warnings.contains("command.description"), "{warnings}");
    assert_eq!(dir_entries(dir.path()), ["answers.txt", "hello-tool"]);
}

#[test]
fn attended_end_of_terminal_input_cancels_instead_of_confirming() {
    let dir = TempDir::new().unwrap();
    let sheet = completed_sheet(dir.path());

    let output = run_attended(dir.path(), &["new-project", "--answers", "-"], &sheet, "");

    assert!(!output.status.success());
    assert!(stderr(&output).contains("confirmation declined; nothing was run"));
    assert!(dir_entries(dir.path()).is_empty());
}

#[test]
fn stdin_trailing_yes_never_confirms_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let mut sheet = completed_sheet(dir.path());
    sheet.push_str("yes\n");

    let output = run_standout(dir.path(), &["new-project", "--answers", "-"], &sheet);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("questionnaire input"));
    assert!(dir_entries(dir.path()).is_empty());
}

#[test]
fn stdin_stale_fingerprint_is_rejected_before_any_write() {
    let dir = TempDir::new().unwrap();
    let stale = completed_sheet(dir.path()).replacen(
        "#! fingerprint: sha256:",
        "#! fingerprint: sha256:00",
        1,
    );

    let output = run_standout(
        dir.path(),
        &["new-project", "--answers", "-", "--yes"],
        &stale,
    );

    assert!(!output.status.success());
    let errors = stderr(&output);
    assert!(errors.contains("render a fresh answer sheet"));
    assert!(errors.contains("answer sheet from stdin"));
    assert!(dir_entries(dir.path()).is_empty());
}

#[test]
fn stdin_sheet_accumulates_errors_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let rendered = run_standout(dir.path(), &["new-project", "questions"], "");
    let sheet = stdout(&rendered);
    let sheet = fill(&sheet, "project.name", "9bad");
    let sheet = fill(&sheet, "command.name", "greet");
    let sheet = fill(&sheet, "command.description", "Greet one value");
    let sheet = fill(&sheet, "command.inputs.name", "name");
    let sheet = fill(&sheet, "command.inputs.sources", "argument,teleport");

    let output = run_standout(
        dir.path(),
        &["new-project", "--answers", "-", "--yes"],
        &sheet,
    );

    assert!(!output.status.success());
    let errors = stderr(&output);
    assert!(errors.contains("[project.name]"));
    assert!(errors.contains("[command.inputs[0].sources]"));
    assert!(errors.contains("derived questionnaire answers are invalid"));
    assert!(dir_entries(dir.path()).is_empty());
}

#[test]
fn interactive_stdin_cannot_supply_the_dash_answer_sheet() {
    // Piped-but-empty stdin is the closest a real CLI test can get to the
    // no-document case; the unit tests cover the interactive-terminal
    // refusal through the injected reader.
    let dir = TempDir::new().unwrap();

    let output = run_standout(dir.path(), &["new-project", "--answers", "-", "--yes"], "");

    assert!(!output.status.success());
    assert!(stderr(&output).contains("questionnaire input"));
    assert!(dir_entries(dir.path()).is_empty());
}

#[test]
fn stale_fingerprint_is_rejected_before_any_write() {
    let dir = TempDir::new().unwrap();
    let stale = completed_sheet(dir.path()).replacen(
        "#! fingerprint: sha256:",
        "#! fingerprint: sha256:00",
        1,
    );
    fs::write(dir.path().join("answers.txt"), stale).unwrap();

    let output = run_standout(
        dir.path(),
        &["new-project", "--answers", "answers.txt"],
        "yes\n",
    );

    assert!(!output.status.success());
    let errors = stderr(&output);
    assert!(errors.contains("render a fresh answer sheet"));
    assert_eq!(dir_entries(dir.path()), ["answers.txt"]);
}

#[test]
fn invalid_sheet_accumulates_errors_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let rendered = run_standout(dir.path(), &["new-project", "questions"], "");
    let sheet = stdout(&rendered);
    let sheet = fill(&sheet, "project.name", "9bad");
    let sheet = fill(&sheet, "command.name", "greet");
    let sheet = fill(&sheet, "command.description", "Greet one value");
    let sheet = fill(&sheet, "command.inputs.name", "name");
    let sheet = fill(&sheet, "command.inputs.sources", "argument,teleport");
    let sheet = fill(&sheet, "result.shape", "message");
    let sheet = fill(&sheet, "result.fields", "summary,extra");
    fs::write(dir.path().join("answers.txt"), sheet).unwrap();

    let output = run_standout(
        dir.path(),
        &["new-project", "--answers", "answers.txt"],
        "yes\n",
    );

    assert!(!output.status.success());
    let errors = stderr(&output);
    assert!(errors.contains("[project.name]"));
    assert!(errors.contains("[command.inputs[0].sources]"));
    assert!(errors.contains("[result.fields]"));
    assert!(errors.contains("derived questionnaire answers are invalid"));
    assert_eq!(dir_entries(dir.path()), ["answers.txt"]);
}

#[test]
fn missing_answer_file_is_an_error_without_partial_writes() {
    let dir = TempDir::new().unwrap();

    let output = run_standout(
        dir.path(),
        &["new-project", "--answers", "missing.txt"],
        "yes\n",
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("Could not read the answer sheet"));
    assert!(dir_entries(dir.path()).is_empty());
}
