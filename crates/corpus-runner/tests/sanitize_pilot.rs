//! The committed-pilot sanitizer as an external command — path specificity,
//! host inventory removal, and preservation of ordinary transcript text —
//! plus the secret-shape scan: the review's manual leak pass (home paths,
//! usernames, hostnames, email and token shapes) as a permanent regression
//! test over every committed run artifact in `corpus/pilot/` and
//! `corpus/demo/`.

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
/// not leaks. Every detector class honors this list: each entry is the exact
/// matched *value* a detector reports (the email address, the token, the
/// path — not the surrounding excerpt), so vouching for one fixture never
/// silences a different value of the same class. Additions need the same
/// scrutiny the original manual leak scan applied: a new secret-shaped
/// string fails the scan until a human vouches for it here.
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

/// One secret-shaped match: `value` is the exact matched string an
/// [`ALLOWED_MATCHES`] entry must equal to vouch for it; `shown` is the
/// classified, excerpted failure-message line.
struct Hit {
    value: String,
    shown: String,
}

/// The contiguous path token starting at `at` — what an allowlist entry for
/// a vouched path-shaped fixture must equal.
fn path_token(text: &str, at: usize) -> String {
    text[at..]
        .chars()
        .take_while(|&c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '-' | '+'))
        .collect()
}

/// The contiguous token starting at `at` over `extra` characters beyond
/// ASCII alphanumerics — what an allowlist entry for a vouched token-shaped
/// fixture must equal.
fn token_at(text: &str, at: usize, extra: &[char]) -> String {
    text[at..]
        .chars()
        .take_while(|&c| c.is_ascii_alphanumeric() || extra.contains(&c))
        .collect()
}

/// Reports every home-directory shape: any `/Users/…` (the macOS form — the
/// pilot host), and `/home/<name>` for any concrete account name except the
/// generic `user` the agents write into their own examples.
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

/// The pilot host's account name, in any form a path substitution could
/// have missed. The matched value is the surrounding word, so one vouched
/// mention could be allowlisted without disabling the detector.
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

/// Reports every email-shaped string (see [`email_at`]).
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

/// The pilot host's known machine names — the hostname leak class of the
/// manual review, matched as exact identifiers (case-insensitive, bounded by
/// non-name characters) rather than a broad domain regex, so ordinary URLs
/// and prose stay quiet.
const PILOT_HOST_IDENTIFIERS: &[&str] = &["astron", "imac-de-arthur"];

/// Host-inventory JSON keys the sanitizer strips from init events; their
/// presence in a committed artifact means a hostname-bearing structure
/// slipped through, whatever its value.
const HOST_INVENTORY_KEYS: &[&str] = &[
    "\"hostname\"",
    "\"host_name\"",
    "\"nodename\"",
    "\"computer_name\"",
    "\"local_hostname\"",
    "\"fqdn\"",
];

/// Reports hostname shapes: the pilot host's exact identifiers, structured
/// host-inventory fields, and hyphenated mDNS `.local` names — the shape
/// macOS mints by default ("iMac-de-Arthur.local"). The hyphen requirement
/// keeps code idioms like `threading.local` quiet.
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
    for (at, _) in text.match_indices(".local") {
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
                value: format!("{label}.local"),
                shown: format!("mDNS host name: {}", excerpt(text, at - label_len)),
            });
        }
    }
}

/// Reports token-shaped strings: known credential prefixes, AWS access-key
/// ids, Slack tokens, private-key armor, and two-segment JWTs. Armor's
/// matched value is the marker itself — key material is never fixture data,
/// so it has no per-value vouching story.
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

/// A short context window around a hit, for the failure message.
fn excerpt(text: &str, at: usize) -> String {
    let before: usize = text[..at].chars().rev().take(20).map(char::len_utf8).sum();
    let after: usize = text[at..].chars().take(60).map(char::len_utf8).sum();
    format!("…{}…", text[at - before..at + after].escape_debug())
}

/// Scans one artifact's text, returning every secret-shaped hit whose exact
/// matched value is not an allowlisted fixture string.
fn secret_shaped_hits(text: &str) -> Vec<String> {
    hits_against(text, ALLOWED_MATCHES)
}

/// The scan against an explicit allowlist — split from [`secret_shaped_hits`]
/// so a test can prove vouching works for every detector class.
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
        "uname reports astron.is here",
        "host iMac-de-Arthur.local answering",
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

/// A vouched fixture of any detector class can be allowlisted by its exact
/// matched value — and vouching one value silences neither other classes nor
/// other values of the same class.
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
