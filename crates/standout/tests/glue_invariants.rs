//! Glue invariants from ADR-0031 / ROB04-WS06.
//!
//! These are tests, not `cargo deny`. Point-of-use defaults are not grepped:
//! `App` after `build()` holds theme, output-mode fallback, registry, and
//! ambiguous-width policy, and the ROB01 snapshot matrix fails if a call site
//! invents another.
//!
//! Serialization copies are forbidden by naming the thing actually forbidden:
//! production deps on `serde_yaml` / `csv` / `quick-xml`, and source uses of
//! those crates' paths or the leaf serializer helpers. A match-arm parser of
//! `OutputMode::<variant> =>` is not needed; glue cannot serialize what it
//! cannot depend on, and a helper-name scan catches a copied `serialize_to_xml`
//! that a Cargo.toml parse would miss.

use std::fs;
use std::path::{Path, PathBuf};

const BANNED_SERIALIZER_CRATES: &[&str] = &["serde_yaml", "csv", "quick-xml"];

/// Leaf helpers glue must not copy. `pub use` re-exports from standout-render
/// are allowed; a local definition or a call is not.
const BANNED_SERIALIZER_HELPERS: &[&str] = &[
    "serialize_to_xml",
    "flatten_json_for_csv",
    "serialize_structured",
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
    line.trim_start().starts_with("//")
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

/// Crate names from a Cargo dependency table: the key, or `package` when set.
fn collect_dep_crates(deps: &toml::Value, names: &mut Vec<String>) {
    let Some(table) = deps.as_table() else {
        return;
    };
    for (key, value) in table {
        match value {
            toml::Value::Table(entry) => {
                if let Some(package) = entry.get("package").and_then(|v| v.as_str()) {
                    names.push(package.to_string());
                } else {
                    names.push(key.clone());
                }
            }
            _ => names.push(key.clone()),
        }
    }
}

/// Production crate names from `[dependencies]`, `[dependencies.NAME]`,
/// and `[target.'.'.dependencies]` (including `package` aliases).
/// Dev-dependencies are ignored.
fn production_dependency_crates(manifest: &str) -> Result<Vec<String>, String> {
    let value: toml::Value =
        toml::from_str(manifest).map_err(|e| format!("Cargo.toml is not valid TOML: {e}"))?;
    let mut names = Vec::new();
    if let Some(deps) = value.get("dependencies") {
        collect_dep_crates(deps, &mut names);
    }
    if let Some(targets) = value.get("target").and_then(|t| t.as_table()) {
        for spec in targets.values() {
            if let Some(deps) = spec.get("dependencies") {
                collect_dep_crates(deps, &mut names);
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn banned_production_serializers(manifest: &str) -> Result<Vec<String>, String> {
    let names = production_dependency_crates(manifest)?;
    Ok(names
        .into_iter()
        .filter(|name| BANNED_SERIALIZER_CRATES.contains(&name.as_str()))
        .collect())
}

/// `(line, needle)` for each uncommented use of a banned serializer path or
/// helper. `pub use` re-exports are allowed. A helper is a copy when it is
/// defined (`fn serialize_to_xml`) or called (`serialize_to_xml(`), not when
/// it appears in a re-export list.
fn banned_serializer_uses(source: &str) -> Vec<(usize, String)> {
    let mut uses = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        if is_comment_line(line) || line.contains("pub use") {
            continue;
        }
        for crate_name in BANNED_SERIALIZER_CRATES {
            let needle = format!("{}::", crate_name.replace('-', "_"));
            if line.contains(&needle) {
                uses.push((idx, needle));
            }
        }
        for helper in BANNED_SERIALIZER_HELPERS {
            if line.contains(&format!("fn {helper}")) || line.contains(&format!("{helper}(")) {
                uses.push((idx, (*helper).to_string()));
            }
        }
    }
    uses
}

#[test]
fn standout_production_deps_exclude_serializer_crates() {
    let manifest = fs::read_to_string(glue_root().join("Cargo.toml")).unwrap();
    let banned = banned_production_serializers(&manifest).unwrap();
    assert!(
        banned.is_empty(),
        "standout production dependencies must not list serializer crates {:?}; they live in standout-render. found {banned:?}",
        BANNED_SERIALIZER_CRATES
    );
}

#[test]
fn production_dep_scan_flags_package_alias() {
    let manifest = r#"
[dependencies]
yaml = { package = "serde_yaml", version = "0.9" }
"#;
    let banned = banned_production_serializers(manifest).unwrap();
    assert_eq!(banned, ["serde_yaml"]);
}

#[test]
fn production_dep_scan_flags_inline_table_section() {
    let manifest = r#"
[dependencies.yaml]
package = "serde_yaml"
version = "0.9"
"#;
    let banned = banned_production_serializers(manifest).unwrap();
    assert_eq!(banned, ["serde_yaml"]);
}

#[test]
fn production_dep_scan_flags_dotted_workspace_key() {
    let manifest = r#"
[dependencies]
serde_yaml.workspace = true
"#;
    let banned = banned_production_serializers(manifest).unwrap();
    assert_eq!(banned, ["serde_yaml"]);
}

#[test]
fn production_dep_scan_flags_target_specific_deps() {
    let manifest = r#"
[target.'cfg(unix)'.dependencies]
csv = "1"

[target.'cfg(windows)'.dependencies.quick-xml]
version = "0.36"
"#;
    let banned = banned_production_serializers(manifest).unwrap();
    assert_eq!(banned, ["csv", "quick-xml"]);
}

#[test]
fn production_dep_scan_ignores_dev_dependencies() {
    let manifest = r#"
[dependencies]
serde = "1"

[dev-dependencies]
serde_yaml = "0.9"
csv = "1"
quick-xml = "0.36"
"#;
    let banned = banned_production_serializers(manifest).unwrap();
    assert!(
        banned.is_empty(),
        "dev-dependencies must not count: {banned:?}"
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
fn glue_does_not_call_leaf_serializers() {
    let mut violations = Vec::new();
    for path in walk_rs(&glue_src()) {
        let source = fs::read_to_string(&path).unwrap();
        for (idx, needle) in banned_serializer_uses(&source) {
            let line = source.lines().nth(idx).unwrap_or("");
            violations.push(format!(
                "{}:{} uses {needle}: {}",
                path.strip_prefix(glue_root()).unwrap_or(&path).display(),
                idx + 1,
                line.trim()
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "glue must not use leaf serializer paths or helpers (those copies live in standout-render):\n{}",
        violations.join("\n")
    );
}

#[test]
fn serializer_use_scan_flags_crate_path() {
    let src = r#"
        fn serialize_in_glue(data: &Value) {
            serde_yaml::to_string(data).unwrap();
        }
    "#;
    let uses: Vec<_> = banned_serializer_uses(src)
        .into_iter()
        .map(|(_, needle)| needle)
        .collect();
    assert_eq!(uses, ["serde_yaml::"]);
}

#[test]
fn serializer_use_scan_flags_helper_name() {
    let src = r#"
        fn copy_in_glue(data: &Value) {
            serialize_to_xml(data).unwrap();
        }
    "#;
    let uses: Vec<_> = banned_serializer_uses(src)
        .into_iter()
        .map(|(_, needle)| needle)
        .collect();
    assert_eq!(uses, ["serialize_to_xml"]);
}

#[test]
fn serializer_use_scan_allows_pub_use_and_ignores_comments() {
    let src = r#"
        // serde_yaml::to_string would be a serializer copy
        /// flatten_json_for_csv documented, not code
        pub use standout_render::{flatten_json_for_csv, serialize_to_xml};
    "#;
    assert!(
        banned_serializer_uses(src).is_empty(),
        "comments and pub use re-exports are not copies"
    );
}
