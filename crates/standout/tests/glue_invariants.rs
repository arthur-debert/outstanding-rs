//! Glue invariants from ADR-0031 / ROB04-WS06.
//!
//! These are tests, not `cargo deny`. Point-of-use defaults are not grepped:
//! `App` after `build()` holds theme, output-mode fallback, registry, and
//! ambiguous-width policy, and the ROB01 snapshot matrix fails if a call site
//! invents another.

use std::fs;
use std::path::{Path, PathBuf};

const BANNED_SERIALIZER_CRATES: &[&str] = &["serde_yaml", "csv", "quick-xml"];

/// Glue may match on `OutputMode::<variant>` only in these functions, which
/// route without serializing (help mapping, flag parsing). Serialization
/// arms belong in `standout-render`. None exist in glue today.
const ALLOWED_OUTPUT_MODE_ARM_FNS: &[&str] = &[];

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

fn skip_ws(text: &str, mut i: usize) -> usize {
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        i += ch.len_utf8();
    }
    i
}

fn ident_at(text: &str, i: usize) -> Option<(&str, usize)> {
    let rest = text.get(i..)?;
    let mut chars = rest.char_indices();
    let (_, first) = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let mut end = first.len_utf8();
    for (offset, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            end = offset + ch.len_utf8();
        } else {
            break;
        }
    }
    Some((&rest[..end], end))
}

/// True when `OutputMode::<variant>` at `after_ident` is a match arm
/// (`=>`, with `| OutputMode::Other` alternatives allowed).
fn arm_arrow_ahead(text: &str, mut i: usize) -> bool {
    loop {
        i = skip_ws(text, i);
        if text[i..].starts_with("=>") {
            return true;
        }
        if !text[i..].starts_with('|') {
            return false;
        }
        i += 1;
        i = skip_ws(text, i);
        if !text[i..].starts_with("OutputMode") {
            return false;
        }
        i += "OutputMode".len();
        i = skip_ws(text, i);
        if !text[i..].starts_with("::") {
            return false;
        }
        i += 2;
        i = skip_ws(text, i);
        let Some((_, nlen)) = ident_at(text, i) else {
            return false;
        };
        i += nlen;
    }
}

fn uncommented_source(source: &str) -> String {
    source
        .lines()
        .filter(|line| !is_comment_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Variant names in `OutputMode::<variant> =>` arms, whitespace-insensitive.
fn output_mode_arm_variants(source: &str) -> Vec<String> {
    let text = uncommented_source(source);
    let mut variants = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find("OutputMode") {
        let start = search_from + rel;
        let mut i = start + "OutputMode".len();
        i = skip_ws(&text, i);
        if !text[i..].starts_with("::") {
            search_from = start + 1;
            continue;
        }
        i += 2;
        i = skip_ws(&text, i);
        let Some((name, nlen)) = ident_at(&text, i) else {
            search_from = start + 1;
            continue;
        };
        i += nlen;
        if arm_arrow_ahead(&text, i) {
            variants.push(name.to_string());
        }
        search_from = start + 1;
    }
    variants
}

fn first_output_mode_arm_line(source: &str) -> Option<usize> {
    for (idx, line) in source.lines().enumerate() {
        if is_comment_line(line) {
            continue;
        }
        if line.contains("OutputMode::") || line.contains("OutputMode ::") {
            return Some(idx);
        }
    }
    None
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
fn glue_has_no_serializer_match_arms_on_output_mode() {
    let mut violations = Vec::new();
    for path in walk_rs(&glue_src()) {
        let source = fs::read_to_string(&path).unwrap();
        let variants = output_mode_arm_variants(&source);
        if !variants.is_empty() {
            let lines: Vec<&str> = source.lines().collect();
            let idx = first_output_mode_arm_line(&source).unwrap_or(0);
            let owner = enclosing_fn(&lines, idx).unwrap_or("<module>");
            if !ALLOWED_OUTPUT_MODE_ARM_FNS.contains(&owner) {
                violations.push(format!(
                    "{}:{} in `{owner}`: OutputMode match arm(s) {variants:?}",
                    path.strip_prefix(glue_root()).unwrap_or(&path).display(),
                    idx + 1,
                    variants = variants
                ));
            }
        }
        for (idx, line) in source.lines().enumerate() {
            if is_comment_line(line) {
                continue;
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

#[test]
fn output_mode_arm_scan_rejects_future_variant() {
    let src = r#"
        fn serialize_in_glue(mode: OutputMode) {
            match mode {
                OutputMode::Toml => drop("future serializer copy"),
            }
        }
    "#;
    assert_eq!(output_mode_arm_variants(src), ["Toml"]);
}

#[test]
fn output_mode_arm_scan_handles_whitespace_and_or_patterns() {
    let src = r#"
        match mode {
            OutputMode::Yaml
                => {}
            OutputMode::Json | OutputMode::Xml => {}
        }
    "#;
    let variants = output_mode_arm_variants(src);
    assert_eq!(variants, ["Yaml", "Json", "Xml"]);
}

#[test]
fn output_mode_arm_scan_ignores_matches_macro_and_comments() {
    let src = r#"
        // OutputMode::Toml => would be a serializer copy
        /// OutputMode::Csv => documented, not code
        matches!(mode, OutputMode::Json | OutputMode::Yaml);
        let _ = OutputMode::Xml;
    "#;
    assert!(
        output_mode_arm_variants(src).is_empty(),
        "comments, matches!, and constructions are not match arms"
    );
}
