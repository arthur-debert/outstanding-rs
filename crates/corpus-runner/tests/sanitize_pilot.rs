//! The committed-pilot sanitizer as an external command: path specificity,
//! host inventory removal, and preservation of ordinary transcript text.

#![cfg(unix)]

use std::fs;
use std::process::Command;

#[test]
fn sanitizer_prefers_specific_paths_without_rewriting_bare_usernames() {
    let temp = tempfile::tempdir().unwrap();
    let temp_root = fs::canonicalize(temp.path()).unwrap();
    let run = temp_root.join("run");
    let workspace = run.join("workspace");
    let home = temp_root.join("root");
    let dest = temp_root.join("sanitized");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&home).unwrap();

    fs::write(
        run.join("report.json"),
        format!(
            "{{\"workspace\":{:?},\"home\":{:?},\"words\":\"root art cartoon\"}}\n",
            workspace.to_string_lossy(),
            home.to_string_lossy(),
        ),
    )
    .unwrap();
    fs::write(
        run.join("transcript.jsonl"),
        format!(
            "{{\"type\":\"system\",\"subtype\":\"init\",\"cwd\":{:?},\"session_id\":\"12345678-1234-1234-1234-123456789abc\",\"tools\":[\"host-tool\"],\"note\":\"root art cartoon\"}}\n",
            workspace.to_string_lossy(),
        ),
    )
    .unwrap();

    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/pilot/sanitize-run.py");
    let output = Command::new("python3")
        .arg(script)
        .arg(&run)
        .arg(&dest)
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "sanitizer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = fs::read_to_string(dest.join("report.json")).unwrap();
    assert!(report.contains(r#""workspace":"[workspace]""#), "{report}");
    assert!(report.contains(r#""home":"[home]""#), "{report}");
    assert!(report.contains("root art cartoon"), "{report}");
    assert!(!report.contains("[run]/workspace"), "{report}");

    let transcript = fs::read_to_string(dest.join("transcript.jsonl")).unwrap();
    assert!(
        transcript.contains(r#""cwd":"[workspace]""#),
        "{transcript}"
    );
    assert!(transcript.contains("root art cartoon"), "{transcript}");
    assert!(!transcript.contains("host-tool"), "{transcript}");
    assert!(!transcript.contains("12345678-1234-1234-1234-123456789abc"));
}

#[test]
fn committed_pilot_transcripts_use_the_specific_workspace_placeholder() {
    let runs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/pilot/runs");
    for entry in fs::read_dir(runs).unwrap() {
        let transcript = entry.unwrap().path().join("transcript.jsonl");
        let text = fs::read_to_string(&transcript).unwrap();
        assert!(
            !text.contains("[run]/workspace"),
            "{} contains the shadowed workspace placeholder",
            transcript.display()
        );
    }
}
