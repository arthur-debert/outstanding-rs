//! Glue invariants from ADR-0031 / ROB04-WS06.
//!
//! These are tests, not `cargo deny`. Point-of-use defaults are not grepped:
//! `App` after `build()` holds theme, output-mode fallback, registry, and
//! ambiguous-width policy, and the ROB01 snapshot matrix fails if a call site
//! invents another.

use std::fs;
use std::path::{Path, PathBuf};

const BANNED_SERIALIZER_CRATES: &[&str] = &["serde_yaml", "csv", "quick-xml"];

const SERIALIZER_OUTPUT_MODE_ARMS: &[&str] = &[
    "OutputMode::Json =>",
    "OutputMode::Yaml =>",
    "OutputMode::Xml =>",
    "OutputMode::Csv =>",
];

fn glue_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn glue_src() -> PathBuf {
    glue_root().join("src")
}

fn walk_rs(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk_rs(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn is_comment_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("//!")
}

fn fn_name_in_line(line: &str) -> Option<&str> {
    let mut rest = line.trim_start();
    if let Some(after_pub) = rest.strip_prefix("pub") {
        rest = after_pub.trim_start();
        if rest.starts_with('(') {
            let close = rest.find(')')?;
            rest = rest[close + 1..].trim_start();
        }
    }
    if let Some(after_async) = rest.strip_prefix("async") {
        rest = after_async.trim_start();
    }
    let rest = rest.strip_prefix("fn")?.trim_start();
    let name = rest
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn enclosing_fn<'a>(lines: &'a [&str], line_idx: usize) -> Option<&'a str> {
    lines[..line_idx].iter().rev().find_map(|line| {
        if is_comment_line(line) {
            None
        } else {
            fn_name_in_line(line)
        }
    })
}

fn production_dependency_keys(manifest: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = &trimmed[1..trimmed.len() - 1];
            if name == "dependencies" {
                in_deps = true;
            } else if let Some(rest) = name.strip_prefix("dependencies.") {
                in_deps = true;
                keys.push(rest.to_string());
            } else {
                in_deps = false;
            }
            continue;
        }
        if !in_deps || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(key) = trimmed.split('=').next() {
            let key = key.trim();
            if !key.is_empty() {
                keys.push(key.to_string());
            }
        }
    }
    keys
}

#[test]
fn standout_production_deps_exclude_serializer_crates() {
    let manifest = fs::read_to_string(glue_root().join("Cargo.toml")).unwrap();
    let keys = production_dependency_keys(&manifest);
    let banned: Vec<_> = keys
        .iter()
        .filter(|key| BANNED_SERIALIZER_CRATES.contains(&key.as_str()))
        .cloned()
        .collect();
    assert!(
        banned.is_empty(),
        "standout [dependencies] must not list serializer crates {:?}; they live in standout-render. found {banned:?}",
        BANNED_SERIALIZER_CRATES
    );
}

#[test]
fn minijinja_engine_new_exists_only_in_build() {
    let mut violations = Vec::new();
    for path in walk_rs(&glue_src()) {
        let source = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if is_comment_line(line) || !line.contains("MiniJinjaEngine::new()") {
                continue;
            }
            let owner = enclosing_fn(&lines, idx).unwrap_or("<module>");
            if owner != "build" {
                violations.push(format!(
                    "{}:{} in `{owner}`: {}",
                    path.strip_prefix(glue_root()).unwrap_or(&path).display(),
                    idx + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "MiniJinjaEngine::new() may appear in glue only inside build();\n{}",
        violations.join("\n")
    );
}

#[test]
fn glue_has_no_serializer_match_arms_on_output_mode() {
    let mut violations = Vec::new();
    for path in walk_rs(&glue_src()) {
        let source = fs::read_to_string(&path).unwrap();
        for (idx, line) in source.lines().enumerate() {
            if is_comment_line(line) {
                continue;
            }
            for arm in SERIALIZER_OUTPUT_MODE_ARMS {
                if line.contains(arm) {
                    violations.push(format!(
                        "{}:{}: {}",
                        path.strip_prefix(glue_root()).unwrap_or(&path).display(),
                        idx + 1,
                        line.trim()
                    ));
                }
            }
            for crate_name in BANNED_SERIALIZER_CRATES {
                let needle = format!("{}::", crate_name.replace('-', "_"));
                if line.contains(&needle) && !line.contains("pub use") {
                    violations.push(format!(
                        "{}:{} uses {needle}: {}",
                        path.strip_prefix(glue_root()).unwrap_or(&path).display(),
                        idx + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "glue must not match on OutputMode to serialize (that copy lives in standout-render):\n{}",
        violations.join("\n")
    );
}
