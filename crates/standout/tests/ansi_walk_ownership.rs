use std::fs;
use std::path::{Path, PathBuf};

const WALKER: &str = "AnsiCodeIterator";
const OWNER: &str = "crates/standout-bbparser/src/ansi.rs";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn walk_rs(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            walk_rs(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn production_lines(source: &str) -> Vec<(usize, &str)> {
    let mut lines = Vec::new();
    let mut test_module_indent = None;
    for (number, line) in source.lines().enumerate() {
        let indent = line.len() - line.trim_start().len();
        match test_module_indent {
            Some(open) if indent <= open && line.trim_start().starts_with('}') => {
                test_module_indent = None;
                continue;
            }
            Some(_) => continue,
            None => {}
        }
        if line.trim_start().starts_with("#[cfg(test)]") {
            test_module_indent = Some(indent);
            continue;
        }
        if line.trim_start().starts_with("//") {
            continue;
        }
        lines.push((number + 1, line));
    }
    lines
}

/// Walking through the owning module is what puts `AnsiBalance` within reach of
/// a cutter; whether to use it stays each cutter's decision, and one of them
/// walks through the module without balancing.
#[test]
fn only_the_bbparser_ansi_module_walks_ansi_units() {
    let root = workspace_root();
    let mut files = Vec::new();
    walk_rs(&root.join("crates"), &mut files);
    files.sort();

    let mut offenders = Vec::new();
    for file in &files {
        let source = fs::read_to_string(file).unwrap();
        let relative = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative == OWNER || !relative.contains("/src/") {
            continue;
        }
        for (number, line) in production_lines(&source) {
            if line.contains(WALKER) {
                offenders.push(format!("{relative}:{number}: {}", line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "{WALKER} belongs to {OWNER}; call `ansi_units` instead of walking again:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_scan_reads_past_a_test_module_and_a_comment() {
    let source = "\
fn production() {
    let _ = AnsiCodeIterator::new(\"\");
}

#[cfg(test)]
mod tests {
    fn t() {
        let _ = AnsiCodeIterator::new(\"\");
    }
}

// AnsiCodeIterator in a comment
fn after() {}
";
    let hits: Vec<_> = production_lines(source)
        .into_iter()
        .filter(|(_, line)| line.contains(WALKER))
        .map(|(number, _)| number)
        .collect();
    assert_eq!(hits, [2]);
}
