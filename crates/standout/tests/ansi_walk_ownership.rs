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

fn offenders_in(relative: &str, source: &str) -> Vec<String> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(WALKER))
        .map(|(number, line)| format!("{relative}:{}: {}", number + 1, line.trim()))
        .collect()
}

/// Walking through the owning module is what puts `AnsiBalance` within reach of
/// a cutter; whether to use it stays each cutter's decision, and one of them
/// walks through the module without balancing. The rule is that the name
/// appears nowhere else, test code included, so the scan reads every line and
/// has nothing it can skip.
#[test]
fn only_the_bbparser_ansi_module_names_the_walker() {
    let root = workspace_root();
    let mut files = Vec::new();
    walk_rs(&root.join("crates"), &mut files);
    files.sort();

    let mut offenders = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative == OWNER || !relative.contains("/src/") {
            continue;
        }
        let source = fs::read_to_string(file).unwrap();
        offenders.extend(offenders_in(&relative, &source));
    }

    assert!(
        offenders.is_empty(),
        "{WALKER} belongs to {OWNER}; call `ansi_units` instead of walking again:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_scan_reads_lines_a_cfg_test_item_would_have_hidden() {
    let source = "\
#[cfg(test)]
use console::AnsiCodeIterator;

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
    assert_eq!(
        offenders_in("file.rs", source),
        [
            "file.rs:2: use console::AnsiCodeIterator;",
            "file.rs:5: let _ = AnsiCodeIterator::new(\"\");",
            "file.rs:11: let _ = AnsiCodeIterator::new(\"\");",
            "file.rs:15: // AnsiCodeIterator in a comment",
        ]
    );
}
