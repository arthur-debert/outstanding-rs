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

fn skip_balanced_closer(text: &str, mut i: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 1i32;
    i += open.len_utf8();
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(i + close.len_utf8());
            }
        }
        i += ch.len_utf8();
    }
    None
}

/// Tuple/struct variant payload after `OutputMode::Ident`.
fn skip_optional_payload(text: &str, mut i: usize) -> usize {
    i = skip_ws(text, i);
    match text[i..].chars().next() {
        Some('(') => skip_balanced_closer(text, i, '(', ')').unwrap_or(i),
        Some('{') => skip_balanced_closer(text, i, '{', '}').unwrap_or(i),
        _ => i,
    }
}

fn is_if_keyword_at(text: &str, i: usize) -> bool {
    text[i..].starts_with("if")
        && !matches!(
            text[i + 2..].chars().next(),
            Some(ch) if ch.is_ascii_alphanumeric() || ch == '_'
        )
}

fn consume_output_mode_variant(text: &str, i: &mut usize) -> bool {
    *i = skip_ws(text, *i);
    if !text[*i..].starts_with("OutputMode") {
        return false;
    }
    *i += "OutputMode".len();
    *i = skip_ws(text, *i);
    if !text[*i..].starts_with("::") {
        return false;
    }
    *i += 2;
    *i = skip_ws(text, *i);
    let Some((_, nlen)) = ident_at(text, *i) else {
        return false;
    };
    *i += nlen;
    *i = skip_optional_payload(text, *i);
    true
}

/// Skip a match-guard expression until `=>` at nesting depth 0.
fn skip_guard_to_arrow(text: &str, mut i: usize) -> Option<usize> {
    let mut depth = 0i32;
    while i < text.len() {
        if depth == 0 && text[i..].starts_with("=>") {
            return Some(i);
        }
        let ch = text[i..].chars().next().unwrap();
        match ch {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += ch.len_utf8();
    }
    None
}

/// True when `OutputMode::<variant>` at `after_ident` is a match arm:
/// optional payload, `|` alternatives, optional `if` guard, then `=>`.
fn arm_arrow_ahead(text: &str, mut i: usize) -> bool {
    i = skip_optional_payload(text, i);
    loop {
        i = skip_ws(text, i);
        if text[i..].starts_with("=>") {
            return true;
        }
        if is_if_keyword_at(text, i) {
            i = skip_ws(text, i + 2);
            return skip_guard_to_arrow(text, i).is_some();
        }
        if !text[i..].starts_with('|') {
            return false;
        }
        i += 1;
        if !consume_output_mode_variant(text, &mut i) {
            return false;
        }
    }
}

fn uncommented_source(source: &str) -> String {
    source
        .lines()
        .map(|line| if is_comment_line(line) { "" } else { line })
        .collect::<Vec<_>>()
        .join("\n")
}

fn line_index(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
}

/// `(line, variant)` for each `OutputMode::<variant> =>` arm.
fn output_mode_match_arms(source: &str) -> Vec<(usize, String)> {
    let text = uncommented_source(source);
    let mut arms = Vec::new();
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
            arms.push((line_index(&text, start), name.to_string()));
        }
        search_from = start + 1;
    }
    arms
}

fn output_mode_arm_variants(source: &str) -> Vec<String> {
    output_mode_match_arms(source)
        .into_iter()
        .map(|(_, variant)| variant)
        .collect()
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
        let lines: Vec<&str> = source.lines().collect();
        for (idx, variant) in output_mode_match_arms(&source) {
            let owner = enclosing_fn(&lines, idx).unwrap_or("<module>");
            if !ALLOWED_OUTPUT_MODE_ARM_FNS.contains(&owner) {
                violations.push(format!(
                    "{}:{} in `{owner}`: OutputMode::{variant} =>",
                    path.strip_prefix(glue_root()).unwrap_or(&path).display(),
                    idx + 1
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

#[test]
fn output_mode_arm_scan_rejects_guarded_arms() {
    let src = r#"
        match mode {
            OutputMode::Json if should_serialize() => drop("guarded copy"),
            OutputMode::Toml
                if extra() => {}
            OutputMode::Yaml | OutputMode::Xml if both() => {}
        }
    "#;
    assert_eq!(
        output_mode_arm_variants(src),
        ["Json", "Toml", "Yaml", "Xml"]
    );
}

#[test]
fn output_mode_arm_scan_reports_the_arm_line_not_an_earlier_use() {
    let src = r#"
fn set_mode() {
    let _ = OutputMode::Text;
}
fn serialize_in_glue(mode: OutputMode) {
    match mode {
        OutputMode::Json => {}
    }
}
"#;
    let arms = output_mode_match_arms(src);
    assert_eq!(arms.len(), 1);
    assert_eq!(arms[0].1, "Json");
    let lines: Vec<&str> = src.lines().collect();
    assert_eq!(enclosing_fn(&lines, arms[0].0), Some("serialize_in_glue"));
}
