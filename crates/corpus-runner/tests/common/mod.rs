// Scaffolding shared by the corpus-runner integration tests; every caller is `#![cfg(unix)]`.
#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

// Prepends to the process-wide PATH: a caller must run alone in its own test binary.
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

// The agent sandbox denies reads under the source checkout.
pub fn stage_dir(src: &Path, dest: &Path) {
    fs::create_dir_all(dest).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dest.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            stage_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

// Answers are spliced into an awk program verbatim: no awk or shell metacharacters.
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
