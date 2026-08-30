//! The book's graph, checked against this repository.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate sits two directories below the repository root")
        .to_path_buf()
}

#[test]
fn every_page_is_reachable_from_the_summary() {
    let orphans = standout_docs::book::unreachable(&repo_root()).expect("the book is readable");
    assert!(
        orphans.is_empty(),
        "docs/SUMMARY.md mounts no entry for {} page(s); add each one or delete it:\n  {}",
        orphans.len(),
        orphans
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn every_link_between_pages_resolves() {
    let broken = standout_docs::book::broken_links(&repo_root()).expect("the book is readable");
    assert!(
        broken.is_empty(),
        "{} link(s) do not resolve:\n  {}",
        broken.len(),
        broken
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn every_readme_link_into_the_book_resolves() {
    let broken = standout_docs::book::broken_site_links(&repo_root(), Path::new("README.md"))
        .expect("the README is readable");
    assert!(
        broken.is_empty(),
        "{} README link(s) into the book do not resolve:\n  {}",
        broken.len(),
        broken
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
