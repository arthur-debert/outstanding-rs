//! Spec-tree reference hygiene (issue #405).
//!
//! A Spec moves when its epic merges: `docs/spec/<slug>.md` becomes
//! `docs/spec/implemented/<slug>.md`. Two classes of reference break in that
//! move, and nothing else in the suite reads either one:
//!
//! - a literal Spec path written elsewhere in the tree — prose, a doc comment,
//!   an archetype manifest — still names the old location;
//! - a relative Markdown link inside the moved Spec (`](../adr/0019-x.md)`) is
//!   now one directory too shallow.
//!
//! So this test reads every tracked file for literal Spec paths, and every
//! tracked Markdown file under `docs/spec` for relative links, and requires
//! each to name a file that exists.
//!
//! Literal paths to files *outside* `docs/spec` are deliberately not checked:
//! an archived Spec describes the tree as it stood before its epic ran, so a
//! path to a since-moved source file is history it must keep stating, not a
//! stale reference.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The repository's tracked files, repo-root-relative. Tracked rather than
/// on-disk: a developer's untracked scratch file is not a repository
/// reference, and the checkout also carries build and environment trees that
/// are expensive to walk and never prose.
fn tracked_files() -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(["ls-files", "-z"])
        .output()
        .expect("running `git ls-files`");
    assert!(
        out.status.success(),
        "`git ls-files` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("`git ls-files` paths are UTF-8")
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// Characters a path token may continue with. A reference ends at the first
/// character outside this set — a backtick, a quote, a paren, whitespace — so
/// placeholder spellings such as `docs/spec/<slug>.md` stop before the `<` and
/// then fail the `.md` test in [`spec_paths`] instead of being resolved.
fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')
}

/// Every Spec-file path spelled out in `text`, in appearance order.
fn spec_paths(text: &str) -> Vec<String> {
    const PREFIX: &str = "docs/spec/";
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(PREFIX) {
        let tail = &rest[at..];
        let end = tail
            .char_indices()
            .find(|(_, c)| !is_path_char(*c))
            .map_or(tail.len(), |(i, _)| i);
        let candidate = &tail[..end];
        if candidate.ends_with(".md") {
            found.push(candidate.to_string());
        }
        rest = &tail[end.max(PREFIX.len())..];
    }
    found
}

/// Every relative Markdown link target in `text` — external URLs, mail links,
/// and in-page anchors dropped, `#fragment` suffixes trimmed.
fn relative_links(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        let tail = &rest[at + 2..];
        let Some(end) = tail.find(')') else { break };
        let target = tail[..end].trim();
        let target = target.split('#').next().unwrap_or_default();
        if !target.is_empty()
            && !target.starts_with("http://")
            && !target.starts_with("https://")
            && !target.starts_with("mailto:")
        {
            found.push(target.to_string());
        }
        rest = &tail[end..];
    }
    found
}

#[test]
fn literal_spec_paths_name_files_that_exist() {
    let root = repo_root();
    let mut stale = Vec::new();
    for file in tracked_files() {
        // Binary and non-UTF-8 content carries no prose to check.
        let Ok(text) = fs::read_to_string(root.join(&file)) else {
            continue;
        };
        for path in spec_paths(&text) {
            if !root.join(&path).exists() {
                stale.push(format!("{file}: {path}"));
            }
        }
    }
    stale.sort();
    assert!(
        stale.is_empty(),
        "references to Spec files that do not exist (a Spec moved without its \
         referrers following):\n{}",
        stale.join("\n")
    );
}

#[test]
fn relative_links_in_specs_resolve() {
    let root = repo_root();
    let specs = tracked_files()
        .into_iter()
        .filter(|f| f.starts_with("docs/spec/") && f.ends_with(".md"));

    let mut broken = Vec::new();
    for spec in specs {
        let path = root.join(&spec);
        let text = fs::read_to_string(&path).expect("Spec is UTF-8");
        let dir = path.parent().expect("Spec has a parent directory");
        for link in relative_links(&text) {
            if !dir.join(&link).exists() {
                broken.push(format!("{spec}: {link}"));
            }
        }
    }
    broken.sort();
    assert!(
        broken.is_empty(),
        "Markdown links in docs/spec that resolve to nothing (a Spec changed \
         directory depth without its links following):\n{}",
        broken.join("\n")
    );
}

#[test]
fn spec_path_scan_reads_only_complete_markdown_paths() {
    // Spelled through `dir` so this file does not itself carry the invented
    // paths that `literal_spec_paths_name_files_that_exist` would then chase.
    let dir = "docs/spec/";
    assert_eq!(
        spec_paths(&format!("see `{dir}implemented/a.md` and ({dir}b.md)")),
        vec![format!("{dir}implemented/a.md"), format!("{dir}b.md")]
    );
    // A directory mention and a placeholder name no file.
    assert!(spec_paths(&format!("write it to {dir} as {dir}<slug>.md")).is_empty());
}

#[test]
fn link_scan_keeps_relative_targets_only() {
    assert_eq!(
        relative_links("[a](../../adr/0019-x.md) [b](https://example.com) [c](./y.md#top)"),
        vec!["../../adr/0019-x.md".to_string(), "./y.md".to_string()]
    );
}
