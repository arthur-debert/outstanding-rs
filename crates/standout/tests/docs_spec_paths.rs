//! Spec-tree reference hygiene (issue #405).
//!
//! A Spec moves when its epic merges: `docs/spec/<slug>.md` becomes
//! `docs/spec/implemented/<slug>.md`. Two classes of reference break in that
//! move, and nothing else in the suite reads either one:
//!
//! - a literal Spec path written elsewhere in the tree — prose, a doc comment,
//!   an archetype manifest — still names the old location;
//! - a relative Markdown link or image inside the moved Spec
//!   (`[ADR-0019](../adr/0019-x.md)`) is now one directory too shallow.
//!
//! So this test reads every tracked file for literal Spec paths, parses every
//! tracked Markdown file under `docs/spec` for its relative links, and
//! requires each reference to name a file that exists.
//!
//! Literal paths to files *outside* `docs/spec` are deliberately not checked:
//! an archived Spec describes the tree as it stood before its epic ran, so a
//! path to a since-moved source file is history it must keep stating, not a
//! stale reference.

use pulldown_cmark::{Event, Options, Parser, Tag};
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

/// The URI scheme a link destination carries, if any — `https` in
/// `https://example.com`, `mailto` in `mailto:a@b.c`. RFC 3986 spells a
/// scheme as a letter followed by letters, digits, `+`, `-`, or `.`, then a
/// colon; anything else is a path, including one holding a colon later on.
fn uri_scheme(dest: &str) -> Option<&str> {
    let end = dest.find(':')?;
    let scheme = &dest[..end];
    let mut chars = scheme.chars();
    let first_is_alpha = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let rest_is_scheme = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    (first_is_alpha && rest_is_scheme).then_some(scheme)
}

/// Every link and image destination in `text` that names a file next to it, in
/// appearance order — `#fragment` suffixes trimmed, in-page anchors and any
/// destination carrying a URI scheme (`https:`, `mailto:`) dropped.
///
/// Markdown is parsed rather than scanned for `](`: that substring reads a
/// link written inside a code span or a fenced block — which a Spec discussing
/// links does write — as a live reference, mistakes a link title or an
/// angle-bracketed destination for part of the path, and never sees a
/// reference-style `[text][label]` link at all, whose destination sits in a
/// `[label]: path` definition elsewhere in the file. The parser resolves all
/// three, and it is the crate rustdoc parses Markdown with.
fn relative_links(text: &str) -> Vec<String> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let mut found = Vec::new();
    for event in Parser::new_ext(text, options) {
        let (Event::Start(Tag::Link { dest_url, .. }) | Event::Start(Tag::Image { dest_url, .. })) =
            event
        else {
            continue;
        };
        if uri_scheme(&dest_url).is_some() {
            continue;
        }
        let target = dest_url.split('#').next().unwrap_or_default().trim();
        if !target.is_empty() {
            found.push(target.to_string());
        }
    }
    found
}

#[test]
fn literal_spec_paths_name_files_that_exist() {
    let root = repo_root();
    let mut stale = Vec::new();
    let mut read = 0usize;
    for file in tracked_files() {
        // Binary and non-UTF-8 content carries no prose to check.
        let Ok(text) = fs::read_to_string(root.join(&file)) else {
            continue;
        };
        for path in spec_paths(&text) {
            read += 1;
            if !root.join(&path).exists() {
                stale.push(format!("{file}: {path}"));
            }
        }
    }
    // A scan that reads nothing passes for the wrong reason. The repository
    // names Spec files in prose all over; zero means the scanner broke.
    assert!(read > 0, "no `docs/spec/` path found anywhere in the tree");
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
    let mut read = 0usize;
    for spec in specs {
        let path = root.join(&spec);
        let text = fs::read_to_string(&path).expect("Spec is UTF-8");
        let dir = path.parent().expect("Spec has a parent directory");
        for link in relative_links(&text) {
            read += 1;
            if !dir.join(&link).exists() {
                broken.push(format!("{spec}: {link}"));
            }
        }
    }
    // Same reason as above: the Specs cross-reference their ADRs, so a scan
    // that comes back empty is reading nothing rather than finding nothing.
    assert!(
        read > 0,
        "no relative link found in any Spec under docs/spec"
    );
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
    // A title, an angle-bracketed destination, a mail link, an in-page anchor.
    assert_eq!(
        relative_links("[a](./t.md \"Title\") [b](<./sp ace.md>) [c](mailto:a@b.c) [d](#section)"),
        vec!["./t.md".to_string(), "./sp ace.md".to_string()]
    );
    // A reference-style link resolves through its definition; an undefined
    // label is not a link in Markdown, so there is nothing to resolve.
    assert_eq!(
        relative_links("[a][decision] and [b][missing]\n\n[decision]: ../../adr/0019-x.md\n"),
        vec!["../../adr/0019-x.md".to_string()]
    );
    // Code spans and fenced blocks quote links, they do not make them.
    assert_eq!(
        relative_links("`[a](./nope.md)`\n\n```\n[b](./also-nope.md)\n```\n"),
        Vec::<String>::new()
    );
    // Images break on a move exactly as links do.
    assert_eq!(
        relative_links("![diagram](./img/flow.png)"),
        vec!["./img/flow.png".to_string()]
    );
}

#[test]
fn scheme_detection_separates_urls_from_paths() {
    assert_eq!(uri_scheme("https://example.com"), Some("https"));
    assert_eq!(uri_scheme("mailto:a@b.c"), Some("mailto"));
    assert_eq!(uri_scheme("ftp://example.com/x"), Some("ftp"));
    // A relative path is a path, colon or no colon.
    assert_eq!(uri_scheme("../../adr/0019-x.md"), None);
    assert_eq!(uri_scheme("./notes/a:b.md"), None);
    assert_eq!(uri_scheme("2026-plan.md"), None);
}
