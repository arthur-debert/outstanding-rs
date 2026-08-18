//! The committed-pilot sanitizer as an external command — path specificity,
//! host inventory removal, and preservation of ordinary transcript text —
//! plus the secret-shape scan: the review's manual leak pass (home paths,
//! usernames, email and token shapes) as a permanent regression test over
//! every committed run artifact in `corpus/pilot/` and `corpus/demo/`.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
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
            "{{\"type\":\"system\",\"subtype\":\"init\",\"cwd\":{:?},\"session_id\":\"12345678-ABCD-1234-ABCD-123456789ABC\",\"tools\":[\"host-tool\"],\"note\":\"root art cartoon\"}}\n",
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
    assert!(!transcript.contains("12345678-ABCD-1234-ABCD-123456789ABC"));
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

// ---------------------------------------------------------------------------
// Secret-shape scan over the committed artifacts
// ---------------------------------------------------------------------------

/// Strings that match a secret shape but are known committed fixture data,
/// not leaks. Additions need the same scrutiny the original manual leak scan
/// applied: every entry is an exact matched excerpt, so a new email or token
/// shape fails the scan until a human vouches for it here.
const ALLOWED_MATCHES: &[&str] = &[
    // formlike's own test fixture address, written by the blind agent.
    "valid@email.com",
];

/// Every committed run-artifact root the scan sweeps, relative to the repo.
const SCANNED_ROOTS: &[&str] = &[
    "corpus/pilot/runs",
    "corpus/pilot/scorecard.md",
    "corpus/demo",
];

fn is_email_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
}

fn is_email_domain(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-')
}

/// Extracts the email-shaped string around the `@` at `at`, if one is there:
/// a non-empty local part, and a domain whose final label is alphabetic and
/// at least two characters.
fn email_at(text: &str, at: usize) -> Option<String> {
    let local: String = text[..at]
        .chars()
        .rev()
        .take_while(|&c| is_email_local(c))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let domain: String = text[at + 1..]
        .chars()
        .take_while(|&c| is_email_domain(c))
        .collect();
    let domain = domain.trim_end_matches(['.', '-']);
    if local.is_empty() {
        return None;
    }
    let tld = domain.rsplit('.').next()?;
    if domain.contains('.') && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()) {
        Some(format!("{local}@{domain}"))
    } else {
        None
    }
}

/// Reports every home-directory shape: any `/Users/…` (the macOS form — the
/// pilot host), and `/home/<name>` for any concrete account name except the
/// generic `user` the agents write into their own examples.
fn home_path_hits(text: &str, hits: &mut Vec<String>) {
    if let Some(at) = text.find("/Users/") {
        hits.push(format!("macOS home path: {}", excerpt(text, at)));
    }
    for (at, _) in text.match_indices("/home/") {
        let name: String = text[at + "/home/".len()..]
            .chars()
            .take_while(|&c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
            .collect();
        // A leading dot is a dotfile under some literal `home/` directory
        // (agents write those into their own sandboxes), not an account name.
        if !name.is_empty() && !name.starts_with('.') && name != "user" {
            hits.push(format!(
                "home path for account {name:?}: {}",
                excerpt(text, at)
            ));
        }
    }
}

/// Reports token-shaped strings: known credential prefixes, AWS access-key
/// ids, Slack tokens, private-key armor, and two-segment JWTs.
fn token_hits(text: &str, hits: &mut Vec<String>) {
    for prefix in ["ghp_", "gho_", "ghs_", "ghu_", "github_pat_", "sk-ant-"] {
        if let Some(at) = text.find(prefix) {
            hits.push(format!("token prefix {prefix}: {}", excerpt(text, at)));
        }
    }
    for (at, _) in text.match_indices("AKIA") {
        let tail: Vec<char> = text[at + 4..].chars().take(16).collect();
        if tail.len() == 16
            && tail
                .iter()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            hits.push(format!("AWS access key id: {}", excerpt(text, at)));
        }
    }
    for kind in ["xoxb-", "xoxp-", "xoxa-", "xoxr-", "xoxs-"] {
        if let Some(at) = text.find(kind) {
            hits.push(format!("Slack token: {}", excerpt(text, at)));
        }
    }
    if let Some(at) = text.find("PRIVATE KEY-----") {
        hits.push(format!("private-key armor: {}", excerpt(text, at)));
    }
    for (at, _) in text.match_indices("eyJ") {
        let header_len = text[at..]
            .chars()
            .take_while(|&c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            .count();
        if text[at + header_len..].starts_with(".eyJ") {
            hits.push(format!("JWT shape: {}", excerpt(text, at)));
        }
    }
}

/// A short context window around a hit, for the failure message.
fn excerpt(text: &str, at: usize) -> String {
    let before: usize = text[..at].chars().rev().take(20).map(char::len_utf8).sum();
    let after: usize = text[at..].chars().take(60).map(char::len_utf8).sum();
    format!("…{}…", text[at - before..at + after].escape_debug())
}

/// Scans one artifact's text, returning every secret-shaped hit that is not
/// an allowlisted fixture string.
fn secret_shaped_hits(text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    home_path_hits(text, &mut hits);
    // The pilot host's account name, in any form a path substitution could
    // have missed.
    if let Some(at) = text.find("adebert") {
        hits.push(format!("host username: {}", excerpt(text, at)));
    }
    for (at, _) in text.match_indices('@') {
        if let Some(email) = email_at(text, at) {
            if !ALLOWED_MATCHES.contains(&email.as_str()) {
                hits.push(format!("email shape: {email}"));
            }
        }
    }
    token_hits(text, &mut hits);
    hits
}

/// Collects every file under `root` (or `root` itself when it is a file).
fn artifact_files(root: &Path, files: &mut Vec<PathBuf>) {
    if root.is_file() {
        files.push(root.to_path_buf());
        return;
    }
    for entry in fs::read_dir(root).unwrap() {
        artifact_files(&entry.unwrap().path(), files);
    }
}

/// The manual leak scan of the pilot review, permanent: no committed report,
/// transcript, or scorecard may carry a home path, the host username, or an
/// email/token shape beyond the vouched-for fixture strings.
#[test]
fn committed_run_artifacts_carry_no_secret_shapes() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for root in SCANNED_ROOTS {
        let root = repo.join(root);
        assert!(root.exists(), "scanned root missing: {}", root.display());
        artifact_files(&root, &mut files);
    }
    assert!(
        files.len() >= 10,
        "the scan should sweep the committed pilot and demo artifacts"
    );

    let mut findings = Vec::new();
    for file in &files {
        let text = String::from_utf8_lossy(&fs::read(file).unwrap()).into_owned();
        for hit in secret_shaped_hits(&text) {
            findings.push(format!("{}: {hit}", file.display()));
        }
    }
    assert!(
        findings.is_empty(),
        "secret-shaped content in committed artifacts — sanitize the artifact \
         (corpus/pilot/sanitize-run.py) or, for vouched fixture data, extend \
         ALLOWED_MATCHES:\n{}",
        findings.join("\n")
    );
}

/// The scanner itself catches each shape class — so a green scan means the
/// artifacts are clean, not that the patterns rotted.
#[test]
fn secret_shape_patterns_catch_each_class() {
    for leak in [
        "\"cwd\":\"/Users/carol/checkout\"",
        "path /home/carol/.config",
        "by adebert on the host",
        "mail carol.smith+x@gmail.com now",
        "token ghp_abcdefghij",
        "key AKIAABCDEFGHIJKLMNOP end",
        "xoxb-1234-abc",
        "-----END OPENSSH PRIVATE KEY-----",
        "bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig",
    ] {
        assert!(
            !secret_shaped_hits(leak).is_empty(),
            "pattern missed: {leak}"
        );
    }
    for benign in [
        "see /home/user/project and [home]/x",
        "sandbox dotfile: $W/home/.config/gitlike/config.toml",
        "fixture valid@email.com",
        "sha256 49489f0d5fb49e8c277ce49efce51e414166338aa7adb7626c4e6328e0b0ae73",
        "https://github.com/arthur-debert/standout/issues/351",
        "an eyJ fragment without a second segment",
    ] {
        assert!(
            secret_shaped_hits(benign).is_empty(),
            "false positive on: {benign} — {:?}",
            secret_shaped_hits(benign)
        );
    }
}
