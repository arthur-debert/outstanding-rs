//! Walking the book: which pages it mounts, and where its links point.
//!
//! Two properties are checked here, both of them about the book as a graph
//! rather than about any page's prose. Reachability: `docs/SUMMARY.md` is
//! mdbook's table of contents, and a page under a mounted root that no entry
//! names is a page the book never renders and no reader can reach. Resolution:
//! a relative link between two pages must name a file that exists, and a link
//! carrying a `#fragment` must name a heading that exists on that page.
//!
//! The scanner ignores fenced code blocks and inline code spans, because a
//! template's `[tag]` vocabulary and a Rust example's `#` lines otherwise read
//! as links and headings. Anchors are computed with mdbook's own rule — keep
//! alphanumerics, `_` and `-`, turn whitespace into `-`, lowercase, and suffix
//! a repeated anchor with `-1`, `-2` and so on.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Directories whose every `.md` page must be reachable from `SUMMARY.md`.
///
/// `docs/crates` holds one symlink per documented crate, so walking it reaches
/// `crates/*/docs` under the paths the book itself uses.
pub const PAGE_ROOTS: &[&str] = &["docs/topics", "docs/guides", "docs/crates"];

/// The book's table of contents, relative to the repository root.
pub const SUMMARY: &str = "docs/SUMMARY.md";

/// A link found on a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The link target exactly as written.
    pub target: String,
    /// The 1-indexed line the link was written on.
    pub line: usize,
}

/// A link that does not resolve, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broken {
    /// The page the link was written on, relative to the repository root.
    pub page: PathBuf,
    /// The link itself.
    pub link: Link,
    /// What went wrong, phrased for a test failure message.
    pub reason: String,
}

impl std::fmt::Display for Broken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: `{}` — {}",
            self.page.display(),
            self.link.line,
            self.link.target,
            self.reason
        )
    }
}

/// Every `.md` page the book mounts, named the way `SUMMARY.md` names it.
///
/// Paths run through `docs/` — including `docs/crates/<name>/…`, where the
/// symlink lives — because that is the directory a relative link between two
/// pages resolves from. The top level of `docs/` is included alongside
/// [`PAGE_ROOTS`] because the book's introduction lives there; `SUMMARY.md`
/// names itself and is excluded.
pub fn pages(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let docs = root.join("docs");
    if docs.is_dir() {
        for entry in fs::read_dir(&docs)? {
            let path = entry?.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "md") {
                found.push(path);
            }
        }
    }
    for relative in PAGE_ROOTS {
        collect_markdown(&root.join(relative), &mut found)?;
    }
    let summary = canonical(&root.join(SUMMARY));
    found.retain(|path| canonical(path) != summary);
    found.sort();
    let mut seen = HashSet::new();
    found.retain(|path| seen.insert(canonical(path)));
    Ok(found)
}

/// The pages under [`PAGE_ROOTS`] that no `SUMMARY.md` entry names.
pub fn unreachable(root: &Path) -> io::Result<Vec<PathBuf>> {
    let summary_path = root.join(SUMMARY);
    let summary = fs::read_to_string(&summary_path)?;
    let mounted: HashSet<PathBuf> = links(&summary)
        .into_iter()
        .filter_map(|link| target_path(&summary_path, &link.target))
        .map(|path| canonical(&path))
        .collect();
    Ok(pages(root)?
        .into_iter()
        .filter(|page| !mounted.contains(&canonical(page)))
        .map(|page| relative_to(root, &page))
        .collect())
}

/// Every relative link on a mounted page (and in `SUMMARY.md`) that does not resolve.
pub fn broken_links(root: &Path) -> io::Result<Vec<Broken>> {
    let mut anchors: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut broken = Vec::new();
    let mut sources = pages(root)?;
    sources.push(root.join(SUMMARY));

    for page in &sources {
        let text = fs::read_to_string(page)?;
        for link in links(&text) {
            let Some((target, fragment)) = split_target(&link.target) else {
                continue;
            };
            let target = target.to_string();
            let fragment = fragment.map(str::to_string);
            let path = if target.is_empty() {
                page.clone()
            } else {
                match target_path(page, &target) {
                    Some(path) => path,
                    None => continue,
                }
            };
            if !path.exists() {
                broken.push(Broken {
                    page: relative_to(root, page),
                    link,
                    reason: format!("no file at {}", relative_to(root, &path).display()),
                });
                continue;
            }
            let Some(fragment) = fragment else { continue };
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let key = canonical(&path);
            if !anchors.contains_key(&key) {
                let text = fs::read_to_string(&key)?;
                anchors.insert(key.clone(), heading_anchors_set(&text));
            }
            if !anchors[&key].contains(&fragment) {
                broken.push(Broken {
                    page: relative_to(root, page),
                    link,
                    reason: format!(
                        "{} has no heading anchored `#{}`",
                        relative_to(root, &path).display(),
                        fragment
                    ),
                });
            }
        }
    }
    broken.sort_by(|a, b| (&a.page, a.link.line).cmp(&(&b.page, b.link.line)));
    Ok(broken)
}

/// The deployed book's origin, as the README spells it.
pub const SITE: &str = "https://standout.magik.works/";

/// Every link into the deployed book, from a page outside the book, that no
/// page backs.
///
/// The README links the book by URL rather than by relative path, so nothing
/// else here can see those links go stale. A `<page>.html` URL is backed by
/// `docs/<page>.md`; a directory URL is backed by that directory's `index.md`.
pub fn broken_site_links(root: &Path, page: &Path) -> io::Result<Vec<Broken>> {
    let text = fs::read_to_string(root.join(page))?;
    let mut broken = Vec::new();
    for link in links(&text) {
        let Some(rest) = link.target.strip_prefix(SITE) else {
            continue;
        };
        let (path, fragment) = match rest.split_once('#') {
            Some((path, fragment)) => (path.to_string(), Some(fragment.to_string())),
            None => (rest.to_string(), None),
        };
        let relative = match path.strip_suffix(".html") {
            Some(stem) => format!("{stem}.md"),
            None if path.is_empty() || path.ends_with('/') => format!("{path}index.md"),
            None => {
                broken.push(Broken {
                    page: page.to_path_buf(),
                    link,
                    reason: "not a book page URL".to_string(),
                });
                continue;
            }
        };
        let backing = root.join("docs").join(&relative);
        if !backing.exists() {
            broken.push(Broken {
                page: page.to_path_buf(),
                link,
                reason: format!("no page at docs/{relative}"),
            });
            continue;
        }
        let Some(fragment) = fragment else { continue };
        let anchors = heading_anchors_set(&fs::read_to_string(&backing)?);
        if !anchors.contains(&fragment) {
            broken.push(Broken {
                page: page.to_path_buf(),
                link,
                reason: format!("docs/{relative} has no heading anchored `#{fragment}`"),
            });
        }
    }
    Ok(broken)
}

/// Every markdown link on a page, in source order, skipping code.
pub fn links(markdown: &str) -> Vec<Link> {
    let mut found = Vec::new();
    for (index, raw) in prose_lines(markdown) {
        let line = blank_code_spans(&raw);
        let mut rest = line.as_str();
        let mut consumed = 0usize;
        while let Some(open) = rest.find("](") {
            let after = open + 2;
            let Some(close) = matching_paren(&rest[after..]) else {
                break;
            };
            let target = rest[after..after + close].trim();
            if !target.is_empty() {
                found.push(Link {
                    target: target.to_string(),
                    line: index,
                });
            }
            consumed += after + close + 1;
            rest = &line[consumed..];
        }
    }
    found
}

/// Every heading anchor on a page, in mdbook's spelling.
pub fn heading_anchors(markdown: &str) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut anchors = Vec::new();
    for (_, line) in prose_lines(markdown) {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let text = trimmed.trim_start_matches('#');
        if !text.starts_with(' ') && !text.is_empty() {
            continue;
        }
        let anchor = normalize_id(&strip_inline_markup(
            text.trim().trim_end_matches('#').trim(),
        ));
        if anchor.is_empty() {
            continue;
        }
        let count = seen.entry(anchor.clone()).or_insert(0);
        anchors.push(if *count == 0 {
            anchor.clone()
        } else {
            format!("{}-{}", anchor, count)
        });
        *count += 1;
    }
    anchors
}

/// mdbook's heading-to-anchor rule.
pub fn normalize_id(content: &str) -> String {
    content
        .chars()
        .filter_map(|ch| {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

fn heading_anchors_set(markdown: &str) -> HashSet<String> {
    heading_anchors(markdown).into_iter().collect()
}

fn collect_markdown(dir: &Path, found: &mut Vec<PathBuf>) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_markdown(&path, found)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            found.push(path);
        }
    }
    Ok(())
}

fn split_target(target: &str) -> Option<(&str, Option<&str>)> {
    let target = match target.split_once(char::is_whitespace) {
        Some((path, _title)) => path,
        None => target,
    };
    let target = target.trim_start_matches('<').trim_end_matches('>');
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
    {
        return None;
    }
    Some(match target.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (target, None),
    })
}

fn target_path(page: &Path, target: &str) -> Option<PathBuf> {
    let (path, _) = split_target(target)?;
    if path.is_empty() {
        return None;
    }
    Some(normalize_dots(&page.parent()?.join(path)))
}

fn matching_paren(rest: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(index),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The page's lines with fenced blocks dropped, 1-indexed.
fn prose_lines(markdown: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut fence: Option<String> = None;
    for (index, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        let opener = trimmed
            .starts_with("```")
            .then_some("```")
            .or_else(|| trimmed.starts_with("~~~").then_some("~~~"));
        match (&fence, opener) {
            (Some(open), Some(found)) if open == found => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, Some(found)) => {
                fence = Some(found.to_string());
                continue;
            }
            (None, None) => {}
        }
        lines.push((index + 1, line.to_string()));
    }
    lines
}

fn blank_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
            out.push(' ');
        } else if in_code {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

fn strip_inline_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '`' | '*' => {}
            '[' => {}
            ']' => {
                if chars.peek() == Some(&'(') {
                    for skipped in chars.by_ref() {
                        if skipped == ')' {
                            break;
                        }
                    }
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn relative_to(root: &Path, path: &Path) -> PathBuf {
    if let Ok(relative) = path.strip_prefix(root) {
        return normalize_dots(relative);
    }
    let canonical_root = canonical(root);
    match canonical(path).strip_prefix(&canonical_root) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => path.to_path_buf(),
    }
}

/// Collapse `a/b/../c` textually, the way a browser resolves a relative href.
///
/// The filesystem would resolve `..` through `docs/crates/<name>`, which is a
/// symlink, and land outside the book; the rendered page's link does not.
fn normalize_dots(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir
                if parts.last().is_some_and(|part| part != ".." && part != "/") =>
            {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_os_string()),
        }
    }
    parts.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("standout-docs-{}-{}", std::process::id(), name));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("docs/topics")).unwrap();
            fs::create_dir_all(dir.join("docs/guides")).unwrap();
            Fixture(dir)
        }

        fn write(&self, relative: &str, contents: &str) -> &Self {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
            self
        }

        fn root(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn links_are_read_outside_code() {
        let markdown = "\
See [one](a.md) and [two](b.md#frag).
`[code](never.md)` stays out.

```rust
let s = \"[fenced](never.md)\";
```

[three](c.md)
";
        let found = links(markdown);
        let targets: Vec<&str> = found.iter().map(|link| link.target.as_str()).collect();
        assert_eq!(targets, ["a.md", "b.md#frag", "c.md"]);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[2].line, 8);
    }

    #[test]
    fn anchors_follow_mdbooks_rule() {
        let markdown = "\
# Term vs Text
## `App::run_with` and friends
### Term vs Text
#not-a-heading
";
        assert_eq!(
            heading_anchors(markdown),
            [
                "term-vs-text".to_string(),
                "apprun_with-and-friends".to_string(),
                "term-vs-text-1".to_string(),
            ]
        );
    }

    #[test]
    fn a_page_no_summary_entry_names_is_reported() {
        let fixture = Fixture::new("unreachable");
        fixture
            .write(
                "docs/SUMMARY.md",
                "# Summary\n\n- [Mounted](./topics/mounted.md)\n",
            )
            .write("docs/topics/mounted.md", "# Mounted\n")
            .write("docs/topics/orphan.md", "# Orphan\n");

        let found = unreachable(fixture.root()).unwrap();
        assert_eq!(found, [PathBuf::from("docs/topics/orphan.md")]);
    }

    #[test]
    fn a_link_to_a_missing_file_is_reported() {
        let fixture = Fixture::new("missing-file");
        fixture
            .write("docs/SUMMARY.md", "# Summary\n\n- [One](./topics/one.md)\n")
            .write("docs/topics/one.md", "# One\n\nSee [two](two.md).\n");

        let found = broken_links(fixture.root()).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].link.target, "two.md");
        assert!(found[0].reason.contains("no file at"), "{}", found[0]);
    }

    #[test]
    fn a_link_to_a_missing_heading_is_reported() {
        let fixture = Fixture::new("missing-heading");
        fixture
            .write("docs/SUMMARY.md", "# Summary\n\n- [One](./topics/one.md)\n")
            .write("docs/topics/one.md", "# One\n\nSee [two](two.md#gone).\n")
            .write("docs/topics/two.md", "# Two\n\n## Here\n");

        let found = broken_links(fixture.root()).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].reason.contains("no heading anchored `#gone`"),
            "{}",
            found[0]
        );
    }

    #[test]
    fn a_resolving_book_reports_nothing() {
        let fixture = Fixture::new("clean");
        fixture
            .write(
                "docs/SUMMARY.md",
                "# Summary\n\n- [One](./topics/one.md)\n- [Two](./topics/two.md)\n",
            )
            .write(
                "docs/topics/one.md",
                "# One\n\nSee [two](two.md#here) and [self](#one).\n",
            )
            .write("docs/topics/two.md", "# Two\n\n## Here\n");

        assert_eq!(unreachable(fixture.root()).unwrap(), Vec::<PathBuf>::new());
        assert_eq!(broken_links(fixture.root()).unwrap(), Vec::<Broken>::new());
    }
}
