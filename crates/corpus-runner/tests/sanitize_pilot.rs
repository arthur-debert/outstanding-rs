// The committed-pilot sanitizer as an external command, plus a secret-shape
// scan over every committed run artifact in `corpus/pilot/` and
// `corpus/demo/`.

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

// A session with a shell prints file owners, so the account name has to be
// scrubbable — but only when the operator names it, or the fixture words
// above would go with it.
#[test]
fn a_named_account_is_replaced_everywhere_it_appears_as_a_word() {
    let temp = tempfile::tempdir().unwrap();
    let temp_root = fs::canonicalize(temp.path()).unwrap();
    let run = temp_root.join("run");
    let dest = temp_root.join("sanitized");
    fs::create_dir_all(run.join("workspace")).unwrap();

    fs::write(
        run.join("report.json"),
        "{\"owner\":\"drwxr-xr-x 4 hostperson wheel\",\"word\":\"hostpersonal\"}\n",
    )
    .unwrap();
    fs::write(
        run.join("transcript.jsonl"),
        "{\"type\":\"user\",\"text\":\"total 8 hostperson wheel\"}\n",
    )
    .unwrap();

    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/pilot/sanitize-run.py");
    let output = Command::new("python3")
        .arg(script)
        .arg(&run)
        .arg(&dest)
        .args(["--account", "hostperson"])
        .env("HOME", &temp_root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "sanitizer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = fs::read_to_string(dest.join("report.json")).unwrap();
    assert!(report.contains("4 [user] wheel"), "{report}");
    // A longer word that merely starts with the account name is not the
    // account.
    assert!(report.contains("\"hostpersonal\""), "{report}");
    let transcript = fs::read_to_string(dest.join("transcript.jsonl")).unwrap();
    assert!(transcript.contains("total 8 [user] wheel"), "{transcript}");
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

// Known fixture strings that match a secret shape but are not leaks; each
// entry is the exact matched value, so vouching one never silences another.
// The two `example.com` addresses are values the gcloudlike agent wrote into
// its own tests for a CLI whose config carries an account property; that domain
// is reserved (RFC 2606) and resolves to nobody.
const ALLOWED_MATCHES: &[&str] = &["valid@email.com", "me@example.com", "who@example.com"];

const SCANNED_ROOTS: &[&str] = &[
    "corpus/pilot/runs",
    "corpus/pilot/scorecard.md",
    "corpus/rerun",
    "corpus/completion",
    "corpus/demo",
];

fn is_email_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
}

fn is_email_domain(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-')
}

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

struct Hit {
    value: String,
    shown: String,
}

fn path_token(text: &str, at: usize) -> String {
    text[at..]
        .chars()
        .take_while(|&c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '-' | '+'))
        .collect()
}

fn token_at(text: &str, at: usize, extra: &[char]) -> String {
    text[at..]
        .chars()
        .take_while(|&c| c.is_ascii_alphanumeric() || extra.contains(&c))
        .collect()
}

fn home_path_hits(text: &str, hits: &mut Vec<Hit>) {
    for (at, _) in text.match_indices("/Users/") {
        hits.push(Hit {
            value: path_token(text, at),
            shown: format!("macOS home path: {}", excerpt(text, at)),
        });
    }
    for (at, _) in text.match_indices("/home/") {
        let name: String = text[at + "/home/".len()..]
            .chars()
            .take_while(|&c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
            .collect();
        // A leading dot is a dotfile under some literal `home/` directory
        // (agents write those into their own sandboxes), not an account name.
        if !name.is_empty() && !name.starts_with('.') && name != "user" {
            hits.push(Hit {
                value: path_token(text, at),
                shown: format!("home path for account {name:?}: {}", excerpt(text, at)),
            });
        }
    }
}

fn username_hits(text: &str, hits: &mut Vec<Hit>) {
    let is_word = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    for (at, needle) in text.match_indices("adebert") {
        let before: usize = text[..at]
            .chars()
            .rev()
            .take_while(|&c| is_word(c))
            .map(char::len_utf8)
            .sum();
        let after: usize = text[at + needle.len()..]
            .chars()
            .take_while(|&c| is_word(c))
            .map(char::len_utf8)
            .sum();
        hits.push(Hit {
            value: text[at - before..at + needle.len() + after].to_string(),
            shown: format!("host username: {}", excerpt(text, at)),
        });
    }
}

fn email_hits(text: &str, hits: &mut Vec<Hit>) {
    for (at, _) in text.match_indices('@') {
        if let Some(email) = email_at(text, at) {
            hits.push(Hit {
                shown: format!("email shape: {email}"),
                value: email,
            });
        }
    }
}

const PILOT_HOST_IDENTIFIERS: &[&str] = &["astron", "imac-de-arthur"];

const HOST_INVENTORY_KEYS: &[&str] = &[
    "\"hostname\"",
    "\"host_name\"",
    "\"nodename\"",
    "\"computer_name\"",
    "\"local_hostname\"",
    "\"fqdn\"",
];

fn hostname_hits(text: &str, hits: &mut Vec<Hit>) {
    let lower = text.to_ascii_lowercase();
    for identifier in PILOT_HOST_IDENTIFIERS {
        for (at, _) in lower.match_indices(identifier) {
            let end = at + identifier.len();
            let bounded_before =
                !lower[..at].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '-');
            let bounded_after = !lower[end..].starts_with(|c: char| c.is_ascii_alphanumeric());
            if bounded_before && bounded_after {
                hits.push(Hit {
                    value: text[at..end].to_string(),
                    shown: format!("pilot hostname: {}", excerpt(text, at)),
                });
            }
        }
    }
    for key in HOST_INVENTORY_KEYS {
        for (at, _) in lower.match_indices(key) {
            hits.push(Hit {
                value: (*key).to_string(),
                shown: format!("host-inventory field {key}: {}", excerpt(text, at)),
            });
        }
    }
    for (at, _) in lower.match_indices(".local") {
        let is_label = |c: char| c.is_ascii_alphanumeric() || c == '-';
        let label_len: usize = text[..at]
            .chars()
            .rev()
            .take_while(|&c| is_label(c))
            .map(char::len_utf8)
            .sum();
        let label = &text[at - label_len..at];
        let end = at + ".local".len();
        let bounded_after = !text[end..].starts_with(|c: char| c.is_ascii_alphanumeric());
        if label.contains('-') && label.chars().any(|c| c.is_ascii_alphanumeric()) && bounded_after
        {
            hits.push(Hit {
                value: text[at - label_len..end].to_string(),
                shown: format!("mDNS host name: {}", excerpt(text, at - label_len)),
            });
        }
    }
}

fn token_hits(text: &str, hits: &mut Vec<Hit>) {
    for prefix in ["ghp_", "gho_", "ghs_", "ghu_", "github_pat_", "sk-ant-"] {
        for (at, _) in text.match_indices(prefix) {
            hits.push(Hit {
                value: token_at(text, at, &['_', '-']),
                shown: format!("token prefix {prefix}: {}", excerpt(text, at)),
            });
        }
    }
    for (at, _) in text.match_indices("AKIA") {
        let tail: Vec<char> = text[at + 4..].chars().take(16).collect();
        if tail.len() == 16
            && tail
                .iter()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            hits.push(Hit {
                value: text[at..at + 20].to_string(),
                shown: format!("AWS access key id: {}", excerpt(text, at)),
            });
        }
    }
    for kind in ["xoxb-", "xoxp-", "xoxa-", "xoxr-", "xoxs-"] {
        for (at, _) in text.match_indices(kind) {
            hits.push(Hit {
                value: token_at(text, at, &['-']),
                shown: format!("Slack token: {}", excerpt(text, at)),
            });
        }
    }
    for (at, marker) in text.match_indices("PRIVATE KEY-----") {
        hits.push(Hit {
            value: marker.to_string(),
            shown: format!("private-key armor: {}", excerpt(text, at)),
        });
    }
    for (at, _) in text.match_indices("eyJ") {
        let header_len = text[at..]
            .chars()
            .take_while(|&c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            .count();
        if text[at + header_len..].starts_with(".eyJ") {
            hits.push(Hit {
                value: token_at(text, at, &['_', '-', '.']),
                shown: format!("JWT shape: {}", excerpt(text, at)),
            });
        }
    }
}

fn excerpt(text: &str, at: usize) -> String {
    let before: usize = text[..at].chars().rev().take(20).map(char::len_utf8).sum();
    let after: usize = text[at..].chars().take(60).map(char::len_utf8).sum();
    format!("…{}…", text[at - before..at + after].escape_debug())
}

fn secret_shaped_hits(text: &str) -> Vec<String> {
    hits_against(text, ALLOWED_MATCHES)
}

fn hits_against(text: &str, allowed: &[&str]) -> Vec<String> {
    let mut hits = Vec::new();
    home_path_hits(text, &mut hits);
    username_hits(text, &mut hits);
    email_hits(text, &mut hits);
    hostname_hits(text, &mut hits);
    token_hits(text, &mut hits);
    hits.retain(|hit| !allowed.contains(&hit.value.as_str()));
    hits.into_iter().map(|hit| hit.shown).collect()
}

fn artifact_files(root: &Path, files: &mut Vec<PathBuf>) {
    if root.is_file() {
        files.push(root.to_path_buf());
        return;
    }
    for entry in fs::read_dir(root).unwrap() {
        artifact_files(&entry.unwrap().path(), files);
    }
}

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

#[test]
fn secret_shape_patterns_catch_each_class() {
    for leak in [
        "\"cwd\":\"/Users/carol/checkout\"",
        "path /home/carol/.config",
        "by adebert on the host",
        "uname reports astron.is here",
        "host iMac-de-Arthur.local answering",
        "host Build-Agent.LOCAL answering",
        "\"hostname\":\"redacted\"",
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
        "python threading.local() in the agent's code",
        "notes on astronomy and localhost resolution",
    ] {
        assert!(
            secret_shaped_hits(benign).is_empty(),
            "false positive on: {benign} — {:?}",
            secret_shaped_hits(benign)
        );
    }
}

#[test]
fn allowlisting_is_exact_and_works_for_every_class() {
    let text = "token ghp_abcdefghij beside path /Users/carol/checkout";
    assert_eq!(
        hits_against(text, &[]).len(),
        2,
        "{:?}",
        hits_against(text, &[])
    );

    let partly = hits_against(text, &["ghp_abcdefghij"]);
    assert_eq!(partly.len(), 1, "{partly:?}");
    assert!(partly[0].contains("macOS home path"), "{partly:?}");

    assert!(hits_against(text, &["ghp_abcdefghij", "/Users/carol/checkout"]).is_empty());

    let two_paths = "at /Users/carol/checkout then /Users/dave/scratch";
    let unvouched = hits_against(two_paths, &["/Users/carol/checkout"]);
    assert_eq!(unvouched.len(), 1, "{unvouched:?}");
    assert!(
        unvouched[0].contains("/Users/dave/scratch"),
        "{unvouched:?}"
    );
}
