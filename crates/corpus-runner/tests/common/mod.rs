//! Scaffolding shared by the corpus-runner integration tests: executable
//! stand-in scripts, the fake `cargo` that installs a canned binary where the
//! runner's `--target-dir` points, and the awk-scripted questionnaire agent.
//! Each test binary pulls this in with `mod common;`; every caller is
//! `#![cfg(unix)]`-gated, so nothing here needs its own gate.

// Each test binary uses its own subset of these helpers; the unused rest is
// not dead weight, just unclaimed by that binary.
#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Writes `body` as an executable `#!/bin/sh` script named `name` in `dir`
/// and returns its path.
pub fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Installs a fake toolchain in `bin_dir` and prepends it to the
/// process-wide PATH: a `cargo` that, instead of compiling, copies a canned
/// implementation (`impl_body`, a shell-script body) to
/// `<--target-dir>/debug/<binary_name>` — proving the runner's target-dir
/// plumbing without a network or a real compile.
///
/// PATH is process-wide state: a test binary that calls this must run alone
/// in its own integration-test file (as `hermetic_loop.rs` and
/// `hermetic_roster_loop.rs` do).
pub fn install_fake_cargo(bin_dir: &Path, binary_name: &str, impl_body: &str) {
    fs::create_dir_all(bin_dir).unwrap();
    let impl_path = script(bin_dir, &format!("{binary_name}-impl"), impl_body);
    script(
        bin_dir,
        "cargo",
        &format!(
            r#"td=""
prev=""
for a in "$@"; do
  if [ "$prev" = "--target-dir" ]; then td="$a"; fi
  prev="$a"
done
[ -n "$td" ] || {{ echo "no --target-dir passed" >&2; exit 1; }}
mkdir -p "$td/debug"
cp "{impl_path}" "$td/debug/{binary_name}""#,
            impl_path = impl_path.display()
        ),
    );
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            bin_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
}

/// Writes the scripted questionnaire agent as `name` in `dir` and returns
/// its path: after running `preamble` (shell lines, may be empty), it answers
/// the rendered answer sheet in place — one `(question id, answer)` line
/// inserted under each tagged question via awk — and, when `result_event` is
/// set, ends with a Claude Code stream-json result event so session
/// instrumentation has data.
///
/// Answers are spliced into an awk program verbatim: keep them free of awk
/// and shell metacharacters (the fixtures' plain sentences are).
pub fn questionnaire_agent(
    dir: &Path,
    name: &str,
    preamble: &str,
    answers: &[(&str, &str)],
    result_event: bool,
) -> PathBuf {
    let rules: String = answers
        .iter()
        .map(|(id, answer)| format!("/<id:{id}>$/ {{ print \"{answer}\" }}\n"))
        .collect();
    let event = if result_event {
        "\necho '{\"type\":\"result\",\"num_turns\":1,\"usage\":{\"input_tokens\":10,\"output_tokens\":20}}'"
    } else {
        ""
    };
    let preamble = if preamble.is_empty() {
        String::new()
    } else {
        format!("{preamble}\n")
    };
    script(
        dir,
        name,
        &format!(
            "set -e\n{preamble}awk '{{ print }}\n{rules}' QUESTIONNAIRE.md > QUESTIONNAIRE.md.filled\nmv QUESTIONNAIRE.md.filled QUESTIONNAIRE.md{event}"
        ),
    )
}
